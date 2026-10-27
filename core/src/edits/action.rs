//! Graben und Bauen als Spieler-Aktion (#14): Ziel über einen Strahl vom Auge, feste Größen und
//! Werkzeug-Stufen. Gemeinsam für den Server (prüft und führt aus), den Godot-Client ohne Server
//! und den Viewer.

use std::collections::HashSet;

use glam::{IVec3, Vec3};

use super::{DigResult, PlaceResult, Voxel, WorldEdits};
use crate::material::{MaterialId, MaterialRegistry};
use crate::movement::Settings;
use crate::volume::CHUNK_SIZE;

/// Werkzeug-Stufe: Name und höchste abbaubare Härte (Mohs, siehe `materials.ron`). Echte
/// Werkzeuge kommen mit #21.
#[derive(Clone, Copy, Debug)]
pub struct Tool {
    pub name: &'static str,
    pub max_hardness: f32,
}

pub const TOOLS: [Tool; 4] = [
    Tool { name: "Schaufel", max_hardness: 2.5 },
    Tool { name: "Feuersteinhacke", max_hardness: 4.0 },
    Tool { name: "Bronzehacke", max_hardness: 5.5 },
    Tool { name: "Eisenhacke", max_hardness: 7.0 },
];

pub const DIG_RADIUS: f32 = 2.0;
pub const BUILD_RADIUS: f32 = 1.5;
/// Reichweite im Spiel (Auge bis Treffer). Der Viewer nimmt als Debug-Tool mehr.
pub const REACH: f32 = 8.0;
const RAY_STEP: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Graben mit `TOOLS[tool]`.
    Dig { tool: usize },
    /// `material` bauen, höchstens `limit` Voxel (so viel ist im Inventar, #18).
    Build { material: MaterialId, limit: u32 },
}

impl Action {
    pub fn radius(self) -> f32 {
        match self {
            Action::Dig { .. } => DIG_RADIUS,
            Action::Build { .. } => BUILD_RADIUS,
        }
    }

    pub fn is_build(self) -> bool {
        matches!(self, Action::Build { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Dug(DigResult),
    Built(PlaceResult),
}

impl Outcome {
    /// Chunks mit geänderten Voxeln (leer, wenn sich nichts geändert hat).
    pub fn changed(&self) -> &HashSet<IVec3> {
        match self {
            Outcome::Dug(result) => &result.affected,
            Outcome::Built(result) => &result.affected,
        }
    }
}

/// Erster Punkt entlang des Strahls mit festem Voxel und sein Abstand. `direction` normiert.
pub fn ray_hit(origin: Vec3, direction: Vec3, reach: f32, mut solid: impl FnMut(IVec3) -> bool) -> Option<(Vec3, f32)> {
    let mut distance = 0.0;
    while distance < reach {
        let point = origin + direction * distance;
        if solid(point.round().as_ivec3()) {
            return Some((point, distance));
        }
        distance += RAY_STEP;
    }
    None
}

/// Mittelpunkt der Kugel: beim Graben der Treffer, beim Bauen etwas davor, damit die Kugel auf
/// der Fläche sitzt. `None` ohne Ziel in Reichweite.
pub fn target(action: Action, origin: Vec3, direction: Vec3, reach: f32, solid: impl FnMut(IVec3) -> bool) -> Option<Vec3> {
    let (hit, _) = ray_hit(origin, direction, reach, solid)?;
    Some(match action {
        Action::Dig { .. } => hit,
        Action::Build { .. } => hit - direction * (BUILD_RADIUS * 0.8),
    })
}

/// Würde eine gebaute Kugel um `center` den Körper (Kapsel, Auge bei `eye`) treffen? Dann wäre
/// der Spieler eingemauert.
pub fn hits_body(center: Vec3, eye: Vec3, body: &Settings) -> bool {
    let feet = eye - Vec3::Y * body.eye_height;
    let bottom = feet.y + body.radius;
    let top = feet.y + body.height - body.radius;
    let closest = Vec3::new(feet.x, center.y.clamp(bottom, top), feet.z);
    closest.distance(center) < BUILD_RADIUS + body.radius
}

/// Führt die Aktion an `center` aus. `source` liefert den ungeänderten Wert eines Voxels.
///
/// # Panics
/// Bei einem Werkzeug außerhalb von `TOOLS`.
pub fn apply(edits: &mut WorldEdits, materials: &MaterialRegistry, source: impl FnMut(IVec3) -> Voxel, center: Vec3, action: Action) -> Outcome {
    match action {
        Action::Dig { tool } => Outcome::Dug(edits.dig_sphere(materials, source, center, DIG_RADIUS, TOOLS[tool].max_hardness)),
        Action::Build { material, limit } => Outcome::Built(edits.place_sphere(source, center, BUILD_RADIUS, material, limit)),
    }
}

/// Kleinster und größter Voxel, den eine Aktion an `center` liest oder ändert (die Kugel plus
/// 1 m Rand für das Distanzfeld), z. B. um die Chunks vorab zu laden.
pub fn voxel_bounds(center: Vec3, action: Action) -> (IVec3, IVec3) {
    let reach = Vec3::splat(action.radius() + 1.0);
    ((center - reach).floor().as_ivec3(), (center + reach).ceil().as_ivec3())
}

/// Chunks, die nach einer Aktion an `center` neu gebaut werden müssen: alle, deren Samples
/// (`-1..=CHUNK_SIZE`) die Kugel mit Rand berühren, und noch ein Voxel mehr, weil ein Erz sichtbar
/// wird, sobald einer seiner 26 Nachbarn Luft ist (#46).
pub fn affected_chunks(center: Vec3, action: Action) -> Vec<IVec3> {
    let (min, max) = voxel_bounds(center, action);
    let (min, max) = (min - IVec3::ONE, max + IVec3::ONE);
    // Chunk c enthält Voxel v als Sample, wenn c·32 − 1 ≤ v ≤ c·32 + 32.
    let low = (min - IVec3::ONE).div_euclid(IVec3::splat(CHUNK_SIZE));
    let high = (max + IVec3::ONE).div_euclid(IVec3::splat(CHUNK_SIZE));
    let mut chunks = Vec::new();
    for z in low.z..=high.z {
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                chunks.push(IVec3::new(x, y, z));
            }
        }
    }
    chunks
}

/// Kurzer Text für die Anzeige, z. B. „12× Granit, 3 zu hart für Schaufel“.
pub fn summary(outcome: &Outcome, materials: &MaterialRegistry, action: Action) -> String {
    match outcome {
        Outcome::Dug(result) => {
            let mut parts: Vec<String> = result
                .removed
                .iter()
                .map(|(material, count)| format!("{count}× {}", materials.get(*material).map_or("?", |m| m.name.as_str())))
                .collect();
            if result.too_hard > 0 {
                let tool = match action {
                    Action::Dig { tool } => TOOLS.get(tool).map_or("?", |t| t.name),
                    Action::Build { .. } => "?",
                };
                parts.push(format!("{} zu hart für {tool}", result.too_hard));
            }
            if parts.is_empty() { "nichts abgebaut".into() } else { parts.join(", ") }
        }
        Outcome::Built(result) => {
            let name = match action {
                Action::Build { material, .. } => materials.get(material).map_or("?", |m| m.name.as_str()),
                Action::Dig { .. } => "?",
            };
            format!("{} {name} gebaut", result.placed)
        }
    }
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
