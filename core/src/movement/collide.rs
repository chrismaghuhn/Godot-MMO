//! Kollision der Kapsel mit dem Dichtefeld.
//!
//! Die Kapsel wird durch drei Kugeln angenähert (unten, Mitte, oben). Für jede Kugel liefern Dichte
//! und Gradient des trilinear interpolierten Felds einen Abstand zur Fläche (`-dichte / |∇|`, die
//! Dichte ist nahe der Oberfläche ungefähr ein Abstand in Metern) und eine Normale (`-∇/|∇|`). Liegt
//! die Kugel zu tief, wird sie entlang der Normalen herausgeschoben und die Geschwindigkeit in die
//! Fläche hinein entfernt. Berührte Wände merkt sich der Körper für den nächsten Schritt (Gleiten),
//! sonst drückte jede neue Eingabe wieder hinein und die Kamera zitterte (Review-Fund).
//!
//! Baumstämme (#58) sind Kapseln; Abstand und Normale kommen dort direkt aus der Geometrie, sonst
//! gilt dasselbe wie beim Gelände.

use glam::{IVec3, Vec3};

use super::{DensitySource, Settings, Trunk};

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

/// Abstand von `point` zur Oberfläche eines Stamms und Normale nach außen.
fn trunk_surface(trunk: &Trunk, point: Vec3) -> (f32, Vec3) {
    let closest = Vec3::new(trunk.base.x, point.y.clamp(trunk.base.y, trunk.base.y + trunk.height), trunk.base.z);
    let offset = point - closest;
    let length = offset.length();
    // Genau auf der Achse: irgendeine waagerechte Richtung (kommt praktisch nicht vor).
    let normal = if length > 1e-5 { offset / length } else { Vec3::X };
    (length - trunk.radius, normal)
}

/// Liegt `point` in einem der Stämme?
pub(super) fn in_trunk(trunks: &[Trunk], point: Vec3) -> bool {
    trunks.iter().any(|trunk| trunk_surface(trunk, point).0 < 0.0)
}

/// Nächste Fläche (Gelände oder Stamm) an `point`: Abstand und Normale nach außen.
fn nearest(field: &impl DensitySource, trunks: &[Trunk], point: Vec3) -> Option<(f32, Vec3)> {
    let terrain = surface(field, point);
    trunks.iter().map(|trunk| trunk_surface(trunk, point)).chain(terrain).min_by(|a, b| a.0.total_cmp(&b.0))
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
pub(super) fn resolve(position: &mut Vec3, velocity: &mut Vec3, settings: &Settings, field: &impl DensitySource, trunks: &[Trunk]) -> Contact {
    let r = settings.radius;
    let centers = [r, settings.height * 0.5, settings.height - r];
    let mut contact = Contact::default();
    for _ in 0..ITERATIONS {
        let mut moved = false;
        for &offset in &centers {
            // Gelände und jeder Stamm einzeln: Eine Kugel kann zugleich am Hang und am Stamm liegen.
            // Jede Fläche sieht die Lage nach dem vorigen Herausschieben.
            let terrain = surface(field, *position + Vec3::Y * offset);
            for index in 0..=trunks.len() {
                let point = *position + Vec3::Y * offset;
                let found = match index {
                    0 => terrain,
                    _ => Some(trunk_surface(&trunks[index - 1], point)),
                };
                let Some((distance, normal)) = found else { continue };
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
        }
        if !moved {
            break;
        }
    }
    for (index, &offset) in centers.iter().enumerate() {
        let point = *position + Vec3::Y * offset;
        let terrain = surface(field, point);
        for (distance, normal) in terrain.into_iter().chain(trunks.iter().map(|trunk| trunk_surface(trunk, point))) {
            if distance < r - STUCK_TOLERANCE {
                contact.stuck = true;
            }
            if distance >= r + CONTACT_TOLERANCE {
                continue;
            }
            contact.ground |= index == 0 && normal.y >= settings.max_slope;
            add_wall(&mut contact, normal, settings);
        }
    }
    contact
}

/// Wie weit (m) die Kapsel senkrecht sinken müsste, bis die untere Kugel begehbaren Boden berührt;
/// `None`, wenn darunter innerhalb von `snap` keiner ist. Aus Abstand und Normale der nächsten
/// Fläche: Auf einer Ebene mit Normale `n` ist der senkrechte Abstand `abstand / n.y`; zweimal
/// gerechnet, weil Gelände gekrümmt ist.
pub(super) fn ground_below(position: Vec3, settings: &Settings, field: &impl DensitySource, trunks: &[Trunk]) -> Option<f32> {
    let r = settings.radius;
    let mut drop = 0.0;
    for _ in 0..2 {
        let (distance, normal) = nearest(field, trunks, position + Vec3::Y * (r - drop))?;
        if normal.y < settings.max_slope {
            return None;
        }
        drop += (distance - r) / normal.y;
    }
    (drop >= 0.0 && drop <= settings.snap).then_some(drop + CONTACT_TOLERANCE * 0.5)
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
