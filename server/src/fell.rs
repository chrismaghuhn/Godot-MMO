//! Bäume fällen (#18): Der Server sucht den anvisierten Baum mit denselben Regeln wie das
//! Streaming des Clients (`vegetation::targeted_tree`, freier Kreis um den Spawn), fällt ihn und
//! legt die Ausbeute ins Inventar. Holz wird wie jedes Material nach Ort bewertet.

use std::sync::PoisonError;

use glam::{IVec2, IVec3, Vec3};
use terraforge_core::inventory::Item;
use terraforge_core::vegetation::{felling_yield, targeted_tree};
use terraforge_protocol::EditOutcome;

use crate::ServerWorld;
use crate::edit::refused;

impl ServerWorld {
    /// Fällt den vordersten Baum im Strahl vor dem Gelände (unter `edit_lock`).
    pub(crate) fn fell(&self, eye: Vec3, direction: Vec3) -> EditOutcome {
        // `voxel` und `felled` nehmen das Edits-Lock jeweils kurz selbst (kein verschachteltes Lesen).
        let felled = |id| self.edits.read().unwrap_or_else(PoisonError::into_inner).is_felled(id);
        let target = targeted_tree(&self.world, |position| self.column(position), Some(self.clearing), |voxel| self.voxel(voxel).density, felled, eye, direction);
        let Some((base, tree)) = target else {
            return refused("kein Baum im Visier");
        };
        let (material, count) = felling_yield(&tree);
        let item = Item::from_sample(self.world.mined_sample(base.round().as_ivec3(), material));
        let mut holdings = self.holdings.lock().unwrap_or_else(PoisonError::into_inner);
        // Sonst wäre der Baum weg und alles Holz verloren (Review-Fund).
        if holdings.inventory.room_for(&item) == 0 {
            return refused("Inventar voll");
        }
        let lost = holdings.inventory.add(&item, count);
        drop(holdings);
        self.edits.write().unwrap_or_else(PoisonError::into_inner).fell_tree(tree.id);
        // Neu bauen muss die Spalte: einer ihrer Oberflächen-Chunks (der Fuß des Baums kann in einem
        // Chunk liegen, den die Spalte als massiv einstuft, dann ginge die Markierung an die Höhlen).
        let column = self.column(IVec2::new(tree.id.chunk_x, tree.id.chunk_z));
        let chunk_y = column.surface_chunks().next().unwrap_or(0);
        let chunk = IVec3::new(tree.id.chunk_x, chunk_y, tree.id.chunk_z);
        EditOutcome::Felled { tree: (tree.id.chunk_x, tree.id.chunk_z, tree.id.cell), material: material.0, count, lost, chunk: chunk.to_array() }
    }

    /// Alle gefällten Bäume für einen neu verbundenen Client.
    pub fn felled_trees(&self) -> Vec<(i32, i32, u16)> {
        self.edits.read().unwrap_or_else(PoisonError::into_inner).felled_trees().map(|id| (id.chunk_x, id.chunk_z, id.cell)).collect()
    }
}
