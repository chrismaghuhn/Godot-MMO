//! Chunk-Streaming um die Kamera.
//!
//! - `columns`: Spalten im Sichtradius; pro Spalte die Oberflächen-Chunks und Bäume
//! - `caves`:   reine Höhlen-Chunks unter der Oberfläche, nur nahe der Kamera (#4)
//! - `far`:     grobes Fernterrain bis ~3,5 km (#33)
//!
//! Die Spalten legen ihre Höhendaten im `ColumnCache` ab. Daraus entscheidet das
//! Höhlen-Streaming ohne Neuberechnung, welche Chunks unter der Oberfläche liegen.

mod caves;
mod columns;
mod far;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use terraforge_core::volume::{self, ColumnData};

pub use caves::{CaveChunk, CaveJobs};
pub use columns::{ColumnJobs, ColumnMap};
pub use far::{FAR_GUARANTEED, FAR_VIEW, FarTileMarker, MAX_VIEW_RADIUS};

pub const CHUNK_SIZE: f32 = volume::CHUNK_SIZE as f32;

pub struct StreamingPlugin;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ColumnMap>()
            .init_resource::<ColumnJobs>()
            .init_resource::<ColumnCache>()
            .insert_resource(caves::CaveSettings::from_env())
            .init_resource::<caves::CaveMap>()
            .init_resource::<CaveJobs>()
            .init_resource::<DirtyChunks>()
            .insert_resource(far::FarState::from_env())
            .add_systems(Update, (columns::stream_columns, caves::stream_caves, far::stream_far).chain());
    }
}

#[derive(Resource)]
pub struct ChunkAssets {
    pub terrain_material: Handle<StandardMaterial>,
    pub tree_material: Handle<StandardMaterial>,
    pub tree_meshes: Vec<Handle<Mesh>>,
}

/// Durch Graben/Bauen veränderte Chunks, die neu gebaut werden müssen (#6). Spalten bauen ihre
/// Oberflächen- und Luft-Chunks neu, das Höhlen-Streaming die massiven.
#[derive(Resource, Default)]
pub struct DirtyChunks {
    pub columns: HashSet<IVec2>,
    pub caves: HashSet<IVec3>,
}

/// Höhen und Biome der geladenen Spalten, geteilt mit den Höhlen-Jobs.
#[derive(Resource, Default)]
pub struct ColumnCache(pub HashMap<IVec2, Arc<ColumnData>>);

/// Ein Baum mit seiner stabilen ID (#7).
#[derive(Component)]
pub struct Tree(pub terraforge_core::vegetation::TreeId);

/// Ein 3D-Terrain-Chunk (32³) mit Mesh, an der Oberfläche oder in einer Höhle.
#[derive(Component)]
pub struct TerrainChunk;

/// Chunk-Position der Kamera.
fn camera_chunk(translation: Vec3) -> IVec3 {
    (translation / CHUNK_SIZE).floor().as_ivec3()
}
