//! Graben und Bauen für Clients (#14): Der Server prüft und führt aus.
//!
//! Der Client schickt Auge, Blickrichtung und Aktion. Das Ziel bestimmt der Server per Strahl
//! durch den echten Untergrund (mit Höhlen und Änderungen), Radius, Reichweite und Material legt er
//! selbst fest, die Härte begrenzt das Werkzeug. Bauen in den eigenen Körper lehnt er ab.
//!
//! Antworten verraten nichts Verborgenes (D-005): Gegraben wird nur von einem Auge in der Luft aus,
//! `too_hard` zählt nur zu harte Voxel, die danach an Luft grenzen (die sieht der Client ohnehin).
//!
//! **Noch nicht geprüft:** ob das Auge wirklich dort ist. Die Position meldet vorerst der Client;
//! damit lässt sich an beliebiger Stelle graben und bauen und abtasten, ob ein Punkt Luft ist und
//! wo die nächste Wand liegt.
//! Server-autoritative Bewegung kommt mit Phase 6 (#25).

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use terraforge_core::edits::action::{self, Action, DIG_RADIUS, Outcome, REACH, TOOLS};
use terraforge_core::edits::{Voxel, owner_chunk};
use terraforge_core::movement::Settings;
use terraforge_core::volume::{CHUNK_SIZE, DensityChunk};
use terraforge_core::MaterialId;
use terraforge_protocol::{EditAction, EditOutcome, StackInfo};

use crate::ServerWorld;
use crate::world::in_world;

/// Höchstens eine Änderung pro Zeitraum und Verbindung.
pub(crate) const MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Eine geprüfte Anfrage; was gebaut wird, entscheidet erst `edit` mit dem Inventar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Dig { tool: usize },
    /// Platz und das Material, das der Client dort sieht.
    Build { slot: usize, material: MaterialId },
    Fell,
}

/// Prüft eine Anfrage, bevor sie an die Worker geht (Format, Tempo). Gibt die Aktion zurück oder
/// den Grund der Ablehnung.
pub(crate) fn check(eye: [f32; 3], direction: [f32; 3], action: EditAction, last: &mut Option<Instant>) -> Result<(Vec3, Vec3, Request), String> {
    let invalid = || "ungültige Position oder Richtung".to_string();
    let eye = Vec3::from_array(eye);
    // `try_normalize`: Sehr lange Vektoren ergäben mit `normalize` den Nullvektor (Review-Fund).
    let direction = Vec3::from_array(direction).try_normalize().ok_or_else(invalid)?;
    if !eye.is_finite() {
        return Err(invalid());
    }
    if !in_world(owner_chunk(eye.floor().as_ivec3())) {
        return Err("außerhalb der Welt".into());
    }
    let action = match action {
        EditAction::Dig { tool } if usize::from(tool) < TOOLS.len() => Request::Dig { tool: tool.into() },
        EditAction::Dig { .. } => return Err("unbekanntes Werkzeug".into()),
        EditAction::Build { slot, material } => Request::Build { slot: slot.into(), material: MaterialId(material) },
        EditAction::Fell => Request::Fell,
    };
    let now = Instant::now();
    if last.is_some_and(|last| now.duration_since(last) < MIN_INTERVAL) {
        return Err("zu schnell".into());
    }
    *last = Some(now);
    Ok((eye, direction, action))
}

pub(crate) fn refused(reason: &str) -> EditOutcome {
    EditOutcome::Refused { reason: reason.into() }
}

impl ServerWorld {
    /// Führt eine geprüfte Aktion aus. `Edited` mit den Chunks, die der Client neu anfragen muss.
    /// Graben legt die Ausbeute ins Inventar, Bauen verbraucht vom gewählten Platz (#18).
    ///
    /// Aktionen laufen nacheinander (`edit_lock`): Das Ziel gilt noch, wenn die Änderung angewendet
    /// wird, und das Inventar ändert sich nur hier. Gibt dazu den Inventar-Stand direkt nach der
    /// Aktion zurück (unter demselben Lock, damit kein älterer Stand zuletzt beim Client ankommt).
    pub fn edit(&self, eye: Vec3, direction: Vec3, request: Request) -> (EditOutcome, Vec<StackInfo>) {
        let _serial = self.edit_lock.lock().unwrap_or_else(PoisonError::into_inner);
        let outcome = self.apply_edit(eye, direction, request);
        (outcome, self.inventory_view())
    }

    fn apply_edit(&self, eye: Vec3, direction: Vec3, request: Request) -> EditOutcome {
        if self.eye_in_rock(eye) {
            return refused("Auge im Fels");
        }
        let action = match request {
            Request::Fell => return self.fell(eye, direction),
            Request::Dig { tool } => Action::Dig { tool },
            Request::Build { slot, material } => match self.build_material(slot) {
                Some((found, limit)) if found == material => Action::Build { material, limit },
                // Der Client hatte einen älteren Stand (Review-Fund: sonst baute er unbemerkt anderes).
                Some(_) => return refused("auf diesem Platz liegt inzwischen etwas anderes"),
                None => return refused("nichts zum Bauen auf diesem Platz"),
            },
        };
        let Some(center) = action::target(action, eye, direction, REACH, |voxel| self.voxel(voxel).density > 0.0) else {
            return refused("kein Ziel in Reichweite");
        };
        if action.is_build() && action::hits_body(center, eye, &Settings::default()) {
            return refused("zu nah am Spieler");
        }
        let (min, max) = action::voxel_bounds(center, action);
        // Ein Voxel Rand für die Prüfung der Nachbarn bei `too_hard`.
        let (min, max) = (min - IVec3::ONE, max + IVec3::ONE);
        if !in_world(owner_chunk(min)) || !in_world(owner_chunk(max)) {
            return refused("außerhalb der Welt");
        }
        // Chunks vorab erzeugen und festhalten (teuer, ohne Edits-Lock): Der gemeinsame Cache kann
        // sich zwischendurch leeren. Die Kugel ist kleiner als ein Chunk, die Ecken treffen also alle.
        let mut chunks: HashMap<IVec3, Arc<DensityChunk>> = HashMap::new();
        for x in [min.x, max.x] {
            for y in [min.y, max.y] {
                for z in [min.z, max.z] {
                    let owner = owner_chunk(IVec3::new(x, y, z));
                    chunks.entry(owner).or_insert_with(|| self.full_chunk(owner));
                }
            }
        }
        let generated = |voxel: IVec3| {
            let owner = owner_chunk(voxel);
            let local = voxel - owner * CHUNK_SIZE;
            let chunk = chunks.get(&owner).cloned().unwrap_or_else(|| self.full_chunk(owner));
            Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
        };
        let outcome = {
            let mut edits = self.edits.write().unwrap_or_else(PoisonError::into_inner);
            // Ein Panic mitten im Anwenden ließe einen Teil der Voxel geändert zurück: Der Client muss
            // die Chunks trotzdem neu laden, sonst weichen beide Stände dauerhaft ab (Review-Fund).
            match catch_unwind(AssertUnwindSafe(|| action::apply(&mut edits, &self.world.materials, generated, center, action))) {
                Ok(outcome) => outcome,
                Err(_) => {
                    eprintln!("terraforge-server: Fehler beim Graben/Bauen bei {center}");
                    let chunks = action::affected_chunks(center, action).iter().map(IVec3::to_array).collect();
                    return EditOutcome::Failed { chunks };
                }
            }
        };
        let chunks: Vec<[i32; 3]> =
            if outcome.changed().is_empty() { Vec::new() } else { action::affected_chunks(center, action).iter().map(IVec3::to_array).collect() };
        match (outcome, request) {
            (Outcome::Dug(result), _) => EditOutcome::Dug {
                lost: self.store_yield(center.round().as_ivec3(), &result.removed_at),
                removed: result.removed.into_iter().map(|(material, count)| (material.0, count)).collect(),
                too_hard: match action {
                    Action::Dig { tool } => self.exposed_too_hard(center, TOOLS[tool].max_hardness),
                    Action::Build { .. } => 0,
                },
                chunks,
            },
            (Outcome::Built(result), Request::Build { slot, material }) => {
                self.consume(slot, &result.placed_at);
                EditOutcome::Built { placed: result.placed, material: material.0, chunks }
            }
            (Outcome::Built(result), _) => EditOutcome::Built { placed: result.placed, material: 0, chunks },
        }
    }

    /// Nur wenn die ganze Zelle um das Auge fest ist: Der nächste Voxel allein kann schon in der
    /// Wand liegen, wenn der Spieler dicht davor steht (Review-Fund: ~5 % der Aktionen an Wänden
    /// abgelehnt).
    pub(crate) fn eye_in_rock(&self, eye: Vec3) -> bool {
        let cell = eye.floor().as_ivec3();
        (0..8).all(|i| self.voxel(cell + IVec3::new(i & 1, (i >> 1) & 1, (i >> 2) & 1)).density > 0.0)
    }

    /// Feste Voxel in der Grab-Kugel, die zu hart waren und an Luft grenzen (26 Nachbarn). Nur die
    /// sieht der Client; verborgene harte Voxel zu zählen hieße, den Untergrund zu verraten
    /// (Review-Fund: Graben im Fels zählte alle).
    fn exposed_too_hard(&self, center: Vec3, max_hardness: f32) -> u32 {
        let (min, max) = action::voxel_bounds(center, Action::Dig { tool: 0 });
        let mut count = 0;
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let voxel = IVec3::new(x, y, z);
                    if voxel.as_vec3().distance(center) > DIG_RADIUS {
                        continue;
                    }
                    let current = self.voxel(voxel);
                    let hard = self.world.materials.get(current.material).is_none_or(|m| m.hardness > max_hardness);
                    if current.density <= 0.0 || !hard {
                        continue;
                    }
                    let exposed = (0..27).any(|n| n != 13 && self.voxel(voxel + IVec3::new(n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1)).density <= 0.0);
                    count += u32::from(exposed);
                }
            }
        }
        count
    }
}
