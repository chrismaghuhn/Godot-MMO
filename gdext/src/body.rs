//! `TerraforgeBody`: Bewegung und Kollision eines Körpers aus dem Core (#13).
//!
//! GDScript liefert die Eingabe und ruft `step` im festen Takt (`_physics_process`); Bewegung und
//! Kollision rechnet `terraforge_core::movement`, damit der Server später dasselbe rechnen kann
//! (D-003, D-021). Kollidiert wird mit dem Dichtefeld des Terrains (Chunks nahe der Kamera).

use godot::prelude::*;
use terraforge_core::movement::{Body, Input, Settings};

use crate::terrain::TerraforgeTerrain;

#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub(crate) struct TerraforgeBody {
    body: Body,
    settings: Settings,
}

fn vector(v: glam::Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

#[godot_api]
impl TerraforgeBody {
    /// Körper mit Fußpunkt `position` (Maße vorerst fest, später vom Charakter, #24).
    #[func]
    fn create(position: Vector3) -> Gd<Self> {
        Gd::from_object(Self { body: Body::new(glam::Vec3::new(position.x, position.y, position.z)), settings: Settings::default() })
    }

    /// Ein Schritt: `movement` x nach rechts, y nach vorn (Länge ≤ 1), `yaw` Blickrichtung (0 = −z).
    #[func]
    fn step(&mut self, terrain: Gd<TerraforgeTerrain>, movement: Vector2, yaw: f32, jump: bool, run: bool, delta: f32) {
        let input = Input { movement: glam::Vec2::new(movement.x, movement.y), yaw, jump, run };
        let terrain = terrain.bind();
        if let Some(streamer) = terrain.streamer() {
            self.body.step(input, delta, &self.settings, streamer);
        }
    }

    /// Fußpunkt.
    #[func]
    fn get_position(&self) -> Vector3 {
        vector(self.body.position)
    }

    /// Versetzt den Körper (z. B. nach dem Fliegen), Geschwindigkeit auf null.
    #[func]
    fn set_position(&mut self, position: Vector3) {
        self.body = Body::new(glam::Vec3::new(position.x, position.y, position.z));
    }

    #[func]
    fn get_velocity(&self) -> Vector3 {
        vector(self.body.velocity)
    }

    #[func]
    fn is_on_ground(&self) -> bool {
        self.body.on_ground
    }

    #[func]
    fn eye_height(&self) -> f32 {
        self.settings.eye_height
    }
}
