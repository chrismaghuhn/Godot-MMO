//! Schwung einer Waffe (#62, D-023): Richtung, Bahn der Klinge und Geschwindigkeit an jedem Punkt.
//!
//! Die Bahn ist im Blick-Rahmen des Spielers festgelegt (Auge, rechts, oben, vorn) und hängt nur von
//! Richtung und Fortschritt `s` (0 = ausgeholt, 1 = durchgezogen) ab. Die Zeit dafür kommt aus der
//! Waffe (`Weapon::strike`). Client (Anzeige) und Server (Trefferprüfung) rechnen dieselbe Bahn.

use glam::Vec3;

use super::weapon::Weapon;

/// Schlagrichtung, gewählt mit der Mausbewegung beim Ausholen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Von oben nach unten.
    Overhead,
    /// Von links kommend nach rechts.
    FromLeft,
    /// Von rechts kommend nach links.
    FromRight,
    /// Stich geradeaus.
    Thrust,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Direction::Overhead, Direction::FromLeft, Direction::FromRight, Direction::Thrust];

    /// Aus der Mausbewegung beim Ausholen (x rechts, y nach unten wie in Godot): nach oben = von
    /// oben, nach unten = Stich, zur Seite = von dieser Seite.
    pub fn from_mouse(dx: f32, dy: f32) -> Direction {
        if dx.abs() > dy.abs() {
            if dx > 0.0 { Direction::FromRight } else { Direction::FromLeft }
        } else if dy < 0.0 {
            Direction::Overhead
        } else {
            Direction::Thrust
        }
    }

    /// Feste Nummer (`ALL`), z. B. fürs Netz.
    pub fn id(self) -> u8 {
        Self::ALL.iter().position(|d| *d == self).unwrap_or(0) as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.get(usize::from(id)).copied()
    }

    pub fn is_thrust(self) -> bool {
        self == Direction::Thrust
    }
}

/// Blick-Rahmen: Auge und Achsen (aus Gier und Neigung wie bei der Bewegung: `yaw = 0` blickt nach
/// −z, `pitch` nach oben positiv).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub forward: Vec3,
}

impl View {
    pub fn new(eye: Vec3, yaw: f32, pitch: f32) -> Self {
        let pitch = pitch.clamp(-1.5, 1.5);
        let forward = Vec3::new(-yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let up = right.cross(forward);
        Self { eye, right, up, forward }
    }
}

/// Lage der Waffe: Hand und Richtung der Klinge (von der Hand zur Spitze, Länge 1), Welt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub hand: Vec3,
    pub blade: Vec3,
}

/// Schulter relativ zum Auge (rechts, oben) und Armlänge (m). Später aus dem Charakter (#24).
const SHOULDER: (f32, f32) = (0.22, -0.3);
const ARM: f32 = 0.55;

fn smooth(s: f32) -> f32 {
    let s = s.clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Lage der Waffe im Schlag bei Fortschritt `s` (0..1). Von oben und der Stich gehen auf die
/// Körpermitte (die Hand führt zur Mitte), seitliche Schläge kommen aus der Schulter.
pub fn pose(view: &View, direction: Direction, s: f32) -> Pose {
    let shoulder = match direction {
        Direction::Overhead | Direction::Thrust => view.eye + view.right * 0.05 + view.up * SHOULDER.1,
        _ => view.eye + view.right * SHOULDER.0 + view.up * SHOULDER.1,
    };
    let t = smooth(s);
    let (blade, hand_reach) = match direction {
        Direction::Overhead => {
            let angle = (110.0f32 + (-40.0 - 110.0) * t).to_radians();
            (view.forward * angle.cos() + view.up * angle.sin(), 0.8)
        }
        Direction::FromLeft => {
            let angle = (100.0f32 + (-70.0 - 100.0) * t).to_radians();
            ((view.forward * angle.cos() - view.right * angle.sin() - view.up * 0.15).normalize(), 0.85)
        }
        Direction::FromRight => {
            let angle = (100.0f32 + (-70.0 - 100.0) * t).to_radians();
            ((view.forward * angle.cos() + view.right * angle.sin() - view.up * 0.15).normalize(), 0.85)
        }
        // Arm von angezogen bis gestreckt, dazu ein Ausfallschritt.
        Direction::Thrust => (view.forward, -0.1 + 1.4 * t),
    };
    let arm = if direction.is_thrust() { view.forward * ARM * hand_reach } else { blade * ARM * hand_reach };
    Pose { hand: shoulder + arm, blade }
}

/// Ruhelage ohne Schlag: Klinge schräg nach vorn oben.
pub fn rest(view: &View) -> Pose {
    let shoulder = view.eye + view.right * SHOULDER.0 + view.up * SHOULDER.1;
    Pose { hand: shoulder + view.forward * 0.3, blade: (view.forward + view.up * 1.2).normalize() }
}

/// Lage beim Ausholen (`w` 0..1 von der Ruhelage bis ganz ausgeholt), für die Anzeige.
pub fn windup_pose(view: &View, direction: Direction, w: f32) -> Pose {
    let (from, to) = (rest(view), pose(view, direction, 0.0));
    let w = smooth(w);
    Pose { hand: from.hand.lerp(to.hand, w), blade: from.blade.lerp(to.blade, w).normalize() }
}

/// Lage bei der Erholung (`w` 0..1 von der Endlage des Schlags zurück in die Ruhe), für die Anzeige:
/// beginnt genau dort, wo der Schlag endet (Review-Fund: die Klinge sprang zur Ausgangslage).
pub fn recovery_pose(view: &View, direction: Direction, w: f32) -> Pose {
    let (from, to) = (pose(view, direction, 1.0), rest(view));
    let w = smooth(w);
    Pose { hand: from.hand.lerp(to.hand, w), blade: from.blade.lerp(to.blade, w).normalize() }
}

/// Geschwindigkeit (m/s) des Punkts im Abstand `r` von der Hand bei Fortschritt `s`.
pub fn speed(view: &View, weapon: &Weapon, direction: Direction, s: f32, r: f32) -> f32 {
    let point = |s: f32| {
        let pose = pose(view, direction, s);
        pose.hand + pose.blade * r
    };
    let ds = 0.01;
    let (a, b) = (point((s - ds).max(0.0)), point((s + ds).min(1.0)));
    let span = (s + ds).min(1.0) - (s - ds).max(0.0);
    a.distance(b) / (span * weapon.strike(direction.is_thrust()))
}
