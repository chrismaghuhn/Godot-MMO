//! Benchmark-Modus (#9): misst Frametimes und Hintergrund-Jobs und beendet den Viewer mit einem
//! Bericht.
//!
//! `TERRAFORGE_BENCH=30` misst 30 s (nach 4 s Aufwärmen). Typisch zusammen mit
//! `TERRAFORGE_AUTOPILOT=180` für schnellen Flug. Optional schreibt `TERRAFORGE_BENCH_OUT=pfad`
//! den Bericht zusätzlich in eine Datei.
//!
//! **Aussagekräftig nur**, wenn das Fenster sichtbar ist und keine anderen GPU- oder CPU-lastigen
//! Programme laufen (Spiele, Godot, Videos): Der Treiber drosselt verdeckte Fenster, und andere
//! Programme erzeugen eigene Hänger (#38). Der Bericht zeigt deshalb Fokus und Systemlast mit an.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bevy::{app::AppExit, prelude::*, window::PrimaryWindow};
use terraforge_core::streaming::JobKind;

use crate::env_parse;

const WARMUP_SECONDS: f32 = 4.0;

/// Dauer der Hintergrund-Jobs in ms, je Art (Spalte, Höhle, Fern, Kollisionsfeld). Die Jobs laufen im
/// Core-Streamer; der Viewer trägt ihre Dauer pro Frame ein. Nur im Benchmark relevant.
static JOB_TIMES: Mutex<[Vec<f32>; 4]> = Mutex::new([Vec::new(), Vec::new(), Vec::new(), Vec::new()]);

/// Nur bei laufendem Benchmark sammeln, sonst wüchse `JOB_TIMES` die ganze Sitzung (Review-Fund).
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Ob der Benchmark läuft (dann sammelt der Streamer die Job-Dauern).
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Trägt die Dauer eines Jobs ein, falls der Benchmark läuft.
pub fn record_job(kind: JobKind, duration: Duration) {
    if !active() {
        return;
    }
    let index = match kind {
        JobKind::Column => 0,
        JobKind::Cave => 1,
        JobKind::Far => 2,
        JobKind::Field => 3,
    };
    if let Ok(mut times) = JOB_TIMES.lock() {
        times[index].push(duration.as_secs_f32() * 1000.0);
    }
}

pub struct BenchPlugin;

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        if let Some(seconds) = env_parse::<f32>("TERRAFORGE_BENCH").filter(|s| *s > 0.0) {
            ACTIVE.store(true, Ordering::Relaxed);
            app.insert_resource(Bench { seconds, frames: Vec::new(), unfocused_frames: 0, done: false })
                .add_systems(Last, bench);
        }
    }
}

#[derive(Resource)]
struct Bench {
    seconds: f32,
    frames: Vec<f32>,
    unfocused_frames: u32,
    done: bool,
}

fn bench(
    time: Res<Time>,
    mut bench: ResMut<Bench>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    let t = time.elapsed_secs();
    if bench.done || t < WARMUP_SECONDS {
        if t < WARMUP_SECONDS {
            // Jobs aus der Aufwärmphase (Erstladen) nicht mitzählen.
            if let Ok(mut times) = JOB_TIMES.lock() {
                times.iter_mut().for_each(Vec::clear);
            }
        }
        return;
    }
    bench.frames.push(time.delta_secs() * 1000.0);
    if window.single().is_ok_and(|w| !w.focused) {
        bench.unfocused_frames += 1;
    }
    if t < WARMUP_SECONDS + bench.seconds {
        return;
    }
    bench.done = true;
    let report = report(&bench);
    eprintln!("{report}");
    if let Ok(path) = std::env::var("TERRAFORGE_BENCH_OUT")
        && let Err(error) = std::fs::write(&path, &report)
    {
        eprintln!("Bericht konnte nicht geschrieben werden ({path}): {error}");
    }
    exit.write(AppExit::Success);
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn report(bench: &Bench) -> String {
    let mut frames = bench.frames.clone();
    frames.sort_by(f32::total_cmp);
    let avg = frames.iter().sum::<f32>() / frames.len().max(1) as f32;
    let over = |ms: f32| frames.iter().filter(|&&f| f > ms).count();
    let mut lines = vec![
        format!("BENCH {:.0} s, {} Frames", bench.seconds, frames.len()),
        format!(
            "  Frame: Ø {:.2} ms  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.1}  | > 20 ms: {}  > 33 ms: {}",
            avg,
            percentile(&frames, 0.5),
            percentile(&frames, 0.95),
            percentile(&frames, 0.99),
            frames.last().copied().unwrap_or(0.0),
            over(20.0),
            over(33.0),
        ),
    ];
    if let Ok(times) = JOB_TIMES.lock() {
        for (name, list) in ["Spalten", "Höhlen", "Fern", "Feld"].iter().zip(times.iter()) {
            let mut sorted = list.clone();
            sorted.sort_by(f32::total_cmp);
            let avg = sorted.iter().sum::<f32>() / sorted.len().max(1) as f32;
            lines.push(format!(
                "  Jobs {name:<8} {:>5}×  Ø {:.2} ms  p95 {:.2}  max {:.2}",
                sorted.len(),
                avg,
                percentile(&sorted, 0.95),
                sorted.last().copied().unwrap_or(0.0)
            ));
        }
    }
    let load = std::fs::read_to_string("/proc/loadavg").ok().and_then(|s| s.split_whitespace().next().map(String::from));
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    lines.push(format!(
        "  Umgebung: Systemlast {} bei {} Kernen, Fenster ohne Fokus in {} von {} Frames",
        load.as_deref().unwrap_or("?"),
        cores,
        bench.unfocused_frames,
        frames.len()
    ));
    if let Some(load) = load.and_then(|l| l.parse::<f32>().ok())
        && load > cores as f32 * 0.25
    {
        lines.push("  WARNUNG: hohe Systemlast, Ergebnis evtl. nicht aussagekräftig".into());
    }
    lines.join("\n")
}
