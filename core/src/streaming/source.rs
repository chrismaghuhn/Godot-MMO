//! Woher das Streaming die Dichtefelder bekommt (D-005, D-021).
//!
//! Die Oberfläche erzeugt jeder Client selbst. Höhlen und Erze kennt nur, wer den geheimen
//! Untergrund hat: der Viewer (Debug-Tool, Feature `underground`) oder später der Server, von dem
//! der Godot-Client sie übers Netz bekommt (#56). Ohne das Feature kennt das Streaming nur den Trait.

use crate::WorldGen;
use crate::volume::{ChunkKind, ColumnData, DensityChunk};

/// Quelle für die Dichtefelder der Chunks. Wird aus den Worker-Threads aufgerufen.
pub trait UndergroundSource: Send + Sync {
    /// Dichtefeld eines Chunks, **wenn er ein Mesh braucht**; `None` für reine Luft und für
    /// massiven Fels ohne Hohlraum.
    fn density_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk>;

    /// Vollständiges Dichtefeld mit Material, auch für massiven Fels (Graben, Bäume aufsetzen).
    fn materials_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk;

    /// Unterster Chunk (y), in dem es Höhlen geben kann; `None`, wenn die Quelle keine Höhlen
    /// kennt. Massive Chunks werden dann nur geladen, wenn sie verändert wurden.
    fn lowest_cave_chunk(&self) -> Option<i32>;
}

/// Nur die öffentliche Oberfläche: keine Höhlen, kein Erz.
pub struct PublicSurface;

impl UndergroundSource for PublicSurface {
    fn density_chunk(&self, _world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        (column.classify(chunk_y) == ChunkKind::Surface).then(|| DensityChunk::from_column(column, chunk_y))
    }

    fn materials_chunk(&self, _world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk {
        DensityChunk::from_column(column, chunk_y)
    }

    fn lowest_cave_chunk(&self) -> Option<i32> {
        None
    }
}

#[cfg(feature = "underground")]
impl UndergroundSource for crate::underground::Underground {
    fn density_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        crate::underground::Underground::density_chunk(self, world, column, chunk_y)
    }

    fn materials_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk {
        crate::underground::Underground::materials_chunk(self, world, column, chunk_y)
    }

    fn lowest_cave_chunk(&self) -> Option<i32> {
        Some((crate::underground::CAVE_MIN_Y / crate::volume::CHUNK_SIZE as f32).floor() as i32)
    }
}
