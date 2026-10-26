//! Geheimer Untergrund: alles unter der Oberfläche, was ein Client nicht vorab kennen darf (D-005).
//!
//! Dieses Modul gibt es nur mit dem Cargo-Feature `underground`. Der Spiel-Client baut ohne
//! das Feature und kann den Untergrund deshalb nicht selbst erzeugen; er bekommt
//! Untergrund-Chunks später vom Server (#26).
//!
//! Aufbau (geplant):
//! - `seed`:  der geheime Seed
//! - `caves`: Höhlen (#4)
//! - `ores`:  Gestein und Erze (#5)
//!
//! Regel: Der öffentliche Teil (`world`, `volume`, …) importiert nie etwas aus diesem Modul.
//! Der Compiler erzwingt das, weil der öffentliche Teil auch ohne das Feature bauen muss.

mod caves;
mod grid;
mod seed;

pub use caves::{CAVE_MIN_Y, SURFACE_SHELL};
pub use seed::SecretSeed;

use caves::{CaveGrid, CaveNoise, ShellHeights};

use crate::volume::{ChunkKind, ColumnData, DensityChunk};
use crate::world::{WorldConfig, WorldGen};

/// Erzeugt den Untergrund auf Basis des öffentlichen Dichtefelds.
pub struct Underground {
    seed: SecretSeed,
    sea_level: f32,
    caves: CaveNoise,
}

impl Underground {
    pub fn new(seed: SecretSeed, config: &WorldConfig) -> Self {
        Self {
            seed,
            sea_level: config.sea_level,
            caves: CaveNoise::new(&seed),
        }
    }

    pub fn seed(&self) -> SecretSeed {
        self.seed
    }

    /// Was der Chunk nach Anwendung des Untergrunds enthält.
    ///
    /// Konservativ: `Solid` heißt garantiert ohne Hohlraum. `Surface` kann bei einem
    /// Chunk unter der Oberfläche trotzdem ohne Höhle sein (dann liefert
    /// `density_chunk` `None`), weil nur das grobe Gitter geprüft wird.
    pub fn classify(&self, column: &ColumnData, chunk_y: i32) -> ChunkKind {
        match column.classify(chunk_y) {
            ChunkKind::Solid if self.cave_grid(column, chunk_y).is_some() => ChunkKind::Surface,
            kind => kind,
        }
    }

    /// Vollständiges Dichtefeld eines Chunks: Oberfläche plus Höhlen.
    /// `None`, wenn der Chunk kein Mesh braucht (nur Luft oder nur Fels).
    ///
    /// Oberflächen-Chunks im Höhlenbereich werden immer bearbeitet, auch wenn das Gitter
    /// keine Höhle meldet: Das Carving senkt auch positive Werte ab, und ein bearbeiteter
    /// Nachbar muss an gemeinsamen Samples dieselben Werte sehen.
    pub fn density_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        let surface_kind = column.classify(chunk_y);
        if surface_kind == ChunkKind::Air {
            return None;
        }
        if !CaveGrid::in_cave_range(chunk_y) {
            return (surface_kind == ChunkKind::Surface)
                .then(|| DensityChunk::from_column(column, chunk_y));
        }
        let grid = CaveGrid::build(&self.caves, column.chunk_x, chunk_y, column.chunk_z);
        if surface_kind == ChunkKind::Solid && !grid.may_have_caves() {
            return None;
        }
        let shell = ShellHeights::new(world, column);
        let mut chunk = DensityChunk::from_column(column, chunk_y);
        let carved_air = caves::carve(&self.caves, &grid, column, &shell, &mut chunk, self.sea_level);
        (surface_kind == ChunkKind::Surface || carved_air).then_some(chunk)
    }

    /// Höhlen-Gitter, falls der Chunk Höhlen enthalten kann.
    fn cave_grid(&self, column: &ColumnData, chunk_y: i32) -> Option<CaveGrid> {
        if !CaveGrid::in_cave_range(chunk_y) {
            return None;
        }
        let grid = CaveGrid::build(&self.caves, column.chunk_x, chunk_y, column.chunk_z);
        grid.may_have_caves().then_some(grid)
    }
}

#[cfg(test)]
mod tests;
