//! Graben und Bauen über eine echte TCP-Verbindung (#14): Der Server prüft, gräbt im echten
//! Untergrund und schickt danach die geänderten Chunks.

use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use terraforge_core::WorldGen;
use terraforge_core::edits::action::{REACH, ray_hit};
use terraforge_core::edits::owner_chunk;
use terraforge_core::streaming::UndergroundSource;
use terraforge_core::underground::OreKind;
use terraforge_core::volume::{CHUNK_SIZE, ColumnData, SAMPLE_MIN};
use terraforge_protocol::{EditAction, EditOutcome};

mod common;

use common::{SEED, edit, eye, start_server};

/// Graben schickt die Chunks zum Neuladen; danach kommt über das Netz der neue Stand an.
#[test]
fn digging_over_the_network_changes_the_chunks() {
    let (server, source) = start_server();
    let world = WorldGen::new(SEED);
    let eye = eye(&world, 12.0, -7.0);
    let down = Vec3::new(0.3, -1.0, 0.2).normalize();
    let (hit, _) = ray_hit(eye, down, REACH, |v| server.voxel(v).density > 0.0).expect("Boden in Reichweite");
    let EditOutcome::Dug { removed, too_hard, chunks, .. } = edit(&source, eye, down, EditAction::Dig { tool: 3 }) else {
        panic!("nicht gegraben");
    };
    assert!(!removed.is_empty() && too_hard == 0, "Ausbeute {removed:?}, zu hart {too_hard}");
    // Der Treffer ist jetzt Luft.
    assert!(server.voxel(hit.round().as_ivec3()).density <= 0.0, "keine Grube bei {hit}");
    let changed: Vec<IVec3> = chunks.iter().copied().map(IVec3::from_array).collect();
    for &chunk in &changed {
        let column = ColumnData::sample(&world, chunk.x, chunk.z);
        let got = source.materials_chunk(&world, &column, chunk.y);
        let expected = server.client_chunk(chunk, terraforge_protocol::ChunkKind::Full).unwrap();
        assert!(got.same_as(&expected), "Chunk {chunk} über das Netz nicht auf neuem Stand");
    }
    assert!(changed.contains(&owner_chunk(hit.round().as_ivec3())));
}

#[test]
fn the_server_refuses_invalid_edits() {
    let (_server, source) = start_server();
    let world = WorldGen::new(SEED);
    let eye = eye(&world, 12.0, -7.0);
    let refused = |outcome: EditOutcome| match outcome {
        EditOutcome::Refused { reason } => reason,
        other => panic!("nicht abgelehnt: {other:?}"),
    };
    assert_eq!(refused(edit(&source, eye, Vec3::Y, EditAction::Dig { tool: 3 })), "kein Ziel in Reichweite");
    assert_eq!(refused(edit(&source, eye - Vec3::Y * 10.0, Vec3::X, EditAction::Dig { tool: 3 })), "Auge im Fels");
    assert_eq!(refused(edit(&source, eye, Vec3::new(3e38, 0.0, 0.0), EditAction::Dig { tool: 3 })), "ungültige Position oder Richtung");
    assert_eq!(refused(edit(&source, eye, -Vec3::Y, EditAction::Build { slot: 0, material: 2 })), "nichts zum Bauen auf diesem Platz");
    assert_eq!(refused(edit(&source, eye, -Vec3::Y, EditAction::Dig { tool: 9 })), "unbekanntes Werkzeug");
    assert_eq!(refused(edit(&source, eye, Vec3::ZERO, EditAction::Dig { tool: 0 })), "ungültige Position oder Richtung");
    assert_eq!(refused(edit(&source, Vec3::new(f32::NAN, 0.0, 0.0), -Vec3::Y, EditAction::Dig { tool: 0 })), "ungültige Position oder Richtung");
    assert_eq!(refused(edit(&source, Vec3::splat(1e9), -Vec3::Y, EditAction::Dig { tool: 0 })), "außerhalb der Welt");
    // Zwei Aktionen direkt nacheinander: die zweite ist zu schnell.
    let forward = Vec3::new(1.0, -0.4, 0.0).normalize();
    let first = source.send_edit(eye.to_array(), forward.to_array(), EditAction::Dig { tool: 0 }).unwrap();
    let second = source.send_edit(eye.to_array(), forward.to_array(), EditAction::Dig { tool: 0 }).unwrap();
    let start = Instant::now();
    let mut outcomes = Vec::new();
    while outcomes.len() < 2 && start.elapsed() < Duration::from_secs(10) {
        outcomes.extend(source.take_edits());
        std::thread::sleep(Duration::from_millis(5));
    }
    let outcome = |request| outcomes.iter().find(|(r, _)| *r == request).map(|(_, o)| o);
    assert!(matches!(outcome(first), Some(EditOutcome::Dug { .. })), "{:?}", outcome(first));
    assert_eq!(outcome(second), Some(&EditOutcome::Refused { reason: "zu schnell".into() }));
}

/// Nach dem Graben gilt der Erz-Filter mit dem neuen Stand (D-018): Jedes Erz, das der Client
/// bekommt, hat einen Luft-Nachbarn, auch in den neu gegrabenen Gängen.
#[test]
fn ores_stay_hidden_unless_digging_exposes_them() {
    let (server, source) = start_server();
    let world = WorldGen::new(SEED);
    let neighbours: Vec<IVec3> = (0..27).map(|n| IVec3::new(n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1)).filter(|v| *v != IVec3::ZERO).collect();
    let mut chunks = Vec::new();
    // Ein Schacht schräg in den Boden: Jedes Mal steht das Auge in der zuletzt gegrabenen Grube.
    let mut eye = eye(&world, 5.0, 5.0);
    let direction = Vec3::new(1.0, -1.0, 0.3).normalize();
    for _ in 0..8 {
        let (hit, _) = ray_hit(eye, direction, REACH, |v| server.voxel(v).density > 0.0).expect("Boden in Reichweite");
        if let EditOutcome::Dug { chunks: changed, .. } = edit(&source, eye, direction, EditAction::Dig { tool: 3 }) {
            chunks.extend(changed.into_iter().map(IVec3::from_array));
        }
        eye = hit;
    }
    assert!(!chunks.is_empty(), "nichts gegraben");
    chunks.sort_by_key(|c| (c.x, c.y, c.z));
    chunks.dedup();
    let mut ores = 0;
    for chunk in chunks {
        let column = ColumnData::sample(&world, chunk.x, chunk.z);
        let got = source.materials_chunk(&world, &column, chunk.y);
        let origin = chunk * CHUNK_SIZE;
        for z in SAMPLE_MIN..=CHUNK_SIZE {
            for y in SAMPLE_MIN..=CHUNK_SIZE {
                for x in SAMPLE_MIN..=CHUNK_SIZE {
                    if OreKind::from_material(got.material(x, y, z)).is_none() {
                        continue;
                    }
                    ores += 1;
                    let voxel = origin + IVec3::new(x, y, z);
                    assert!(neighbours.iter().any(|&n| server.voxel(voxel + n).density <= 0.0), "verborgenes Erz bei {voxel} geschickt");
                }
            }
        }
    }
    assert!(ores > 0, "keine Erze in den gegrabenen Chunks: Test ohne Aussage");
}

/// `too_hard` zählt nur zu harte Voxel, die nach dem Graben an Luft grenzen (D-005): verborgene
/// harte Voxel in der Kugel verrät die Antwort nicht (Review-Fund).
#[test]
fn too_hard_only_counts_exposed_voxels() {
    let (server, source) = start_server();
    let world = WorldGen::new(SEED);
    let schaufel = terraforge_core::edits::action::TOOLS[0].max_hardness;
    let neighbours: Vec<IVec3> = (0..27).map(|n| IVec3::new(n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1)).filter(|v| *v != IVec3::ZERO).collect();
    let mut hidden_total = 0;
    // Erst mit der Eisenhacke einen Schacht bis in den Fels (die Oberfläche ist weich), dann am
    // Grund mit der Schaufel.
    for (x, z) in [(12.0, -7.0), (-40.0, 30.0)] {
        let mut eye = eye(&world, x, z);
        let down = Vec3::new(0.2, -1.0, 0.0).normalize();
        let mut center = eye;
        let mut too_hard = 0;
        for step in 0..9 {
            (center, _) = ray_hit(eye, down, REACH, |v| server.voxel(v).density > 0.0).expect("Boden in Reichweite");
            let tool = if step < 8 { 3 } else { 0 };
            let EditOutcome::Dug { too_hard: reported, .. } = edit(&source, eye, down, EditAction::Dig { tool }) else {
                panic!("nicht gegraben");
            };
            too_hard = reported;
            eye = center;
        }
        let (mut exposed, mut hidden) = (0, 0);
        let r = terraforge_core::edits::action::DIG_RADIUS as i32 + 1;
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    let voxel = center.round().as_ivec3() + IVec3::new(dx, dy, dz);
                    let current = server.voxel(voxel);
                    let hard = world.materials.get(current.material).is_none_or(|m| m.hardness > schaufel);
                    if voxel.as_vec3().distance(center) > terraforge_core::edits::action::DIG_RADIUS || current.density <= 0.0 || !hard {
                        continue;
                    }
                    if neighbours.iter().any(|&n| server.voxel(voxel + n).density <= 0.0) { exposed += 1 } else { hidden += 1 }
                }
            }
        }
        assert_eq!(too_hard, exposed, "bei ({x}, {z})");
        hidden_total += hidden;
    }
    assert!(hidden_total > 0, "keine verborgenen harten Voxel: Test ohne Aussage");
}

/// Dicht über dem Boden liegt der nächste Voxel des Auges schon im Fels; das ist kein „Auge im
/// Fels“, solange die Zelle um das Auge nicht ganz fest ist (Review-Fund: Aktionen an Wänden
/// abgelehnt).
#[test]
fn actions_close_to_a_wall_are_accepted() {
    let (server, source) = start_server();
    let world = WorldGen::new(SEED);
    let solid = |v: IVec3| server.voxel(v).density > 0.0;
    let eye = (0..400)
        .map(|i| {
            let (x, z) = (i as f32 * 0.37, i as f32 * 0.21);
            Vec3::new(x, world.column_height(x, z) + 0.4, z)
        })
        .find(|eye| solid(eye.round().as_ivec3()) && !solid(eye.floor().as_ivec3() + IVec3::Y))
        .expect("keine Stelle mit Fels am nächsten Voxel");
    match edit(&source, eye, Vec3::new(1.0, -0.2, 0.0).normalize(), EditAction::Dig { tool: 3 }) {
        EditOutcome::Dug { .. } => {}
        other => panic!("abgelehnt bei {eye}: {other:?}"),
    }
}
