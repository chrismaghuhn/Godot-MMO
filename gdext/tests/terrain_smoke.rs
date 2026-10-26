//! Terrain-Streaming und Hauptszene in einem echten Godot (headless, #11). Die Prüfungen stehen in
//! `client/tests/terrain_smoke.gd`. Braucht Godot 4.7 (`GODOT`).

mod common;

#[test]
fn terrain_streams_around_the_camera_in_godot() {
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/terrain_smoke.gd", &["near".into()], &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_TERRAIN ")).expect("keine Ergebniszeile");
    assert!(line.contains("columns=25"), "{line}");
}

#[test]
fn far_terrain_replaces_its_tiles_in_godot() {
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/terrain_smoke.gd", &["far".into()], &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_TERRAIN ")).expect("keine Ergebniszeile");
    assert!(line.contains("columns=81") && line.contains("far_tiles=184"), "{line}");
}

/// Die Hauptszene (Kamera, Nebel, Wasser, Overlay) läuft 300 Frames ohne Skriptfehler.
#[test]
fn main_scene_runs_without_errors() {
    let project = common::TestProject::new();
    common::run(&project, &["--quit-after", "300"], &[]);
}
