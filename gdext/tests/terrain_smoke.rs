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

/// Spielstand (#19): Graben und Bauen, Godot beenden (der Server speichert), neu starten: Grube,
/// Gebautes und Inventar sind da. Ein kaputter Spielstand sperrt das Graben, statt still ohne ihn
/// weiterzuspielen, und bleibt unverändert.
#[test]
fn digging_survives_a_restart_in_godot() {
    let server = common::server_binary().display().to_string();
    let dir = std::env::temp_dir().join(format!("terraforge-persist-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let save = dir.display().to_string();
    let run = |args: Vec<String>| {
        let project = common::TestProject::new();
        let stdout = common::run_script(&project, "res://tests/persist_smoke.gd", &args, &["Spielstand"]);
        stdout.lines().find(|l| l.starts_with("TERRAFORGE_PERSIST ")).unwrap_or_else(|| panic!("keine Ergebniszeile: {stdout}")).to_string()
    };
    let first = run(vec![server.clone(), save.clone(), "first".into()]);
    let free = first.split_whitespace().find_map(|part| part.strip_prefix("free=")).expect("kein Abstand");
    let second = run(vec![server.clone(), save.clone(), "second".into(), free.into()]);
    assert!(second.contains(&format!("free={free}")), "{first} / {second}");
    // Kaputt machen: Der Server lädt ihn nicht und fasst ihn nicht an.
    let file = dir.join("world.tfsave");
    let bytes = std::fs::read(&file).unwrap();
    std::fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
    assert!(run(vec![server.clone(), save.clone(), "broken".into()]).contains("broken"));
    assert_eq!(std::fs::read(&file).unwrap(), &bytes[..bytes.len() / 2]);
    std::fs::remove_dir_all(&dir).ok();
}

/// Tests schreiben nie in den `user://`-Ordner des echten Spiels (#19): Nach Graben mit Server liegt
/// der Spielstand im eigenen Ordner des Testprojekts, der danach gelöscht ist; im Ordner des
/// Spiels ändert sich keine Datei.
#[test]
fn tests_do_not_touch_the_game_save() {
    let game = common::data_home().join("godot/app_userdata/MMO");
    let listing = || -> Vec<(std::path::PathBuf, Option<std::time::SystemTime>)> {
        let mut files = Vec::new();
        let mut stack = vec![game.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.components().any(|c| c.as_os_str() == "saves") {
                    files.push((path.clone(), entry.metadata().ok().and_then(|m| m.modified().ok())));
                }
            }
        }
        files.sort();
        files
    };
    let before = listing();
    let project = common::TestProject::new();
    let user = project.1.clone();
    let server = common::server_binary();
    common::run_script(&project, "res://tests/dig_smoke.gd", &[server.display().to_string()], &[]);
    assert!(user.join("saves/seed-42/world.tfsave").exists(), "kein Spielstand im eigenen Ordner: {}", user.display());
    assert_eq!(listing(), before, "Spielstand des echten Spiels verändert");
    drop(project);
    assert!(!user.exists());
}
