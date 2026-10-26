//! Fernterrain im Viewer (#33): zwei Stufen grober Kacheln um die Nahzone.
//!
//! - Stufe 1: 256-m-Kacheln mit 4-m-Raster, in den 3×3 Stufe-2-Feldern um die Kamera.
//!   Zellen in der Nahzone werden ausgespart.
//! - Stufe 2: 1024-m-Kacheln mit 16-m-Raster, in den 7×7 Feldern ohne die inneren 3×3.
//!
//! Die Aussparung rastet auf `HOLE_SNAP` Chunks ein und ist kleiner als der Lade-Radius der
//! Spalten: Sie liegt immer innerhalb der geladenen Nahzone, und der Rand der Nahzone überlappt
//! mit Fernterrain, das minimal tiefer liegt. So entstehen beim Nachladen keine Löcher, und die
//! Randkacheln werden nur alle 128 m neu gebaut statt bei jedem Chunk-Wechsel (das kostete
//! ~25 Frames > 20 ms pro 30 s Flug). Kacheln werden erst entfernt, wenn alle gewünschten
//! Kacheln fertig sind (keine Lücken beim Wechsel).
//!
//! Abschalten mit `TERRAFORGE_FAR=0`.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::lod::FarTile;

use super::{CHUNK_SIZE, ChunkAssets, camera_chunk};
use crate::meshes::chunk_mesh_to_bevy;
use crate::{WorldRes, env_parse};

const LEVEL1_TILE: f32 = 256.0;
const LEVEL1_STEP: f32 = 4.0;
const LEVEL2_TILE: f32 = 1024.0;
const LEVEL2_STEP: f32 = 16.0;
/// Ring der Stufe-2-Felder um die Kamera: innen Stufe 1, außen Stufe 2.
const LEVEL1_RING: i32 = 1;
const LEVEL2_RING: i32 = 3;
/// Leicht tiefer, damit überlappendes Fernterrain nie durch feineres Terrain scheint.
const LEVEL1_DROP: f32 = 0.4;
const LEVEL2_DROP: f32 = 1.2;
const FAR_JOBS_IN_FLIGHT: usize = 6;
/// Raster (in Chunks), auf das die Aussparung einrastet.
const HOLE_SNAP: i32 = 4;
const FAR_INSERTS_PER_FRAME: usize = 1;

/// Sichtweite des Fernterrains (Mitte bis Rand der äußeren Stufe), für Nebel und Kamera.
pub const FAR_VIEW: f32 = LEVEL2_TILE * (LEVEL2_RING as f32 + 0.5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    level: u8,
    x: i32,
    z: i32,
}

/// Ausgesparter Bereich (Weltkoordinaten, halboffen) oder keiner.
type Hole = Option<(f32, f32, f32, f32)>;

#[derive(Component)]
pub struct FarTileMarker;

struct FarBuild {
    key: TileKey,
    hole: Hole,
    mesh: Option<Mesh>,
}

#[derive(Resource)]
pub(super) struct FarState {
    enabled: bool,
    /// Gezeichnete Kacheln mit der Aussparung, mit der sie gebaut wurden.
    tiles: HashMap<TileKey, (Option<Entity>, Hole)>,
    pending: HashMap<TileKey, (Task<FarBuild>, Hole)>,
    ready: Vec<FarBuild>,
}

impl FarState {
    pub(super) fn from_env() -> Self {
        Self {
            enabled: env_parse::<u32>("TERRAFORGE_FAR").unwrap_or(1) != 0,
            tiles: HashMap::new(),
            pending: HashMap::new(),
            ready: Vec::new(),
        }
    }
}

fn tile_size(level: u8) -> f32 {
    if level == 1 { LEVEL1_TILE } else { LEVEL2_TILE }
}

/// Aussparung einer Kachel: der Teil der Nahzone, der in der Kachel liegt (nur Stufe 1).
fn hole_for(key: TileKey, near: (f32, f32, f32, f32)) -> Hole {
    if key.level != 1 {
        return None;
    }
    let (x0, z0) = (key.x as f32 * LEVEL1_TILE, key.z as f32 * LEVEL1_TILE);
    let (x1, z1) = (x0 + LEVEL1_TILE, z0 + LEVEL1_TILE);
    let overlaps = near.0 < x1 && near.2 > x0 && near.1 < z1 && near.3 > z0;
    overlaps.then_some(near)
}

pub(super) fn stream_far(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    world: Res<WorldRes>,
    assets: Res<ChunkAssets>,
    mut state: ResMut<FarState>,
) {
    if !state.enabled {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };
    let position = transform.translation;

    // Aussparung: Mittelpunkt auf HOLE_SNAP Chunks gerundet (weicht höchstens HOLE_SNAP/2 ab),
    // Radius so, dass sie samt Abweichung eine Reihe innerhalb der geladenen Spalten bleibt.
    let center = camera_chunk(position).xz();
    let snapped = IVec2::new(
        (center.x as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
        (center.y as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
    );
    let inner = (world.0.cfg.view_radius - 1 - HOLE_SNAP / 2).max(0);
    let near = (
        (snapped.x - inner) as f32 * CHUNK_SIZE,
        (snapped.y - inner) as f32 * CHUNK_SIZE,
        (snapped.x + inner + 1) as f32 * CHUNK_SIZE,
        (snapped.y + inner + 1) as f32 * CHUNK_SIZE,
    );

    // Gewünschte Kacheln.
    let field = ((position.x / LEVEL2_TILE).floor() as i32, (position.z / LEVEL2_TILE).floor() as i32);
    let mut wanted: HashMap<TileKey, Hole> = HashMap::new();
    let per_field = (LEVEL2_TILE / LEVEL1_TILE) as i32;
    for dz in -LEVEL2_RING..=LEVEL2_RING {
        for dx in -LEVEL2_RING..=LEVEL2_RING {
            let (fx, fz) = (field.0 + dx, field.1 + dz);
            if dx.abs() <= LEVEL1_RING && dz.abs() <= LEVEL1_RING {
                for sz in 0..per_field {
                    for sx in 0..per_field {
                        let key = TileKey { level: 1, x: fx * per_field + sx, z: fz * per_field + sz };
                        wanted.insert(key, hole_for(key, near));
                    }
                }
            } else {
                wanted.insert(TileKey { level: 2, x: fx, z: fz }, None);
            }
        }
    }

    // Fertige Jobs übernehmen (veraltete verwerfen).
    let mut finished = Vec::new();
    state.pending.retain(|_, (task, _)| match block_on(poll_once(task)) {
        Some(build) => {
            finished.push(build);
            false
        }
        None => true,
    });
    state.ready.extend(finished);
    let ready = std::mem::take(&mut state.ready);
    let mut inserted = 0;
    for build in ready {
        if wanted.get(&build.key) != Some(&build.hole) {
            continue;
        }
        if inserted >= FAR_INSERTS_PER_FRAME {
            state.ready.push(build);
            continue;
        }
        inserted += 1;
        let drop = if build.key.level == 1 { LEVEL1_DROP } else { LEVEL2_DROP };
        let size = tile_size(build.key.level);
        let entity = build.mesh.map(|mesh| {
            commands
                .spawn((
                    FarTileMarker,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(assets.terrain_material.clone()),
                    Transform::from_xyz(build.key.x as f32 * size, -drop, build.key.z as f32 * size),
                ))
                .id()
        });
        // Alte Version derselben Kachel erst jetzt ersetzen (keine Lücke).
        if let Some((Some(old), _)) = state.tiles.insert(build.key, (entity, build.hole)) {
            commands.entity(old).despawn();
        }
    }

    // Neue Jobs: fehlende oder mit veralteter Aussparung, nächste zuerst.
    let mut missing: Vec<(TileKey, Hole, f32)> = wanted
        .iter()
        .filter(|(key, hole)| {
            state.tiles.get(key).map(|t| &t.1) != Some(hole) && state.pending.get(key).map(|p| &p.1) != Some(hole)
                && !state.ready.iter().any(|b| b.key == **key && b.hole == **hole)
        })
        .map(|(key, hole)| {
            let size = tile_size(key.level);
            let (cx, cz) = ((key.x as f32 + 0.5) * size, (key.z as f32 + 0.5) * size);
            (*key, *hole, (cx - position.x).hypot(cz - position.z))
        })
        .collect();
    missing.sort_by(|a, b| a.2.total_cmp(&b.2));
    let slots = FAR_JOBS_IN_FLIGHT.saturating_sub(state.pending.len());
    for (key, hole, _) in missing.into_iter().take(slots) {
        let world = Arc::clone(&world.0);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let (size, step) = if key.level == 1 { (LEVEL1_TILE, LEVEL1_STEP) } else { (LEVEL2_TILE, LEVEL2_STEP) };
            let tile = FarTile {
                origin_x: key.x as f32 * size,
                origin_z: key.z as f32 * size,
                cells: (size / step) as usize,
                step,
                skirt: if key.level == 1 { 12.0 } else { 40.0 },
            };
            let mesh = tile.build(&world, |x, z| hole.is_some_and(|h| x >= h.0 && x < h.2 && z >= h.1 && z < h.3));
            FarBuild { key, hole, mesh: (!mesh.is_empty()).then(|| chunk_mesh_to_bevy(mesh)) }
        });
        state.pending.insert(key, (task, hole));
    }

    // Nicht mehr gewünschte Kacheln erst entfernen, wenn nichts mehr aussteht.
    if state.pending.is_empty() && state.ready.is_empty() {
        state.tiles.retain(|key, (entity, _)| {
            if wanted.contains_key(key) {
                return true;
            }
            if let Some(entity) = entity {
                commands.entity(*entity).despawn();
            }
            false
        });
    }
}
