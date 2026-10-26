//! Vegetation: welche Bäume wo stehen und wie sie aussehen.
//!
//! - `placement`:  Baumplanung pro Chunk (Dichte, Arten je Biom-Familie)
//! - `geometry`:   Formen der einzelnen Baumarten
//! - `primitives`: Geometrie-Bausteine (Prisma, Kegel, Blob, Ast) und Vertex-Puffer
//! - `rng`:        deterministischer Zufall und Hashes

mod geometry;
mod placement;
mod primitives;
mod rng;

use glam::Vec3;

pub use geometry::build_tree_geometry;
pub use placement::plan_trees;

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
    pub style: TreeStyle,
    pub variant: usize,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: f32,
    pub scale_y: f32,
}

#[derive(Default)]
pub struct TreeGeo {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}
