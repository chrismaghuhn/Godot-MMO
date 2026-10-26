//! Röntgenmodus (#47, nur Debug): Taste `R` macht das Terrain durchsichtig und zeigt Erze im
//! Umkreis von einem Chunk um die Kamera (3×3×3 Chunks ≈ 100 m) als farbige Würfel.
//!
//! Gezeigt werden nur Erz-Voxel am Rand eines Vorkommens (mindestens ein Nachbar ist kein Erz);
//! das hält die Meshes klein. Die Daten kommen mit Untergrund und Spieler-Änderungen – der Viewer
//! ist ein Debug-Werkzeug (D-005); ein Spiel-Client hätte diese Daten nicht.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::{
    underground::OreKind,
    volume::{CHUNK_SIZE, ColumnData},
};

use crate::streaming::{ChunkAssets, ColumnCache};
use crate::{EditsRes, UndergroundRes, WorldRes};

const RADIUS: i32 = 1;
const CUBE: f32 = 0.45;

pub struct XrayPlugin;

impl Plugin for XrayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Xray>()
            .insert_resource(XrayStart(crate::env_parse::<u32>("TERRAFORGE_XRAY").unwrap_or(0) != 0))
            .add_systems(Update, xray);
    }
}

#[derive(Resource, Default)]
pub struct Xray {
    pub enabled: bool,
    markers: HashMap<IVec3, Option<Entity>>,
    pending: HashMap<IVec3, Task<Option<Mesh>>>,
    material: Option<Handle<StandardMaterial>>,
}

#[derive(Component)]
struct OreMarker;

/// `TERRAFORGE_XRAY=1`: Röntgenmodus beim Start einschalten (zum Testen).
#[derive(Resource)]
struct XrayStart(bool);

#[allow(clippy::too_many_arguments)]
fn xray(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Transform, With<Camera3d>>,
    world: Res<WorldRes>,
    underground: Res<UndergroundRes>,
    edits: Res<EditsRes>,
    columns: Res<ColumnCache>,
    assets: Res<ChunkAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut state: ResMut<Xray>,
    mut start: ResMut<XrayStart>,
) {
    let start_now = std::mem::take(&mut start.0);
    if keys.just_pressed(KeyCode::KeyR) || start_now {
        state.enabled = !state.enabled;
        // Terrain durchsichtig bzw. wieder deckend.
        if let Some(mut terrain) = materials.get_mut(&assets.terrain_material) {
            let alpha = if state.enabled { 0.18 } else { 1.0 };
            terrain.base_color = Color::srgba(1.0, 1.0, 1.0, alpha);
            terrain.alpha_mode = if state.enabled { AlphaMode::Blend } else { AlphaMode::Opaque };
        }
        if !state.enabled {
            for entity in state.markers.drain().filter_map(|(_, e)| e) {
                commands.entity(entity).despawn();
            }
            state.pending.clear();
            return;
        }
    }
    if !state.enabled {
        return;
    }
    let Ok(transform) = camera.single() else {
        return;
    };
    let material = state
        .material
        .get_or_insert_with(|| materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() }))
        .clone();
    let center = (transform.translation / CHUNK_SIZE as f32).floor().as_ivec3();
    let wanted: HashSet<IVec3> = (-RADIUS..=RADIUS)
        .flat_map(|dz| (-RADIUS..=RADIUS).flat_map(move |dy| (-RADIUS..=RADIUS).map(move |dx| center + IVec3::new(dx, dy, dz))))
        .collect();

    // Fertige Jobs einsetzen, entfernte Marker abbauen.
    let mut finished = Vec::new();
    state.pending.retain(|&position, task| match block_on(poll_once(task)) {
        Some(mesh) => {
            finished.push((position, mesh));
            false
        }
        None => true,
    });
    for (position, mesh) in finished {
        if !wanted.contains(&position) {
            continue;
        }
        let entity = mesh.map(|mesh| {
            commands
                .spawn((OreMarker, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_translation(position.as_vec3() * CHUNK_SIZE as f32)))
                .id()
        });
        state.markers.insert(position, entity);
    }
    let stale: Vec<IVec3> = state.markers.keys().filter(|p| !wanted.contains(p)).copied().collect();
    for position in stale {
        if let Some(Some(entity)) = state.markers.remove(&position) {
            commands.entity(entity).despawn();
        }
    }

    // Neue Jobs.
    for position in wanted {
        if state.markers.contains_key(&position) || state.pending.contains_key(&position) {
            continue;
        }
        let (world, underground, edits) = (Arc::clone(&world.0), Arc::clone(&underground.0), Arc::clone(&edits.0));
        let column = columns.0.get(&position.xz()).cloned();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let column = column.unwrap_or_else(|| Arc::new(ColumnData::sample(&world, position.x, position.z)));
            let mut chunk = underground.materials_chunk(&world, &column, position.y);
            edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
            ore_mesh(&world.materials, &chunk)
        });
        state.pending.insert(position, task);
    }
}

/// Würfel für Erz-Voxel am Rand ihres Vorkommens (lokale Koordinaten 0..32).
fn ore_mesh(materials: &terraforge_core::MaterialRegistry, chunk: &terraforge_core::volume::DensityChunk) -> Option<Mesh> {
    let is_ore = |x: i32, y: i32, z: i32| OreKind::from_material(chunk.material(x, y, z)).is_some();
    let (mut positions, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if !is_ore(x, y, z) {
                    continue;
                }
                let inner = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .all(|&(dx, dy, dz)| is_ore(x + dx, y + dy, z + dz));
                if inner {
                    continue;
                }
                let rgb = materials.display_color(chunk.material(x, y, z));
                let color = [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0];
                let c = Vec3::new(x as f32, y as f32, z as f32);
                let base = positions.len() as u32;
                for corner in 0..8 {
                    let offset = Vec3::new(
                        if corner & 1 == 0 { -CUBE } else { CUBE },
                        if corner & 2 == 0 { -CUBE } else { CUBE },
                        if corner & 4 == 0 { -CUBE } else { CUBE },
                    );
                    positions.push((c + offset).to_array());
                    colors.push(color);
                }
                // 12 Dreiecke, beidseitig nicht nötig (unbeleuchtet, von außen gesehen).
                for face in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
                    indices.extend_from_slice(&[base + face[0], base + face[1], base + face[2], base + face[0], base + face[2], base + face[3]]);
                }
            }
        }
    }
    if indices.is_empty() {
        return None;
    }
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(indices)),
    )
}
