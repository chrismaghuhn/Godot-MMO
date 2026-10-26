//! F3-Overlay mit Frametimes, Chunk-Zahlen, Position und Biom.

use std::collections::VecDeque;

use bevy::{ecs::system::SystemParam, prelude::*};

use crate::controls::WalkMode;
use crate::streaming::{CaveChunk, CaveJobs, ColumnJobs, ColumnMap, TerrainChunk, Tree};
use crate::WorldRes;

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerfStats>()
            .add_systems(Startup, setup_perf_overlay)
            .add_systems(Update, perf_overlay);
    }
}

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
    chunks: Res<'w, ColumnMap>,
    jobs: Res<'w, ColumnJobs>,
    cave_jobs: Res<'w, CaveJobs>,
    world: Res<'w, WorldRes>,
    walk: Res<'w, WalkMode>,
    stats: ResMut<'w, PerfStats>,
}

/// Zählt Bäume, Terrain-Chunks und Höhlen-Chunks für die Anzeige.
#[derive(SystemParam)]
struct EntityCounts<'w, 's> {
    trees: Query<'w, 's, (), With<Tree>>,
    terrain: Query<'w, 's, (), With<TerrainChunk>>,
    caves: Query<'w, 's, (), With<CaveChunk>>,
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
    counts: EntityCounts,
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
         Spalten {:>3}   3D-Chunks {:>4} (Hoehlen {:>3})   Jobs {:>1}+{:<1} / {:<1}   Trees {:>5}   Seed {}\n\
         X {:>7.1}   Y {:>6.1}   Z {:>7.1}   {}   [F3]\n\
         {} ({})   Boden {:.0}m   T{:+.2}   H{:+.2}",
        fps,
        avg,
        p95,
        max,
        ctx.chunks.0.len(),
        counts.terrain.iter().count(),
        counts.caves.iter().count(),
        ctx.jobs.pending_count(),
        ctx.jobs.ready_count(),
        ctx.cave_jobs.pending_count(),
        counts.trees.iter().count(),
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
