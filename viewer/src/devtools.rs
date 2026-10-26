//! Entwickler-Werkzeuge: optionaler Framelimiter und Auto-Screenshot.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

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
