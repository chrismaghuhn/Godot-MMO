//! Inventar über eine echte TCP-Verbindung (#18): Graben füllt es, Bauen verbraucht, Erze tragen
//! Reinheit und Spurenelemente vom Server.

mod common;

use glam::Vec3;
use terraforge_core::MaterialId;
use terraforge_core::WorldGen;
use terraforge_core::underground::OreKind;
use terraforge_protocol::{EditAction, EditOutcome, StackInfo};

use common::{SEED, edit, eye, inventory, start_server};

fn total(stacks: &[StackInfo]) -> u32 {
    stacks.iter().map(|stack| stack.count).sum()
}

#[test]
fn digging_fills_and_building_empties_the_inventory() {
    let (_server, source) = start_server();
    let world = WorldGen::new(SEED);
    assert!(inventory(&source).is_empty(), "nach der Anmeldung leer");
    let eye = eye(&world, 12.0, -7.0);
    let EditOutcome::Dug { removed, lost, .. } = edit(&source, eye, Vec3::new(0.3, -1.0, 0.2).normalize(), EditAction::Dig { tool: 3 }) else {
        panic!("nicht gegraben");
    };
    let stacks = inventory(&source);
    assert_eq!(total(&stacks), removed.iter().map(|(_, n)| n).sum::<u32>() - lost);
    for stack in &stacks {
        let base = world.materials.get(MaterialId(stack.material)).expect("unbekanntes Material");
        assert!((stack.properties[0] / base.hardness - 1.0).abs() < 0.2, "{stack:?}");
        assert!(stack.ore.is_none(), "Oberfläche ohne Erz: {stack:?}");
    }
    let (slot, material) = (stacks[0].slot, stacks[0].material);
    let build = EditAction::Build { slot, material };
    assert_eq!(edit(&source, eye, -Vec3::Y, build), EditOutcome::Refused { reason: "zu nah am Spieler".into() });
    // Ein älterer Stand des Clients: Dort liegt laut Server etwas anderes.
    let stale = EditAction::Build { slot, material: material + 1 };
    assert_eq!(edit(&source, eye, Vec3::X, stale), EditOutcome::Refused { reason: "auf diesem Platz liegt inzwischen etwas anderes".into() });
    assert_eq!(edit(&source, eye, Vec3::X, EditAction::Build { slot: 39, material }), EditOutcome::Refused { reason: "nichts zum Bauen auf diesem Platz".into() });
    // Bauen, bis der Platz leer ist (in verschiedene Richtungen): Jedes Mal fehlt genau, was gebaut
    // wurde; beim letzten Mal begrenzt der Rest die Kugel, danach ist der Platz leer und bleibt es.
    let mut current = stacks;
    for i in 0..12 {
        let Some(count) = current.iter().find(|s| s.slot == slot).map(|s| s.count) else {
            break;
        };
        let angle = i as f32 * 0.5;
        let direction = Vec3::new(angle.cos(), -0.3, angle.sin()).normalize();
        let EditOutcome::Built { placed, material: built, .. } = edit(&source, eye, direction, build) else {
            continue;
        };
        assert_eq!(built, material);
        assert!(placed > 0 && placed <= count);
        let after = inventory(&source);
        assert_eq!(total(&after), total(&current) - placed);
        let left = after.iter().find(|s| s.slot == slot).map_or(0, |s| s.count);
        assert_eq!(left, count - placed, "Platz {slot}");
        current = after;
    }
    assert!(current.iter().all(|s| s.slot != slot), "Platz {slot} nicht leer gebaut");
    // Die anderen Plätze behalten ihre Nummern.
    let later: Vec<u8> = current.iter().map(|s| s.slot).collect();
    assert!(later.iter().all(|&s| s > slot), "{later:?}");
}

/// Ein Erz, das an einer Höhle freiliegt, von der Höhlenluft aus abbauen: Es kommt mit Reinheit
/// und Spurenelementen an, anderes Material ohne.
#[test]
fn mined_ore_carries_purity_and_traces() {
    let (server, source) = start_server();
    let world = WorldGen::new(SEED);
    inventory(&source);
    let (eye, target) = exposed_ore(&server).expect("kein freiliegendes Erz gefunden");
    let EditOutcome::Dug { removed, .. } = edit(&source, eye, (target - eye).normalize(), EditAction::Dig { tool: 3 }) else {
        panic!("nicht gegraben");
    };
    assert!(removed.iter().any(|(m, _)| OreKind::from_material(MaterialId(*m)).is_some()), "kein Erz abgebaut: {removed:?}");
    let stacks = inventory(&source);
    let ore = stacks.iter().find(|stack| stack.ore.is_some()).expect("Erz ohne Erz-Daten im Inventar");
    assert!(OreKind::from_material(MaterialId(ore.material)).is_some());
    let (purity, traces) = ore.ore.as_ref().unwrap();
    assert!((0.05..=1.0).contains(purity));
    assert_eq!(traces.len(), world.materials.get(MaterialId(ore.material)).unwrap().traces.len());
    for stack in stacks.iter().filter(|stack| OreKind::from_material(MaterialId(stack.material)).is_none()) {
        assert!(stack.ore.is_none(), "{stack:?}");
    }
}

/// Erz-Voxel mit einem Luft-Nachbarn (wie der Client sie sieht): Auge in der Luft daneben, Ziel.
fn exposed_ore(server: &terraforge_server::ServerWorld) -> Option<(Vec3, Vec3)> {
    use glam::IVec3;
    use terraforge_core::volume::CHUNK_SIZE;
    for cy in (-5..=0).rev() {
        for cz in -3..=3 {
            for cx in -3..=3 {
                let chunk = IVec3::new(cx, cy, cz);
                let data = server.client_chunk(chunk, terraforge_protocol::ChunkKind::Full)?;
                for z in 1..CHUNK_SIZE - 1 {
                    for y in 1..CHUNK_SIZE - 1 {
                        for x in 1..CHUNK_SIZE - 1 {
                            if OreKind::from_material(data.material(x, y, z)).is_none() {
                                continue;
                            }
                            let voxel = chunk * CHUNK_SIZE + IVec3::new(x, y, z);
                            // Freie Zelle (2 Voxel Luft) vor dem Erz für das Auge.
                            for n in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                                let (a, b) = (voxel + n, voxel + n * 2);
                                if server.voxel(a).density <= 0.0 && server.voxel(b).density <= 0.0 {
                                    return Some((b.as_vec3() + Vec3::splat(0.01), voxel.as_vec3()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Bäume fällen (#18): Holz ins Inventar, der Baum ist danach weg, auch für einen zweiten Versuch.
#[test]
fn felling_a_tree_yields_wood_once() {
    use terraforge_core::spawn::find_spawn;
    use terraforge_core::vegetation::{SPAWN_CLEARING, column_trees};
    use terraforge_core::volume::{CHUNK_SIZE, ColumnData};
    let (server, port) = common::start_server_port();
    let source = common::connect(port);
    let world = WorldGen::new(SEED);
    inventory(&source);
    let spawn = find_spawn(&world);
    let clearing = Some((glam::Vec2::new(spawn.x, spawn.z), SPAWN_CLEARING));
    // Ein Baum nahe am Spawn mit ebenem Boden 3 m daneben.
    let (base, eye) = (-2..=2)
        .flat_map(|dz| (-2..=2).map(move |dx| (dx, dz)))
        .flat_map(|(dx, dz)| {
            let column = ColumnData::sample(&world, (spawn.x / 32.0).floor() as i32 + dx, (spawn.z / 32.0).floor() as i32 + dz);
            let origin = Vec3::new(column.chunk_x as f32, 0.0, column.chunk_z as f32) * CHUNK_SIZE as f32;
            column_trees(&world, &column, clearing, |v| server.voxel(v).density).into_iter().map(move |tree| origin + tree.position)
        })
        .find_map(|base| {
            let stand = base + Vec3::X * 3.0;
            let ground = world.column_height(stand.x, stand.z);
            ((ground - base.y).abs() < 1.0).then(|| (base, Vec3::new(stand.x, ground + 1.65, stand.z)))
        })
        .expect("kein Baum nahe am Spawn");
    let aim = (base + Vec3::Y * 1.5 - eye).normalize();
    let EditOutcome::Felled { count, material, lost, .. } = edit(&source, eye, aim, EditAction::Fell) else {
        panic!("nicht gefällt");
    };
    assert!(count > 0 && lost == 0);
    let stacks = inventory(&source);
    assert_eq!(stacks.iter().filter(|s| s.material == material).map(|s| s.count).sum::<u32>(), count);
    assert_eq!(edit(&source, eye, aim, EditAction::Fell), EditOutcome::Refused { reason: "kein Baum im Visier".into() });
    // Ein neu verbundener Client erfährt den gefällten Baum (Review-Fund).
    let second = common::connect(port);
    inventory(&second);
    let start = std::time::Instant::now();
    let mut felled = Vec::new();
    while felled.is_empty() && start.elapsed() < std::time::Duration::from_secs(5) {
        felled = second.take_felled();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(felled.len(), 1);
}
