//! terraforge-core: deterministische, engine-unabhängige Weltgenerierung.
//!
//! Öffentlich (darf jeder Client erzeugen): `world`, `biome`, `noise`, `vegetation`,
//! `volume`, `mesh`. Geheim (nur mit Feature `underground`, D-005): `underground`.

pub mod biome;
pub mod mesh;
pub mod noise;
#[cfg(feature = "underground")]
pub mod underground;
pub mod vegetation;
pub mod volume;
pub mod world;

pub use biome::{FamilyKind, VariantTag};
pub use world::{BiomeRef, ColumnSample, WorldConfig, WorldGen, smoothstep};
