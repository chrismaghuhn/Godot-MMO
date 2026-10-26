//! Fernterrain (#33): grobe Kacheln aus der öffentlichen Höhenkarte für weite Sicht.
//!
//! In der Ferne spielen Höhlen und Überhänge keine Rolle, deshalb reicht die Höhenkarte. Alles
//! kommt aus öffentlichen Daten (D-005): Höhe, Biom-Farbe, Oberflächenmaterial.
//!
//! - Eine Kachel ist ein Raster mit `cells × cells` Zellen der Weite `step`.
//! - Zellen können ausgespart werden (dort liegt feineres Terrain).
//! - An jeder Kante zwischen einer gezeichneten und einer nicht gezeichneten Zelle (bzw. dem
//!   Kachelrand) hängt eine senkrechte **Schürze** nach unten. Sie verdeckt Risse zu Nachbarn mit
//!   anderer Auflösung. Schürzen haben beide Seiten, damit sie aus jeder Richtung sichtbar sind.
//!
//! Gemeinsame Kanten zweier Kacheln gleicher Auflösung nutzen dieselben Weltkoordinaten und
//! sind deshalb bitgleich.

use crate::geology::material_at;
use crate::material::MaterialId;
use crate::mesh::ChunkMesh;
use crate::world::WorldGen;

/// Beschreibung einer Fernkachel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FarTile {
    /// Weltposition der Ecke mit den kleinsten Koordinaten.
    pub origin_x: f32,
    pub origin_z: f32,
    /// Zellen pro Seite.
    pub cells: usize,
    /// Zellweite in Metern.
    pub step: f32,
    /// Länge der Schürzen nach unten (m).
    pub skirt: f32,
}

impl FarTile {
    /// Mesh der Kachel. `exclude(x, z)` bekommt die Weltkoordinaten der **Zellmitte**; `true`
    /// spart die Zelle aus. Positionen sind relativ zu (`origin_x`, 0, `origin_z`).
    pub fn build(&self, world: &WorldGen, exclude: impl Fn(f32, f32) -> bool) -> ChunkMesh {
        let n = self.cells;
        let verts = n + 1;
        let step = self.step;
        // Höhen mit einem Rand von einer Zelle, für Normalen an der Kante.
        let side = verts + 2;
        let mut heights = vec![0.0f32; side * side];
        let mut colors = vec![[0u8; 3]; verts * verts];
        let mut materials = vec![MaterialId::AIR; verts * verts];
        for iz in 0..side {
            for ix in 0..side {
                let x = self.origin_x + (ix as f32 - 1.0) * step;
                let z = self.origin_z + (iz as f32 - 1.0) * step;
                let sample = world.sample(x, z);
                heights[iz * side + ix] = sample.height;
                let inner = (1..=verts).contains(&ix) && (1..=verts).contains(&iz);
                if inner {
                    let (vx, vz) = (ix - 1, iz - 1);
                    let biome = world.biome_at(&sample);
                    colors[vz * verts + vx] =
                        world.apply_altitude(world.surface_color(&sample, biome), sample.height);
                    // Oberflächenmaterial wie im Nahbereich (Boden fehlt an Steilhängen).
                    let east = world.column_height(x + 1.0, z);
                    let south = world.column_height(x, z + 1.0);
                    let slope = (east - sample.height).abs().max((south - sample.height).abs());
                    let geology = world.geology.column(x, z, &sample, biome, slope);
                    materials[vz * verts + vx] = material_at(&geology, 0.25, sample.height - 0.25);
                }
            }
        }
        let height = |vx: usize, vz: usize| heights[(vz + 1) * side + vx + 1];

        let included: Vec<bool> = (0..n * n)
            .map(|i| {
                let (cx, cz) = (i % n, i / n);
                let mid_x = self.origin_x + (cx as f32 + 0.5) * step;
                let mid_z = self.origin_z + (cz as f32 + 0.5) * step;
                !exclude(mid_x, mid_z)
            })
            .collect();
        let cell_in = |cx: isize, cz: isize| {
            cx >= 0 && cz >= 0 && (cx as usize) < n && (cz as usize) < n && included[cz as usize * n + cx as usize]
        };

        let mut mesh = ChunkMesh::default();
        for vz in 0..verts {
            for vx in 0..verts {
                // Zentrale Differenzen über den Rand hinweg (Rand-Höhen liegen in `heights`).
                let left = heights[(vz + 1) * side + vx];
                let right = heights[(vz + 1) * side + vx + 2];
                let down = heights[vz * side + vx + 1];
                let up = heights[(vz + 2) * side + vx + 1];
                let normal = normalize([left - right, 2.0 * step, down - up]);
                mesh.positions.push([vx as f32 * step, height(vx, vz), vz as f32 * step]);
                mesh.normals.push(normal);
                mesh.colors.push(colors[vz * verts + vx]);
                mesh.materials.push(materials[vz * verts + vx]);
            }
        }
        let index = |vx: usize, vz: usize| (vz * verts + vx) as u32;
        for cz in 0..n {
            for cx in 0..n {
                if !included[cz * n + cx] {
                    continue;
                }
                let (a, b, c, d) = (index(cx, cz), index(cx + 1, cz), index(cx, cz + 1), index(cx + 1, cz + 1));
                // Gegen den Uhrzeigersinn von oben gesehen (Normale +y), wie `surface_nets`.
                mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }

        // Schürzen an allen Kanten zur Aussparung bzw. zum Kachelrand.
        for cz in 0..n as isize {
            for cx in 0..n as isize {
                if !cell_in(cx, cz) {
                    continue;
                }
                let (ux, uz) = (cx as usize, cz as usize);
                let sides = [
                    (cx, cz - 1, (ux, uz), (ux + 1, uz)),
                    (cx, cz + 1, (ux, uz + 1), (ux + 1, uz + 1)),
                    (cx - 1, cz, (ux, uz), (ux, uz + 1)),
                    (cx + 1, cz, (ux + 1, uz), (ux + 1, uz + 1)),
                ];
                for (nx, nz, p, q) in sides {
                    if cell_in(nx, nz) {
                        continue;
                    }
                    self.add_skirt(&mut mesh, index(p.0, p.1), index(q.0, q.1));
                }
            }
        }
        mesh
    }

    /// Senkrechter Streifen unter der Kante `top_a`-`top_b`, beidseitig.
    fn add_skirt(&self, mesh: &mut ChunkMesh, top_a: u32, top_b: u32) {
        let base = mesh.positions.len() as u32;
        for top in [top_a, top_b] {
            let mut position = mesh.positions[top as usize];
            position[1] -= self.skirt;
            mesh.positions.push(position);
            mesh.normals.push(mesh.normals[top as usize]);
            mesh.colors.push(mesh.colors[top as usize]);
            mesh.materials.push(mesh.materials[top as usize]);
        }
        let (bottom_a, bottom_b) = (base, base + 1);
        mesh.indices.extend_from_slice(&[top_a, bottom_a, top_b, top_b, bottom_a, bottom_b]);
        mesh.indices.extend_from_slice(&[top_a, top_b, bottom_a, top_b, bottom_b, bottom_a]);
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / length, v[1] / length, v[2] / length]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: f32, z: f32) -> FarTile {
        FarTile { origin_x: x, origin_z: z, cells: 16, step: 4.0, skirt: 10.0 }
    }

    #[test]
    fn heights_match_the_world_and_triangles_face_up() {
        let world = WorldGen::new(42);
        let t = tile(320.0, -128.0);
        let mesh = t.build(&world, |_, _| false);
        let verts = t.cells + 1;
        for vz in 0..verts {
            for vx in 0..verts {
                let p = mesh.positions[vz * verts + vx];
                let expected = world.column_height(t.origin_x + p[0], t.origin_z + p[2]);
                assert_eq!(p[1], expected);
            }
        }
        // Erste n² * 6 Indizes sind Oberfläche: alle Dreiecke zeigen nach oben.
        for tri in mesh.indices[..t.cells * t.cells * 6].chunks(3) {
            let p = |i: u32| glam::Vec3::from(mesh.positions[i as usize]);
            let normal = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
            assert!(normal.y > 0.0, "Dreieck zeigt nach unten");
        }
        assert_eq!(mesh.materials.len(), mesh.positions.len());
        assert!(mesh.materials.iter().all(|m| !m.is_air()));
        assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
    }

    #[test]
    fn neighbouring_tiles_share_identical_edges() {
        let world = WorldGen::new(42);
        let (a, b) = (tile(0.0, 0.0), tile(64.0, 0.0));
        let (ma, mb) = (a.build(&world, |_, _| false), b.build(&world, |_, _| false));
        let verts = a.cells + 1;
        for vz in 0..verts {
            let pa = ma.positions[vz * verts + a.cells];
            let pb = mb.positions[vz * verts];
            assert_eq!(pa[1].to_bits(), pb[1].to_bits(), "Kante unterschiedlich bei Zeile {vz}");
            assert_eq!(ma.colors[vz * verts + a.cells], mb.colors[vz * verts]);
        }
    }

    #[test]
    fn excluded_cells_have_no_surface_and_holes_get_skirts() {
        let world = WorldGen::new(42);
        let t = tile(0.0, 0.0);
        let full = t.build(&world, |_, _| false);
        // Mittlerer Block von 8×8 Zellen ausgespart.
        let holed = t.build(&world, |x, z| (16.0..48.0).contains(&x) && (16.0..48.0).contains(&z));
        let surface = |m: &ChunkMesh| m.indices.len().min(t.cells * t.cells * 6);
        assert_eq!(surface(&full), 16 * 16 * 6);
        // Oberfläche: 256 - 64 Zellen; Schürzen: Außenrand 4·16 + Lochrand 4·8 Kanten, je 12 Indizes.
        let expected = (256 - 64) * 6 + (4 * 16 + 4 * 8) * 12;
        assert_eq!(holed.indices.len(), expected);
        assert_eq!(full.indices.len(), 256 * 6 + 4 * 16 * 12);
        // Keine Oberflächen-Dreiecke im Loch.
        for tri in holed.indices[..(256 - 64) * 6].chunks(3) {
            let centre = tri.iter().map(|&i| holed.positions[i as usize]).fold([0.0; 3], |s, p| [s[0] + p[0] / 3.0, 0.0, s[2] + p[2] / 3.0]);
            assert!(!((16.0..48.0).contains(&centre[0]) && (16.0..48.0).contains(&centre[2])), "Dreieck im Loch");
        }
    }

    #[test]
    fn fully_excluded_tile_is_empty() {
        let world = WorldGen::new(42);
        assert!(tile(0.0, 0.0).build(&world, |_, _| true).is_empty());
    }
}
