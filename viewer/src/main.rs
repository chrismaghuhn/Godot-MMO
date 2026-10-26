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
//! `TERRAFORGE_NO_FOG`, `TERRAFORGE_FPS_CAP`, `TERRAFORGE_SHOT`, `TERRAFORGE_SHOT_AFTER`,
//! `TERRAFORGE_SECRET_SEED` (geheimer Untergrund-Seed; ohne: unsicherer Entwickler-Seed),
//! `TERRAFORGE_CAVE_RADIUS`, `TERRAFORGE_PITCH`/`TERRAFORGE_YAW` (Grad), `TERRAFORGE_HEADLAMP`.

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
use terraforge_core::{
    WorldGen,
    underground::{SecretSeed, Underground},
};

/// Die öffentliche Weltgenerierung, geteilt mit den Hintergrund-Jobs.
#[derive(Resource)]
pub struct WorldRes(pub Arc<WorldGen>);

/// Der geheime Untergrund (D-005). Im Spiel-Client gibt es das nicht, dort kommt der
/// Untergrund vom Server; der Viewer erzeugt ihn als Debug-Tool selbst.
#[derive(Resource)]
pub struct UndergroundRes(pub Arc<Underground>);

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(42);

    let mut world = WorldGen::new(seed);
    if let Some(radius) = env_parse("TERRAFORGE_VIEW_RADIUS") {
        world.cfg.view_radius = radius;
    }
    let (secret_seed, dev_seed) = secret_seed_from_env(seed);
    let underground = Underground::new(secret_seed, &world.cfg);
    let window_width = env_parse("TERRAFORGE_WIN_W").unwrap_or(1600);
    let window_height = env_parse("TERRAFORGE_WIN_H").unwrap_or(900);

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.58, 0.72, 0.86)))
        .insert_resource(WorldRes(Arc::new(world)))
        .insert_resource(UndergroundRes(Arc::new(underground)))
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
        .add_systems(Startup, move || {
            // Hier statt in main(): vor App::run ist Bevys Logging noch nicht aktiv.
            if dev_seed {
                info!("Untergrund: unsicherer Entwickler-Seed (TERRAFORGE_SECRET_SEED nicht gesetzt)");
            }
        })
        .run();
}

/// `TERRAFORGE_SECRET_SEED` oder, falls nicht gesetzt, der unsichere Entwickler-Seed.
/// Der zweite Wert sagt, ob der Entwickler-Seed benutzt wird.
fn secret_seed_from_env(public_seed: u32) -> (SecretSeed, bool) {
    match env_parse::<u64>("TERRAFORGE_SECRET_SEED") {
        Some(value) => (SecretSeed::from_u64(value), false),
        None => (SecretSeed::insecure_dev_seed(public_seed), true),
    }
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
