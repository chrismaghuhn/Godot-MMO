//! terraforge-viewer: Debug-Ansicht für terraforge-core.
//!
//! Module (je ein Bevy-Plugin bzw. Thema):
//! - `scene`:     Start-Szene (Spawnpunkt, Kamera, Licht, Wasser, Materialien)
//! - `streaming`: Chunk-Streaming um die Kamera
//! - `meshes`:    Core-Meshes → Bevy-Meshes
//! - `controls`:  Laufmodus und Autopilot
//! - `overlay`:   F3-Overlay
//! - `devtools`:  Framelimiter, Auto-Screenshot
//!
//! Umgebungsvariablen: `TERRAFORGE_VIEW_RADIUS`, `TERRAFORGE_WIN_W/H`, `TERRAFORGE_PRESENT`,
//! `TERRAFORGE_SPAWN_X/Z`, `TERRAFORGE_CAM_Y`, `TERRAFORGE_WALK`, `TERRAFORGE_AUTOPILOT`,
//! `TERRAFORGE_NO_FOG`, `TERRAFORGE_FPS_CAP`, `TERRAFORGE_SHOT`, `TERRAFORGE_SHOT_AFTER`.

mod controls;
mod devtools;
mod meshes;
mod overlay;
mod scene;
mod streaming;

use std::sync::Arc;

use bevy::{
    camera_controller::free_camera::FreeCameraPlugin,
    prelude::*,
    window::{PresentMode, WindowResolution},
};
use terraforge_core::WorldGen;

/// Die Weltgenerierung, geteilt mit den Hintergrund-Jobs.
#[derive(Resource)]
pub struct WorldRes(pub Arc<WorldGen>);

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(42);

    let mut world = WorldGen::new(seed);
    if let Some(radius) = env_parse("TERRAFORGE_VIEW_RADIUS") {
        world.cfg.view_radius = radius;
    }
    let window_width = env_parse("TERRAFORGE_WIN_W").unwrap_or(1600);
    let window_height = env_parse("TERRAFORGE_WIN_H").unwrap_or(900);

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.58, 0.72, 0.86)))
        .insert_resource(WorldRes(Arc::new(world)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("terraforge | Seed {seed}"),
                resolution: WindowResolution::new(window_width, window_height),
                present_mode: present_mode_from_env(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            FreeCameraPlugin,
            streaming::StreamingPlugin,
            controls::ControlsPlugin,
            overlay::OverlayPlugin,
            devtools::DevToolsPlugin,
        ))
        .add_systems(Startup, scene::setup_scene)
        .run();
}

fn present_mode_from_env() -> PresentMode {
    match std::env::var("TERRAFORGE_PRESENT").as_deref() {
        Ok("fifo") => PresentMode::Fifo,
        Ok("mailbox") => PresentMode::Mailbox,
        Ok("immediate") => PresentMode::Immediate,
        _ => PresentMode::AutoVsync,
    }
}

/// Liest eine Umgebungsvariable und parst sie; `None`, wenn sie fehlt oder ungültig ist.
pub fn env_parse<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|value| value.parse().ok())
}
