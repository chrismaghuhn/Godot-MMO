//! terraforge-core: deterministische, engine-unabhängige Weltgenerierung.
//!
//! Öffentlich (darf jeder Client erzeugen): `world`, `biome`, `geology`, `material`, `properties`, `combat`,
//! `inventory`, `noise`, `regions`, `vegetation`, `volume`, `mesh`, `lod`, `edits`, `occlusion`, `streaming`,
//! `spawn`, `movement`. Geheim (nur mit Feature `underground`, D-005): `underground`.

pub mod biome;
pub mod combat;
pub mod edits;
pub mod geology;
pub mod inventory;
pub mod lod;
pub mod material;
pub mod mesh;
pub mod movement;
pub mod noise;
pub mod occlusion;
pub mod properties;
pub mod regions;
pub mod spawn;
pub mod streaming;
#[cfg(feature = "underground")]
pub mod underground;
pub mod vegetation;
pub mod volume;
pub mod world;

/// Ob der geheime Untergrund mitkompiliert ist (Feature `underground`). Der Spiel-Client prüft
/// damit, dass ihn keine Feature-Vereinigung von Cargo hineingezogen hat (D-005).
pub const UNDERGROUND_COMPILED: bool = cfg!(feature = "underground");

pub use biome::{FamilyKind, VariantTag};
pub use material::{MaterialCategory, MaterialId, MaterialRegistry};
pub use world::{BiomeRef, ColumnSample, WorldConfig, WorldGen, smoothstep};
