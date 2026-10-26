//! Geheimer Untergrund: alles unter der Oberfläche, was ein Client nicht vorab kennen darf (D-005).
//!
//! Dieses Modul gibt es nur mit dem Cargo-Feature `underground`. Der Spiel-Client baut ohne
//! das Feature und kann den Untergrund deshalb nicht selbst erzeugen; er bekommt
//! Untergrund-Chunks später vom Server (#26).
//!
//! Aufbau:
//! - `seed`:      der geheime Seed
//! - `grid`:      grobes 4-m-Gitter für teure 3D-Noise
//! - `caves`:     Höhlen und Schutz-Schale (#4)
//! - `provinces`: geologische Provinzen mit ihren Erzarten (#45)
//! - `ore_rules`: Regeln je Erz (Form, Wirtsgestein, Tiefe)
//! - `ores`:      Platzierung der Erze (#45)
//!
//! Regel: Der öffentliche Teil (`world`, `volume`, …) importiert nie etwas aus diesem Modul.
//! Der Compiler erzwingt das, weil der öffentliche Teil auch ohne das Feature bauen muss.

mod caves;
mod grid;
mod ore_rules;
mod ores;
mod provinces;
mod seed;

pub use caves::{CAVE_MIN_Y, SURFACE_SHELL};
pub use ore_rules::{Form, OreKind, OreRule};
pub use provinces::{Deposit, MAX_ORES, PROVINCE_SIZE, Province, ProvinceId};
pub use seed::SecretSeed;

use caves::{CaveGrid, CaveNoise, ShellHeights};
use ores::OreNoise;
use provinces::ProvinceMap;

use crate::material::MaterialId;
use crate::volume::{ChunkKind, ColumnData, DensityChunk};
use crate::world::{WorldConfig, WorldGen};

/// Erzeugt den Untergrund auf Basis des öffentlichen Dichtefelds.
pub struct Underground {
    seed: SecretSeed,
    sea_level: f32,
    caves: CaveNoise,
    provinces: ProvinceMap,
    ores: OreNoise,
}

impl Underground {
    pub fn new(seed: SecretSeed, config: &WorldConfig) -> Self {
        Self {
            seed,
            sea_level: config.sea_level,
            caves: CaveNoise::new(&seed),
            provinces: ProvinceMap::new(&seed),
            ores: OreNoise::new(&seed),
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

    /// Dichtefeld eines Chunks mit Höhlen und Erzen, **wenn er ein Mesh braucht**.
    /// `None` für reine Luft und für massiven Fels ohne Hohlraum (dessen Material liefert
    /// `materials_chunk`).
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
        let (chunk, carved_air) = self.build(world, column, chunk_y, &grid);
        (surface_kind == ChunkKind::Surface || carved_air).then_some(chunk)
    }

    /// Vollständiges Dichtefeld und Material eines Chunks, auch für massiven Fels
    /// (z. B. zum Graben, #6). Stimmt mit `density_chunk` überein, wo beide etwas liefern.
    pub fn materials_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk {
        if column.classify(chunk_y) == ChunkKind::Air || !CaveGrid::in_cave_range(chunk_y) {
            return DensityChunk::from_column(column, chunk_y);
        }
        let grid = CaveGrid::build(&self.caves, column.chunk_x, chunk_y, column.chunk_z);
        self.build(world, column, chunk_y, &grid).0
    }

    /// Provinz an einer Position (für Server, Werkzeuge und Tests).
    pub fn province_at(&self, world: &WorldGen, x: f32, z: f32) -> Province {
        self.provinces.province(world, self.provinces.province_at(x, z))
    }

    /// Reinheit (0,05..1) eines Erzes an x/z; `None`, wenn die Provinz dieses Erz nicht hat.
    pub fn purity(&self, world: &WorldGen, x: f32, z: f32, material: MaterialId) -> Option<f32> {
        let kind = OreKind::from_material(material)?;
        let province = self.province_at(world, x, z);
        let deposit = province.deposit(kind)?;
        Some(self.provinces.purity(deposit, x, z))
    }

    /// Öffentliches Feld → Höhlen → Erze. Gibt zurück, ob Höhlen Luft geschaffen haben.
    fn build(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32, grid: &CaveGrid) -> (DensityChunk, bool) {
        let shell = ShellHeights::cached(world, column);
        let mut chunk = DensityChunk::from_column(column, chunk_y);
        let carved_air = caves::carve(&self.caves, grid, column, &shell, &mut chunk, self.sea_level);
        ores::apply_ores(&self.ores, &self.provinces, world, column, &shell, &mut chunk);
        (chunk, carved_air)
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
mod ore_tests;
#[cfg(test)]
mod tests;
