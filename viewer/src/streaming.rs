//! Chunk-Streaming um die Kamera: Jobs im Hintergrund, Einfügen/Entfernen von Chunks.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::{
    mesh::ChunkMesh,
    vegetation::{TreePlacement, TreeStyle, plan_trees},
    volume::{self, ColumnData},
};

use crate::meshes::chunk_mesh_to_bevy;
use crate::scene::SpawnPoint;
use crate::{UndergroundRes, WorldRes};

pub const CHUNK_SIZE: f32 = volume::CHUNK_SIZE as f32;
const CHUNK_JOBS_IN_FLIGHT: usize = 8;
const CHUNKS_READY_PER_FRAME: usize = 3;
const CHUNK_DESPAWNS_PER_FRAME: usize = 4;

pub struct StreamingPlugin;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<ChunkJobs>()
            .add_systems(Update, stream_chunks);
    }
}

#[derive(Resource)]
pub struct ChunkAssets {
    pub terrain_material: Handle<StandardMaterial>,
    pub tree_material: Handle<StandardMaterial>,
    pub tree_meshes: Vec<Handle<Mesh>>,
}

#[derive(Resource, Default)]
pub struct ChunkMap(pub HashMap<IVec2, Entity>);

/// Ergebnis eines Spalten-Jobs: alle 3D-Chunks mit Oberfläche in dieser Spalte.
struct ChunkBuild {
    position: IVec2,
    /// (Chunk-Höhe, Mesh) für jeden 3D-Chunk mit Oberfläche.
    meshes: Vec<(i32, Mesh)>,
    trees: Vec<TreePlacement>,
}

#[derive(Resource, Default)]
pub struct ChunkJobs {
    pending: HashMap<IVec2, Task<ChunkBuild>>,
    ready: Vec<ChunkBuild>,
}

impl ChunkJobs {
    /// Jobs, die gerade im Hintergrund laufen.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Fertige Jobs, die noch auf das Einfügen warten (Budget pro Frame).
    pub fn ready_count(&self) -> usize {
        self.ready.len()
    }
}

#[derive(Component)]
pub struct Tree;

/// Ein 3D-Terrain-Chunk (32³) mit Mesh.
#[derive(Component)]
pub struct TerrainChunk;

#[derive(SystemParam)]
struct ChunkContext<'w> {
    world: Res<'w, WorldRes>,
    underground: Res<'w, UndergroundRes>,
    assets: Res<'w, ChunkAssets>,
    spawn: Res<'w, SpawnPoint>,
    chunks: ResMut<'w, ChunkMap>,
    jobs: ResMut<'w, ChunkJobs>,
}

fn stream_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut ctx: ChunkContext,
) {
    let Ok(transform) = camera.single() else {
        return;
    };

    let center = IVec2::new(
        (transform.translation.x / CHUNK_SIZE).floor() as i32,
        (transform.translation.z / CHUNK_SIZE).floor() as i32,
    );
    let radius = ctx.world.0.cfg.view_radius;
    let clearing = ctx.spawn.0;

    let mut despawn_budget = CHUNK_DESPAWNS_PER_FRAME;
    ctx.chunks.0.retain(|position, entity| {
        if (position.x - center.x).abs() > radius + 1 || (position.y - center.y).abs() > radius + 1
        {
            if despawn_budget == 0 {
                return true;
            }
            despawn_budget -= 1;
            commands.entity(*entity).despawn();
            false
        } else {
            true
        }
    });

    ctx.jobs.pending.retain(|position, _| {
        (position.x - center.x).abs() <= radius + 2 && (position.y - center.y).abs() <= radius + 2
    });
    ctx.jobs.ready.retain(|build| {
        let position = build.position;
        !ctx.chunks.0.contains_key(&position)
            && (position.x - center.x).abs() <= radius + 2
            && (position.y - center.y).abs() <= radius + 2
    });

    let mut finished = Vec::new();
    ctx.jobs.pending.retain(|_, task| {
        if let Some(build) = block_on(poll_once(task)) {
            finished.push(build);
            false
        } else {
            true
        }
    });
    ctx.jobs.ready.extend(finished);

    let mut ready = std::mem::take(&mut ctx.jobs.ready);
    ready.sort_by_key(|build| {
        let dx = build.position.x - center.x;
        let dz = build.position.y - center.y;
        dx * dx + dz * dz
    });
    let mut deferred = Vec::new();
    let mut inserted = 0usize;
    for build in ready {
        let position = build.position;
        let far = (position.x - center.x).abs() > radius + 1
            || (position.y - center.y).abs() > radius + 1;
        if far || ctx.chunks.0.contains_key(&position) {
            continue;
        }
        if inserted >= CHUNKS_READY_PER_FRAME {
            deferred.push(build);
            continue;
        }
        insert_chunk(
            &mut commands,
            &mut meshes,
            &ctx.assets,
            &mut ctx.chunks,
            build,
        );
        inserted += 1;
    }
    ctx.jobs.ready = deferred;

    let mut missing = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let position = center + IVec2::new(dx, dz);
            if ctx.chunks.0.contains_key(&position)
                || ctx.jobs.pending.contains_key(&position)
                || ctx
                    .jobs
                    .ready
                    .iter()
                    .any(|build| build.position == position)
            {
                continue;
            }
            missing.push((position, dx * dx + dz * dz));
        }
    }
    missing.sort_by_key(|(_, distance)| *distance);

    let slots = CHUNK_JOBS_IN_FLIGHT.saturating_sub(ctx.jobs.pending.len());
    for (position, _) in missing.into_iter().take(slots) {
        let world = Arc::clone(&ctx.world.0);
        let underground = Arc::clone(&ctx.underground.0);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let column = ColumnData::sample(&world, position.x, position.y);
            // Höhlen (#4) werden auch Chunks unter der Oberfläche betreffen; dann reicht
            // `surface_chunks()` nicht mehr und es braucht vertikales Streaming.
            let meshes = column
                .surface_chunks()
                .filter_map(|chunk_y| {
                    let density = underground.density_chunk(&column, chunk_y)?;
                    let mesh = ChunkMesh::build(&world, &column, &density);
                    (!mesh.is_empty()).then(|| (chunk_y, chunk_mesh_to_bevy(mesh)))
                })
                .collect();
            let mut trees = plan_trees(
                world.cfg.seed,
                world.cfg.sea_level,
                position.x,
                position.y,
                &column.heights,
                &column.biomes,
            );
            trees.retain(|tree| {
                let world_x = position.x as f32 * CHUNK_SIZE + tree.position.x;
                let world_z = position.y as f32 * CHUNK_SIZE + tree.position.z;
                Vec2::new(world_x - clearing.x, world_z - clearing.z).length() > 17.0
            });
            ChunkBuild {
                position,
                meshes,
                trees,
            }
        });
        ctx.jobs.pending.insert(position, task);
    }
}

fn insert_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &ChunkAssets,
    chunks: &mut ChunkMap,
    build: ChunkBuild,
) {
    let ChunkBuild {
        position,
        meshes: chunk_meshes,
        trees,
    } = build;
    // Die Spalte ist der Eltern-Knoten: Streaming und Despawn laufen weiter pro Spalte,
    // die Terrain-Meshes darunter sind echte 3D-Chunks.
    let mut entity = commands.spawn((
        Transform::from_xyz(
            position.x as f32 * CHUNK_SIZE,
            0.0,
            position.y as f32 * CHUNK_SIZE,
        ),
        Visibility::default(),
    ));
    let chunk_entity = entity.id();

    entity.with_children(|parent| {
        for (chunk_y, mesh) in chunk_meshes {
            parent.spawn((
                TerrainChunk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.terrain_material.clone()),
                Transform::from_xyz(0.0, chunk_y as f32 * CHUNK_SIZE, 0.0),
            ));
        }
    });

    if !trees.is_empty() {
        entity.with_children(|parent| {
            for tree in &trees {
                parent.spawn((
                    Tree,
                    Mesh3d(assets.tree_meshes[tree_mesh_index(tree.style, tree.variant)].clone()),
                    MeshMaterial3d(assets.tree_material.clone()),
                    Transform::from_translation(tree.position)
                        .with_rotation(Quat::from_rotation_y(tree.rotation))
                        .with_scale(Vec3::new(tree.scale, tree.scale * tree.scale_y, tree.scale)),
                ));
            }
        });
    }

    chunks.0.insert(position, chunk_entity);
}

fn tree_mesh_index(style: TreeStyle, variant: usize) -> usize {
    style.index() * 3 + variant
}
