//! Tests der Spieler-Aktion (#14).

use glam::{IVec3, Vec3};

use super::*;
use crate::edits::chunks_containing;
use crate::volume::ColumnData;
use crate::world::WorldGen;

/// Öffentliche Oberfläche als Voxel-Quelle (ohne Cache, die Tests sind klein).
fn public(world: &WorldGen) -> impl Fn(IVec3) -> Voxel + '_ {
    move |voxel| {
        let owner = crate::edits::owner_chunk(voxel);
        let chunk = crate::volume::DensityChunk::from_column(&ColumnData::sample(world, owner.x, owner.z), owner.y);
        let local = voxel - owner * CHUNK_SIZE;
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    }
}

/// Auge 1,65 m über der Oberfläche an (x, z).
fn eye(world: &WorldGen, x: f32, z: f32) -> Vec3 {
    Vec3::new(x, world.column_height(x, z) + Settings::default().eye_height, z)
}

#[test]
fn the_ray_finds_the_ground_and_building_sits_in_front_of_it() {
    let world = WorldGen::new(42);
    let source = public(&world);
    let eye = eye(&world, 10.0, 10.0);
    let down = Vec3::new(0.0, -1.0, 0.6).normalize();
    let dig = target(Action::Dig { tool: 0 }, eye, down, REACH, |v| source(v).density > 0.0).expect("Boden in Reichweite");
    assert!(source(dig.round().as_ivec3()).density > 0.0);
    let build = target(Action::Build, eye, down, REACH, |v| source(v).density > 0.0).expect("Boden in Reichweite");
    assert!((build - (dig - down * BUILD_RADIUS * 0.8)).length() < 1e-4);
    // Nach oben ist nichts in Reichweite.
    assert_eq!(target(Action::Dig { tool: 0 }, eye, Vec3::Y, REACH, |v| source(v).density > 0.0), None);
}

#[test]
fn building_into_the_own_body_is_detected() {
    let body = Settings::default();
    let eye = Vec3::new(0.0, 101.65, 0.0);
    assert!(hits_body(Vec3::new(0.3, 100.2, 0.0), eye, &body), "an den Füßen");
    assert!(hits_body(Vec3::new(0.0, 103.0, 0.0), eye, &body), "über dem Kopf");
    assert!(!hits_body(Vec3::new(2.5, 100.0, 0.0), eye, &body), "neben dem Körper");
    assert!(!hits_body(Vec3::new(0.0, 98.0, 0.0), eye, &body), "unter den Füßen");
}

/// Jeder Chunk, dessen Samples eine Änderung oder einen ihrer 26 Nachbarn enthalten (Erz-Filter), ist
/// in `affected_chunks`, auch an Chunk-Grenzen.
#[test]
fn affected_chunks_cover_every_changed_sample_and_its_neighbours() {
    let world = WorldGen::new(42);
    let source = public(&world);
    for center in [Vec3::new(31.6, 0.0, -0.4), Vec3::new(0.2, 31.7, 64.5), Vec3::new(-33.0, -1.2, 15.0)] {
        for action in [Action::Dig { tool: 3 }, Action::Build] {
            let center = Vec3::new(center.x, world.column_height(center.x, center.z) + center.y.fract(), center.z);
            let mut edits = WorldEdits::default();
            let outcome = apply(&mut edits, &world.materials, &source, center, action);
            assert!(!outcome.changed().is_empty(), "{action:?} bei {center} ändert nichts");
            let affected = affected_chunks(center, action);
            let (min, max) = voxel_bounds(center, action);
            for z in min.z..=max.z {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        let voxel = IVec3::new(x, y, z);
                        if edits.get(voxel).is_none() {
                            continue;
                        }
                        // Der Erz-Filter prüft alle 26 Nachbarn.
                        for n in 0..27 {
                            let offset = IVec3::new(n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1);
                            for chunk in chunks_containing(voxel + offset) {
                                assert!(affected.contains(&chunk), "{action:?} bei {center}: {chunk} fehlt");
                            }
                        }
                    }
                }
            }
            for chunk in outcome.changed() {
                assert!(affected.contains(chunk));
            }
        }
    }
}
