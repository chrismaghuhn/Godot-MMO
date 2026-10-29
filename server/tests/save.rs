//! Spielstand (#19): Speichern und Laden ergibt denselben Stand, fremde oder kaputte Dateien werden
//! nicht geladen, ein neu verbundener Client erfährt die veränderten Chunks.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use glam::{IVec3, Vec3};
use terraforge_core::WorldGen;
use terraforge_core::edits::action::{REACH, ray_hit};
use terraforge_core::underground::SecretSeed;
use terraforge_protocol::EditOutcome;
use terraforge_server::{Request, ServerWorld, save_path};

use common::{SEED, eye};

/// Eigener, leerer Ordner je Test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("terraforge-save-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    dir
}

/// Auge 3 m neben einem Baum nahe am Spawn und Blick auf seinen Stamm.
fn tree_target(server: &ServerWorld, world: &WorldGen) -> (Vec3, Vec3) {
    use terraforge_core::spawn::find_spawn;
    use terraforge_core::vegetation::{SPAWN_CLEARING, column_trees};
    use terraforge_core::volume::{CHUNK_SIZE, ColumnData};
    let spawn = find_spawn(world);
    let clearing = Some((glam::Vec2::new(spawn.x, spawn.z), SPAWN_CLEARING));
    let (cx, cz) = ((spawn.x / 32.0).floor() as i32, (spawn.z / 32.0).floor() as i32);
    for (dx, dz) in (-2..=2).flat_map(|dz| (-2..=2).map(move |dx| (dx, dz))) {
        let column = ColumnData::sample(world, cx + dx, cz + dz);
        let origin = Vec3::new(column.chunk_x as f32, 0.0, column.chunk_z as f32) * CHUNK_SIZE as f32;
        for tree in column_trees(world, &column, clearing, |v| server.voxel(v).density) {
            let base = origin + tree.position;
            let stand = base + Vec3::X * 3.0;
            let ground = world.column_height(stand.x, stand.z);
            if (ground - base.y).abs() < 1.0 {
                let eye = Vec3::new(stand.x, ground + 1.65, stand.z);
                return (eye, (base + Vec3::Y * 1.5 - eye).normalize());
            }
        }
    }
    panic!("kein Baum nahe am Spawn");
}

fn world() -> ServerWorld {
    ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED))
}

/// Graben, Bauen, Graben des Gebauten: was danach im Inventar und in der Welt ist.
#[test]
fn saving_and_loading_keeps_edits_inventory_and_built_items() {
    let dir = temp_dir("roundtrip");
    let before = world();
    let generator = WorldGen::new(SEED);
    let eye = eye(&generator, 12.0, -7.0);
    let down = Vec3::new(0.3, -1.0, 0.2).normalize();
    let (hit, _) = ray_hit(eye, down, REACH, |v| before.voxel(v).density > 0.0).unwrap();
    assert!(matches!(before.edit(eye, down, Request::Dig { tool: 3 }).0, EditOutcome::Dug { .. }));
    let stacks = before.inventory_view();
    let (slot, material) = (stacks[0].slot, stacks[0].material);
    let forward = Vec3::new(1.0, -0.3, 0.0).normalize();
    let EditOutcome::Built { placed, .. } = before.edit(eye, forward, Request::Build { slot: slot.into(), material: terraforge_core::MaterialId(material) }).0 else {
        panic!("nicht gebaut");
    };
    assert!(placed > 0);
    // Einen Baum nahe am Spawn fällen.
    let (tree_eye, aim) = tree_target(&before, &generator);
    assert!(matches!(before.edit(tree_eye, aim, Request::Fell).0, EditOutcome::Felled { .. }));
    before.save(&dir).unwrap();

    let after = world();
    assert!(after.load(&dir).unwrap());
    assert_eq!(after.felled_trees(), before.felled_trees());
    assert_eq!(after.edit(tree_eye, aim, Request::Fell).0, EditOutcome::Refused { reason: "kein Baum im Visier".into() });
    assert!(after.voxel(hit.round().as_ivec3()).density <= 0.0, "Grube nach dem Laden weg");
    // Jeder geänderte Voxel ist gleich, das Inventar auch.
    for z in -12..12 {
        for y in -8..8 {
            for x in -12..12 {
                let voxel = eye.round().as_ivec3() + IVec3::new(x, y, z);
                assert_eq!(after.voxel(voxel), before.voxel(voxel), "{voxel}");
            }
        }
    }
    assert_eq!(after.inventory_view(), before.inventory_view());
    assert_eq!(after.touched_chunks().len(), before.touched_chunks().len());
    // Das Gebaute wieder abgraben: In beiden Welten kommt dasselbe ins Inventar (das gespeicherte
    // Item, nicht eine neue Bewertung am Ort).
    let (a, b) = (before.edit(eye, forward, Request::Dig { tool: 3 }).0, after.edit(eye, forward, Request::Dig { tool: 3 }).0);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(after.inventory_view(), before.inventory_view());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn foreign_or_broken_saves_are_not_loaded() {
    let dir = temp_dir("foreign");
    let generator = WorldGen::new(SEED);
    let original = world();
    original.edit(eye(&generator, 12.0, -7.0), -Vec3::Y, Request::Dig { tool: 3 });
    original.save(&dir).unwrap();
    let bytes = std::fs::read(save_path(&dir)).unwrap();
    // Anderer geheimer Seed: andere Welt.
    let other = ServerWorld::new(SEED, SecretSeed::from_u64(7));
    assert!(other.load(&dir).unwrap_err().contains("anderen Welt"));
    assert!(other.touched_chunks().is_empty(), "teilweise übernommen");
    // Anderer öffentlicher Seed.
    assert!(ServerWorld::new(43, SecretSeed::insecure_dev_seed(SEED)).load(&dir).is_err());
    // Die Datei bleibt dabei unverändert.
    assert_eq!(std::fs::read(save_path(&dir)).unwrap(), bytes);
    // Kaputt: nicht geladen. Fehlt: neuer Spielstand.
    std::fs::write(save_path(&dir), &bytes[..bytes.len() / 2]).unwrap();
    assert!(world().load(&dir).is_err());
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(world().load(&dir), Ok(false));
}

/// Nach dem Laden erfährt ein Client, welche Chunks verändert sind, und bekommt sie auf dem
/// geladenen Stand.
#[test]
fn a_client_after_loading_learns_the_changed_chunks() {
    let dir = temp_dir("client");
    let generator = WorldGen::new(SEED);
    let eye = eye(&generator, 12.0, -7.0);
    let first = world();
    first.edit(eye, Vec3::new(0.3, -1.0, 0.2).normalize(), Request::Dig { tool: 3 });
    first.save(&dir).unwrap();
    let expected = first.touched_chunks();

    let loaded = Arc::new(world());
    loaded.load(&dir).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Arc::clone(&loaded);
    std::thread::spawn(move || terraforge_server::serve(listener, server, 2, false));
    let source = common::connect(port);
    common::inventory(&source);
    let start = std::time::Instant::now();
    let mut touched = Vec::new();
    while touched.len() < expected.len() && start.elapsed() < std::time::Duration::from_secs(5) {
        touched.extend(source.take_touched());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    touched.sort();
    let mut expected = expected;
    expected.sort();
    assert_eq!(touched, expected);
    std::fs::remove_dir_all(&dir).ok();
}
