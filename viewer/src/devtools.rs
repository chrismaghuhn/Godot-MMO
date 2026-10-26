//! Entwickler-Werkzeuge: optionaler Framelimiter und Auto-Screenshot.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PrimaryWindow,
};

const SHOT_PATH: &str = "shots/auto.png";

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FrameCap::from_env())
            .init_resource::<ShotState>()
            .add_systems(Last, frame_cap)
            .add_systems(Update, auto_screenshot);
    }
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

fn auto_screenshot(
    mut commands: Commands,
    mut state: ResMut<ShotState>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    if std::env::var_os("TERRAFORGE_SHOT").is_none() || state.requested {
        return;
    }
    state.frame += 1;
    let after: u32 = std::env::var("TERRAFORGE_SHOT_AFTER")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(240);
    // Solange das Fenster (noch) keine echte Größe hat, wäre die Swapchain 1×1 (#34).
    let sized = window
        .single()
        .is_ok_and(|window| window.physical_width() > 1 && window.physical_height() > 1);
    if state.frame >= after && sized {
        std::fs::create_dir_all("shots").ok();
        commands.spawn(Screenshot::primary_window()).observe(save_shot);
        state.requested = true;
    }
}

/// Speichert den Screenshot. Ein leeres 1×1-Bild wird verworfen und neu angefordert (#34).
fn save_shot(captured: On<ScreenshotCaptured>, mut state: ResMut<ShotState>) {
    let size = captured.image.size();
    if size.x <= 1 || size.y <= 1 {
        warn!("Screenshot ist nur {}×{} Pixel, neuer Versuch", size.x, size.y);
        state.requested = false;
        return;
    }
    save_to_disk(SHOT_PATH)(captured);
}
