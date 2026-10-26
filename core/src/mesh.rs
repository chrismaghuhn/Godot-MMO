//! Mesh-Erzeugung aus dem Dichtefeld mit Surface Nets (#2, D-004).
//!
//! Pro Zelle mit Vorzeichenwechsel entsteht ein Vertex (Mittel der Kanten-Schnittpunkte),
//! pro geschnittener Gitterkante ein Quad aus den vier angrenzenden Zellen.
//!
//! Nahtlosigkeit: Ein Chunk besitzt genau die Kanten, deren Startpunkt lokal in
//! `0..CHUNK_SIZE` liegt. Die Zellen der Randlage `-1` berechnet er zusätzlich aus den
//! Rand-Samples, identisch zu den Zellen `CHUNK_SIZE - 1` des Nachbarn.

use crate::volume::{ColumnData, DensityChunk, CHUNK_SIZE, SAMPLE_MIN};
use crate::world::WorldGen;

/// Zellen pro Achse: lokale Zellkoordinaten `-1..CHUNK_SIZE`.
const CELLS: usize = CHUNK_SIZE as usize + 1;
const NO_VERTEX: u32 = u32::MAX;

/// Engine-unabhängiges Mesh. Positionen sind lokal zum Chunk-Ursprung (`DensityChunk::origin`).
/// Dreiecke sind gegen den Uhrzeigersinn orientiert, von außen (Luftseite) gesehen.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Vertex-Farben in sRGB.
    pub colors: Vec<[u8; 3]>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Mesh inklusive Biom-Farben.
    pub fn build(world: &WorldGen, column: &ColumnData, chunk: &DensityChunk) -> Self {
        let mut mesh = surface_nets(chunk);
        let origin_y = chunk.origin().y as f32;
        mesh.colors = mesh
            .positions
            .iter()
            .map(|&[x, y, z]| {
                let local_x = (x.round() as i32).clamp(SAMPLE_MIN, CHUNK_SIZE + 1);
                let local_z = (z.round() as i32).clamp(SAMPLE_MIN, CHUNK_SIZE + 1);
                let biome = column.biome(local_x, local_z);
                world.apply_altitude(world.biome_color(biome), origin_y + y)
            })
            .collect();
        mesh
    }
}

#[inline]
fn cell_index(x: i32, y: i32, z: i32) -> usize {
    let ix = (x - SAMPLE_MIN) as usize;
    let iy = (y - SAMPLE_MIN) as usize;
    let iz = (z - SAMPLE_MIN) as usize;
    (iz * CELLS + iy) * CELLS + ix
}

const CORNERS: [[i32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// Die 12 Zellkanten als Paare von Ecken-Indizes.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Surface Nets ohne Farben.
pub fn surface_nets(chunk: &DensityChunk) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    let mut vertex_of_cell = vec![NO_VERTEX; CELLS * CELLS * CELLS];

    // 1. Vertices: eine pro Zelle mit Vorzeichenwechsel.
    for z in SAMPLE_MIN..CHUNK_SIZE {
        for y in SAMPLE_MIN..CHUNK_SIZE {
            for x in SAMPLE_MIN..CHUNK_SIZE {
                let mut density = [0.0f32; 8];
                let mut solid_mask = 0u8;
                for (corner, offset) in CORNERS.iter().enumerate() {
                    let value = chunk.get(x + offset[0], y + offset[1], z + offset[2]);
                    density[corner] = value;
                    if value > 0.0 {
                        solid_mask |= 1 << corner;
                    }
                }
                if solid_mask == 0 || solid_mask == 0xFF {
                    continue;
                }

                let mut sum = [0.0f32; 3];
                let mut crossings = 0.0f32;
                for &(a, b) in &EDGES {
                    let (da, db) = (density[a], density[b]);
                    if (da > 0.0) == (db > 0.0) {
                        continue;
                    }
                    let t = da / (da - db);
                    for axis in 0..3 {
                        let pa = CORNERS[a][axis] as f32;
                        let pb = CORNERS[b][axis] as f32;
                        sum[axis] += pa + (pb - pa) * t;
                    }
                    crossings += 1.0;
                }

                // Gradient aus den Zell-Ecken; Normale zeigt von fest nach Luft (= -Gradient).
                let gx = (density[1] - density[0])
                    + (density[3] - density[2])
                    + (density[5] - density[4])
                    + (density[7] - density[6]);
                let gy = (density[2] - density[0])
                    + (density[3] - density[1])
                    + (density[6] - density[4])
                    + (density[7] - density[5]);
                let gz = (density[4] - density[0])
                    + (density[5] - density[1])
                    + (density[6] - density[2])
                    + (density[7] - density[3]);
                let length = (gx * gx + gy * gy + gz * gz).sqrt().max(1e-6);

                vertex_of_cell[cell_index(x, y, z)] = mesh.positions.len() as u32;
                mesh.positions.push([
                    x as f32 + sum[0] / crossings,
                    y as f32 + sum[1] / crossings,
                    z as f32 + sum[2] / crossings,
                ]);
                mesh.normals.push([-gx / length, -gy / length, -gz / length]);
            }
        }
    }

    // 2. Quads: eine pro geschnittener Kante, deren Startpunkt dem Chunk gehört.
    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let base = chunk.get(x, y, z);
                let base_solid = base > 0.0;
                // Achse a mit den beiden anderen Achsen (b, c) in zyklischer Reihenfolge,
                // damit e_b × e_c = e_a gilt.
                for axis in 0..3 {
                    let mut next = [x, y, z];
                    next[axis] += 1;
                    if (chunk.get(next[0], next[1], next[2]) > 0.0) == base_solid {
                        continue;
                    }
                    let b = (axis + 1) % 3;
                    let c = (axis + 2) % 3;
                    let cell = |db: i32, dc: i32| {
                        let mut p = [x, y, z];
                        p[b] -= db;
                        p[c] -= dc;
                        vertex_of_cell[cell_index(p[0], p[1], p[2])]
                    };
                    let v00 = cell(1, 1);
                    let v10 = cell(0, 1);
                    let v11 = cell(0, 0);
                    let v01 = cell(1, 0);
                    debug_assert!([v00, v10, v11, v01].iter().all(|&v| v != NO_VERTEX));
                    // Fest am Startpunkt: Außenseite zeigt in +a, sonst in -a.
                    if base_solid {
                        mesh.indices.extend_from_slice(&[v00, v10, v11, v00, v11, v01]);
                    } else {
                        mesh.indices.extend_from_slice(&[v00, v11, v10, v00, v01, v11]);
                    }
                }
            }
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume::{ChunkKind, ColumnData};
    use glam::Vec3;

    fn triangle_normal(mesh: &ChunkMesh, tri: &[u32]) -> Vec3 {
        let p = |i: u32| Vec3::from(mesh.positions[i as usize]);
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        (b - a).cross(c - a)
    }

    fn surface_chunk(world: &WorldGen, chunk_x: i32, chunk_z: i32) -> (ColumnData, DensityChunk) {
        let column = ColumnData::sample(world, chunk_x, chunk_z);
        let chunk_y = column.surface_chunks().next().expect("Spalte ohne Oberfläche");
        let chunk = DensityChunk::from_column(&column, chunk_y);
        (column, chunk)
    }

    #[test]
    fn flat_plane_is_flat_and_faces_up() {
        // Ebene auf Höhe 10.5 als künstliche Spalte.
        let world = WorldGen::new(1);
        let mut column = ColumnData::sample(&world, 0, 0);
        for row in column.heights.iter_mut() {
            row.fill(10.5);
        }
        let chunk = DensityChunk::from_column(&column, 0);
        let mesh = surface_nets(&chunk);
        assert!(!mesh.is_empty());
        // 32×32 eigene Kanten in y-Richtung → 2 Dreiecke je Kante.
        assert_eq!(mesh.indices.len(), 32 * 32 * 6);
        for position in &mesh.positions {
            assert!((position[1] - 10.5).abs() < 1e-5, "Vertex nicht auf der Ebene: {position:?}");
        }
        for normal in &mesh.normals {
            assert!((normal[1] - 1.0).abs() < 1e-5);
        }
        for tri in mesh.indices.chunks(3) {
            assert!(triangle_normal(&mesh, tri).y > 0.0, "Dreieck zeigt nach unten");
        }
    }

    #[test]
    fn triangle_winding_matches_density_normals() {
        let world = WorldGen::new(42);
        for (chunk_x, chunk_z) in [(0, 0), (9, -4), (-20, 13)] {
            let (_, chunk) = surface_chunk(&world, chunk_x, chunk_z);
            let mesh = surface_nets(&chunk);
            let mut agree = 0usize;
            for tri in mesh.indices.chunks(3) {
                let face = triangle_normal(&mesh, tri);
                let vertex_normal: Vec3 = tri.iter().map(|&i| Vec3::from(mesh.normals[i as usize])).sum();
                if face.dot(vertex_normal) > 0.0 {
                    agree += 1;
                }
            }
            let total = mesh.indices.len() / 3;
            assert!(agree * 100 >= total * 99, "nur {agree}/{total} Dreiecke korrekt orientiert");
        }
    }

    #[test]
    fn neighbouring_chunks_share_border_vertices() {
        // Zellen der Randlage x = -1 im rechten Nachbarn müssen den Zellen x = CHUNK_SIZE - 1
        // im linken Chunk entsprechen, sonst entstehen sichtbare Nähte.
        let world = WorldGen::new(42);
        let mut compared = 0usize;
        for (chunk_x, chunk_z) in [(0, 0), (6, 2), (-11, -5)] {
            let column_a = ColumnData::sample(&world, chunk_x, chunk_z);
            let column_b = ColumnData::sample(&world, chunk_x + 1, chunk_z);
            for chunk_y in column_a.surface_chunks() {
                if column_b.classify(chunk_y) != ChunkKind::Surface {
                    continue;
                }
                let a = surface_nets(&DensityChunk::from_column(&column_a, chunk_y));
                let b = surface_nets(&DensityChunk::from_column(&column_b, chunk_y));
                // x in gemeinsame Koordinaten (relativ zu Chunk A); y und z sind in beiden gleich.
                let size = CHUNK_SIZE as f32;
                let border = |mesh: &ChunkMesh, low: f32, shift: f32| {
                    let mut points: Vec<[f32; 3]> = mesh
                        .positions
                        .iter()
                        .filter(|p| p[0] >= low && p[0] <= low + 1.0)
                        .map(|p| [p[0] + shift, p[1], p[2]])
                        .collect();
                    points.sort_by(|p, q| {
                        p[2].total_cmp(&q[2]).then(p[1].total_cmp(&q[1])).then(p[0].total_cmp(&q[0]))
                    });
                    points
                };
                let border_a = border(&a, size - 1.0, 0.0);
                let border_b = border(&b, -1.0, size);
                assert_eq!(
                    border_a.len(),
                    border_b.len(),
                    "Anzahl Randvertices unterschiedlich bei {chunk_x},{chunk_y},{chunk_z}"
                );
                for (p, q) in border_a.iter().zip(&border_b) {
                    let distance = Vec3::from(*p).distance(Vec3::from(*q));
                    assert!(distance < 1e-4, "Naht bei {chunk_x},{chunk_y},{chunk_z}: {p:?} vs {q:?}");
                }
                compared += border_a.len();
            }
        }
        assert!(compared > 100, "zu wenige Randvertices verglichen: {compared}");
    }

    #[test]
    fn meshing_is_deterministic() {
        let world = WorldGen::new(123);
        let (column, chunk) = surface_chunk(&world, 4, 4);
        let first = ChunkMesh::build(&world, &column, &chunk);
        let (column, chunk) = surface_chunk(&world, 4, 4);
        let second = ChunkMesh::build(&world, &column, &chunk);
        assert_eq!(first, second);
        assert_eq!(first.colors.len(), first.positions.len());
    }

    #[test]
    fn all_indices_are_valid() {
        let world = WorldGen::new(42);
        let column = ColumnData::sample(&world, 15, -9);
        for chunk_y in column.surface_chunks() {
            let mesh = surface_nets(&DensityChunk::from_column(&column, chunk_y));
            assert_eq!(mesh.indices.len() % 3, 0);
            assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
        }
    }
}
