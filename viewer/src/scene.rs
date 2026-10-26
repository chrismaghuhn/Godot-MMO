//! Szene beim Start: Spawnpunkt, Kamera, Licht, Wasser, Materialien.

use bevy::{
    camera_controller::free_camera::FreeCamera, light::GlobalAmbientLight, prelude::*,
};
use terraforge_core::{
    spawn::{find_spawn, spawn_look},
    vegetation::{TreeStyle, build_tree_geometry},
};

use crate::controls::{AutoPilot, WalkMode, WalkState};
use crate::meshes::tree_geo_to_mesh;
use crate::streaming::{CHUNK_SIZE, ChunkAssets, FAR_GUARANTEED, FAR_VIEW};
use crate::{WorldRes, env_parse};

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
    // Mit Fernterrain (#33) reicht die Sicht bis ~3,5 km, sonst bis zum Rand der Nahzone.
    let far = env_parse::<u32>("TERRAFORGE_FAR").unwrap_or(1) != 0;
    // Nebel endet vor der garantierten Reichweite (Kamera am Feldrand), sonst sieht man die Kante.
    let fog_end = if far { FAR_GUARANTEED * 0.95 } else { world.0.cfg.view_radius as f32 * CHUNK_SIZE - 20.0 };
    let fog_start = if far { fog_end * 0.30 } else { fog_end * 0.55 };
    let camera_position = spawn + Vec3::new(0.0, 4.5 + camera_lift, 0.0);
    commands.insert_resource(WalkState(camera_position));
    let look_target = spawn_look(&world.0, spawn);
    let mut camera_transform =
        Transform::from_translation(camera_position).looking_at(look_target, Vec3::Y);
    // Fester Blickwinkel in Grad, z. B. zum Prüfen von Höhlen (Pitch negativ = nach unten).
    if let Some(pitch) = env_parse::<f32>("TERRAFORGE_PITCH") {
        let yaw = env_parse::<f32>("TERRAFORGE_YAW").unwrap_or(0.0);
        camera_transform.rotation =
            Quat::from_euler(EulerRot::YXZ, yaw.to_radians(), pitch.to_radians(), 0.0);
    }

    let mut camera = commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: FAR_VIEW * 2.0,
            ..default()
        }),
        camera_transform,
        FreeCamera {
            walk_speed: 32.0,
            run_speed: 180.0,
            ..default()
        },
    ));
    // Kopflampe für unter Tage, dort kommt kein Sonnenlicht hin.
    if std::env::var_os("TERRAFORGE_HEADLAMP").is_some() {
        camera.with_child(PointLight {
            intensity: 20_000_000.0,
            range: 90.0,
            shadow_maps_enabled: false,
            ..default()
        });
    }
    if std::env::var_os("TERRAFORGE_NO_FOG").is_none() {
        camera.insert(DistanceFog {
            color: Color::srgb(0.62, 0.74, 0.86),
            falloff: FogFalloff::Linear {
                start: fog_start,
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

    let water_size = FAR_VIEW * 3.0;
    let water_mesh = meshes.add(Plane3d::default().mesh().size(water_size, water_size));
    let water_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.10, 0.33, 0.45, 0.72),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.18,
        metallic: 0.15,
        ..default()
    });
    commands.spawn((
        Water,
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

/// Wasserfläche auf Meereshöhe; folgt der Kamera, damit sie bis zum Horizont reicht.
#[derive(Component)]
pub struct Water;

pub fn water_follows_camera(
    camera: Query<&Transform, (With<Camera3d>, Without<Water>)>,
    mut water: Query<&mut Transform, With<Water>>,
) {
    let (Ok(camera), Ok(mut water)) = (camera.single(), water.single_mut()) else {
        return;
    };
    // Auf 64 m runden, damit die Fläche nicht mit jedem Schritt "mitzittert".
    water.translation.x = (camera.translation.x / 64.0).round() * 64.0;
    water.translation.z = (camera.translation.z / 64.0).round() * 64.0;
}
