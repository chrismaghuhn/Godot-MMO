//! Durch Graben/Bauen veränderte Chunks, die neu gebaut werden müssen (#6).
//!
//! Massive Chunks baut das Höhlen-Streaming neu, alle anderen die Spalte. Ist die Spalte nicht
//! geladen, geht die Markierung an die Spalten und verfällt dort (beim Laden wird die Änderung
//! ohnehin angewendet).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{IVec2, IVec3, Vec3Swizzles};

use crate::volume::{ChunkKind, ColumnData};

#[derive(Default)]
pub(super) struct DirtyChunks {
    pub(super) columns: HashSet<IVec2>,
    pub(super) caves: HashSet<IVec3>,
}

impl DirtyChunks {
    pub(super) fn mark(&mut self, chunks: impl IntoIterator<Item = IVec3>, cache: &HashMap<IVec2, Arc<ColumnData>>) {
        for chunk in chunks {
            let solid = cache.get(&chunk.xz()).is_some_and(|column| column.classify(chunk.y) == ChunkKind::Solid);
            if solid {
                self.caves.insert(chunk);
            } else {
                self.columns.insert(chunk.xz());
            }
        }
    }
}
