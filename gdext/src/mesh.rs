//! Core-Meshes → Godot-Meshes.
//!
//! `MeshData::from_core` läuft im Worker-Thread des Streamers (nur Rust-Daten, keine Godot-Objekte),
//! `to_array_mesh` im Hauptthread: Dort fallen nur noch Kopien in Godot-Arrays und der Upload an.

use godot::classes::ArrayMesh;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::prelude::*;
use terraforge_core::mesh::ChunkMesh;
use terraforge_core::vegetation::TreeGeo;

/// Vorbereitetes Mesh im Godot-Format, aber ohne Godot-Objekte (darf zwischen Threads wandern).
pub(crate) struct MeshData {
    vertices: Vec<Vector3>,
    normals: Vec<Vector3>,
    colors: Vec<Color>,
    indices: Vec<i32>,
}

fn vector([x, y, z]: [f32; 3]) -> Vector3 {
    Vector3::new(x, y, z)
}

/// Der Core erzeugt Dreiecke gegen den Uhrzeigersinn (von außen gesehen), Godot erwartet sie im
/// Uhrzeigersinn: Pro Dreieck werden zwei Ecken getauscht. Gilt für Chunks und Bäume gleich.
fn godot_indices(indices: &[u32]) -> Vec<i32> {
    let mut indices: Vec<i32> = indices.iter().map(|&i| i32::try_from(i).expect("Mesh mit mehr als 2³¹ Vertices")).collect();
    for triangle in indices.as_chunks_mut::<3>().0 {
        triangle.swap(1, 2);
    }
    indices
}

impl MeshData {
    /// Chunk- oder Fern-Mesh. Farben bleiben sRGB, das Material wandelt sie um
    /// (`vertex_color_is_srgb`).
    pub(crate) fn from_core(mesh: ChunkMesh) -> Self {
        Self {
            vertices: mesh.positions.into_iter().map(vector).collect(),
            normals: mesh.normals.into_iter().map(vector).collect(),
            colors: mesh.colors.into_iter().map(|[r, g, b]| Color::from_rgba8(r, g, b, 255)).collect(),
            indices: godot_indices(&mesh.indices),
        }
    }

    /// Baum-Geometrie (#12). Farben sind schon linear.
    pub(crate) fn from_tree(tree: TreeGeo) -> Self {
        Self {
            vertices: tree.positions.into_iter().map(vector).collect(),
            normals: tree.normals.into_iter().map(vector).collect(),
            colors: tree.colors.into_iter().map(|[r, g, b, a]| Color::from_rgba(r, g, b, a)).collect(),
            indices: godot_indices(&tree.indices),
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

    /// Anteil der Dreiecke, deren Ecken von der Normalen aus gesehen im Uhrzeigersinn liegen (so
    /// zeichnet Godot Vorderseiten): Das Kreuzprodukt der Kanten zeigt dann gegen die Normale.
    fn clockwise_share(data: &MeshData) -> f32 {
        let triangles = data.indices.as_chunks::<3>().0;
        let clockwise = triangles
            .iter()
            .filter(|triangle| {
                let [a, b, c] = triangle.map(|i| data.vertices[i as usize]);
                (b - a).cross(c - a).dot(data.normals[triangle[0] as usize]) < 0.0
            })
            .count();
        clockwise as f32 / triangles.len() as f32
    }

    #[test]
    fn chunk_triangles_face_outwards_in_godot() {
        let world = terraforge_core::WorldGen::new(42);
        let column = terraforge_core::volume::ColumnData::sample(&world, 0, 0);
        let chunk_y = column.surface_chunks().next().expect("Oberflächen-Chunk");
        let density = terraforge_core::volume::DensityChunk::from_column(&column, chunk_y);
        let data = MeshData::from_core(ChunkMesh::build(&world, &column, &density));
        assert!(!data.indices.is_empty());
        // Gemittelte Normalen können an Kanten einzelner Dreiecke abweichen.
        assert!(clockwise_share(&data) >= 0.99, "{}", clockwise_share(&data));
    }

    /// Bäume haben ein beidseitiges Material: Falsch herum würde Godot die Normalen der Rückseiten
    /// umdrehen und die Bäume von innen beleuchten (Review-Fund: Umkehr nur für Chunks getestet).
    #[test]
    fn tree_triangles_face_outwards_in_godot() {
        for style in terraforge_core::vegetation::TreeStyle::ALL {
            for variant in 0..3 {
                let data = MeshData::from_tree(terraforge_core::vegetation::build_tree_geometry(style, variant));
                assert_eq!(clockwise_share(&data), 1.0, "{style:?} {variant}");
            }
        }
    }
}
