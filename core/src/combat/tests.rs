//! Tests des Nahkampfs: Physik der Waffen, Bahnen, Trefferzonen.

use glam::Vec3;

use super::*;

fn view() -> View {
    View::new(Vec3::new(0.0, 1.65, 0.0), 0.0, 0.0)
}

/// Puppe `distance` m vor dem Spieler (Blick nach −z), ihm zugewandt.
fn dummy(distance: f32) -> Target {
    Target { feet: Vec3::new(0.0, 0.0, -distance), yaw: std::f32::consts::PI }
}

#[test]
fn heavier_weapons_are_slower_but_cost_more() {
    let [sword, axe, spear] = WEAPONS;
    assert!(sword.windup() < axe.windup() && axe.windup() < spear.windup());
    assert!(sword.stamina_cost(false) < axe.stamina_cost(false));
    for weapon in WEAPONS {
        assert!(weapon.stamina_cost(true) < weapon.stamina_cost(false));
        assert!((0.3..1.0).contains(&weapon.windup()), "{}: {}", weapon.name, weapon.windup());
        assert!(weapon.swing_mass(weapon.reach) < weapon.thrust_mass());
    }
}

#[test]
fn the_view_frame_matches_the_movement_convention() {
    let v = View::new(Vec3::ZERO, 0.0, 0.0);
    assert!((v.forward - Vec3::NEG_Z).length() < 1e-5 && (v.right - Vec3::X).length() < 1e-5 && (v.up - Vec3::Y).length() < 1e-5);
    let v = View::new(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.0);
    assert!((v.forward - Vec3::NEG_X).length() < 1e-5);
}

#[test]
fn mouse_movement_picks_the_direction() {
    assert_eq!(Direction::from_mouse(0.0, -5.0), Direction::Overhead);
    assert_eq!(Direction::from_mouse(0.0, 5.0), Direction::Thrust);
    assert_eq!(Direction::from_mouse(-5.0, 1.0), Direction::FromLeft);
    assert_eq!(Direction::from_mouse(5.0, -1.0), Direction::FromRight);
}

/// Jede Richtung trifft die Puppe in Reichweite, in einer passenden Zone; zu weit weg nicht.
#[test]
fn each_direction_hits_where_it_should() {
    for weapon in WEAPONS {
        for direction in Direction::ALL {
            // Der Stich trifft erst gegen Ende mit voller Wucht, ein Schwung schon nah am Körper.
            let close = dummy(0.6 + weapon.reach * if direction.is_thrust() { 0.9 } else { 0.7 });
            let hit = strike(&view(), &weapon, direction, &[close]).unwrap_or_else(|| panic!("{} {direction:?} trifft nicht", weapon.name));
            assert!(hit.total() > 0.0 && hit.speed > 1.0, "{} {direction:?}: {hit:?}", weapon.name);
            let expected: &[Zone] = match direction {
                Direction::Overhead => &[Zone::Head, Zone::Torso, Zone::LeftArm, Zone::RightArm],
                Direction::Thrust => &[Zone::Torso, Zone::LeftArm, Zone::RightArm, Zone::Head],
                // Von links kommend trifft zuerst die rechte Seite der Puppe (sie schaut uns an).
                Direction::FromLeft => &[Zone::RightArm, Zone::Torso, Zone::Head],
                Direction::FromRight => &[Zone::LeftArm, Zone::Torso, Zone::Head],
            };
            assert!(expected.contains(&hit.zone), "{} {direction:?} traf {:?}", weapon.name, hit.zone);
        }
        let far = dummy(0.6 + weapon.reach + 1.0);
        for direction in Direction::ALL {
            assert_eq!(strike(&view(), &weapon, direction, &[far]), None, "{} {direction:?} trifft zu weit", weapon.name);
        }
    }
}

/// Die Axt schlägt härter als das Schwert (Masse am Kopf), der Speer sticht am weitesten.
#[test]
fn weapons_feel_different() {
    let [sword, axe, spear] = WEAPONS;
    let target = [dummy(1.1)];
    let overhead = |weapon: &Weapon| strike(&view(), weapon, Direction::Overhead, &target).unwrap().total();
    assert!(overhead(&axe) > overhead(&sword), "Axt {} vs. Schwert {}", overhead(&axe), overhead(&sword));
    let reach = [dummy(2.0)];
    assert!(strike(&view(), &spear, Direction::Thrust, &reach).is_some());
    assert!(strike(&view(), &sword, Direction::Thrust, &reach).is_none());
    // Stich mit dem Speer: vor allem Stich-Schaden.
    assert_eq!(strike(&view(), &spear, Direction::Thrust, &reach).unwrap().kind(), "Stich");
    assert_eq!(strike(&view(), &sword, Direction::FromLeft, &target).unwrap().kind(), "Schnitt");
}

/// Kopftreffer zählen mehr als Beintreffer bei sonst gleichem Impuls.
#[test]
fn zones_weigh_the_damage() {
    assert!(Zone::Head.factor() > Zone::Torso.factor() && Zone::Torso.factor() > Zone::LeftLeg.factor());
}

/// Die Puppe dreht mit: Von hinten getroffen ist links und rechts vertauscht.
#[test]
fn targets_rotate_their_zones() {
    let facing = Target { feet: Vec3::ZERO, yaw: std::f32::consts::PI };
    let away = Target { feet: Vec3::ZERO, yaw: 0.0 };
    assert!((facing.zone(Zone::LeftArm).0.x - away.zone(Zone::RightArm).0.x).abs() < 1e-5);
}

#[test]
fn stamina_is_spent_and_regenerates() {
    let mut stamina = Stamina::default();
    let cost = WEAPONS[1].stamina_cost(false);
    let mut swings = 0;
    while stamina.spend(cost) {
        swings += 1;
    }
    assert!((3..10).contains(&swings), "{swings} Schwünge");
    stamina.regenerate(2.5);
    assert!(stamina.spend(cost));
}


/// Tempo und Ausdauer: zu früh nach einem Schlag oder zu erschöpft geht nicht.
#[test]
fn fighters_respect_timing_and_stamina() {
    let sword = WEAPONS[0];
    let mut fighter = Fighter::default();
    assert_eq!(fighter.release(10.0, &sword, Direction::Overhead), Ok(()));
    assert_eq!(fighter.release(10.2, &sword, Direction::Overhead), Err("zu früh"));
    let next = 10.0 + f64::from(sword.strike(false) + sword.recovery() + sword.windup());
    // Ohne neues Ausholen nach der Erholung: noch zu früh.
    assert_eq!(fighter.release(next - 0.1, &sword, Direction::Overhead), Err("zu früh"));
    assert_eq!(fighter.release(next, &sword, Direction::Overhead), Ok(()));
    // Im Dauerschlagen wird man müde, mit der schweren Axt früher als mit dem Schwert; nach einer
    // Pause geht es weiter.
    let swings_until_tired = |weapon: &Weapon| {
        let mut fighter = Fighter::default();
        let mut now = 0.0;
        let mut swings = 0;
        while fighter.release(now, weapon, Direction::Overhead).is_ok() {
            swings += 1;
            now += f64::from(weapon.strike(false) + weapon.recovery() + weapon.windup());
            assert!(swings < 200, "{} nie erschöpft", weapon.name);
        }
        assert_eq!(fighter.release(now, weapon, Direction::Overhead), Err("zu erschöpft"));
        assert_eq!(fighter.release(now + 3.0, weapon, Direction::Overhead), Ok(()));
        swings
    };
    let (sword, axe) = (swings_until_tired(&WEAPONS[0]), swings_until_tired(&WEAPONS[1]));
    assert!(axe < sword && axe >= 6, "Axt {axe}, Schwert {sword} Schläge");
}

#[test]
fn the_training_dummy_stands_in_view_of_the_spawn() {
    let world = crate::WorldGen::new(42);
    let spawn = crate::spawn::find_spawn(&world);
    let dummies = training_dummies(&world);
    assert_eq!(dummies.len(), 1);
    let dummy = dummies[0];
    let distance = (dummy.feet - spawn).length();
    assert!((3.0..8.0).contains(&distance), "{distance}");
    assert!((dummy.feet.y - world.column_height(dummy.feet.x, dummy.feet.z)).abs() < 0.01);
    // Sie schaut zum Spawn.
    let facing = glam::Vec3::new(-dummy.yaw.sin(), 0.0, -dummy.yaw.cos());
    assert!(facing.dot((spawn - dummy.feet).with_y(0.0).normalize()) > 0.99);
}

/// Nach einem Wechsel zählt die Ausholzeit der neuen Waffe (Review-Fund: Axt → Schwert wurde als
/// „zu früh“ abgelehnt, obwohl mit dem Schwert fertig ausgeholt war).
#[test]
fn switching_weapons_uses_the_new_windup() {
    for (from, to) in [(1, 0), (2, 0), (2, 1), (0, 1), (0, 2)] {
        let (a, b) = (WEAPONS[from], WEAPONS[to]);
        let mut fighter = Fighter::default();
        assert_eq!(fighter.release(0.0, &a, Direction::Overhead), Ok(()));
        let ready = f64::from(a.strike(false) + a.recovery() + b.windup());
        assert_eq!(fighter.release(ready, &b, Direction::Overhead), Ok(()), "{} → {}", a.name, b.name);
        let mut fighter = Fighter::default();
        fighter.release(0.0, &a, Direction::Overhead).unwrap();
        assert_eq!(fighter.release(ready - 0.1, &b, Direction::Overhead), Err("zu früh"), "{} → {}", a.name, b.name);
    }
}

/// Die Erholung beginnt, wo der Schlag endet, und endet in der Ruhelage.
#[test]
fn recovery_continues_from_the_end_of_the_strike() {
    for direction in Direction::ALL {
        let (end, start) = (pose(&view(), direction, 1.0), recovery_pose(&view(), direction, 0.0));
        assert!(end.hand.distance(start.hand) < 1e-5 && end.blade.distance(start.blade) < 1e-5, "{direction:?}");
        let done = recovery_pose(&view(), direction, 1.0);
        assert!(done.hand.distance(rest(&view()).hand) < 1e-5);
    }
}

/// Die Spitze kommt je Prüfschritt höchstens so weit, dass keine Zone durchrutscht, auch mit dem
/// langen Speer (Review-Fund: bis 0,18 m bei 48 festen Schritten, dünnste Zone 0,2 m).
#[test]
fn strike_steps_are_finer_than_the_thinnest_zone() {
    let thinnest = Zone::ALL.iter().map(|zone| Target { feet: Vec3::ZERO, yaw: 0.0 }.zone(*zone).2).fold(f32::MAX, f32::min) * 2.0;
    for weapon in WEAPONS {
        for direction in Direction::ALL {
            let steps = super::steps(&view(), &weapon, direction);
            let tip = |s: f32| {
                let p = pose(&view(), direction, s);
                p.hand + p.blade * weapon.reach
            };
            let longest = (0..steps).map(|i| tip(i as f32 / steps as f32).distance(tip((i + 1) as f32 / steps as f32))).fold(0.0, f32::max);
            assert!(longest < thinnest * 0.5, "{} {direction:?}: {longest} m je Schritt", weapon.name);
        }
    }
}

/// Liegt die Klinge schon beim Ausholen am Ziel, gibt es keinen Treffer ohne Wucht (Review-Fund).
#[test]
fn contact_without_speed_is_no_hit() {
    let spear = WEAPONS[2];
    for distance in [0.9, 1.1, 1.3] {
        if let Some(hit) = strike(&view(), &spear, Direction::Thrust, &[dummy(distance)]) {
            assert!(hit.speed >= MIN_SPEED, "Stich auf {distance} m: {hit:?}");
        }
    }
}
