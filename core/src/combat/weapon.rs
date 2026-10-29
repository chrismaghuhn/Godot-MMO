//! Waffen (#62, D-023): vorerst feste Platzhalter, im Datenformat, das später das Schmieden füllt
//! (D-019). Was sich im Kampf anfühlt, folgt aus der Physik: Die Trägheit um die Hand bestimmt
//! Ausholzeit, Schlagdauer, Ausdauer und Wucht. Kopflastig (Axt) ist langsamer, aber wuchtiger.

/// Wie eine Waffe trifft: Anteile von Schnitt, Stich und Wucht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    pub cut: f32,
    pub pierce: f32,
    pub blunt: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponKind {
    Sword,
    Axe,
    Spear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weapon {
    pub kind: WeaponKind,
    pub name: &'static str,
    /// Von der Hand bis zur Spitze (m).
    pub reach: f32,
    /// Hinter der Hand (Knauf, Schaftende), für die Optik (m).
    pub butt: f32,
    /// Masse (kg).
    pub mass: f32,
    /// Abstand des Schwerpunkts von der Hand, zur Spitze hin (m).
    pub balance: f32,
    /// Ab hier trifft die Waffe (m von der Hand): Klinge, Axtkopf, Speerspitze.
    pub striking_from: f32,
    /// Wirkung im Schwung (oben, seitlich) und im Stich.
    pub swing: Damage,
    pub thrust: Damage,
}

/// Anteil von Arm (Schwung) bzw. Arm und Körper (Stich) an der wirksamen Masse (kg). Später aus dem
/// Charakter (#24).
const SWING_BODY_MASS: f32 = 0.5;
const THRUST_BODY_MASS: f32 = 1.0;

/// Platzhalter, bis das Schmieden Waffen liefert. Reale Richtwerte: Langschwert ≈ 1,3 kg,
/// Schwerpunkt eine Handbreit vor der Parierstange; Axt mit schwerem Kopf am Ende; Speer lang,
/// Schwerpunkt nahe der Hand (in der Mitte gegriffen).
pub const WEAPONS: [Weapon; 3] = [
    Weapon {
        kind: WeaponKind::Sword,
        name: "Schwert",
        reach: 0.95,
        butt: 0.2,
        mass: 1.3,
        balance: 0.12,
        striking_from: 0.15,
        swing: Damage { cut: 1.0, pierce: 0.0, blunt: 0.1 },
        thrust: Damage { cut: 0.0, pierce: 1.0, blunt: 0.0 },
    },
    Weapon {
        kind: WeaponKind::Axe,
        name: "Axt",
        reach: 0.7,
        butt: 0.1,
        mass: 1.6,
        balance: 0.45,
        striking_from: 0.55,
        swing: Damage { cut: 0.8, pierce: 0.0, blunt: 0.5 },
        thrust: Damage { cut: 0.0, pierce: 0.1, blunt: 0.3 },
    },
    Weapon {
        kind: WeaponKind::Spear,
        name: "Speer",
        reach: 1.5,
        butt: 0.7,
        mass: 1.8,
        balance: 0.25,
        striking_from: 1.25,
        swing: Damage { cut: 0.2, pierce: 0.0, blunt: 0.4 },
        thrust: Damage { cut: 0.0, pierce: 1.2, blunt: 0.0 },
    },
];

impl Weapon {
    /// Trägheitsmoment um die Hand (kg·m²): Masse am Schwerpunkt plus Verteilung über die Länge.
    pub fn inertia(&self) -> f32 {
        let length = self.reach + self.butt;
        self.mass * (self.balance * self.balance + length * length / 12.0)
    }

    /// Ausholen bis zum frühesten Schlag (s).
    pub fn windup(&self) -> f32 {
        0.25 + 0.5 * self.inertia().sqrt()
    }

    /// Dauer des Schlags selbst (s); ein Stich ist kürzer (nur gerade nach vorn).
    pub fn strike(&self, thrust: bool) -> f32 {
        let swing = 0.15 + 0.2 * self.inertia().sqrt();
        if thrust { swing * 0.7 } else { swing }
    }

    /// Erholung nach dem Schlag, bevor neu ausgeholt werden kann (s).
    pub fn recovery(&self) -> f32 {
        0.15 + 0.3 * self.inertia().sqrt()
    }

    /// Ausdauer je Schwung; ein Stich kostet weniger (nur der Arm, kein Körperschwung).
    pub fn stamina_cost(&self, thrust: bool) -> f32 {
        let cost = 8.0 + 20.0 * self.inertia();
        if thrust { cost * 0.7 } else { cost }
    }

    /// Wirksame Masse im Schwung an einem Punkt im Abstand `r` von der Hand (kg): die Waffe (weiter
    /// vorn weniger, sie dreht leichter weiter) plus der Arm, der sie führt.
    pub fn swing_mass(&self, r: f32) -> f32 {
        let r = r.max(0.1);
        (self.inertia() / (r * r)).min(self.mass) + SWING_BODY_MASS
    }

    /// Wirksame Masse im Stich (kg): die ganze Waffe plus Arm und Körper, die nachschieben.
    pub fn thrust_mass(&self) -> f32 {
        self.mass + THRUST_BODY_MASS
    }
}
