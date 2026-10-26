//! Höhlen-Streaming (#4): reine Höhlen-Chunks unter der Oberfläche, nur nahe der Kamera.
//!
//! Oberflächen-Chunks (auch mit Höhleneingängen) gehören zu den Spalten. Hier geht es nur
//! um Chunks, die laut Oberfläche massiv sind, aber Höhlen enthalten können. Ohne
//! Begrenzung müsste der Viewer überall die Höhlensysteme mitrendern (bis Cave Culling, #8).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::{mesh::ChunkMesh, underground::CAVE_MIN_Y, volume::ChunkKind};

use super::{CHUNK_SIZE, ChunkAssets, ColumnCache, DirtyChunks, TerrainChunk, camera_chunk};
use crate::meshes::chunk_mesh_to_bevy;
use crate::{EditsRes, UndergroundRes, WorldRes, env_parse};

const CAVE_JOBS_IN_FLIGHT: usize = 6;
const CAVES_READY_PER_FRAME: usize = 4;
const CAVE_DESPAWNS_PER_FRAME: usize = 8;

/// Markiert Terrain-Chunks, die nur wegen Höhlen existieren.
#[derive(Component)]
pub struct CaveChunk;

/// Radius (in Chunks, Chebyshev-Abstand in 3D) um die Kamera, in dem Höhlen geladen werden.
/// Umgebungsvariable `TERRAFORGE_CAVE_RADIUS`, Standard 3 (≈ 100 m).
#[derive(Resource)]
pub(super) struct CaveSettings {
    radius: i32,
}

impl CaveSettings {
    pub(super) fn from_env() -> Self {
        Self {
            radius: env_parse("TERRAFORGE_CAVE_RADIUS").unwrap_or(3),
        }
    }
}

/// Bearbeitete Positionen: `Some` = Chunk mit Mesh, `None` = geprüft, keine Höhle.
#[derive(Resource, Default)]
pub(super) struct CaveMap(HashMap<IVec3, Option<Entity>>);

struct CaveBuild {
    position: IVec3,
    mesh: Option<Mesh>,
}

#[derive(Resource, Default)]
pub struct CaveJobs {
    pending: HashMap<IVec3, Task<CaveBuild>>,
    ready: Vec<CaveBuild>,
}

impl CaveJobs {
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[derive(SystemParam)]
pub(super) struct CaveContext<'w> {
    world: Res<'w, WorldRes>,
    underground: Res<'w, UndergroundRes>,
    assets: Res<'w, ChunkAssets>,
    settings: Res<'w, CaveSettings>,
    cache: Res<'w, ColumnCache>,
    map: ResMut<'w, CaveMap>,
    jobs: ResMut<'w, CaveJobs>,
    edits: Res<'w, EditsRes>,
    dirty: ResMut<'w, DirtyChunks>,
}

/// Chebyshev-Abstand: größte Abweichung über alle Achsen.
fn distance(a: IVec3, b: IVec3) -> i32 {
    (a - b).abs().max_element()
}

pub(super) fn stream_caves(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut ctx: CaveContext,
) {
    let Ok(transform) = camera.single() else {
        return;
    };
    let center = camera_chunk(transform.translation);
    let radius = ctx.settings.radius;

    // Veränderte massive Chunks gelten in allen geladenen Spalten, nicht nur im Höhlen-Radius
    // und auch unter CAVE_MIN_Y (sonst stünde ein gegrabener Schacht offen über dem Nichts).
    let touched: std::collections::HashSet<IVec3> = ctx
        .edits
        .0
        .read()
        .expect("Edits-Lock vergiftet")
        .touched_chunks()
        .filter(|c| ctx.cache.0.get(&c.xz()).is_some_and(|column| column.classify(c.y) == ChunkKind::Solid))
        .collect();
    let wanted = |position: IVec3, margin: i32| distance(position, center) <= radius + margin || touched.contains(&position);

    // 1. Weit entfernte Chunks entfernen (mit Budget), laufende Jobs dort verwerfen.
    let mut despawn_budget = CAVE_DESPAWNS_PER_FRAME;
    ctx.map.0.retain(|&position, entity| {
        if wanted(position, 1) {
            return true;
        }
        match entity {
            Some(entity) if despawn_budget > 0 => {
                despawn_budget -= 1;
                commands.entity(*entity).despawn();
                false
            }
            Some(_) => true,
            None => false,
        }
    });
    ctx.jobs.pending.retain(|&position, _| wanted(position, 2));

    // 2. Fertige Jobs einsammeln und mit Budget einfügen (nächste zuerst).
    let mut finished = Vec::new();
    ctx.jobs.pending.retain(|_, task| match block_on(poll_once(task)) {
        Some(build) => {
            finished.push(build);
            false
        }
        None => true,
    });
    let mut ready = std::mem::take(&mut ctx.jobs.ready);
    ready.extend(finished);
    ready.retain(|build| wanted(build.position, 1));
    ready.sort_by_key(|build| distance(build.position, center));
    let mut inserted = 0;
    for build in ready {
        if inserted >= CAVES_READY_PER_FRAME {
            ctx.jobs.ready.push(build);
            continue;
        }
        let entity = build.mesh.map(|mesh| {
            inserted += 1;
            commands
                .spawn((
                    TerrainChunk,
                    CaveChunk,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(ctx.assets.terrain_material.clone()),
                    Transform::from_translation(build.position.as_vec3() * CHUNK_SIZE),
                ))
                .id()
        });
        // Neubau nach Graben: altes Mesh erst jetzt entfernen.
        if let Some(Some(old)) = ctx.map.0.insert(build.position, entity) {
            commands.entity(old).despawn();
        }
    }

    // 3. Neue Jobs für unbearbeitete Positionen unter der Oberfläche.
    let lowest = (CAVE_MIN_Y / CHUNK_SIZE).floor() as i32;
    let mut missing = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let column_position = center.xz() + IVec2::new(dx, dz);
            // Ohne geladene Spalte keine Höhen: später erneut versuchen.
            let Some(column) = ctx.cache.0.get(&column_position) else {
                continue;
            };
            for dy in -radius..=radius {
                let chunk_y = center.y + dy;
                let position = IVec3::new(column_position.x, chunk_y, column_position.y);
                if chunk_y < lowest
                    || ctx.map.0.contains_key(&position)
                    || ctx.jobs.pending.contains_key(&position)
                    || ctx.jobs.ready.iter().any(|build| build.position == position)
                    // Oberflächen-Chunks lädt die Spalte, Luft braucht nichts.
                    || column.classify(chunk_y) != ChunkKind::Solid
                {
                    continue;
                }
                missing.push((position, Arc::clone(column)));
            }
        }
    }
    for &position in &touched {
        let known = ctx.map.0.contains_key(&position)
            || ctx.jobs.pending.contains_key(&position)
            || ctx.jobs.ready.iter().any(|build| build.position == position)
            || missing.iter().any(|(p, _)| *p == position);
        if !known && let Some(column) = ctx.cache.0.get(&position.xz()) {
            missing.push((position, Arc::clone(column)));
        }
    }
    missing.sort_by_key(|(position, _)| distance(*position, center));

    // Veränderte massive Chunks zuerst neu bauen (nur bereits bearbeitete Positionen; der Rest
    // bekommt die Änderung beim ersten Laden).
    // Die Markierung bleibt, solange schon ein Job für die Position läuft oder fertig wartet,
    // und verschwindet erst, wenn der Neubau-Job startet.
    let dirty: Vec<IVec3> = ctx.dirty.caves.iter().copied().collect();
    let mut rebuilds = Vec::new();
    for position in dirty {
        let in_flight = ctx.jobs.pending.contains_key(&position) || ctx.jobs.ready.iter().any(|b| b.position == position);
        if in_flight {
            continue;
        }
        match ctx.cache.0.get(&position.xz()) {
            Some(column) if ctx.map.0.contains_key(&position) => rebuilds.push((position, Arc::clone(column))),
            _ => {
                // Nicht geladen und kein Job: Beim Laden wird die Änderung ohnehin angewendet.
                ctx.dirty.caves.remove(&position);
            }
        }
    }
    missing.retain(|(p, _)| !rebuilds.iter().any(|(r, _)| r == p));
    rebuilds.extend(missing);
    let missing = rebuilds;

    let slots = CAVE_JOBS_IN_FLIGHT.saturating_sub(ctx.jobs.pending.len());
    for (position, column) in missing.into_iter().take(slots) {
        ctx.dirty.caves.remove(&position);
        let world = Arc::clone(&ctx.world.0);
        let underground = Arc::clone(&ctx.underground.0);
        let edits = Arc::clone(&ctx.edits.0);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let job_start = std::time::Instant::now();
            // Lock nur kurz halten, damit Graben nicht auf Jobs wartet.
            let touched = edits.read().expect("Edits-Lock vergiftet").touches(position);
            let density = if touched {
                let mut chunk = underground.materials_chunk(&world, &column, position.y);
                edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
                Some(chunk)
            } else {
                underground.density_chunk(&world, &column, position.y)
            };
            let mesh = density
                .map(|density| ChunkMesh::build(&world, &column, &density))
                .filter(|mesh| !mesh.is_empty())
                .map(chunk_mesh_to_bevy);
            crate::bench::record_job(crate::bench::JobKind::Cave, job_start);
            CaveBuild { position, mesh }
        });
        ctx.jobs.pending.insert(position, task);
    }
}
