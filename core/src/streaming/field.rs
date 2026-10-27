//! Dichtefeld nahe der Kamera für Kollision und Bewegung (#13).
//!
//! Gehalten werden die Chunks im Umkreis `FIELD_RADIUS` um die Kamera (in der Ich-Perspektive ist
//! das der Spieler). Die Jobs für Meshes bauen die Dichten ohnehin; was fehlt (weil der Chunk am
//! Rand der Sicht gebaut wurde und die Kamera erst später kam, Review-Fund), holen eigene Jobs
//! nach (`JobKind::Field`). Für alles andere gilt die öffentliche Höhe der Spalte: Unbekannte
//! Höhlen sind dann massiv, Spieler-Änderungen fehlen. Das betrifft nur Stellen weiter weg und
//! die ersten Augenblicke, bis ein Chunk da ist.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{IVec2, IVec3, Vec3Swizzles};

use super::jobs::{JobKind, JobPool, Ticket};
use super::wanted::distance;
use super::{Built, Context};
use crate::WorldGen;
use crate::volume::{CHUNK_SIZE, ChunkKind, ColumnData, DensityChunk};

/// Chunks (Chebyshev) um den Kamera-Chunk mit Dichtefeld: gewünscht 5×5×5 (≈ 30 MB), gehalten
/// bis Abstand 3 (Hysterese, höchstens 7×7×7 ≈ 80 MB).
pub(super) const FIELD_RADIUS: i32 = 2;
/// Laufende Jobs für fehlende Chunks.
const FIELD_JOBS: usize = 4;

pub(super) struct FieldBuild {
    position: IVec3,
    chunk: Arc<DensityChunk>,
}

/// Dichtefeld nahe der Kamera.
///
/// **Edit-Version (#14):** Jeder Job merkt sich beim Start `epoch`. Wird ein Chunk verändert, zählt
/// `epoch` hoch und der Chunk merkt sich den neuen Stand (`changed`). Ergebnisse von Jobs, die vor
/// der letzten Änderung gestartet sind, verwirft das Feld: Sonst könnte ein älterer Neubau einer
/// Spalte, der erst nach dem neuen Stand fertig wird, ihn wieder überschreiben (Review-Fund #13).
/// `changed` wird nicht aufgeräumt: ein Eintrag pro je veränderten Chunk.
#[derive(Default)]
pub(super) struct DensityField {
    chunks: HashMap<IVec3, Arc<DensityChunk>>,
    pending: HashMap<IVec3, Ticket>,
    /// Veränderte Chunks: Der alte Stand gilt weiter, bis der neue da ist.
    stale: HashSet<IVec3>,
    epoch: u64,
    /// Stand (`epoch`) der letzten Änderung je Chunk.
    changed: HashMap<IVec3, u64>,
}

impl DensityField {
    /// Aktueller Stand der Änderungen; Jobs merken ihn sich beim Start.
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Ist ein Ergebnis, dessen Job bei `epoch` startete, älter als die letzte Änderung des Chunks?
    fn outdated(&self, position: IVec3, epoch: u64) -> bool {
        self.changed.get(&position).is_some_and(|&changed| epoch < changed)
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn accept(&mut self, id: u64, build: FieldBuild) {
        if self.pending.get(&build.position).is_some_and(|ticket| ticket.id == id) {
            self.pending.remove(&build.position);
            self.stale.remove(&build.position);
            self.chunks.insert(build.position, build.chunk);
        }
    }

    pub(super) fn forget(&mut self, id: u64) {
        self.pending.retain(|_, ticket| ticket.id != id);
    }

    /// Veränderte Chunks (Graben/Bauen) neu holen; laufende Jobs dafür sind veraltet. Der alte Stand
    /// bleibt bis dahin: Ohne ihn gälte die öffentliche Höhe, Gegrabenes wäre kurz massiv und der
    /// Körper würde herausgeschoben (Review-Fund: 8,7 m aus einer Grube).
    pub(super) fn invalidate(&mut self, chunk: IVec3) {
        self.epoch += 1;
        self.changed.insert(chunk, self.epoch);
        self.pending.remove(&chunk);
        if self.chunks.contains_key(&chunk) {
            self.stale.insert(chunk);
        }
    }

    /// Fehlende Chunks nahe der Kamera nachholen (nächste zuerst). Reine Luft braucht nichts
    /// (die öffentliche Höhe stimmt dort), außer sie wurde verändert (Gebautes).
    pub(super) fn update<M: Send + 'static>(&mut self, ctx: &Context<M>, pool: &mut JobPool<Built<M>>, columns: &HashMap<IVec2, Arc<ColumnData>>, camera: IVec3) {
        self.prune(camera);
        self.pending.retain(|&position, _| distance(position, camera) <= FIELD_RADIUS + 1);
        let slots = FIELD_JOBS.saturating_sub(self.pending.len());
        if slots == 0 {
            return;
        }
        let mut missing = Vec::new();
        {
            let edits = ctx.edits.read().expect("Edits-Lock vergiftet");
            for dz in -FIELD_RADIUS..=FIELD_RADIUS {
                for dy in -FIELD_RADIUS..=FIELD_RADIUS {
                    for dx in -FIELD_RADIUS..=FIELD_RADIUS {
                        let position = camera + IVec3::new(dx, dy, dz);
                        let current = self.chunks.contains_key(&position) && !self.stale.contains(&position);
                        if current || self.pending.contains_key(&position) {
                            continue;
                        }
                        let Some(column) = columns.get(&position.xz()) else { continue };
                        if column.classify(position.y) == ChunkKind::Air && !edits.touches(position) {
                            continue;
                        }
                        missing.push((position, Arc::clone(column)));
                    }
                }
            }
        }
        missing.sort_by_key(|(position, _)| distance(*position, camera));
        for (position, column) in missing.into_iter().take(slots) {
            let ctx = ctx.clone();
            let ticket = pool.spawn(JobKind::Field, move || {
                let mut chunk = ctx.source.materials_chunk(&ctx.world, &column, position.y);
                let edits = ctx.edits.read().expect("Edits-Lock vergiftet");
                if edits.touches(position) {
                    edits.apply(&mut chunk);
                }
                Built::Field(FieldBuild { position, chunk: Arc::new(chunk) })
            });
            self.pending.insert(position, ticket);
        }
    }

    /// Übernimmt einen fertigen Chunk eines Jobs, der bei `epoch` startete, wenn er nahe genug an
    /// der Kamera liegt und nicht älter als die letzte Änderung ist.
    pub(super) fn insert(&mut self, chunk: Arc<DensityChunk>, epoch: u64, camera: IVec3) {
        if self.outdated(chunk.position, epoch) {
            return;
        }
        self.stale.remove(&chunk.position);
        if distance(chunk.position, camera) <= FIELD_RADIUS {
            self.chunks.insert(chunk.position, chunk);
        } else {
            self.chunks.remove(&chunk.position);
        }
    }

    /// Alle Chunks einer Spalte vergessen (Neubau oder Entfernen der Spalte).
    pub(super) fn remove_column(&mut self, column: IVec2) {
        self.chunks.retain(|position, _| position.xz() != column);
        self.stale.retain(|position| position.xz() != column);
    }

    /// Ein Job (gestartet bei `epoch`) meldet: kein Dichtefeld nötig (massiv ohne Hohlraum).
    pub(super) fn remove_built(&mut self, chunk: IVec3, epoch: u64) {
        if !self.outdated(chunk, epoch) {
            self.remove(chunk);
        }
    }

    pub(super) fn remove(&mut self, chunk: IVec3) {
        self.chunks.remove(&chunk);
        self.stale.remove(&chunk);
    }

    /// Chunks, die zu weit von der Kamera weg sind, freigeben.
    pub(super) fn prune(&mut self, camera: IVec3) {
        self.chunks.retain(|&position, _| distance(position, camera) <= FIELD_RADIUS + 1);
        let chunks = &self.chunks;
        self.stale.retain(|position| chunks.contains_key(position));
    }

    pub(super) fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Dichte am Welt-Voxel (> 0 fest, <= 0 Luft).
    pub(super) fn density(&self, voxel: IVec3, columns: &HashMap<IVec2, Arc<ColumnData>>, world: &WorldGen) -> f32 {
        let owner = voxel.div_euclid(IVec3::splat(CHUNK_SIZE));
        let local = voxel - owner * CHUNK_SIZE;
        if let Some(chunk) = self.chunks.get(&owner) {
            return chunk.get(local.x, local.y, local.z);
        }
        match columns.get(&owner.xz()) {
            Some(column) => column.height(local.x, local.z) - voxel.y as f32,
            None => world.column_height(voxel.x as f32, voxel.z as f32) - voxel.y as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein älterer Neubau, der nach dem neuen Stand fertig wird, überschreibt ihn nicht (#14).
    #[test]
    fn older_builds_do_not_overwrite_a_newer_state() {
        let world = WorldGen::new(42);
        let column = ColumnData::sample(&world, 0, 0);
        let chunk_y = column.surface_chunks().next().expect("Oberflächen-Chunk");
        let position = IVec3::new(0, chunk_y, 0);
        let build = || Arc::new(DensityChunk::from_column(&column, chunk_y));
        let mut field = DensityField::default();
        let before = field.epoch();
        field.insert(build(), before, position);
        field.invalidate(position);
        let after = field.epoch();
        let new = build();
        field.insert(Arc::clone(&new), after, position);
        // Gestartet vor der Änderung, fertig erst jetzt.
        field.insert(build(), before, position);
        field.remove_built(position, before);
        assert!(Arc::ptr_eq(&field.chunks[&position], &new));
    }
}
