//! Sichtbarkeits-Filter (#46, D-005/D-013): Erze erst zeigen, wenn sie freiliegen.
//!
//! Bevor der Server einen Chunk an einen Client schickt, ersetzt er Erz- und Spezialmaterial in
//! Voxeln ohne Luft-Nachbarn (26er-Nachbarschaft, also auch über Kanten und Ecken) durch das **öffentliche** Gestein an dieser
//! Stelle – genau das, was der Client selbst erzeugen würde. Reinheit wird ohnehin nicht im
//! Chunk übertragen. Freiliegende Erze (an Höhlenluft, Gegrabenem, der Oberfläche) bleiben.
//!
//! Warum 26 Nachbarn: Surface Nets färbt einen Vertex mit dem Material der festesten Ecke seiner
//! Zelle, und diese Ecke berührt die Luft oft nur über eine Kante oder Ecke. Genau diese Voxel
//! kann das Mesh zeigen, mehr wird nicht übertragen (Review-Fund: mit 6 Nachbarn fehlte fast die
//! Hälfte der sichtbaren Erzflächen).
//!
//! "Freiliegend" wird pro Welt-Voxel entschieden: Für Nachbarn außerhalb des Sample-Bereichs fragt
//! der Filter `air_at` (Daten der Nachbar-Chunks). So liefern benachbarte Chunks an gemeinsamen
//! Samples dasselbe Ergebnis.

use glam::IVec3;

use crate::geology::material_at;
use crate::volume::{CHUNK_SIZE, ColumnData, DensityChunk, SAMPLE_MIN};

use super::ore_rules::OreKind;

/// Versätze zu allen 26 Nachbarn eines Voxels.
pub(crate) const NEIGHBOURS_26: [(i32, i32, i32); 26] = {
    let mut out = [(0, 0, 0); 26];
    let mut i = 0;
    let mut n = 0;
    while n < 27 {
        let (dx, dy, dz) = (n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1);
        if dx != 0 || dy != 0 || dz != 0 {
            out[i] = (dx, dy, dz);
            i += 1;
        }
        n += 1;
    }
    out
};

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
                let exposed = NEIGHBOURS_26.iter().any(|&(dx, dy, dz)| {
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
