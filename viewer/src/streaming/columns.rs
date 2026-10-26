//! Spalten-Streaming: Spalten im Sichtradius, pro Spalte die Oberflächen-Chunks und Bäume.

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
    volume::{ChunkKind, ColumnData},
};

use super::{CHUNK_SIZE, ChunkAssets, ColumnCache, DirtyChunks, TerrainChunk, Tree, camera_chunk};
use crate::meshes::chunk_mesh_to_bevy;
use crate::scene::SpawnPoint;
use crate::{EditsRes, UndergroundRes, WorldRes};

const COLUMN_JOBS_IN_FLIGHT: usize = 8;
const COLUMNS_READY_PER_FRAME: usize = 3;
const COLUMN_DESPAWNS_PER_FRAME: usize = 4;

/// Geladene Spalten (Eltern-Entity mit Terrain-Chunks und Bäumen als Kinder).
#[derive(Resource, Default)]
pub struct ColumnMap(pub HashMap<IVec2, Entity>);

/// Ergebnis eines Spalten-Jobs: alle 3D-Chunks mit Oberfläche in dieser Spalte.
struct ColumnBuild {
    position: IVec2,
    column: Arc<ColumnData>,
    /// (Chunk-Höhe, Mesh) für jeden 3D-Chunk mit Oberfläche.
    meshes: Vec<(i32, Mesh)>,
    trees: Vec<TreePlacement>,
    /// Neubau nach Graben/Bauen: ersetzt die geladene Spalte.
    rebuild: bool,
}

#[derive(Resource, Default)]
pub struct ColumnJobs {
    pending: HashMap<IVec2, Task<ColumnBuild>>,
    ready: Vec<ColumnBuild>,
}

impl ColumnJobs {
    /// Jobs, die gerade im Hintergrund laufen.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Fertige Jobs, die noch auf das Einfügen warten (Budget pro Frame).
    pub fn ready_count(&self) -> usize {
        self.ready.len()
    }
}

#[derive(SystemParam)]
pub(super) struct ColumnContext<'w> {
    world: Res<'w, WorldRes>,
    underground: Res<'w, UndergroundRes>,
    assets: Res<'w, ChunkAssets>,
    spawn: Res<'w, SpawnPoint>,
    chunks: ResMut<'w, ColumnMap>,
    jobs: ResMut<'w, ColumnJobs>,
    cache: ResMut<'w, ColumnCache>,
    edits: Res<'w, EditsRes>,
    dirty: ResMut<'w, DirtyChunks>,
}

pub(super) fn stream_columns(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut ctx: ColumnContext,
) {
    let Ok(transform) = camera.single() else {
        return;
    };

    let center = camera_chunk(transform.translation).xz();
    let radius = ctx.world.0.cfg.view_radius;
    let clearing = ctx.spawn.0;

    let mut despawn_budget = COLUMN_DESPAWNS_PER_FRAME;
    let cache = &mut ctx.cache.0;
    ctx.chunks.0.retain(|position, entity| {
        if (position.x - center.x).abs() > radius + 1 || (position.y - center.y).abs() > radius + 1
        {
            if despawn_budget == 0 {
                return true;
            }
            despawn_budget -= 1;
            commands.entity(*entity).despawn();
            cache.remove(position);
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
        (build.rebuild || !ctx.chunks.0.contains_key(&position))
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
        if far || (!build.rebuild && ctx.chunks.0.contains_key(&position)) {
            continue;
        }
        if inserted >= COLUMNS_READY_PER_FRAME {
            deferred.push(build);
            continue;
        }
        ctx.cache.0.insert(position, Arc::clone(&build.column));
        // Neubau: alte Spalte erst entfernen, wenn die neue da ist (kein Flackern).
        if let Some(old) = ctx.chunks.0.remove(&position) {
            commands.entity(old).despawn();
        }
        insert_column(
            &mut commands,
            &mut meshes,
            &ctx.assets,
            &mut ctx.chunks,
            build,
        );
        inserted += 1;
    }
    ctx.jobs.ready = deferred;

    // Neubau veränderter Spalten zuerst. Läuft schon ein Job für die Spalte, warten (er kennt
    // die Änderung evtl. nicht); nicht geladene Spalten bekommen die Änderung beim Laden.
    let mut missing = Vec::new();
    let dirty: Vec<IVec2> = ctx.dirty.columns.iter().copied().collect();
    for position in dirty {
        if !ctx.chunks.0.contains_key(&position) {
            ctx.dirty.columns.remove(&position);
        } else if !ctx.jobs.pending.contains_key(&position) {
            ctx.dirty.columns.remove(&position);
            missing.push((position, -1, true));
        }
    }
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
            missing.push((position, dx * dx + dz * dz, false));
        }
    }
    missing.sort_by_key(|(_, distance, _)| *distance);

    let slots = COLUMN_JOBS_IN_FLIGHT.saturating_sub(ctx.jobs.pending.len());
    for (position, _, rebuild) in missing.into_iter().take(slots) {
        let world = Arc::clone(&ctx.world.0);
        let underground = Arc::clone(&ctx.underground.0);
        let edits = Arc::clone(&ctx.edits.0);
        // Beim Neubau die vorhandenen Höhen wiederverwenden.
        let cached = if rebuild { ctx.cache.0.get(&position).cloned() } else { None };
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let column = cached.unwrap_or_else(|| Arc::new(ColumnData::sample(&world, position.x, position.y)));
            let edits = edits.read().expect("Edits-Lock vergiftet");
            // Chunks mit Oberfläche (inkl. Höhleneingängen) und veränderte Chunks, die laut
            // Generator Luft sind (z. B. Gebautes). Massive Chunks – auch veränderte – lädt das
            // Höhlen-Streaming in Kameranähe.
            let mut heights: Vec<i32> = column.surface_chunks().collect();
            for chunk_y in edits.touched_in_column(position.x, position.y) {
                if column.classify(chunk_y) == ChunkKind::Air && !heights.contains(&chunk_y) {
                    heights.push(chunk_y);
                }
            }
            let meshes = heights
                .into_iter()
                .filter_map(|chunk_y| {
                    let chunk_position = IVec3::new(position.x, chunk_y, position.y);
                    let density = if edits.touches(chunk_position) {
                        let mut chunk = underground.materials_chunk(&world, &column, chunk_y);
                        edits.apply(&mut chunk);
                        chunk
                    } else {
                        underground.density_chunk(&world, &column, chunk_y)?
                    };
                    let mesh = ChunkMesh::build(&world, &column, &density);
                    (!mesh.is_empty()).then(|| (chunk_y, chunk_mesh_to_bevy(mesh)))
                })
                .collect();
            let mut trees = plan_trees(
                world.cfg.seed,
                world.cfg.sea_level,
                position.x,
                position.y,
                column.heights(),
                &column.biomes,
                &column.temperatures,
            );
            trees.retain(|tree| {
                let world_x = position.x as f32 * CHUNK_SIZE + tree.position.x;
                let world_z = position.y as f32 * CHUNK_SIZE + tree.position.z;
                Vec2::new(world_x - clearing.x, world_z - clearing.z).length() > 17.0
            });
            ColumnBuild {
                position,
                column,
                meshes,
                trees,
                rebuild,
            }
        });
        ctx.jobs.pending.insert(position, task);
    }
}

fn insert_column(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &ChunkAssets,
    chunks: &mut ColumnMap,
    build: ColumnBuild,
) {
    let ColumnBuild {
        position,
        meshes: chunk_meshes,
        trees,
        ..
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
