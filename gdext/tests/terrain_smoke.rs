//! Terrain-Streaming (#11), Höhlen vom Server (#56), Bewegung (#13), Graben und Bauen (#14) und
//! Hauptszene in einem echten Godot (headless). Die Prüfungen stehen in `client/tests/*.gd`.
//! Braucht Godot 4.7 (`GODOT`).

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

/// Mit Server (#56): Godot startet ihn als Kindprozess, Höhlen kommen an.
#[test]
fn caves_come_from_the_server_in_godot() {
    let server = common::server_binary();
    let project = common::TestProject::new();
    let args = ["caves".to_string(), server.display().to_string()];
    let stdout = common::run_script(&project, "res://tests/terrain_smoke.gd", &args, &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_TERRAIN ")).expect("keine Ergebniszeile");
    assert!(line.contains("mode=caves") && !line.contains("caves=0 "), "{line}");
}

/// Die Hauptszene (Kamera, Nebel, Wasser, Overlay) läuft 300 Frames ohne Skriptfehler.
#[test]
fn main_scene_runs_without_errors() {
    let project = common::TestProject::new();
    common::run(&project, &["--quit-after", "300"], &[]);
}

/// Bewegung und Kollision (#13): landen, laufen ohne einzusinken, springen.
#[test]
fn player_walks_on_the_terrain_in_godot() {
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/player_smoke.gd", &[], &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_PLAYER ")).expect("keine Ergebniszeile");
    assert!(!line.contains("fields=0"), "kein Dichtefeld: {line}");
}

/// Graben und Bauen über den Server (#14): Grube unter den Füßen, Erde vor dem Auge, kein Bauen in
/// den eigenen Körper; der neue Stand kommt auch im Dichtefeld der Kollision an.
#[test]
fn digging_and_building_go_through_the_server_in_godot() {
    let server = common::server_binary();
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/dig_smoke.gd", &[server.display().to_string()], &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_DIG ")).expect("keine Ergebniszeile");
    assert!(line.contains("server=verbunden"), "{line}");
}

/// Ohne Server gräbt der Client selbst in der öffentlichen Oberfläche.
#[test]
fn digging_and_building_work_without_a_server_in_godot() {
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/dig_smoke.gd", &["off".into()], &[]);
    let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_DIG ")).expect("keine Ergebniszeile");
    assert!(line.contains("server=aus"), "{line}");
}

/// Bäume fällen (#18): mit Server und ohne; Holz kommt ins Inventar, der Baum ist danach weg.
#[test]
fn felling_trees_works_with_and_without_a_server_in_godot() {
    let server = common::server_binary();
    for mode in [server.display().to_string(), "off".to_string()] {
        let project = common::TestProject::new();
        let stdout = common::run_script(&project, "res://tests/fell_smoke.gd", std::slice::from_ref(&mode), &[]);
        let line = stdout.lines().find(|l| l.starts_with("TERRAFORGE_FELL ")).expect("keine Ergebniszeile");
        assert!(line.contains(if mode == "off" { "server=aus" } else { "server=verbunden" }), "{line}");
    }
}

/// Kollision mit Baumstämmen (#58): Der Körper bleibt vor einem Stamm stehen und kommt nach dem
/// Fällen durch.
#[test]
fn trunks_stop_the_player_until_felled_in_godot() {
    let project = common::TestProject::new();
    let stdout = common::run_script(&project, "res://tests/trunk_smoke.gd", &[], &[]);
    assert!(stdout.lines().any(|l| l.starts_with("TERRAFORGE_TRUNK through=")), "{stdout}");
}
