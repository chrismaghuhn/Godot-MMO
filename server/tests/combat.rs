//! Kampf über eine echte TCP-Verbindung (#62): Puppen kommen nach der Anmeldung, der Server rechnet
//! Schläge und Treffer und prüft Tempo und Waffe.

mod common;

use std::time::{Duration, Instant};

use glam::Vec3;
use terraforge_core::WorldGen;
use terraforge_core::combat::{Direction, Zone};
use terraforge_protocol::{AttackOutcome, NetworkSource};

use common::{SEED, start_server};

fn attack(source: &NetworkSource, eye: Vec3, yaw: f32, weapon: u8, direction: Direction) -> AttackOutcome {
    let request = source.send_attack(eye.to_array(), yaw, 0.0, weapon, direction.id()).expect("gesendet");
    let start = Instant::now();
    loop {
        if let Some((_, outcome)) = source.take_struck().into_iter().find(|(r, _)| *r == request) {
            return outcome;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "keine Antwort");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn strikes_hit_the_training_dummy() {
    let (_server, source) = start_server();
    let world = WorldGen::new(SEED);
    let start = Instant::now();
    let dummies = loop {
        if let Some(dummies) = source.take_dummies() {
            break dummies;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "keine Puppen");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(dummies.len(), 1);
    let feet = Vec3::from_array(dummies[0].0);
    // 1,2 m vor der Puppe stehen und sie ansehen.
    let toward = (feet - Vec3::new(feet.x + 1.2, feet.y, feet.z)).normalize();
    let stand = feet - toward * 1.2;
    let eye = Vec3::new(stand.x, world.column_height(stand.x, stand.z) + 1.65, stand.z);
    let yaw = (-toward.x).atan2(-toward.z);
    let AttackOutcome::Swung { stamina, hit: Some(hit) } = attack(&source, eye, yaw, 0, Direction::Overhead) else {
        panic!("Schlag von oben trifft die Puppe nicht");
    };
    assert!(stamina < 100.0 && hit.dummy == 0 && hit.damage.iter().sum::<f32>() > 1.0, "{hit:?}");
    assert!(matches!(Zone::from_id(hit.zone), Some(Zone::Head | Zone::Torso | Zone::LeftArm | Zone::RightArm)), "{hit:?}");
    // Gleich noch einmal: zu früh.
    assert_eq!(attack(&source, eye, yaw, 0, Direction::Overhead), AttackOutcome::Refused { reason: "zu früh".into() });
    // Nach Erholung und Ausholen, aber abgewandt: geschlagen, nichts getroffen.
    std::thread::sleep(Duration::from_millis(1100));
    assert!(matches!(attack(&source, eye, yaw + std::f32::consts::PI, 0, Direction::FromLeft), AttackOutcome::Swung { hit: None, .. }));
    assert_eq!(attack(&source, eye, yaw, 7, Direction::Thrust), AttackOutcome::Refused { reason: "unbekannte Waffe oder Richtung".into() });
    assert_eq!(attack(&source, Vec3::splat(f32::NAN), yaw, 0, Direction::Thrust), AttackOutcome::Refused { reason: "ungültige Position oder Richtung".into() });
}
