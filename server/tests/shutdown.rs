//! Spielstand beim Beenden (#19): Der echte Server-Prozess speichert die letzte Aktion, auch wenn
//! Verbindung und stdin gleichzeitig enden (so beendet Godot ihn) und der Spielstand nicht klein ist
//! (Review-Fund: Ein Thread beendete den Prozess, während der andere noch schrieb).

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use glam::Vec3;
use terraforge_core::WorldGen;
use terraforge_core::edits::action::{REACH, ray_hit};
use terraforge_core::underground::SecretSeed;
use terraforge_protocol::{EditAction, EditOutcome, NetworkSource};
use terraforge_server::{Request, ServerWorld};

use common::{SEED, edit, eye};

#[test]
fn the_last_action_is_saved_when_the_game_quits() {
    let dir = std::env::temp_dir().join(format!("terraforge-shutdown-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let generator = WorldGen::new(SEED);
    // Ein Spielstand mit über 100 KB: viele Gruben.
    let prepared = ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED));
    for i in 0..400 {
        let (x, z) = ((i % 20) as f32 * 9.0 - 300.0, (i / 20) as f32 * 9.0 - 300.0);
        prepared.edit(eye(&generator, x, z), Vec3::new(0.2, -1.0, 0.1).normalize(), Request::Dig { tool: 3 });
    }
    prepared.save(&dir).unwrap();
    let size = std::fs::metadata(terraforge_server::save_path(&dir)).unwrap().len();
    assert!(size > 100_000, "Spielstand nur {size} Byte");

    for round in 0..5 {
        let mut child = Command::new(env!("CARGO_BIN_EXE_terraforge-server"))
            .args(["--seed", &SEED.to_string(), "--port", "0", "--exit-with-client", "--save"])
            .arg(&dir)
            .env_remove("TERRAFORGE_SECRET_SEED")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let port: u16 = line.trim().strip_prefix("TERRAFORGE_SERVER port=").and_then(|p| p.parse().ok()).unwrap_or_else(|| panic!("{line:?}"));
        let source = NetworkSource::connect(("127.0.0.1", port), SEED, Duration::from_secs(5)).unwrap();
        // Jede Runde eine neue Grube, deren Mitte vorher fest war.
        let eye = eye(&generator, 200.0 + round as f32 * 11.0, 150.0);
        let down = Vec3::new(0.1, -1.0, 0.2).normalize();
        let reference = ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED));
        let (hit, _) = ray_hit(eye, down, REACH, |v| reference.voxel(v).density > 0.0).unwrap();
        assert!(matches!(edit(&source, eye, down, EditAction::Dig { tool: 3 }), EditOutcome::Dug { .. }));
        // Wie Godot: Verbindung zu, stdin zu, fast gleichzeitig.
        let stdin = child.stdin.take();
        drop(source);
        drop(stdin);
        let status = child.wait().unwrap();
        assert!(status.success(), "Runde {round}: {status}");
        let loaded = ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED));
        assert!(loaded.load(&dir).unwrap());
        assert!(loaded.voxel(hit.round().as_ivec3()).density <= 0.0, "Runde {round}: letzte Grube nicht gespeichert");
    }
    std::fs::remove_dir_all(&dir).ok();
}
