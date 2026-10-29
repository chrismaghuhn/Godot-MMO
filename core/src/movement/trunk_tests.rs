//! Kollision mit Baumstämmen (#58) auf flachem Boden.

use glam::{IVec3, Vec2, Vec3, Vec3Swizzles};

use super::super::{Body, DensitySource, Input, Settings, Trunk};
use super::{DT, run, walk};

/// Flacher Boden mit Baumstämmen (#58).
struct Forest(Vec<Trunk>);

impl DensitySource for Forest {
    fn density(&self, voxel: IVec3) -> f32 {
        10.0 - voxel.y as f32
    }

    fn trunks(&self, center: Vec3, radius: f32) -> Vec<Trunk> {
        self.0.iter().copied().filter(|t| t.base.xz().distance(center.xz()) <= radius + t.radius).collect()
    }
}

fn trunk_at(x: f32, z: f32) -> Trunk {
    Trunk { base: Vec3::new(x, 10.0, z), radius: 0.3, height: 4.0 }
}

/// Frontal gegen einen Stamm: Der Körper bleibt davor stehen, ohne einzudringen, zu zittern oder
/// darum herumzukriechen, auch bei dünnen Stämmen (Review-Fund im Godot-Test: Die Wand-Prüfung
/// 0,6 m hinter dem Körper lag bei einem dünnen Stamm schon dahinter).
#[test]
fn a_trunk_stops_the_body() {
    for radius in [0.12, 0.3, 0.6] {
        // Ein paar Zentimeter neben der Laufrichtung, wie im Spiel fast immer.
        let forest = Forest(vec![Trunk { radius, ..trunk_at(0.55, -3.0) }]);
        let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
        run(&mut body, Input::default(), 0.5, &forest);
        let gap = Settings::default().radius + radius;
        run(&mut body, walk(1.0), 2.0, &forest);
        let distance = body.position.xz().distance(Vec2::new(0.55, -3.0));
        assert!(distance > gap - 0.02 && distance < gap + 0.1, "r {radius}: Abstand {distance} bei {}", body.position);
        let before = body.position;
        run(&mut body, walk(1.0), 2.0, &forest);
        assert!(body.position.distance(before) < 0.05, "r {radius}: zittert oder kriecht: {before} → {}", body.position);
        assert!(body.on_ground);
    }
}

/// Schräg auf einen Stamm zu: Der Körper gleitet vorbei und kommt nie hinein.
#[test]
fn the_body_slides_past_a_trunk() {
    let forest = Forest(vec![trunk_at(0.8, -3.0)]);
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, Input::default(), 0.5, &forest);
    let settings = Settings::default();
    let gap = settings.radius + 0.3;
    for _ in 0..180 {
        body.step(walk(1.0), DT, &settings, &forest);
        let distance = body.position.xz().distance(Vec2::new(0.8, -3.0));
        assert!(distance > gap - 0.02, "im Stamm: {distance} bei {}", body.position);
    }
    assert!(body.position.z < -6.0, "nicht vorbeigekommen: {}", body.position);
}
