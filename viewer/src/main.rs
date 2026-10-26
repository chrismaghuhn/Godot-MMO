use std::collections::{HashMap, VecDeque};
use std::f32::consts::TAU;
use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, FreeCameraState},
    ecs::system::SystemParam,
    light::GlobalAmbientLight,
    mesh::Indices,
    prelude::*,
    render::{
        render_resource::PrimitiveTopology,
        view::screenshot::{Screenshot, save_to_disk},
    },
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
    window::WindowResolution,
};

use terraforge_core::{
    BiomeRef, FamilyKind, WorldGen,
    vegetation::{TreeGeo, TreePlacement, TreeStyle, build_tree_geometry, plan_trees},
};

const CHUNK_CELLS: usize = 32;
const CHUNK_SAMPLES: usize = CHUNK_CELLS + 3;
const CHUNK_SIZE: f32 = CHUNK_CELLS as f32;
const CHUNK_JOBS_IN_FLIGHT: usize = 8;
const CHUNKS_READY_PER_FRAME: usize = 3;
const CHUNK_DESPAWNS_PER_FRAME: usize = 4;

#[derive(Resource)]
struct WorldRes(Arc<WorldGen>);

#[derive(Resource)]
struct ChunkAssets {
    terrain_material: Handle<StandardMaterial>,
    tree_material: Handle<StandardMaterial>,
    tree_meshes: Vec<Handle<Mesh>>,
}

#[derive(Resource)]
struct SpawnPoint(Vec3);

#[derive(Resource, Default)]
struct WalkMode(bool);

#[derive(Resource, Default)]
struct WalkState(Vec3);

#[derive(Resource)]
struct AutoPilot(f32);

#[derive(Resource, Default)]
struct ChunkMap(HashMap<IVec2, Entity>);

struct ChunkBuild {
    position: IVec2,
    mesh: Mesh,
    trees: Vec<TreePlacement>,
}

#[derive(Resource, Default)]
struct ChunkJobs {
    pending: HashMap<IVec2, Task<ChunkBuild>>,
    ready: Vec<ChunkBuild>,
}

#[derive(Component)]
struct Tree;

#[derive(Component)]
struct PerfOverlay;

#[derive(Resource)]
struct PerfStats {
    frame_times: VecDeque<f32>,
    refresh: f32,
}

impl Default for PerfStats {
    fn default() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(240),
            refresh: 0.25,
        }
    }
}

#[derive(SystemParam)]
struct PerfContext<'w> {
    chunks: Res<'w, ChunkMap>,
    jobs: Res<'w, ChunkJobs>,
    world: Res<'w, WorldRes>,
    walk: Res<'w, WalkMode>,
    stats: ResMut<'w, PerfStats>,
}

#[derive(SystemParam)]
struct ChunkContext<'w> {
    world: Res<'w, WorldRes>,
    assets: Res<'w, ChunkAssets>,
    spawn: Res<'w, SpawnPoint>,
    chunks: ResMut<'w, ChunkMap>,
    jobs: ResMut<'w, ChunkJobs>,
}

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(42);

    let mut world = WorldGen::new(seed);
    if let Ok(value) = std::env::var("TERRAFORGE_VIEW_RADIUS")
        && let Ok(radius) = value.parse()
    {
        world.cfg.view_radius = radius;
    }
    let window_width = std::env::var("TERRAFORGE_WIN_W")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1600);
    let window_height = std::env::var("TERRAFORGE_WIN_H")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(900);

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.58, 0.72, 0.86)))
        .insert_resource(WorldRes(Arc::new(world)))
        .init_resource::<ChunkMap>()
        .init_resource::<ChunkJobs>()
        .init_resource::<PerfStats>()
        .init_resource::<ShotState>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("terraforge | Seed {seed}"),
                resolution: WindowResolution::new(window_width, window_height),
                present_mode: match std::env::var("TERRAFORGE_PRESENT").as_deref() {
                    Ok("fifo") => bevy::window::PresentMode::Fifo,
                    Ok("mailbox") => bevy::window::PresentMode::Mailbox,
                    Ok("immediate") => bevy::window::PresentMode::Immediate,
                    _ => bevy::window::PresentMode::AutoVsync,
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FreeCameraPlugin)
        .insert_resource(FrameCap::from_env())
        .add_systems(Last, frame_cap)
        .add_systems(Startup, (setup_scene, setup_perf_overlay))
        .add_systems(
            Update,
            (
                stream_chunks,
                walk_controller,
                auto_pilot,
                perf_overlay,
                auto_screenshot,
            ),
        )
        .run();
}

fn setup_scene(
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

fn setup_perf_overlay(mut commands: Commands) {
    commands.spawn((
        PerfOverlay,
        Text::new("FPS ..."),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(0.55, 1.0, 0.55)),
        TextLayout::no_wrap(),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(10),
            padding: UiRect::all(px(8)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.72)),
        ZIndex(100),
    ));
}

fn perf_overlay(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    trees: Query<(), With<Tree>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut overlay: Query<(&mut Text, &mut TextColor, &mut Visibility), With<PerfOverlay>>,
    mut ctx: PerfContext,
) {
    let delta_ms = time.delta_secs() * 1000.0;
    ctx.stats.frame_times.push_back(delta_ms);
    if ctx.stats.frame_times.len() > 240 {
        ctx.stats.frame_times.pop_front();
    }
    ctx.stats.refresh += time.delta_secs();

    let Ok((mut text, mut color, mut visibility)) = overlay.single_mut() else {
        return;
    };
    if keys.just_pressed(KeyCode::F3) {
        *visibility = if *visibility == Visibility::Hidden {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if *visibility == Visibility::Hidden || ctx.stats.refresh < 0.25 {
        return;
    }
    ctx.stats.refresh = 0.0;

    let mut sorted: Vec<f32> = ctx.stats.frame_times.iter().copied().collect();
    sorted.sort_unstable_by(f32::total_cmp);
    let avg = ctx.stats.frame_times.iter().sum::<f32>() / ctx.stats.frame_times.len() as f32;
    let p95 = sorted[((sorted.len() as f32 * 0.95) as usize).min(sorted.len() - 1)];
    let max = *sorted.last().unwrap();
    let recent = ctx.stats.frame_times.len().min(30);
    let recent_avg = ctx.stats.frame_times.iter().rev().take(recent).sum::<f32>() / recent as f32;
    let fps = 1000.0 / recent_avg.max(0.001);
    let position = camera.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    let mode = if ctx.walk.0 { "Lauf" } else { "Flug" };
    // Biom-Infos stehen hier statt im Fenstertitel: Titel-Updates blockieren
    // unter i3 + picom + NVIDIA jedes Mal ~80 ms.
    let sample = ctx.world.0.sample(position.x, position.z);
    let biome = ctx.world.0.biome_at(&sample);

    text.0 = format!(
        "FPS {:>4.0}   Frame {:>5.2} ms   p95 {:>5.2} ms   max {:>5.2} ms\n\
         Chunks {:>3}   Jobs {:>1}+{:<1}   Trees {:>5}   Seed {}\n\
         X {:>7.1}   Y {:>6.1}   Z {:>7.1}   {}   [F3]\n\
         {} ({})   Boden {:.0}m   T{:+.2}   H{:+.2}",
        fps,
        avg,
        p95,
        max,
        ctx.chunks.0.len(),
        ctx.jobs.pending.len(),
        ctx.jobs.ready.len(),
        trees.iter().count(),
        ctx.world.0.cfg.seed,
        position.x,
        position.y,
        position.z,
        mode,
        ctx.world.0.biome_name(biome),
        ctx.world.0.family(biome.kind).label,
        sample.height,
        sample.temperature,
        sample.humidity,
    );
    color.0 = if p95 <= 16.7 {
        Color::srgb(0.55, 1.0, 0.55)
    } else if p95 <= 25.0 {
        Color::srgb(1.0, 0.85, 0.35)
    } else {
        Color::srgb(1.0, 0.45, 0.4)
    };
}

fn walk_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    world: Res<WorldRes>,
    mut mode: ResMut<WalkMode>,
    mut state: ResMut<WalkState>,
    mut camera: Query<(&mut Transform, &FreeCameraState), With<Camera3d>>,
) {
    let Ok((mut transform, free)) = camera.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::KeyF) {
        mode.0 = !mode.0;
        if mode.0 {
            state.0 = transform.translation;
        }
    }
    if !mode.0 {
        return;
    }

    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }

    if input != Vec2::ZERO {
        let yaw = free.yaw;
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let direction = (forward * input.y + right * input.x).normalize_or_zero();
        let sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let speed = if sprint { 9.0 } else { 4.5 };
        state.0.x += direction.x * speed * time.delta_secs();
        state.0.z += direction.z * speed * time.delta_secs();
    }

    let ground = world.0.column_height(state.0.x, state.0.z);
    let floor = ground.max(world.0.cfg.sea_level - 0.9);
    state.0.y = floor + 1.7;
    transform.translation = state.0;
}

fn auto_pilot(
    time: Res<Time>,
    pilot: Option<Res<AutoPilot>>,
    world: Res<WorldRes>,
    walk: Res<WalkMode>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    let Some(pilot) = pilot else {
        return;
    };
    if walk.0 {
        return;
    }
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    transform.translation.x += pilot.0 * time.delta_secs();
    let ground = world
        .0
        .column_height(transform.translation.x, transform.translation.z);
    transform.translation.y = ground + 32.0;
}

/// Software-Framelimiter. VSync greift auf NVIDIA + X11 ohne Compositor
/// nicht zuverlässig, dann rendert das Spiel mit ~1000 FPS und tearing.
#[derive(Resource)]
struct FrameCap {
    target: Option<std::time::Duration>,
    last: std::time::Instant,
}

impl FrameCap {
    fn from_env() -> Self {
        let target = std::env::var("TERRAFORGE_FPS_CAP")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|fps| *fps > 0.0)
            .map(|fps| std::time::Duration::from_secs_f64(1.0 / fps));
        Self {
            target,
            last: std::time::Instant::now(),
        }
    }
}

fn frame_cap(mut cap: ResMut<FrameCap>) {
    let Some(target) = cap.target else {
        return;
    };
    let deadline = cap.last + target;
    let spin_margin = std::time::Duration::from_micros(1500);
    let now = std::time::Instant::now();
    if deadline > now + spin_margin {
        std::thread::sleep(deadline - now - spin_margin);
    }
    while std::time::Instant::now() < deadline {
        std::hint::spin_loop();
    }
    let now = std::time::Instant::now();
    // Nach einem langsamen Frame nicht "aufholen", sonst gibt es Bursts.
    cap.last = if now > deadline + target {
        now
    } else {
        deadline
    };
}

#[derive(Resource, Default)]
struct ShotState {
    frame: u32,
    requested: bool,
}

fn auto_screenshot(mut commands: Commands, mut state: ResMut<ShotState>) {
    if std::env::var_os("TERRAFORGE_SHOT").is_none() || state.requested {
        return;
    }
    state.frame += 1;
    let after: u32 = std::env::var("TERRAFORGE_SHOT_AFTER")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(240);
    if state.frame >= after {
        std::fs::create_dir_all("shots").ok();
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("shots/auto.png"));
        state.requested = true;
    }
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
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let samples = sample_chunk(&world, position.x, position.y);
            let mesh = build_terrain_mesh(&world, &samples);
            let mut trees = plan_trees(
                world.cfg.seed,
                world.cfg.sea_level,
                position.x,
                position.y,
                &samples.heights,
                &samples.biomes,
            );
            trees.retain(|tree| {
                let world_x = position.x as f32 * CHUNK_SIZE + tree.position.x;
                let world_z = position.y as f32 * CHUNK_SIZE + tree.position.z;
                Vec2::new(world_x - clearing.x, world_z - clearing.z).length() > 17.0
            });
            ChunkBuild {
                position,
                mesh,
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
        mesh,
        trees,
    } = build;
    let mut entity = commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(assets.terrain_material.clone()),
        Transform::from_xyz(
            position.x as f32 * CHUNK_SIZE,
            0.0,
            position.y as f32 * CHUNK_SIZE,
        ),
    ));
    let chunk_entity = entity.id();

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

struct ChunkSamples {
    heights: [[f32; CHUNK_SAMPLES]; CHUNK_SAMPLES],
    biomes: [[BiomeRef; CHUNK_SAMPLES]; CHUNK_SAMPLES],
}

fn sample_chunk(world: &WorldGen, chunk_x: i32, chunk_z: i32) -> ChunkSamples {
    let default_biome = BiomeRef {
        kind: FamilyKind::Grassland,
        variant: 0,
    };
    let mut samples = ChunkSamples {
        heights: [[0.0; CHUNK_SAMPLES]; CHUNK_SAMPLES],
        biomes: [[default_biome; CHUNK_SAMPLES]; CHUNK_SAMPLES],
    };

    for sample_z in 0..CHUNK_SAMPLES {
        for sample_x in 0..CHUNK_SAMPLES {
            let local_x = sample_x as f32 - 1.0;
            let local_z = sample_z as f32 - 1.0;
            let world_x = chunk_x as f32 * CHUNK_SIZE + local_x;
            let world_z = chunk_z as f32 * CHUNK_SIZE + local_z;
            let sample = world.sample(world_x, world_z);
            samples.heights[sample_z][sample_x] = sample.height;
            let interior = (1..CHUNK_SAMPLES - 1).contains(&sample_x)
                && (1..CHUNK_SAMPLES - 1).contains(&sample_z);
            if interior {
                samples.biomes[sample_z][sample_x] = world.biome_at(&sample);
            }
        }
    }

    samples
}

fn build_terrain_mesh(world: &WorldGen, samples: &ChunkSamples) -> Mesh {
    const MESH_VERTS: usize = CHUNK_CELLS + 1;
    let heights = &samples.heights;

    let vertex_count = MESH_VERTS * MESH_VERTS;
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut vertex_colors = Vec::with_capacity(vertex_count);

    for z in 0..MESH_VERTS {
        for x in 0..MESH_VERTS {
            let sx = x + 1;
            let sz = z + 1;
            positions.push([x as f32, heights[sz][sx], z as f32]);
            let left = heights[sz][sx - 1];
            let right = heights[sz][sx + 1];
            let down = heights[sz - 1][sx];
            let up = heights[sz + 1][sx];
            let normal = Vec3::new(left - right, 2.0, down - up).normalize();
            normals.push([normal.x, normal.y, normal.z]);

            let biome = samples.biomes[sz][sx];
            let rgb = world.apply_altitude(world.biome_color(biome), heights[sz][sx]);
            let linear = srgb_to_linear(rgb);
            vertex_colors.push([linear[0], linear[1], linear[2], 1.0]);
        }
    }

    let mut indices = Vec::with_capacity(CHUNK_CELLS * CHUNK_CELLS * 6);
    for z in 0..CHUNK_CELLS {
        for x in 0..CHUNK_CELLS {
            let a = (z * MESH_VERTS + x) as u32;
            let b = a + 1;
            let c = a + MESH_VERTS as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vertex_colors)
    .with_inserted_indices(Indices::U32(indices))
}

fn tree_mesh_index(style: TreeStyle, variant: usize) -> usize {
    style.index() * 3 + variant
}

fn tree_geo_to_mesh(geo: TreeGeo) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, geo.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, geo.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, geo.colors)
    .with_inserted_indices(Indices::U32(geo.indices))
}

fn srgb_to_linear(color: [u8; 3]) -> [f32; 3] {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(color[0]), channel(color[1]), channel(color[2])]
}
