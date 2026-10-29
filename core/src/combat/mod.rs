//! Nahkampf (#62, D-023, Design `docs/design/kampf.md`).
//!
//! - `weapon`: Waffen und ihre Physik (Trägheit → Zeiten, Ausdauer, Wucht)
//! - `swing`:  Schlagrichtung, Bahn der Klinge, Geschwindigkeit
//! - `target`: Ziele mit Trefferzonen (Trainingspuppe, später Gegner und Spieler)
//!
//! `strike` rechnet einen ganzen Schlag gegen Ziele: die Klinge entlang ihrer Bahn, der erste
//! Kontakt zählt. Der Schaden ist der Impuls am Kontaktpunkt (wirksame Masse von Waffe und Arm bzw.
//! Körper × Geschwindigkeit),
//! gewichtet nach Art (Schnitt, Stich, Wucht) und Zone. Gelände hält die Klinge vorerst nicht auf.

pub mod swing;
pub mod target;
pub mod weapon;

#[cfg(test)]
mod tests;

pub use swing::{Direction, Pose, View, pose, recovery_pose, rest, speed, windup_pose};
pub use target::{Target, Zone};
pub use weapon::{Damage, WEAPONS, Weapon, WeaponKind};

/// Größter Weg der Spitze je Prüfschritt (m): deutlich unter der dünnsten Zone (Arm 0,14 m plus
/// Klinge), sonst schlüpfte ein Streiftreffer durch (Review-Fund: bei 48 festen Schritten bis 0,18 m).
const MAX_STEP: f32 = 0.04;
/// Darunter (m/s) zählt ein Kontakt nicht: Liegt die Klinge schon beim Ausholen am Ziel, drückt sie
/// nur an (Review-Fund: „Treffer“ mit Schaden nahe 0).
pub const MIN_SPEED: f32 = 2.0;
/// Dicke der Klinge für die Trefferprüfung (m).
const BLADE_RADIUS: f32 = 0.03;

/// Ein Treffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Index des Ziels in der übergebenen Liste.
    pub target: usize,
    pub zone: Zone,
    /// Fortschritt des Schlags beim Kontakt (0..1).
    pub at: f32,
    /// Geschwindigkeit am Kontaktpunkt (m/s) und Impuls (kg·m/s).
    pub speed: f32,
    pub impulse: f32,
    /// Schaden nach Art: Impuls × Anteil × Zone.
    pub damage: Damage,
}

impl Hit {
    pub fn total(&self) -> f32 {
        self.damage.cut + self.damage.pierce + self.damage.blunt
    }

    /// Die vorherrschende Art, z. B. für die Anzeige.
    pub fn kind(&self) -> &'static str {
        damage_kind(&self.damage)
    }
}

/// Rechnet einen Schlag gegen `targets`; `None`, wenn die Klinge nichts trifft.
pub fn strike(view: &View, weapon: &Weapon, direction: Direction, targets: &[Target]) -> Option<Hit> {
    let samples = steps(view, weapon, direction);
    for step in 0..=samples {
        let s = step as f32 / samples as f32;
        let pose = pose(view, direction, s);
        let (from, to) = (pose.hand + pose.blade * weapon.striking_from, pose.hand + pose.blade * weapon.reach);
        let mut best: Option<(f32, usize, Zone, f32)> = None;
        for (index, target) in targets.iter().enumerate() {
            for zone in Zone::ALL {
                let (a, b, radius) = target.zone(zone);
                let (distance, along) = target::segment_distance(from, to, a, b);
                let depth = radius + BLADE_RADIUS - distance;
                if depth > 0.0 && best.is_none_or(|(d, ..)| depth > d) {
                    best = Some((depth, index, zone, along));
                }
            }
        }
        if let Some((_, target, zone, along)) = best {
            let r = weapon.striking_from + (weapon.reach - weapon.striking_from) * along;
            let speed = speed(view, weapon, direction, s, r);
            if speed < MIN_SPEED {
                continue;
            }
            let mass = if direction.is_thrust() { weapon.thrust_mass() } else { weapon.swing_mass(r) };
            let impulse = mass * speed;
            let kind = if direction.is_thrust() { weapon.thrust } else { weapon.swing };
            let scale = impulse * zone.factor();
            let damage = Damage { cut: kind.cut * scale, pierce: kind.pierce * scale, blunt: kind.blunt * scale };
            return Some(Hit { target, zone, at: s, speed, impulse, damage });
        }
    }
    None
}

/// Prüfschritte für einen Schlag: so viele, dass die Spitze je Schritt höchstens `MAX_STEP` weit kommt.
pub(crate) fn steps(view: &View, weapon: &Weapon, direction: Direction) -> usize {
    let tip = |s: f32| {
        let pose = pose(view, direction, s);
        pose.hand + pose.blade * weapon.reach
    };
    let length: f32 = (0..64).map(|i| tip(i as f32 / 64.0).distance(tip((i + 1) as f32 / 64.0))).sum();
    // Der schnellste Abschnitt (Mitte, Smoothstep) ist 1,5× so schnell wie der Durchschnitt.
    ((length * 1.5 / MAX_STEP).ceil() as usize).max(16)
}

/// Vorherrschende Art eines Schadens (Gleichstand: Stich vor Schnitt vor Wucht), für Anzeigen.
pub fn damage_kind(damage: &Damage) -> &'static str {
    if damage.pierce >= damage.cut && damage.pierce >= damage.blunt {
        "Stich"
    } else if damage.cut >= damage.blunt {
        "Schnitt"
    } else {
        "Wucht"
    }
}

/// Ausdauer: Jeder Schwung kostet (`Weapon::stamina_cost`), sie kommt mit der Zeit zurück.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamina {
    pub value: f32,
    pub max: f32,
}

/// Erholung je Sekunde. Im Dauerschlagen wird man müde (Speer nach ≈ 6 Schlägen bzw. 7 s, Axt nach
/// ≈ 13 bzw. 14 s, Schwert nach ≈ 25 bzw. 23 s), eine kurze Pause reicht für die nächsten.
pub const STAMINA_REGEN: f32 = 8.0;

impl Default for Stamina {
    fn default() -> Self {
        Self { value: 100.0, max: 100.0 }
    }
}

impl Stamina {
    pub fn regenerate(&mut self, seconds: f32) {
        self.value = (self.value + STAMINA_REGEN * seconds.max(0.0)).min(self.max);
    }

    /// Zieht `cost` ab, wenn genug da ist.
    pub fn spend(&mut self, cost: f32) -> bool {
        if self.value < cost {
            return false;
        }
        self.value -= cost;
        true
    }
}

/// Tempo und Ausdauer eines Kämpfers (Spieler, später Gegner). Die Zeit `now` in Sekunden kommt von
/// außen (Server-Uhr bzw. Client ohne Server).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fighter {
    pub stamina: Stamina,
    /// Ende von Schlag und Erholung des letzten Schlags; danach beginnt das neue Ausholen.
    recovered_at: f64,
    updated: Option<f64>,
}

impl Default for Fighter {
    fn default() -> Self {
        // Noch nie geschlagen: sofort bereit, auch wenn die Uhr bei 0 beginnt.
        Self { stamina: Stamina::default(), recovered_at: f64::NEG_INFINITY, updated: None }
    }
}

/// Spielraum für Netz und Uhren (s).
const TIMING_TOLERANCE: f64 = 0.05;

impl Fighter {
    /// Ausdauer bis `now` (Sekunden) nachführen.
    pub fn update(&mut self, now: f64) {
        if let Some(updated) = self.updated {
            self.stamina.regenerate((now - updated) as f32);
        }
        self.updated = Some(now);
    }

    /// Ein Schlag wird losgelassen: nur, wenn der letzte samt Erholung vorbei ist und seitdem mit
    /// *dieser* Waffe ausgeholt werden konnte (Review-Fund: mit der Ausholzeit der alten Waffe lehnte
    /// der Server nach einem Wechsel auf eine leichtere ab, und abwechselnde Waffen ließen sich
    /// ausnutzen), und genug Ausdauer da ist. Ob wirklich ausgeholt wurde, prüft das erst ab dem
    /// zweiten Schlag (vorerst kein eigenes „Ausholen beginnt“).
    pub fn release(&mut self, now: f64, weapon: &Weapon, direction: Direction) -> Result<(), &'static str> {
        self.update(now);
        if now + TIMING_TOLERANCE < self.recovered_at + f64::from(weapon.windup()) {
            return Err("zu früh");
        }
        if !self.stamina.spend(weapon.stamina_cost(direction.is_thrust())) {
            return Err("zu erschöpft");
        }
        self.recovered_at = now + f64::from(weapon.strike(direction.is_thrust()) + weapon.recovery());
        Ok(())
    }
}

/// Die Trainingspuppen der Welt (#62): eine, gut 4 m vor dem Spawnpunkt in Blickrichtung, dem Spawn
/// zugewandt, auf dem Boden.
pub fn training_dummies(world: &crate::WorldGen) -> Vec<Target> {
    let spawn = crate::spawn::find_spawn(world);
    let look = crate::spawn::spawn_look(world, spawn);
    let direction = (look - spawn).with_y(0.0).normalize_or(glam::Vec3::NEG_Z);
    let (x, z) = (spawn.x + direction.x * 4.5, spawn.z + direction.z * 4.5);
    let feet = glam::Vec3::new(x, world.column_height(x, z), z);
    // Blick zum Spawn: Richtung (−sin yaw, −cos yaw) = −direction.
    let yaw = direction.x.atan2(direction.z);
    vec![Target { feet, yaw }]
}
