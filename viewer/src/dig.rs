//! Graben und Bauen im Viewer (#6).
//!
//! - Linksklick: graben (Kugel, r = 2 m) mit dem aktuellen Werkzeug
//! - `B`: Erde bauen
//! - `T`: Werkzeug wechseln. Die Werkzeuge sind vorerst nur Härte-Stufen (echte Werkzeuge in #21).
//! - `X`: anvisierten Baum fällen (Vorstufe zur Axt, #21; Holz-Ausbeute folgt mit dem Inventar)
//!
//! Der Zielpunkt kommt aus einem Strahl von der Kamera, der Voxel abtastet (Änderungen plus
//! generierter Wert mit Untergrund – der Viewer ist Debug-Tool, D-005). Nach einer Änderung
//! werden die betroffenen Chunks zum Neubau markiert (`DirtyChunks`).
//!
//! `TERRAFORGE_AUTODIG=n` gräbt beim Start n-mal in Blickrichtung (zum Testen).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use terraforge_core::{
    MaterialId,
    edits::{Voxel, owner_chunk},
    volume::{CHUNK_SIZE, ChunkKind, ColumnData, DensityChunk},
};

use crate::streaming::{ColumnCache, DirtyChunks, Tree};
use crate::{EditsRes, UndergroundRes, WorldRes, env_parse};

const DIG_RADIUS: f32 = 2.0;
const BUILD_RADIUS: f32 = 1.5;
const REACH: f32 = 64.0;
const RAY_STEP: f32 = 0.25;
/// Werkzeug-Stufen: Name und höchste abbaubare Härte (Mohs, siehe `materials.ron`).
const TOOLS: [(&str, f32); 4] = [("Schaufel", 2.5), ("Feuersteinhacke", 4.0), ("Bronzehacke", 5.5), ("Eisenhacke", 7.0)];

pub struct DigPlugin;

impl Plugin for DigPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DigStatus { tool: TOOLS.len() - 1, ..default() })
            .init_resource::<VoxelCache>()
            .insert_resource(AutoDig { remaining: env_parse("TERRAFORGE_AUTODIG").unwrap_or(0), timer: 0.0 })
            .add_systems(Startup, spawn_crosshair)
            .add_systems(Update, (dig_input, fell_tree));
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
        let (name, hardness) = TOOLS[self.tool];
        format!("{name} (Härte ≤ {hardness})")
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
    fn voxel(&mut self, world: &WorldRes, underground: &UndergroundRes, columns: &ColumnCache, voxel: IVec3) -> Voxel {
        let owner = owner_chunk(voxel);
        if !self.0.contains_key(&owner) {
            if self.0.len() > 64 {
                self.0.clear();
            }
            let column = columns
                .0
                .get(&owner.xz())
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
    columns: Res<ColumnCache>,
    edits: Res<EditsRes>,
    mut cache: ResMut<VoxelCache>,
    mut dirty: ResMut<DirtyChunks>,
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
    let hit = {
        let edits = edits.0.read().expect("Edits-Lock vergiftet");
        let mut hit = None;
        let mut distance = 0.0;
        while distance < REACH {
            let point = origin + direction * distance;
            let voxel = point.round().as_ivec3();
            let value = edits.get(voxel).unwrap_or_else(|| cache.voxel(&world, &underground, &columns, voxel));
            if value.density > 0.0 {
                hit = Some(point);
                break;
            }
            distance += RAY_STEP;
        }
        hit
    };
    let Some(hit) = hit else {
        status.last = "kein Ziel in Reichweite".into();
        return;
    };
    let center = if dig { hit } else { hit - direction * (BUILD_RADIUS * 0.8) };
    let radius = if dig { DIG_RADIUS } else { BUILD_RADIUS } + 1.0;

    // Chunks der Kugel vorladen (teuer, ohne Lock); danach braucht das Graben nur noch den Cache.
    // Vorher Platz schaffen, damit der Cache nicht mitten im Vorladen geleert wird.
    if cache.0.len() > 48 {
        cache.0.clear();
    }
    let (min, max) = ((center - Vec3::splat(radius)).floor().as_ivec3(), (center + Vec3::splat(radius)).ceil().as_ivec3());
    for corner in [min, max, IVec3::new(min.x, min.y, max.z), IVec3::new(min.x, max.y, min.z), IVec3::new(max.x, min.y, min.z), IVec3::new(max.x, max.y, min.z), IVec3::new(max.x, min.y, max.z), IVec3::new(min.x, max.y, max.z)] {
        cache.voxel(&world, &underground, &columns, corner);
    }
    let mut edits = edits.0.write().expect("Edits-Lock vergiftet");
    let mut source = |voxel: IVec3| cache.voxel(&world, &underground, &columns, voxel);

    let affected = if dig {
        let (name, hardness) = TOOLS[status.tool];
        let result = edits.dig_sphere(&world.0.materials, &mut source, center, DIG_RADIUS, hardness);
        let mut parts: Vec<String> = result
            .removed
            .iter()
            .map(|(material, count)| {
                let label = world.0.materials.get(*material).map_or("?", |m| m.name.as_str());
                format!("{count}× {label}")
            })
            .collect();
        if result.too_hard > 0 {
            parts.push(format!("{} zu hart für {name}", result.too_hard));
        }
        status.last = if parts.is_empty() { "nichts abgebaut".into() } else { parts.join(", ") };
        result.affected
    } else {
        // Etwas vor die Oberfläche gesetzt (siehe `center`), damit die Kugel auf dem Boden sitzt.
        let result = edits.place_sphere(&mut source, center, BUILD_RADIUS, MaterialId::DIRT);
        status.last = format!("{} Erde gebaut", result.placed);
        result.affected
    };

    // Massive Chunks baut das Höhlen-Streaming neu, alle anderen die Spalte. Ist die Spalte
    // nicht geladen, geht die Markierung an die Spalten und verfällt dort (beim Laden wird die
    // Änderung ohnehin angewendet).
    for chunk in affected {
        let solid = columns.0.get(&chunk.xz()).is_some_and(|column| column.classify(chunk.y) == ChunkKind::Solid);
        if solid {
            dirty.caves.insert(chunk);
        } else {
            dirty.columns.insert(chunk.xz());
        }
    }
}

/// Fällt den Baum, auf den das Fadenkreuz zeigt (Abstand Strahl–Stamm < 1,2 m, Reichweite 32 m).
fn fell_tree(
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Transform, With<Camera3d>>,
    trees: Query<(&Tree, &GlobalTransform)>,
    edits: Res<EditsRes>,
    mut dirty: ResMut<DirtyChunks>,
    mut status: ResMut<DigStatus>,
) {
    if !keys.just_pressed(KeyCode::KeyX) {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };
    let (origin, direction) = (transform.translation, transform.forward().as_vec3());
    let mut best: Option<(f32, terraforge_core::vegetation::TreeId)> = None;
    for (tree, global) in &trees {
        let base = global.translation();
        let height = 5.0 * global.scale().y;
        // Kürzester Abstand zwischen Strahl und Stamm (senkrechte Strecke base..base+height),
        // über Abtastung entlang des Stamms.
        for i in 0..=10 {
            let point = base + Vec3::Y * height * i as f32 / 10.0;
            let along = (point - origin).dot(direction);
            if !(0.0..=32.0).contains(&along) {
                continue;
            }
            let miss = (origin + direction * along).distance(point);
            if miss < 1.2 && best.is_none_or(|(d, _)| along < d) {
                best = Some((along, tree.0));
            }
        }
    }
    let Some((_, id)) = best else {
        status.last = "kein Baum im Visier".into();
        return;
    };
    if edits.0.write().expect("Edits-Lock vergiftet").fell_tree(id) {
        dirty.columns.insert(IVec2::new(id.chunk_x, id.chunk_z));
        status.last = "Baum gefällt".into();
    }
}
