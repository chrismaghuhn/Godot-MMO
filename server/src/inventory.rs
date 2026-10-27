//! Inventar auf dem Server (#18, D-022): Ausbeute beim Graben, Verbrauch beim Bauen
//! (`inventory::Holdings` im Core). Natürlich gewachsenes Material bewertet der Server mit dem
//! geheimen Untergrund (`Underground::mined_sample`, Reinheit und Spurenelemente bei Erzen).

use std::sync::PoisonError;

use glam::{IVec3, Vec3Swizzles};
use terraforge_core::MaterialId;
use terraforge_core::edits::owner_chunk;
use terraforge_protocol::StackInfo;
use terraforge_protocol::inventory::view;

use crate::ServerWorld;

impl ServerWorld {
    /// Der Inventar-Stand für den Client.
    pub fn inventory_view(&self) -> Vec<StackInfo> {
        view(&self.holdings.lock().unwrap_or_else(PoisonError::into_inner).inventory)
    }

    pub(crate) fn build_material(&self, slot: usize) -> Option<(MaterialId, u32)> {
        self.holdings.lock().unwrap_or_else(PoisonError::into_inner).build_material(slot)
    }

    /// Legt die abgebauten Voxel ins Inventar. Gibt zurück, wie viel keinen Platz fand.
    pub(crate) fn store_yield(&self, center: IVec3, removed: &[(IVec3, MaterialId)]) -> u32 {
        let mut holdings = self.holdings.lock().unwrap_or_else(PoisonError::into_inner);
        holdings.store_yield(center, removed, |voxel, material| {
            let column = self.column(owner_chunk(voxel).xz());
            self.underground.mined_sample(&self.world, &column, voxel, material)
        })
    }

    pub(crate) fn consume(&self, slot: usize, placed_at: &[IVec3]) {
        self.holdings.lock().unwrap_or_else(PoisonError::into_inner).consume(slot, placed_at);
    }
}
