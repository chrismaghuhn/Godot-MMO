//! Core-Meshes → Godot-Meshes.
//!
//! `MeshData::from_core` läuft im Worker-Thread des Streamers (nur Rust-Daten, keine Godot-Objekte),
//! `to_array_mesh` im Hauptthread: Dort fallen nur noch Kopien in Godot-Arrays und der Upload an.

use godot::classes::ArrayMesh;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::prelude::*;
use terraforge_core::mesh::ChunkMesh;

/// Vorbereitetes Mesh im Godot-Format, aber ohne Godot-Objekte (darf zwischen Threads wandern).
pub(crate) struct MeshData {
    vertices: Vec<Vector3>,
    normals: Vec<Vector3>,
    colors: Vec<Color>,
    indices: Vec<i32>,
}

impl MeshData {
    /// Der Core erzeugt Dreiecke gegen den Uhrzeigersinn (von außen gesehen), Godot erwartet sie
    /// im Uhrzeigersinn: Pro Dreieck werden zwei Ecken getauscht. Farben bleiben sRGB, das
    /// Material wandelt sie um (`vertex_color_is_srgb`).
    pub(crate) fn from_core(mesh: ChunkMesh) -> Self {
        let vector = |[x, y, z]: [f32; 3]| Vector3::new(x, y, z);
        let mut indices: Vec<i32> = mesh.indices.iter().map(|&i| i32::try_from(i).expect("Mesh mit mehr als 2³¹ Vertices")).collect();
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
        Self {
            vertices: mesh.positions.into_iter().map(vector).collect(),
            normals: mesh.normals.into_iter().map(vector).collect(),
            colors: mesh.colors.into_iter().map(|[r, g, b]| Color::from_rgba8(r, g, b, 255)).collect(),
            indices,
        }
    }

    pub(crate) fn into_array_mesh(self) -> Gd<ArrayMesh> {
        let mut arrays = VarArray::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(ArrayType::VERTEX.ord() as usize, &PackedVector3Array::from(self.vertices).to_variant());
        arrays.set(ArrayType::NORMAL.ord() as usize, &PackedVector3Array::from(self.normals).to_variant());
        arrays.set(ArrayType::COLOR.ord() as usize, &PackedColorArray::from(self.colors).to_variant());
        arrays.set(ArrayType::INDEX.ord() as usize, &PackedInt32Array::from(self.indices).to_variant());
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Godot zeichnet Dreiecke, deren Ecken von außen gesehen im Uhrzeigersinn liegen: Das
    /// Kreuzprodukt der Kanten zeigt dann gegen die Normale (nach innen).
    #[test]
    fn triangles_face_outwards_in_godot() {
        let world = terraforge_core::WorldGen::new(42);
        let column = terraforge_core::volume::ColumnData::sample(&world, 0, 0);
        let chunk_y = column.surface_chunks().next().expect("Oberflächen-Chunk");
        let density = terraforge_core::volume::DensityChunk::from_column(&column, chunk_y);
        let data = MeshData::from_core(ChunkMesh::build(&world, &column, &density));
        assert!(!data.indices.is_empty());
        let mut inward = 0;
        for triangle in data.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| data.vertices[triangle[i] as usize]);
            let normal = data.normals[triangle[0] as usize];
            if (b - a).cross(c - a).dot(normal) < 0.0 {
                inward += 1;
            }
        }
        // Gemittelte Normalen können an Kanten einzelner Dreiecke abweichen.
        assert!(inward * 100 >= data.indices.len() / 3 * 99, "{inward} von {} Dreiecken richtig", data.indices.len() / 3);
    }
}
