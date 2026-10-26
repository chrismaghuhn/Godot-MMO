//! Szene beim Start: Spawnpunkt, Kamera, Licht, Wasser, Materialien.

use std::f32::consts::TAU;

use bevy::{
    camera_controller::free_camera::FreeCamera, light::GlobalAmbientLight, prelude::*,
};
use terraforge_core::{
    WorldGen,
    vegetation::{TreeStyle, build_tree_geometry},
};

use crate::controls::{AutoPilot, WalkMode, WalkState};
use crate::meshes::tree_geo_to_mesh;
use crate::streaming::{CHUNK_SIZE, ChunkAssets};
use crate::WorldRes;

#[derive(Resource)]
pub struct SpawnPoint(pub Vec3);

pub fn setup_scene(
    mut commands: Commands,
    world: Res<WorldRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sea = world.0.cfg.sea_level;
    let mut spawn = find_spawn(&world.0);
    if let (Some(x), Some(z)) = (
        std::env::var("TERRAFORGE_SPAWN_X")
            .ok()
            .and_then(|value| value.parse::<f32>().ok()),
        std::env::var("TERRAFORGE_SPAWN_Z")
            .ok()
            .and_then(|value| value.parse::<f32>().ok()),
    ) {
        spawn.x = x;
        spawn.z = z;
        spawn.y = world.0.column_height(x, z);
    }
    commands.insert_resource(SpawnPoint(spawn));
    let start_walking = std::env::var_os("TERRAFORGE_WALK").is_some();
    commands.insert_resource(WalkMode(start_walking));
    if let Ok(value) = std::env::var("TERRAFORGE_AUTOPILOT")
        && let Ok(speed) = value.parse()
    {
        commands.insert_resource(AutoPilot(speed));
    }
    bevy::log::info!("F = Laufmodus, Shift = rennen | Flugmodus: WASD + Q/E, Shift schnell");
    let camera_lift = std::env::var("TERRAFORGE_CAM_Y")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    let fog_end = world.0.cfg.view_radius as f32 * CHUNK_SIZE - 20.0;
    let camera_position = spawn + Vec3::new(0.0, 4.5 + camera_lift, 0.0);
    commands.insert_resource(WalkState(camera_position));
    let look_target = spawn_look(&world.0, spawn);

    let mut camera = commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(look_target, Vec3::Y),
        FreeCamera {
            walk_speed: 32.0,
            run_speed: 180.0,
            ..default()
        },
    ));
    if std::env::var_os("TERRAFORGE_NO_FOG").is_none() {
        camera.insert(DistanceFog {
            color: Color::srgb(0.62, 0.74, 0.86),
            falloff: FogFalloff::Linear {
                start: fog_end * 0.55,
                end: fog_end,
            },
            ..default()
        });
    }

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 2.2, -0.72, 0.0)),
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.82, 0.96),
        brightness: 220.0,
        ..default()
    });

    let water_mesh = meshes.add(Plane3d::default().mesh().size(8000.0, 8000.0));
    let water_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.10, 0.33, 0.45, 0.72),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.18,
        metallic: 0.15,
        ..default()
    });
    commands.spawn((
        Mesh3d(water_mesh),
        MeshMaterial3d(water_material),
        Transform::from_xyz(0.0, sea, 0.0),
    ));

    let terrain_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.93,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });
    let tree_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.88,
        metallic: 0.0,
        reflectance: 0.0,
        double_sided: true,
        ..default()
    });
    let mut tree_meshes = Vec::with_capacity(TreeStyle::ALL.len() * 3);
    for style in TreeStyle::ALL {
        for variant in 0..3 {
            tree_meshes.push(meshes.add(tree_geo_to_mesh(build_tree_geometry(style, variant))));
        }
    }
    commands.insert_resource(ChunkAssets {
        terrain_material,
        tree_material,
        tree_meshes,
    });
}

fn find_spawn(world: &WorldGen) -> Vec3 {
    let sea = world.cfg.sea_level;
    let mut best = Vec3::new(0.0, sea + 2.0, 0.0);
    let mut best_score = f32::INFINITY;
    for ring in 0..60 {
        let radius = ring as f32 * 12.0;
        for step in 0..24 {
            let angle = step as f32 / 24.0 * TAU;
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            let height = world.column_height(x, z);
            if height < sea + 4.0 || height > sea + 55.0 {
                continue;
            }
            let slope = (height - world.column_height(x + 6.0, z)).abs()
                + (height - world.column_height(x, z + 6.0)).abs();
            let score = slope * 4.0 + (height - sea) * 0.08 + radius * 0.002;
            if score < best_score {
                best_score = score;
                best = Vec3::new(x, height, z);
            }
        }
    }
    best
}

fn spawn_look(world: &WorldGen, spawn: Vec3) -> Vec3 {
    let mut best_angle = 0.0f32;
    let mut best_height = f32::INFINITY;
    for step in 0..16 {
        let angle = step as f32 / 16.0 * TAU;
        let height =
            world.column_height(spawn.x + angle.cos() * 70.0, spawn.z + angle.sin() * 70.0);
        if height < best_height {
            best_height = height;
            best_angle = angle;
        }
    }
    let x = spawn.x + best_angle.cos() * 70.0;
    let z = spawn.z + best_angle.sin() * 70.0;
    Vec3::new(x, best_height.max(spawn.y) + 6.0, z)
}
