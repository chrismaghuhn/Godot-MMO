//! Kampf auf dem Server (#62, D-023): Trainingspuppen, Tempo und Ausdauer des Spielers, Treffer.
//!
//! Der Client schickt nur, wann er mit welcher Waffe in welche Richtung losschlägt; Bahn und Treffer
//! rechnet der Server mit denselben Regeln (`combat::strike`). Vorerst ein Spieler (D-021), die Puppen
//! stehen fest (`combat::training_dummies`). Noch nicht geprüft: die Augenposition (wie beim Graben,
//! #25) und ob Gelände zwischen Klinge und Puppe liegt.

use std::sync::PoisonError;
use std::time::Instant;

use glam::Vec3;
use terraforge_core::combat::{Direction, Fighter, Target, View, WEAPONS, strike, training_dummies};
use terraforge_protocol::{AttackOutcome, HitInfo};

use crate::ServerWorld;
use crate::world::in_world;

/// Kampfzustand der Welt.
pub(crate) struct Combat {
    fighter: Fighter,
    pub(crate) dummies: Vec<Target>,
    start: Instant,
}

impl Combat {
    pub(crate) fn new(world: &terraforge_core::WorldGen) -> Self {
        Self { fighter: Fighter::default(), dummies: training_dummies(world), start: Instant::now() }
    }
}

fn refused(reason: &str) -> AttackOutcome {
    AttackOutcome::Refused { reason: reason.into() }
}

impl ServerWorld {
    /// Ein losgelassener Schlag, empfangen um `received`; Waffe und Richtung als Nummern vom Client.
    pub fn attack(&self, received: Instant, eye: [f32; 3], yaw: f32, pitch: f32, weapon: u8, direction: u8) -> AttackOutcome {
        let eye = Vec3::from_array(eye);
        if !eye.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
            return refused("ungültige Position oder Richtung");
        }
        let (Some(weapon), Some(direction)) = (WEAPONS.get(usize::from(weapon)), Direction::from_id(direction)) else {
            return refused("unbekannte Waffe oder Richtung");
        };
        if !in_world(terraforge_core::edits::owner_chunk(eye.floor().as_ivec3())) {
            return refused("außerhalb der Welt");
        }
        if self.eye_in_rock(eye) {
            return refused("Auge im Fels");
        }
        let mut combat = self.combat.lock().unwrap_or_else(PoisonError::into_inner);
        let now = received.saturating_duration_since(combat.start).as_secs_f64();
        if let Err(reason) = combat.fighter.release(now, weapon, direction) {
            return refused(reason);
        }
        let hit = strike(&View::new(eye, yaw, pitch), weapon, direction, &combat.dummies);
        AttackOutcome::Swung { stamina: combat.fighter.stamina.value, hit: hit.as_ref().map(HitInfo::from) }
    }

    /// Die Puppen für einen neu verbundenen Client.
    pub fn dummies(&self) -> Vec<([f32; 3], f32)> {
        let combat = self.combat.lock().unwrap_or_else(PoisonError::into_inner);
        combat.dummies.iter().map(|dummy| (dummy.feet.to_array(), dummy.yaw)).collect()
    }
}
