//! terraforge-core: deterministische, engine-unabhängige Weltgenerierung.
//!
//! Öffentlich (darf jeder Client erzeugen): `world`, `biome`, `geology`, `material`, `noise`,
//! `regions`, `vegetation`, `volume`, `mesh`, `lod`, `edits`, `occlusion`. Geheim (nur mit Feature `underground`, D-005): `underground`.

pub mod biome;
pub mod edits;
pub mod geology;
pub mod lod;
pub mod material;
pub mod mesh;
pub mod noise;
pub mod occlusion;
pub mod regions;
#[cfg(feature = "underground")]
pub mod underground;
pub mod vegetation;
pub mod volume;
pub mod world;

pub use biome::{FamilyKind, VariantTag};
pub use material::{MaterialCategory, MaterialId, MaterialRegistry};
pub use world::{BiomeRef, ColumnSample, WorldConfig, WorldGen, smoothstep};
