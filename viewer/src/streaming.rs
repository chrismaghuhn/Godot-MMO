//! Chunk-Streaming im Viewer: setzt die Events des Core-Streamers (`terraforge_core::streaming`,
//! #55) in Entities um.
//!
//! Spalten sind Eltern-Entities mit ihren Terrain-Chunks und Bäumen als Kinder, Höhlen-Chunks
//! und Fernkacheln eigene Entities. Meshes wandeln die Worker-Threads des Streamers schon in
//! Bevy-Meshes um.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use terraforge_core::{
    streaming::{Config, Event, FarKey, Stats, Streamer},
    vegetation::{TreePlacement, TreeStyle},
    volume,
};

use crate::meshes::chunk_mesh_to_bevy;
use crate::scene::SpawnPoint;
use crate::xray::Xray;
use crate::{EditsRes, UndergroundRes, WorldRes, env_parse};

pub use terraforge_core::streaming::{FAR_GUARANTEED, FAR_VIEW, MAX_VIEW_RADIUS};

pub const CHUNK_SIZE: f32 = volume::CHUNK_SIZE as f32;

/// Bäume rund um den Spawnpunkt weglassen (freie Sicht beim Start).
const SPAWN_CLEARING: f32 = 17.0;

pub struct StreamingPlugin;

/// Systeme des Streamings. Änderungen an der Welt (Graben, Fällen) laufen davor, damit Neubauten,
/// die im selben Frame starten, sie schon kennen.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamingSet;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Spawned>()
            .add_systems(Startup, create_streamer.after(crate::scene::setup_scene))
            .add_systems(Update, stream.in_set(StreamingSet));
    }
}

#[derive(Resource)]
pub struct ChunkAssets {
    pub terrain_material: Handle<StandardMaterial>,
    pub tree_material: Handle<StandardMaterial>,
    pub tree_meshes: Vec<Handle<Mesh>>,
}

/// Der Core-Streamer. Umgebungsvariablen: `TERRAFORGE_CAVE_RADIUS` (Standard 3),
/// `TERRAFORGE_CULL=0` (ohne Cave Culling), `TERRAFORGE_FAR=0` (ohne Fernterrain).
#[derive(Resource)]
pub struct StreamerRes {
    pub streamer: Streamer<Mesh>,
    culling: bool,
}

/// Entities zu den Positionen des Streamers.
#[derive(Resource, Default)]
struct Spawned {
    columns: HashMap<IVec2, Entity>,
    caves: HashMap<IVec3, Entity>,
    far: HashMap<FarKey, Entity>,
}

/// Ein Baum mit seiner stabilen ID (#7).
#[derive(Component)]
pub struct Tree(pub terraforge_core::vegetation::TreeId);

/// Ein 3D-Terrain-Chunk (32³) mit Mesh, an der Oberfläche oder in einer Höhle.
#[derive(Component)]
pub struct TerrainChunk;

/// Markiert Terrain-Chunks, die nur wegen Höhlen existieren.
#[derive(Component)]
pub struct CaveChunk;

#[derive(Component)]
pub struct FarTileMarker;

fn create_streamer(
    mut commands: Commands,
    world: Res<WorldRes>,
    underground: Res<UndergroundRes>,
    edits: Res<EditsRes>,
    spawn: Res<SpawnPoint>,
) {
    let mut config = Config::new(world.0.cfg.view_radius);
    config.cave_radius = env_parse("TERRAFORGE_CAVE_RADIUS").unwrap_or(3);
    config.culling = env_parse::<u32>("TERRAFORGE_CULL").unwrap_or(1) != 0;
    config.far = env_parse::<u32>("TERRAFORGE_FAR").unwrap_or(1) != 0;
    config.clearing = Some((spawn.0.xz(), SPAWN_CLEARING));
    config.record_job_times = crate::bench::active();
    let culling = config.culling;
    let underground: Arc<dyn terraforge_core::streaming::UndergroundSource> = underground.0.clone();
    let streamer = Streamer::new(Arc::clone(&world.0), underground, Arc::clone(&edits.0), config, chunk_mesh_to_bevy);
    commands.insert_resource(StreamerRes { streamer, culling });
}

impl StreamerRes {
    pub fn stats(&self) -> Stats {
        self.streamer.stats()
    }
}

fn stream(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    assets: Res<ChunkAssets>,
    xray: Res<Xray>,
    mut spawned: ResMut<Spawned>,
    mut res: ResMut<StreamerRes>,
) {
    let Ok(transform) = camera.single() else {
        return;
    };
    // Im Röntgenmodus sollen alle Höhlen durch den Fels sichtbar sein.
    let culling = res.culling && !xray.enabled;
    let streamer = &mut res.streamer;
    streamer.set_culling(culling);
    streamer.update(transform.translation);
    for (kind, duration) in streamer.take_job_times() {
        crate::bench::record_job(kind, duration);
    }

    for event in streamer.drain_events() {
        match event {
            Event::ShowColumn { column, chunks, trees } => {
                let entity = spawn_column(&mut commands, &mut meshes, &assets, column, chunks, &trees);
                // Neubau: die alte Spalte erst jetzt entfernen, wo die neue da ist (kein Flackern).
                if let Some(old) = spawned.columns.insert(column, entity) {
                    commands.entity(old).despawn();
                }
            }
            Event::RemoveColumn { column } => {
                if let Some(entity) = spawned.columns.remove(&column) {
                    commands.entity(entity).despawn();
                }
            }
            Event::ShowCave { chunk, mesh } => {
                let entity = mesh.map(|mesh| {
                    commands
                        .spawn((
                            TerrainChunk,
                            CaveChunk,
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(assets.terrain_material.clone()),
                            Transform::from_translation(chunk.as_vec3() * CHUNK_SIZE),
                        ))
                        .id()
                });
                let old = match entity {
                    Some(entity) => spawned.caves.insert(chunk, entity),
                    None => spawned.caves.remove(&chunk),
                };
                if let Some(old) = old {
                    commands.entity(old).despawn();
                }
            }
            Event::RemoveCave { chunk } => {
                if let Some(entity) = spawned.caves.remove(&chunk) {
                    commands.entity(entity).despawn();
                }
            }
            Event::CaveVisibility { chunk, visible } => {
                if let Some(&entity) = spawned.caves.get(&chunk) {
                    commands.entity(entity).insert(if visible { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
            Event::ShowFar { tile, mesh } => {
                let entity = mesh.map(|mesh| {
                    commands
                        .spawn((
                            FarTileMarker,
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(assets.terrain_material.clone()),
                            Transform::from_translation(tile.origin()),
                        ))
                        .id()
                });
                // Alte Version derselben Kachel erst jetzt ersetzen (keine Lücke).
                let old = match entity {
                    Some(entity) => spawned.far.insert(tile, entity),
                    None => spawned.far.remove(&tile),
                };
                if let Some(old) = old {
                    commands.entity(old).despawn();
                }
            }
            Event::RemoveFar { tile } => {
                if let Some(entity) = spawned.far.remove(&tile) {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn spawn_column(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &ChunkAssets,
    position: IVec2,
    chunk_meshes: Vec<(i32, Mesh)>,
    trees: &[TreePlacement],
) -> Entity {
    // Die Spalte ist der Eltern-Knoten: Streaming und Despawn laufen pro Spalte, die
    // Terrain-Meshes darunter sind echte 3D-Chunks.
    let mut entity = commands.spawn((
        Transform::from_xyz(position.x as f32 * CHUNK_SIZE, 0.0, position.y as f32 * CHUNK_SIZE),
        Visibility::default(),
    ));
    entity.with_children(|parent| {
        for (chunk_y, mesh) in chunk_meshes {
            parent.spawn((
                TerrainChunk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.terrain_material.clone()),
                Transform::from_xyz(0.0, chunk_y as f32 * CHUNK_SIZE, 0.0),
            ));
        }
        for tree in trees {
            parent.spawn((
                Tree(tree.id),
                Mesh3d(assets.tree_meshes[tree_mesh_index(tree.style, tree.variant)].clone()),
                MeshMaterial3d(assets.tree_material.clone()),
                Transform::from_translation(tree.position)
                    .with_rotation(Quat::from_rotation_y(tree.rotation))
                    .with_scale(Vec3::new(tree.scale, tree.scale * tree.scale_y, tree.scale)),
            ));
        }
    });
    entity.id()
}

fn tree_mesh_index(style: TreeStyle, variant: usize) -> usize {
    style.index() * 3 + variant
}
