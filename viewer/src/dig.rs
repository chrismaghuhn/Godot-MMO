//! Graben und Bauen im Viewer (#6).
//!
//! - Linksklick: graben (Kugel, r = 2 m) mit dem aktuellen Werkzeug
//! - `B`: Erde bauen
//! - `T`: Werkzeug wechseln. Die Werkzeuge sind vorerst nur Härte-Stufen (echte Werkzeuge in #21).
//! - `X`: anvisierten Baum fällen (Vorstufe zur Axt, #21; Holz-Ausbeute folgt mit dem Inventar)
//!
//! Der Zielpunkt kommt aus einem Strahl von der Kamera, der Voxel abtastet (Änderungen plus
//! generierter Wert mit Untergrund – der Viewer ist Debug-Tool, D-005). Nach einer Änderung
//! werden die betroffenen Chunks beim Streamer zum Neubau markiert (`mark_changed`).
//!
//! `TERRAFORGE_AUTODIG=n` gräbt beim Start n-mal in Blickrichtung (zum Testen).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use terraforge_core::{
    edits::action::{self, Action, TOOLS},
    edits::{Voxel, WorldEdits, owner_chunk},
    volume::{CHUNK_SIZE, ColumnData, DensityChunk},
};

use crate::streaming::{StreamerRes, StreamingSet, Tree};
use crate::xray::XrayDirty;
use crate::{EditsRes, UndergroundRes, WorldRes, env_parse};

/// Reichweite als Debug-Tool (im Spiel `action::REACH`). Radien und Werkzeuge kommen aus dem Core.
const REACH: f32 = 64.0;

pub struct DigPlugin;

impl Plugin for DigPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DigStatus { tool: TOOLS.len() - 1, ..default() })
            .init_resource::<VoxelCache>()
            .insert_resource(AutoDig { remaining: env_parse("TERRAFORGE_AUTODIG").unwrap_or(0), timer: 0.0 })
            .add_systems(Startup, spawn_crosshair)
            .add_systems(Update, (dig_input, fell_tree).before(StreamingSet));
    }
}

/// Werkzeug und letzte Aktion, für das Overlay.
#[derive(Resource, Default)]
pub struct DigStatus {
    pub tool: usize,
    pub last: String,
}

impl DigStatus {
    pub fn tool_label(&self) -> String {
        let tool = TOOLS[self.tool];
        format!("{} (Härte ≤ {})", tool.name, tool.max_hardness)
    }
}

#[derive(Resource)]
struct AutoDig {
    remaining: u32,
    timer: f32,
}

/// Ungeänderte Chunks (mit Untergrund) für Strahl und Graben. Klein gehalten: Es geht nur um
/// die Umgebung des Zielpunkts.
#[derive(Resource, Default)]
struct VoxelCache(HashMap<IVec3, Arc<DensityChunk>>);

impl VoxelCache {
    fn voxel(&mut self, world: &WorldRes, underground: &UndergroundRes, columns: &StreamerRes, voxel: IVec3) -> Voxel {
        let owner = owner_chunk(voxel);
        if !self.0.contains_key(&owner) {
            if self.0.len() > 64 {
                self.0.clear();
            }
            let column = columns
                .column(owner.xz())
                .cloned()
                .unwrap_or_else(|| Arc::new(ColumnData::sample(&world.0, owner.x, owner.z)));
            let chunk = underground.0.materials_chunk(&world.0, &column, owner.y);
            self.0.insert(owner, Arc::new(chunk));
        }
        let chunk = &self.0[&owner];
        let local = voxel - owner * CHUNK_SIZE;
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    }
}

/// Erster Punkt entlang des Strahls mit festem Voxel (Änderung oder generiert) und sein Abstand.
fn ray_hit(origin: Vec3, direction: Vec3, reach: f32, edits: &WorldEdits, mut voxel_at: impl FnMut(IVec3) -> Voxel) -> Option<(Vec3, f32)> {
    action::ray_hit(origin, direction, reach, |voxel| edits.get(voxel).unwrap_or_else(|| voxel_at(voxel)).density > 0.0)
}

#[derive(Component)]
struct Crosshair;

fn spawn_crosshair(mut commands: Commands) {
    commands.spawn((
        Crosshair,
        Text::new("+"),
        TextFont { font_size: FontSize::Px(22.0), ..default() },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            margin: UiRect { left: px(-6), top: px(-13), ..default() },
            ..default()
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn dig_input(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Transform, With<Camera3d>>,
    world: Res<WorldRes>,
    underground: Res<UndergroundRes>,
    mut columns: ResMut<StreamerRes>,
    edits: Res<EditsRes>,
    mut cache: ResMut<VoxelCache>,
    mut xray_dirty: ResMut<XrayDirty>,
    mut status: ResMut<DigStatus>,
    mut auto: ResMut<AutoDig>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        status.tool = (status.tool + 1) % TOOLS.len();
    }
    // Auto-Graben erst, wenn die Umgebung geladen ist.
    let mut auto_dig = false;
    if auto.remaining > 0 && time.elapsed_secs() > 6.0 {
        auto.timer += time.delta_secs();
        if auto.timer > 0.4 {
            auto.timer = 0.0;
            auto.remaining -= 1;
            auto_dig = true;
        }
    }
    let dig = mouse.just_pressed(MouseButton::Left) || auto_dig;
    let build = keys.just_pressed(KeyCode::KeyB);
    if !dig && !build {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };

    // Strahl: erster Punkt mit festem Voxel (Änderung oder generiert). Nur Lese-Lock, damit
    // laufende Jobs nicht blockiert werden.
    let origin = transform.translation;
    let direction = transform.forward().as_vec3();
    let action = if dig { Action::Dig { tool: status.tool } } else { Action::Build };
    let center = {
        let edits = edits.0.read().expect("Edits-Lock vergiftet");
        let mut voxel_at = |v| cache.voxel(&world, &underground, &columns, v);
        action::target(action, origin, direction, REACH, |v| edits.get(v).unwrap_or_else(|| voxel_at(v)).density > 0.0)
    };
    let Some(center) = center else {
        status.last = "kein Ziel in Reichweite".into();
        return;
    };

    // Chunks der Kugel vorladen (teuer, ohne Lock); danach braucht das Graben nur noch den Cache.
    // Vorher Platz schaffen, damit der Cache nicht mitten im Vorladen geleert wird.
    if cache.0.len() > 48 {
        cache.0.clear();
    }
    let (min, max) = action::voxel_bounds(center, action);
    for corner in [min, max, IVec3::new(min.x, min.y, max.z), IVec3::new(min.x, max.y, min.z), IVec3::new(max.x, min.y, min.z), IVec3::new(max.x, max.y, min.z), IVec3::new(max.x, min.y, max.z), IVec3::new(min.x, max.y, max.z)] {
        cache.voxel(&world, &underground, &columns, corner);
    }
    let mut edits = edits.0.write().expect("Edits-Lock vergiftet");
    let source = |voxel: IVec3| cache.voxel(&world, &underground, &columns, voxel);

    let outcome = action::apply(&mut edits, &world.0.materials, source, center, action);
    status.last = action::summary(&outcome, &world.0.materials, action);
    let affected = outcome.changed();

    xray_dirty.0.extend(affected.iter().copied());
    columns.mark_changed(affected.iter().copied());
}

/// Reichweite fürs Fällen.
const FELL_REACH: f32 = 32.0;

/// Fällt den Baum, auf den das Fadenkreuz zeigt (Abstand Strahl–Stamm < 1,2 m). Bäume hinter
/// Terrain zählen nicht (Strahl endet am ersten festen Voxel). Der Baum verschwindet sofort;
/// die Spalte wird nicht neu gebaut, `felled` sorgt dafür, dass er bei Neubau/Neuladen fehlt.
#[allow(clippy::too_many_arguments)]
fn fell_tree(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Transform, With<Camera3d>>,
    trees: Query<(Entity, &Tree, &GlobalTransform)>,
    world: Res<WorldRes>,
    underground: Res<UndergroundRes>,
    columns: Res<StreamerRes>,
    edits: Res<EditsRes>,
    mut cache: ResMut<VoxelCache>,
    mut status: ResMut<DigStatus>,
) {
    if !keys.just_pressed(KeyCode::KeyX) {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };
    let (origin, direction) = (transform.translation, transform.forward().as_vec3());
    let terrain = {
        let edits = edits.0.read().expect("Edits-Lock vergiftet");
        ray_hit(origin, direction, FELL_REACH, &edits, |v| cache.voxel(&world, &underground, &columns, v))
    };
    let limit = terrain.map_or(FELL_REACH, |(_, distance)| distance + 0.5);
    let mut best: Option<(f32, Entity, terraforge_core::vegetation::TreeId)> = None;
    for (entity, tree, global) in &trees {
        let base = global.translation();
        let height = 5.0 * global.scale().y;
        for i in 0..=10 {
            let point = base + Vec3::Y * height * i as f32 / 10.0;
            let along = (point - origin).dot(direction);
            if !(0.0..=limit).contains(&along) {
                continue;
            }
            let miss = (origin + direction * along).distance(point);
            if miss < 1.2 && best.is_none_or(|(d, _, _)| along < d) {
                best = Some((along, entity, tree.0));
            }
        }
    }
    let Some((_, entity, id)) = best else {
        status.last = "kein Baum im Visier".into();
        return;
    };
    if edits.0.write().expect("Edits-Lock vergiftet").fell_tree(id) {
        // Kann schon mit seiner Spalte verschwinden; dann kein Fehler.
        commands.entity(entity).try_despawn();
        status.last = "Baum gefällt".into();
    }
}
