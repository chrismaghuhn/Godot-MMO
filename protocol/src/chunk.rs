//! Chunks als Unterschied zur öffentlichen Oberfläche (#56).
//!
//! Der Client kann jeden Chunk ohne Untergrund selbst erzeugen (`DensityChunk::from_column`). Der
//! Server schickt nur die Läufe von Samples, die davon abweichen (Höhlen, freiliegende Erze, und
//! das Carving, das auch tief im Fels Dichten absenkt). Gemessen um den Spawn (Seed 42): ≈ 16 %
//! der Dichten und 2 % der Materialien weichen ab, ein Viertel der Oberflächen-Chunks gar nicht.

use serde::{Deserialize, Serialize};
use terraforge_core::MaterialId;
use terraforge_core::volume::DensityChunk;

/// Aufeinanderfolgende abweichende Samples ab Index `start` (Reihenfolge wie `DensityChunk::samples`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub start: u32,
    pub values: Vec<f32>,
    pub materials: Vec<u16>,
}

/// Läufe, in denen `chunk` von `base` abweicht (bitgenau). Leer, wenn beide gleich sind.
pub fn diff(base: &DensityChunk, chunk: &DensityChunk) -> Vec<Run> {
    let ((base_values, base_materials), (values, materials)) = (base.samples(), chunk.samples());
    let differs = |i: usize| values[i].to_bits() != base_values[i].to_bits() || materials[i] != base_materials[i];
    let mut runs = Vec::new();
    let mut i = 0;
    while i < values.len() {
        if !differs(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < values.len() && differs(i) {
            i += 1;
        }
        runs.push(Run { start: start as u32, values: values[start..i].to_vec(), materials: materials[start..i].iter().map(|m| m.0).collect() });
    }
    runs
}

/// `base` mit den Läufen überschrieben; `None` bei ungültigen Daten (Läufe außerhalb, Invariante).
pub fn apply(base: DensityChunk, runs: Vec<Run>) -> Option<DensityChunk> {
    let position = base.position;
    let (values, materials) = base.samples();
    let (mut values, mut materials) = (values.to_vec(), materials.to_vec());
    for run in runs {
        let start = run.start as usize;
        let end = start.checked_add(run.values.len())?;
        if run.materials.len() != run.values.len() || end > values.len() {
            return None;
        }
        values[start..end].copy_from_slice(&run.values);
        for (target, material) in materials[start..end].iter_mut().zip(run.materials) {
            *target = MaterialId(material);
        }
    }
    DensityChunk::from_samples(position, values, materials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use terraforge_core::WorldGen;
    use terraforge_core::volume::ColumnData;

    #[test]
    fn diff_and_apply_rebuild_the_chunk() {
        let world = WorldGen::new(42);
        let column = ColumnData::sample(&world, 0, 0);
        let chunk_y = column.surface_chunks().next().unwrap();
        let base = DensityChunk::from_column(&column, chunk_y);
        assert!(diff(&base, &base).is_empty());

        // Ein "Hohlraum": ein paar Samples zu Luft, verteilt auf zwei Läufe.
        let (values, materials) = base.samples();
        let (mut values, mut materials) = (values.to_vec(), materials.to_vec());
        for i in (100..110).chain(5000..5003) {
            values[i] = -1.5;
            materials[i] = MaterialId::AIR;
        }
        let changed = DensityChunk::from_samples(base.position, values, materials).unwrap();
        let runs = diff(&base, &changed);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs.iter().map(|r| r.values.len()).sum::<usize>(), 13);
        assert!(apply(base.clone(), runs).unwrap().same_as(&changed));
    }

    #[test]
    fn invalid_runs_are_rejected() {
        let world = WorldGen::new(42);
        let base = DensityChunk::from_column(&ColumnData::sample(&world, 0, 0), 0);
        let outside = Run { start: u32::MAX - 1, values: vec![1.0; 4], materials: vec![1; 4] };
        assert!(apply(base.clone(), vec![outside]).is_none());
        // Feste Dichte mit Luft-Material verletzt die Invariante.
        let broken = Run { start: 0, values: vec![5.0], materials: vec![MaterialId::AIR.0] };
        assert!(apply(base, vec![broken]).is_none());
    }
}
