//! Kollision der Kapsel mit dem Dichtefeld.
//!
//! Die Kapsel wird durch drei Kugeln angenähert (unten, Mitte, oben). Für jede Kugel liefern Dichte
//! und Gradient des trilinear interpolierten Felds einen Abstand zur Fläche (`-dichte / |∇|`, die
//! Dichte ist nahe der Oberfläche ungefähr ein Abstand in Metern) und eine Normale (`-∇/|∇|`). Liegt
//! die Kugel zu tief, wird sie entlang der Normalen herausgeschoben und die Geschwindigkeit in die
//! Fläche hinein entfernt. Berührte Wände merkt sich der Körper für den nächsten Schritt (Gleiten),
//! sonst drückte jede neue Eingabe wieder hinein und die Kamera zitterte (Review-Fund).

use glam::{IVec3, Vec3};

use super::{DensitySource, Settings};

/// Wiederholungen pro Teilschritt (Ecken und Kanten brauchen mehrere).
const ITERATIONS: usize = 4;
/// Abstand (m) zu Boden oder Wand, bis zu dem eine Kugel sie noch berührt.
const CONTACT_TOLERANCE: f32 = 0.05;
/// So tief (m) darf eine Kugel nach dem Herausschieben noch stecken, ohne dass der Körper als
/// eingeklemmt gilt.
const STUCK_TOLERANCE: f32 = 0.02;

/// Dichte und Gradient des trilinear interpolierten Felds an `point`.
pub(super) fn sample(field: &impl DensitySource, point: Vec3) -> (f32, Vec3) {
    let base = point.floor();
    let f = point - base;
    let base = base.as_ivec3();
    let d = |x: i32, y: i32, z: i32| field.density(base + IVec3::new(x, y, z));
    let c = [d(0, 0, 0), d(1, 0, 0), d(0, 1, 0), d(1, 1, 0), d(0, 0, 1), d(1, 0, 1), d(0, 1, 1), d(1, 1, 1)];
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    // Entlang x interpolieren, dann y, dann z; Ableitungen aus denselben Ecken.
    let x00 = lerp(c[0], c[1], f.x);
    let x10 = lerp(c[2], c[3], f.x);
    let x01 = lerp(c[4], c[5], f.x);
    let x11 = lerp(c[6], c[7], f.x);
    let y0 = lerp(x00, x10, f.y);
    let y1 = lerp(x01, x11, f.y);
    let value = lerp(y0, y1, f.z);

    let dx0 = lerp(c[1] - c[0], c[3] - c[2], f.y);
    let dx1 = lerp(c[5] - c[4], c[7] - c[6], f.y);
    let dy = lerp(x10 - x00, x11 - x01, f.z);
    let gradient = Vec3::new(lerp(dx0, dx1, f.z), dy, y1 - y0);
    (value, gradient)
}

/// Ergebnis des Herausschiebens.
#[derive(Default)]
pub(super) struct Contact {
    /// Die untere Kugel steht auf begehbarem Boden (Normale nicht steiler als `max_slope`).
    pub(super) ground: bool,
    /// Waagerechte Normalen berührter Wände und Steilhänge (höchstens zwei, z. B. eine Ecke).
    pub(super) walls: Vec<Vec3>,
    /// Die Kapsel steckt danach noch im Gelände (z. B. Gang niedriger als der Körper).
    pub(super) stuck: bool,
}

/// Distanz zur Fläche und Normale (nach außen) an `point`; `None`, wo das Feld flach ist.
pub(super) fn surface(field: &impl DensitySource, point: Vec3) -> Option<(f32, Vec3)> {
    let (density, gradient) = sample(field, point);
    let length = gradient.length();
    (length >= 1e-4).then(|| (-density / length, -gradient / length))
}

/// Schiebt die Kapsel mit Fußpunkt `position` aus dem Gelände und entfernt die Geschwindigkeit in
/// die Flächen hinein. Die Kontakte (Boden, Wände) zählen danach mit einer kleinen Toleranz: Eine
/// Wand knapp vor dem Körper gilt weiter als berührt, sonst liefe er im nächsten Schritt wieder
/// hinein (Review-Fund: Zittern an Wänden, weil die Kante zwischen Boden und Wand im
/// interpolierten Feld eine 45°-Rampe ist, die der Körper immer wieder hinaufläuft).
pub(super) fn resolve(position: &mut Vec3, velocity: &mut Vec3, settings: &Settings, field: &impl DensitySource) -> Contact {
    let r = settings.radius;
    let centers = [r, settings.height * 0.5, settings.height - r];
    let mut contact = Contact::default();
    for _ in 0..ITERATIONS {
        let mut moved = false;
        for &offset in &centers {
            let Some((distance, normal)) = surface(field, *position + Vec3::Y * offset) else { continue };
            if distance >= r {
                continue;
            }
            *position += normal * (r - distance);
            let into = velocity.dot(normal);
            if into < 0.0 {
                *velocity -= normal * into;
            }
            add_wall(&mut contact, normal, settings);
            moved = true;
        }
        if !moved {
            break;
        }
    }
    for (index, &offset) in centers.iter().enumerate() {
        let Some((distance, normal)) = surface(field, *position + Vec3::Y * offset) else { continue };
        if distance < r - STUCK_TOLERANCE {
            contact.stuck = true;
        }
        if distance >= r + CONTACT_TOLERANCE {
            continue;
        }
        contact.ground |= index == 0 && normal.y >= settings.max_slope;
        add_wall(&mut contact, normal, settings);
    }
    contact
}

/// Merkt sich eine Wand oder einen Steilhang (Normale steiler als `max_slope`) als waagerechte
/// Normale; höchstens zwei verschiedene.
fn add_wall(contact: &mut Contact, normal: Vec3, settings: &Settings) {
    if normal.y >= settings.max_slope {
        return;
    }
    let wall = Vec3::new(normal.x, 0.0, normal.z).normalize_or_zero();
    if wall != Vec3::ZERO && contact.walls.len() < 2 && contact.walls.iter().all(|w| w.dot(wall) < 0.99) {
        contact.walls.push(wall);
    }
}
