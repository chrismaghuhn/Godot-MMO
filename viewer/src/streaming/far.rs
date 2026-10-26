//! Fernterrain im Viewer (#33): zwei Stufen grober Kacheln um die Nahzone.
//!
//! - Stufe 1: 256-m-Kacheln mit 4-m-Raster, in den 3×3 Stufe-2-Feldern um die Kamera.
//!   Zellen in der Nahzone werden ausgespart.
//! - Stufe 2: 1024-m-Kacheln mit 16-m-Raster, in den 7×7 Feldern ohne die inneren 3×3.
//!
//! **Aussparung (Nahzone):** Ihr Mittelpunkt rastet auf `HOLE_SNAP` Chunks ein und folgt der
//! Kamera mit Hysterese (erst ab `HOLE_SLACK` Chunks Abstand), damit Randkacheln selten neu gebaut
//! werden. Übernommen wird eine Aussparung erst, wenn **alle Spalten darin geladen sind**; sonst
//! bleibt die alte (falls noch vollständig geladen) oder es gibt keine. Ohne Aussparung liegt
//! das Fernterrain etwas tiefer unter der Nahzone, man sieht also nie durch den Boden – auch
//! nicht beim Start, beim Teleport oder wenn die Spalten beim schnellen Flug nachhängen. Eine
//! alte Aussparung wird aufgegeben, bevor die Spalten darunter entfernt werden.
//!
//! **Austausch:** Eine nicht mehr gewünschte Kachel verschwindet erst, wenn alle gewünschten
//! Kacheln über ihrer Fläche fertig sind (keine Lücken, kaum Überlappung).
//!
//! Abschalten mit `TERRAFORGE_FAR=0`.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::lod::FarTile;

use super::{CHUNK_SIZE, ChunkAssets, ColumnMap, camera_chunk};
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
const FAR_JOBS_IN_FLIGHT: usize = 4;
/// Eine Kachel pro Frame: mehr führte bei Aussparungs-Wechseln zu Frames > 20 ms.
const FAR_INSERTS_PER_FRAME: usize = 1;
/// Raster (in Chunks), auf das die Aussparung einrastet.
const HOLE_SNAP: i32 = 4;
/// So weit (Chunks, Chebyshev) darf sich die Kamera vom Mittelpunkt entfernen, bevor er wandert.
const HOLE_SLACK: i32 = 3;

/// Sichtweite des Fernterrains von der Feldmitte (für Kamera-Sichtweite und Wasser).
pub const FAR_VIEW: f32 = LEVEL2_TILE * (LEVEL2_RING as f32 + 0.5);
/// Garantierte Reichweite von jeder Kameraposition aus (Kamera am Feldrand): Nebel endet davor.
pub const FAR_GUARANTEED: f32 = LEVEL2_TILE * LEVEL2_RING as f32;
/// Größter Sichtradius (Chunks), bei dem die Aussparung sicher in Stufe 1 liegt:
/// (radius + 1) · 32 m ≤ 1024 m Abstand von der Kamera zum Rand der Stufe 1.
pub const MAX_VIEW_RADIUS: i32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    level: u8,
    x: i32,
    z: i32,
}

/// Ausgesparter Bereich (Weltkoordinaten, halboffen) oder keiner. Werte sind ganzzahlige
/// Vielfache von 32 und damit exakt vergleichbar.
type Rect = (f32, f32, f32, f32);
type Hole = Option<Rect>;

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
    /// Eingerasteter Mittelpunkt (Chunks) und die gerade gültige Aussparung.
    snap: Option<IVec2>,
    hole: Option<(IVec2, Rect)>,
    /// Gezeichnete Kacheln mit der Aussparung, mit der sie gebaut wurden.
    tiles: HashMap<TileKey, (Option<Entity>, Hole)>,
    pending: HashMap<TileKey, (Task<FarBuild>, Hole)>,
    ready: Vec<FarBuild>,
}

impl FarState {
    pub(super) fn from_env() -> Self {
        Self {
            enabled: env_parse::<u32>("TERRAFORGE_FAR").unwrap_or(1) != 0,
            snap: None,
            hole: None,
            tiles: HashMap::new(),
            pending: HashMap::new(),
            ready: Vec::new(),
        }
    }
}

fn tile_size(level: u8) -> f32 {
    if level == 1 { LEVEL1_TILE } else { LEVEL2_TILE }
}

fn tile_rect(key: TileKey) -> Rect {
    let size = tile_size(key.level);
    let (x0, z0) = (key.x as f32 * size, key.z as f32 * size);
    (x0, z0, x0 + size, z0 + size)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1
}

/// Eingerasteter Mittelpunkt mit Hysterese: bleibt, solange die Kamera höchstens `HOLE_SLACK`
/// Chunks davon entfernt ist, sonst auf das nächste `HOLE_SNAP`-Raster.
fn snap_center(center: IVec2, current: Option<IVec2>) -> IVec2 {
    if let Some(current) = current
        && (center - current).abs().max_element() <= HOLE_SLACK
    {
        return current;
    }
    IVec2::new(
        (center.x as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
        (center.y as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
    )
}

/// Halbe Seitenlänge (Chunks) der Aussparung: Mit Abweichung ≤ `HOLE_SLACK` liegt sie eine Reihe
/// innerhalb der geladenen Spalten (Radius `view_radius`). `None`, wenn kein Platz ist.
fn hole_inner(view_radius: i32) -> Option<i32> {
    let inner = view_radius - 1 - HOLE_SLACK;
    (inner >= 0).then_some(inner)
}

/// Chunks der Aussparung um `snap`.
fn hole_chunks(snap: IVec2, inner: i32) -> impl Iterator<Item = IVec2> {
    (-inner..=inner).flat_map(move |dz| (-inner..=inner).map(move |dx| snap + IVec2::new(dx, dz)))
}

fn hole_rect(snap: IVec2, inner: i32) -> Rect {
    (
        (snap.x - inner) as f32 * CHUNK_SIZE,
        (snap.y - inner) as f32 * CHUNK_SIZE,
        (snap.x + inner + 1) as f32 * CHUNK_SIZE,
        (snap.y + inner + 1) as f32 * CHUNK_SIZE,
    )
}

/// Gewünschte Kacheln für eine Kameraposition und Aussparung.
fn wanted_tiles(position: Vec3, hole: Hole) -> HashMap<TileKey, Hole> {
    let field = ((position.x / LEVEL2_TILE).floor() as i32, (position.z / LEVEL2_TILE).floor() as i32);
    let per_field = (LEVEL2_TILE / LEVEL1_TILE) as i32;
    let mut wanted = HashMap::new();
    for dz in -LEVEL2_RING..=LEVEL2_RING {
        for dx in -LEVEL2_RING..=LEVEL2_RING {
            let (fx, fz) = (field.0 + dx, field.1 + dz);
            if dx.abs() <= LEVEL1_RING && dz.abs() <= LEVEL1_RING {
                for sz in 0..per_field {
                    for sx in 0..per_field {
                        let key = TileKey { level: 1, x: fx * per_field + sx, z: fz * per_field + sz };
                        let own_hole = hole.filter(|&h| overlaps(h, tile_rect(key)));
                        wanted.insert(key, own_hole);
                    }
                }
            } else {
                wanted.insert(TileKey { level: 2, x: fx, z: fz }, None);
            }
        }
    }
    wanted
}

pub(super) fn stream_far(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    world: Res<WorldRes>,
    assets: Res<ChunkAssets>,
    columns: Res<ColumnMap>,
    mut state: ResMut<FarState>,
) {
    if !state.enabled {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };
    let position = transform.translation;

    // Aussparung nur über vollständig geladenen Spalten.
    let center = camera_chunk(position).xz();
    let snap = snap_center(center, state.snap);
    state.snap = Some(snap);
    let loaded = |snap: IVec2, inner: i32| hole_chunks(snap, inner).all(|c| columns.0.contains_key(&c));
    state.hole = match hole_inner(world.0.cfg.view_radius) {
        Some(inner) if loaded(snap, inner) => Some((snap, hole_rect(snap, inner))),
        // Alte Aussparung nur behalten, solange die Kamera nah genug ist: Spalten werden erst ab
        // `view_radius + 2` entfernt, die alte Aussparung reicht bei ≤ HOLE_SLACK + 1 Abstand
        // höchstens bis `view_radius`. So bleiben 2 Chunks Zeit für den Neubau der Randkacheln,
        // bevor dahinter Spalten verschwinden (Review-Fund: sonst kurz ein Loch hinter der Kamera).
        Some(inner) => state
            .hole
            .filter(|(old, _)| (center - *old).abs().max_element() <= HOLE_SLACK + 1 && loaded(*old, inner)),
        None => None,
    };
    let wanted = wanted_tiles(position, state.hole.map(|(_, rect)| rect));

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
            state.tiles.get(key).map(|t| &t.1) != Some(hole)
                && state.pending.get(key).map(|p| &p.1) != Some(hole)
                && !state.ready.iter().any(|b| b.key == **key && b.hole == **hole)
        })
        .map(|(key, hole)| {
            let rect = tile_rect(*key);
            let (cx, cz) = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
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

    // Nicht mehr gewünschte Kacheln entfernen, sobald ihre Fläche von gewünschten, fertigen
    // Kacheln abgedeckt ist.
    let present: Vec<Rect> = state
        .tiles
        .keys()
        .filter(|key| wanted.contains_key(key))
        .map(|key| tile_rect(*key))
        .collect();
    let wanted_rects: Vec<Rect> = wanted.keys().map(|key| tile_rect(*key)).collect();
    state.tiles.retain(|key, (entity, _)| {
        if wanted.contains_key(key) {
            return true;
        }
        let rect = tile_rect(*key);
        let needed = wanted_rects.iter().filter(|r| overlaps(**r, rect)).count();
        let covered = present.iter().filter(|r| overlaps(**r, rect)).count();
        if covered < needed {
            return true;
        }
        if let Some(entity) = entity {
            commands.entity(*entity).despawn();
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Aussparung liegt für jede Kameraposition innerhalb der geladenen Spalten
    /// (Chebyshev-Radius `view_radius` um den Kamera-Chunk), auch mit Hysterese. Eine noch
    /// behaltene alte Aussparung (Abstand ≤ HOLE_SLACK + 1) reicht höchstens bis `view_radius`,
    /// also vor die Entfernungsgrenze der Spalten (`view_radius + 2`).
    #[test]
    fn hole_stays_inside_loaded_columns() {
        for view_radius in 0..=MAX_VIEW_RADIUS {
            let Some(inner) = hole_inner(view_radius) else {
                assert!(view_radius <= HOLE_SLACK, "Radius {view_radius} ohne Aussparung");
                continue;
            };
            let mut snap = None;
            // Kamera wandert in beiden Richtungen über viele Chunks.
            let path = (-40..40).chain((-40..40).rev()).map(|i| IVec2::new(i, -i / 2));
            for center in path {
                let s = snap_center(center, snap);
                snap = Some(s);
                for chunk in hole_chunks(s, inner) {
                    assert!((chunk - center).abs().max_element() < view_radius, "Radius {view_radius}: {chunk} außerhalb um {center}");
                }
                // Behaltene alte Aussparung: Mittelpunkt höchstens HOLE_SLACK + 1 von der Kamera.
                let slack = HOLE_SLACK + 1;
                for old in [IVec2::new(slack, slack), IVec2::new(-slack, slack), IVec2::new(slack, -slack), IVec2::new(-slack, -slack)] {
                    for chunk in hole_chunks(center + old, inner) {
                        // Spalten werden erst ab view_radius + 2 entfernt.
                        assert!((chunk - center).abs().max_element() <= view_radius, "alte Aussparung reicht zu weit");
                    }
                }
            }
        }
    }

    #[test]
    fn hysteresis_avoids_flipping_back_and_forth() {
        let mut snap = Some(snap_center(IVec2::new(2, 0), None));
        let first = snap;
        for x in [1, 2, 1, 2, 3, 2, 1] {
            snap = Some(snap_center(IVec2::new(x, 0), snap));
            assert_eq!(snap, first, "Mittelpunkt wandert bei kleinen Bewegungen");
        }
    }

    /// Stufe 1 und 2 decken lückenlos und ohne Überlappung ein Quadrat um die Kamera ab, und
    /// die Aussparung (bei maximalem Radius) liegt vollständig in Stufe 1.
    #[test]
    fn tiles_cover_the_area_without_gaps_or_overlaps() {
        for position in [Vec3::new(10.0, 0.0, 10.0), Vec3::new(1023.0, 0.0, -1.0), Vec3::new(-5000.0, 0.0, 3071.0)] {
            let center = camera_chunk(position).xz();
            let inner = hole_inner(MAX_VIEW_RADIUS).unwrap();
            let snap = snap_center(center, None);
            let hole = hole_rect(snap, inner);
            let wanted = wanted_tiles(position, Some(hole));
            let area: f32 = wanted.keys().map(|k| tile_size(k.level).powi(2)).sum();
            let side = LEVEL2_TILE * (2 * LEVEL2_RING + 1) as f32;
            assert_eq!(area, side * side, "Fläche passt nicht (Lücke oder Überlappung)");
            let keys: Vec<_> = wanted.keys().collect();
            for (i, a) in keys.iter().enumerate() {
                for b in &keys[i + 1..] {
                    assert!(!overlaps(tile_rect(**a), tile_rect(**b)), "Überlappung {a:?} {b:?}");
                }
            }
            let level1_hole_area: f32 = wanted
                .iter()
                .filter_map(|(k, h)| h.map(|h| (k, h)))
                .map(|(k, h)| {
                    let r = tile_rect(*k);
                    (r.2.min(h.2) - r.0.max(h.0)).max(0.0) * (r.3.min(h.3) - r.1.max(h.1)).max(0.0)
                })
                .sum();
            let hole_area = (hole.2 - hole.0) * (hole.3 - hole.1);
            assert_eq!(level1_hole_area, hole_area, "Aussparung ragt aus Stufe 1 heraus");
        }
    }
}
