//! Sichtbarkeits-Filter (#46, D-005/D-013): Erze erst zeigen, wenn sie freiliegen.
//!
//! Bevor der Server einen Chunk an einen Client schickt, ersetzt er Erz- und Spezialmaterial in
//! Voxeln ohne Luft-Nachbarn (6er-Nachbarschaft) durch das **öffentliche** Gestein an dieser
//! Stelle – genau das, was der Client selbst erzeugen würde. Reinheit wird ohnehin nicht im
//! Chunk übertragen. Freiliegende Erze (an Höhlenluft, Gegrabenem, der Oberfläche) bleiben.
//!
//! "Freiliegend" wird pro Welt-Voxel entschieden: Für Nachbarn außerhalb des Sample-Bereichs fragt
//! der Filter `air_at` (Daten der Nachbar-Chunks). So liefern benachbarte Chunks an gemeinsamen
//! Samples dasselbe Ergebnis.

use glam::IVec3;

use crate::geology::material_at;
use crate::volume::{CHUNK_SIZE, ColumnData, DensityChunk, SAMPLE_MIN};

use super::ore_rules::OreKind;

/// Ersetzt verborgene Erze im Chunk durch öffentliches Gestein. Gibt die Zahl ersetzter Voxel
/// zurück. `air_at(welt_voxel)` muss für Voxel außerhalb des Sample-Bereichs die aktuelle
/// Belegung liefern (inklusive Höhlen und Spieler-Änderungen).
pub fn hide_unexposed_ores(chunk: &mut DensityChunk, column: &ColumnData, mut air_at: impl FnMut(IVec3) -> bool) -> usize {
    let origin = chunk.origin();
    let in_range = |v: i32| (SAMPLE_MIN..=CHUNK_SIZE).contains(&v);
    let mut hidden = Vec::new();
    for z in SAMPLE_MIN..=CHUNK_SIZE {
        for y in SAMPLE_MIN..=CHUNK_SIZE {
            for x in SAMPLE_MIN..=CHUNK_SIZE {
                if OreKind::from_material(chunk.material(x, y, z)).is_none() {
                    continue;
                }
                let exposed = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)].iter().any(|&(dx, dy, dz)| {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    if in_range(nx) && in_range(ny) && in_range(nz) {
                        chunk.get(nx, ny, nz) <= 0.0
                    } else {
                        air_at(origin + IVec3::new(nx, ny, nz))
                    }
                });
                if !exposed {
                    hidden.push((x, y, z));
                }
            }
        }
    }
    for &(x, y, z) in &hidden {
        let world_y = (origin.y + y) as f32;
        let depth = column.height(x, z) - world_y;
        let public = material_at(column.geology(x, z), depth, world_y);
        chunk.set_material(x, y, z, public);
    }
    hidden.len()
}
