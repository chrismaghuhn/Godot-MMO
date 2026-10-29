//! Graben und Bauen im Godot-Client (#14).
//!
//! Mit Server schickt das Terrain nur Auge, Blickrichtung und Aktion; der Server prüft, gräbt im
//! echten Untergrund und meldet die Chunks, die neu zu laden sind. Das Terrain markiert sie dann
//! als verändert (der Streamer lädt sie vollständig vom Server) und zum Neubau. Ohne Server gräbt
//! der Client selbst in der öffentlichen Oberfläche. Nach einem Ausfall des Servers wird nicht mehr
//! gegraben: Die Welt mischte sonst Server-Stand (Höhlen, Gegrabenes) und öffentliche Oberfläche,
//! und ein Spieler in einer Grube steckte für den Client im Fels (Review-Fund).
//!
//! Ergebnisse sammelt `take_edit_results` (pro Frame abholen): Dictionaries mit `request`, `ok`,
//! `message` (Text für die Anzeige), `removed` (Material → Anzahl), `too_hard`, `lost` und `placed`.
//!
//! Das Inventar (#18) führt der Server; das Terrain hält nur seinen letzten Stand für die Anzeige
//! (`inventory.rs`). Ohne Server führt das Terrain es selbst (`Holdings` aus dem Core).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};

use glam::{IVec2, IVec3, Vec3, Vec3Swizzles};
use godot::prelude::*;
use terraforge_core::edits::action::{self, Action, Outcome, REACH, TOOLS};
use terraforge_core::edits::{DigResult, PlaceResult, Voxel, WorldEdits, owner_chunk};
use terraforge_core::inventory::Holdings;
use terraforge_core::material::{MaterialId, MaterialRegistry};
use terraforge_core::movement::Settings;
use terraforge_core::streaming::{PublicSurface, UndergroundSource};
use terraforge_core::vegetation::TreeId;
use terraforge_core::volume::{CHUNK_SIZE, ColumnData, DensityChunk};
use terraforge_core::WorldGen;
use terraforge_protocol::inventory::view;
use terraforge_protocol::{EditAction, EditOutcome, NetworkSource, StackInfo};

use crate::terrain::TerraforgeTerrain;

/// Zustand für Graben und Bauen im Terrain.
#[derive(Default)]
pub(crate) struct Edits {
    /// Mit dem Streamer geteilt. Mit Server nur Markierungen, welche Chunks verändert sind.
    pub(crate) world: Arc<RwLock<WorldEdits>>,
    next: i64,
    /// Beim Server laufende Anfragen: Nummer beim Server → eigene Nummer und Aktion (`None`: fällen).
    pub(crate) waiting: HashMap<u64, (i64, Option<Action>)>,
    pub(crate) results: Vec<VarDictionary>,
    /// Letzter Inventar-Stand (vom Server bzw. aus `local`).
    pub(crate) stacks: Vec<StackInfo>,
    /// Inventar ohne Server.
    pub(crate) local: Holdings,
}

impl Edits {
    /// Antworten des Server abholen. Gibt die Chunks zurück, die neu zu laden sind, und die
    /// gefällten Bäume (der Streamer nimmt ihre Stämme heraus und baut die Spalten neu).
    pub(crate) fn poll(&mut self, network: Option<&NetworkSource>, materials: &MaterialRegistry) -> (Vec<IVec3>, Vec<TreeId>) {
        let (mut changed, mut felled_trees) = (Vec::new(), Vec::new());
        let Some(network) = network else {
            return (changed, felled_trees);
        };
        // Schon gefällte Bäume (nach der Anmeldung).
        for (chunk_x, chunk_z, cell) in network.take_felled() {
            let tree = TreeId { chunk_x, chunk_z, cell };
            self.world.write().expect("Edits-Lock vergiftet").fell_tree(tree);
            felled_trees.push(tree);
        }
        // Vorher prüfen: Ein `Edited`, das danach noch eintrifft, kommt beim nächsten Aufruf.
        let failed = network.failed();
        if let Some(stacks) = network.take_inventory() {
            self.stacks = stacks;
        }
        for (request, outcome) in network.take_edits() {
            let Some((id, action)) = self.waiting.remove(&request) else {
                continue;
            };
            match outcome {
                EditOutcome::Refused { reason } => self.results.push(refused(id, &reason)),
                EditOutcome::Dug { removed, too_hard, lost, chunks } => {
                    let removed: BTreeMap<MaterialId, u32> = removed.into_iter().map(|(m, n)| (MaterialId(m), n)).collect();
                    let dug = DigResult { removed, too_hard, affected: HashSet::new(), removed_at: Vec::new() };
                    changed.extend(self.touch(chunks));
                    self.results.push(result(id, &Outcome::Dug(dug), materials, action.unwrap_or(Action::Dig { tool: 0 }), lost));
                }
                EditOutcome::Built { placed, material, chunks } => {
                    // Gebaut wurde, was der Server meldet (der Client kann einen älteren Stand haben).
                    let action = Action::Build { material: MaterialId(material), limit: placed };
                    let built = PlaceResult { placed, affected: HashSet::new(), placed_at: Vec::new() };
                    changed.extend(self.touch(chunks));
                    self.results.push(result(id, &Outcome::Built(built), materials, action, 0));
                }
                EditOutcome::Felled { tree, material, count, lost, .. } => {
                    let tree = TreeId { chunk_x: tree.0, chunk_z: tree.1, cell: tree.2 };
                    self.world.write().expect("Edits-Lock vergiftet").fell_tree(tree);
                    felled_trees.push(tree);
                    self.results.push(felled(id, materials, MaterialId(material), count, lost));
                }
                EditOutcome::Failed { chunks } => {
                    changed.extend(self.touch(chunks));
                    self.results.push(refused(id, "Fehler auf dem Server"));
                }
            }
        }
        if failed {
            for (_, (id, _)) in self.waiting.drain() {
                self.results.push(refused(id, "Server ausgefallen"));
            }
        }
        (changed, felled_trees)
    }

    /// Vom Server geänderte Chunks markieren (vollständig neu laden).
    fn touch(&mut self, chunks: Vec<[i32; 3]>) -> Vec<IVec3> {
        let chunks: Vec<IVec3> = chunks.into_iter().map(IVec3::from_array).collect();
        self.world.write().expect("Edits-Lock vergiftet").mark_touched(chunks.iter().copied());
        chunks
    }

    pub(crate) fn next_id(&mut self) -> i64 {
        self.next += 1;
        self.next
    }
}

/// Ergebnis „Baum gefällt“ mit Ausbeute.
pub(crate) fn felled(id: i64, materials: &MaterialRegistry, material: MaterialId, count: u32, lost: u32) -> VarDictionary {
    let mut result = VarDictionary::new();
    let name = materials.get(material).map_or("?", |m| m.name.as_str());
    let mut message = format!("Baum gefällt: {count}× {name}");
    if lost > 0 {
        message += &format!(", {lost} ohne Platz im Inventar");
    }
    result.set("request", id);
    result.set("ok", true);
    result.set("message", message.as_str());
    result.set("felled", i64::from(count));
    result.set("lost", i64::from(lost));
    result
}

pub(crate) fn refused(id: i64, reason: &str) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("request", id);
    result.set("ok", false);
    result.set("message", reason);
    result
}

fn result(id: i64, outcome: &Outcome, materials: &MaterialRegistry, action: Action, lost: u32) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("request", id);
    result.set("ok", true);
    let mut message = action::summary(outcome, materials, action);
    if lost > 0 {
        message += &format!(", {lost} ohne Platz im Inventar");
    }
    result.set("message", message.as_str());
    result.set("lost", i64::from(lost));
    match outcome {
        Outcome::Dug(dug) => {
            let mut removed = VarDictionary::new();
            for (material, count) in &dug.removed {
                let name = materials.get(*material).map_or("?", |m| m.name.as_str());
                removed.set(name, i64::from(*count));
            }
            result.set("removed", &removed);
            result.set("too_hard", i64::from(dug.too_hard));
        }
        Outcome::Built(built) => result.set("placed", i64::from(built.placed)),
    }
    result
}

/// Ohne Server: Graben in der öffentlichen Oberfläche, mit kleinem Chunk-Cache. Gibt dazu die Mitte
/// der Kugel zurück.
fn edit_locally(
    world: &WorldGen,
    edits: &RwLock<WorldEdits>,
    columns: impl Fn(IVec2) -> Option<Arc<ColumnData>>,
    eye: Vec3,
    direction: Vec3,
    action: Action,
) -> Result<(Outcome, Vec3), &'static str> {
    let mut chunks: HashMap<IVec3, DensityChunk> = HashMap::new();
    let mut generated = |voxel: IVec3| {
        let owner = owner_chunk(voxel);
        let chunk = chunks.entry(owner).or_insert_with(|| {
            let column = columns(owner.xz()).unwrap_or_else(|| Arc::new(ColumnData::sample(world, owner.x, owner.z)));
            PublicSurface.materials_chunk(world, &column, owner.y)
        });
        let local = voxel - owner * CHUNK_SIZE;
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    };
    let center = {
        let edits = edits.read().expect("Edits-Lock vergiftet");
        action::target(action, eye, direction, REACH, |v| edits.get(v).unwrap_or_else(|| generated(v)).density > 0.0)
    };
    let center = center.ok_or("kein Ziel in Reichweite")?;
    if action.is_build() && action::hits_body(center, eye, &Settings::default()) {
        return Err("zu nah am Spieler");
    }
    // Chunks der Kugel vor dem Schreib-Lock erzeugen, damit das Streaming nicht darauf wartet.
    let (min, max) = action::voxel_bounds(center, action);
    for i in 0..8 {
        generated(IVec3::new(if i & 1 == 0 { min.x } else { max.x }, if i & 2 == 0 { min.y } else { max.y }, if i & 4 == 0 { min.z } else { max.z }));
    }
    Ok((action::apply(&mut edits.write().expect("Edits-Lock vergiftet"), &world.materials, generated, center, action), center))
}

impl TerraforgeTerrain {
    /// `action` beim Bauen: Material und Anzahl vom gewählten Platz (`slot`) des angezeigten
    /// Inventars; der Server prüft selbst nach.
    fn edit(&mut self, eye: Vector3, direction: Vector3, action: Action, slot: usize) -> i64 {
        let eye = self.base().to_local(eye);
        let (eye, direction) = (Vec3::new(eye.x, eye.y, eye.z), Vec3::new(direction.x, direction.y, direction.z));
        let Some(world) = self.world.clone() else {
            return -1;
        };
        let id = self.edits.next_id();
        let Some(direction) = direction.try_normalize().filter(|_| eye.is_finite()) else {
            self.edits.results.push(refused(id, "ungültige Position oder Richtung"));
            return id;
        };
        if let Some(network) = self.network.clone() {
            let message = match action {
                Action::Dig { tool } => EditAction::Dig { tool: tool as u8 },
                Action::Build { material, .. } => EditAction::Build { slot: slot.min(255) as u8, material: material.0 },
            };
            match network.send_edit(eye.to_array(), direction.to_array(), message) {
                Some(request) => {
                    self.edits.waiting.insert(request, (id, Some(action)));
                }
                None => self.edits.results.push(refused(id, "Server ausgefallen")),
            }
            return id;
        }
        let outcome = {
            let streamer = self.streamer.as_ref();
            let columns = |position: IVec2| streamer.and_then(|s| s.column(position).cloned());
            edit_locally(&world, &self.edits.world, columns, eye, direction, action)
        };
        match outcome {
            Ok((outcome, center)) => {
                if let Some(streamer) = self.streamer.as_mut() {
                    streamer.mark_changed(outcome.changed().iter().copied());
                }
                let lost = match &outcome {
                    Outcome::Dug(dug) => {
                        self.edits.local.store_yield(center.round().as_ivec3(), &dug.removed_at, |voxel, material| world.mined_sample(voxel, material))
                    }
                    Outcome::Built(built) => {
                        self.edits.local.consume(slot, &built.placed_at);
                        0
                    }
                };
                self.edits.stacks = view(&self.edits.local.inventory);
                self.edits.results.push(result(id, &outcome, &world.materials, action, lost));
            }
            Err(reason) => self.edits.results.push(refused(id, reason)),
        }
        id
    }
}

#[godot_api(secondary)]
impl TerraforgeTerrain {
    /// Gräbt eine Kugel (2 m) am ersten festen Punkt in Blickrichtung, höchstens 8 m vom Auge
    /// (Weltkoordinaten). `tool` ist die Werkzeug-Stufe (`tool_name`). Gibt die Nummer der Anfrage
    /// zurück (Ergebnis über `take_edit_results`), −1 vor `_ready`.
    #[func]
    fn dig(&mut self, eye: Vector3, direction: Vector3, tool: i32) -> i64 {
        let tool = usize::try_from(tool).unwrap_or(0).min(TOOLS.len() - 1);
        self.edit(eye, direction, Action::Dig { tool }, 0)
    }

    /// Baut eine Kugel (1,5 m) aus dem Material auf Inventarplatz `slot` vor dem ersten festen
    /// Punkt in Blickrichtung, höchstens so viel, wie dort liegt. Nicht in den eigenen Körper
    /// (Maße wie `TerraforgeBody`).
    #[func]
    fn build(&mut self, eye: Vector3, direction: Vector3, slot: i32) -> i64 {
        let slot = usize::try_from(slot).unwrap_or(usize::MAX);
        let stack = self.edits.stacks.iter().find(|stack| usize::from(stack.slot) == slot).map(|stack| (MaterialId(stack.material), stack.count));
        let Some((material, limit)) = stack else {
            let id = self.edits.next_id();
            self.edits.results.push(refused(id, "nichts zum Bauen auf diesem Platz"));
            return id;
        };
        self.edit(eye, direction, Action::Build { material, limit }, slot)
    }

    /// Dichte am Punkt (Weltkoordinaten, > 0 fest), wie sie die Kollision sieht: nahe der Kamera
    /// mit Höhlen und Änderungen, weiter weg die öffentliche Höhe. Für Tests und Debug.
    #[func]
    fn density_at(&self, position: Vector3) -> f32 {
        let local = self.base().to_local(position);
        let voxel = Vec3::new(local.x, local.y, local.z).round().as_ivec3();
        self.streamer.as_ref().map_or(0.0, |streamer| streamer.density_at(voxel))
    }

    /// Ergebnisse seit dem letzten Aufruf (siehe Modul-Doku).
    #[func]
    fn take_edit_results(&mut self) -> Array<VarDictionary> {
        std::mem::take(&mut self.edits.results).into_iter().collect()
    }

    #[func]
    fn tool_count() -> i32 {
        TOOLS.len() as i32
    }

    /// Name und Härte-Grenze einer Werkzeug-Stufe, z. B. „Schaufel (Härte ≤ 2.5)“.
    #[func]
    fn tool_name(tool: i32) -> GString {
        let label = usize::try_from(tool).ok().and_then(|tool| TOOLS.get(tool)).map(|tool| format!("{} (Härte ≤ {})", tool.name, tool.max_hardness));
        GString::from(label.unwrap_or_default().as_str())
    }
}
