//! Vegetation: welche Bäume wo stehen und wie sie aussehen.
//!
//! - `placement`:  Baumplanung pro Chunk (Dichte, Arten je Biom-Familie)
//! - `geometry`:   Formen der einzelnen Baumarten
//! - `primitives`: Geometrie-Bausteine (Prisma, Kegel, Blob, Ast) und Vertex-Puffer
//! - `rng`:        deterministischer Zufall und Hashes
//! - `support`:    Bäume auf die tatsächliche Volumen-Oberfläche setzen (#7)

mod geometry;
mod placement;
mod primitives;
mod rng;
mod support;

use glam::Vec3;

pub use geometry::build_tree_geometry;
pub use placement::plan_trees;
pub use support::{GROUND_SEARCH, settle_tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeStyle {
    Broadleaf,
    Conifer,
    Palm,
    Cactus,
    Mushroom,
    Crystal,
    Dead,
}

impl TreeStyle {
    pub const ALL: [TreeStyle; 7] = [
        TreeStyle::Broadleaf,
        TreeStyle::Conifer,
        TreeStyle::Palm,
        TreeStyle::Cactus,
        TreeStyle::Mushroom,
        TreeStyle::Crystal,
        TreeStyle::Dead,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TreePlacement {
    /// Stabile ID (Spalte + Zelle), z. B. um "gefällt" zu speichern (#7, #19).
    pub id: TreeId,
    pub style: TreeStyle,
    pub variant: usize,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: f32,
    pub scale_y: f32,
}

/// Stabile ID eines Baums: Spalten-Chunk und Zelle (höchstens ein Baum pro Zelle). Bleibt gleich,
/// egal welche Bäume gefiltert oder entfernt werden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TreeId {
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// `z * CHUNK_SIZE + x` der Zelle.
    pub cell: u16,
}

#[derive(Default)]
pub struct TreeGeo {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}
