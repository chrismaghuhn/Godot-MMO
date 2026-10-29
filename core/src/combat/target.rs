//! Ziele mit Trefferzonen (#62): vorerst die Trainingspuppe, später Gegner und Spieler. Zonen sind
//! Kapseln relativ zum Fußpunkt; ihre Maße entsprechen dem Körper aus `movement` (1,8 m).

use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    Head,
    Torso,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl Zone {
    pub const ALL: [Zone; 6] = [Zone::Head, Zone::Torso, Zone::LeftArm, Zone::RightArm, Zone::LeftLeg, Zone::RightLeg];

    /// Feste Nummer (`ALL`), z. B. fürs Netz.
    pub fn id(self) -> u8 {
        Self::ALL.iter().position(|z| *z == self).unwrap_or(0) as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.get(usize::from(id)).copied()
    }

    /// Wie stark ein Treffer hier wirkt (Rumpf = 1).
    pub fn factor(self) -> f32 {
        match self {
            Zone::Head => 2.0,
            Zone::Torso => 1.0,
            Zone::LeftArm | Zone::RightArm => 0.5,
            Zone::LeftLeg | Zone::RightLeg => 0.6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Zone::Head => "Kopf",
            Zone::Torso => "Rumpf",
            Zone::LeftArm => "linker Arm",
            Zone::RightArm => "rechter Arm",
            Zone::LeftLeg => "linkes Bein",
            Zone::RightLeg => "rechtes Bein",
        }
    }

    /// Kapsel (Anfang, Ende relativ zum Fuß in Blickrichtung −z, Radius) eines aufrecht stehenden
    /// Körpers: x rechts, y oben.
    fn capsule(self) -> (Vec3, Vec3, f32) {
        match self {
            Zone::Head => (Vec3::new(0.0, 1.57, 0.0), Vec3::new(0.0, 1.7, 0.0), 0.12),
            Zone::Torso => (Vec3::new(0.0, 0.98, 0.0), Vec3::new(0.0, 1.38, 0.0), 0.19),
            Zone::LeftArm => (Vec3::new(-0.3, 0.95, 0.0), Vec3::new(-0.3, 1.38, 0.0), 0.07),
            Zone::RightArm => (Vec3::new(0.3, 0.95, 0.0), Vec3::new(0.3, 1.38, 0.0), 0.07),
            Zone::LeftLeg => (Vec3::new(-0.1, 0.08, 0.0), Vec3::new(-0.1, 0.88, 0.0), 0.09),
            Zone::RightLeg => (Vec3::new(0.1, 0.08, 0.0), Vec3::new(0.1, 0.88, 0.0), 0.09),
        }
    }
}

/// Ein Ziel: Fußpunkt und Blickrichtung (`yaw` wie beim Spieler).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub feet: Vec3,
    pub yaw: f32,
}

impl Target {
    /// Kapsel einer Zone in Weltkoordinaten.
    pub fn zone(&self, zone: Zone) -> (Vec3, Vec3, f32) {
        let (a, b, radius) = zone.capsule();
        let (sin, cos) = self.yaw.sin_cos();
        let rotate = |p: Vec3| Vec3::new(p.x * cos + p.z * sin, p.y, -p.x * sin + p.z * cos);
        (self.feet + rotate(a), self.feet + rotate(b), radius)
    }
}

/// Kürzester Abstand zweier Strecken und die Stelle auf der ersten (Anteil 0..1).
pub(super) fn segment_distance(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (f32, f32) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (mut s, mut t);
    if a <= 1e-9 && e <= 1e-9 {
        return (p1.distance(p2), 0.0);
    }
    if a <= 1e-9 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-9 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            s = if denom > 1e-9 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    ((p1 + d1 * s).distance(p2 + d2 * t), s)
}
