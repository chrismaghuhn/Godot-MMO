//! Nachrichten zwischen Client und Server.
//!
//! Ablauf: Client `Hello` → Server `Welcome` (oder `Rejected`), danach beliebig viele
//! `Chunk`- und `Edit`-Anfragen, die der Server in beliebiger Reihenfolge beantwortet (zugeordnet
//! über `request`).
//!
//! Graben und Bauen (#14): Der Client schickt nur Auge, Blickrichtung und Aktion. Ziel, Größe und
//! Härte bestimmt der Server mit dem echten Untergrund; danach meldet er die Chunks, die der Client
//! neu anfragen muss. Ein `Edited` kommt erst, wenn die Änderung gilt: Jede danach gestellte
//! Chunk-Anfrage enthält sie.
//!
//! Inventar (#18, D-022): Der Server führt es und schickt nach dem `Welcome` und nach jeder
//! Änderung den ganzen Stand (`Inventory`, wenige KB). Nach dem `Welcome` kommen außerdem die
//! schon gefällten Bäume (`FelledTrees`), damit ein neu verbundener Client sie nicht mehr zeigt.

use serde::{Deserialize, Serialize};

use crate::chunk::Run;

/// Version des Protokolls; Client und Server müssen gleich sein.
pub const VERSION: u32 = 3;

/// Welche Daten der Client für einen Chunk braucht (wie `UndergroundSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkKind {
    /// Dichtefeld, wenn der Chunk ein Mesh braucht (`density_chunk`).
    Mesh,
    /// Vollständiges Dichtefeld mit Material, auch für massiven Fels (`materials_chunk`).
    Full,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Erste Nachricht: Protokoll-Version und öffentlicher Seed des Clients.
    Hello { version: u32, seed: u32 },
    Chunk { request: u64, position: [i32; 3], kind: ChunkKind },
    /// Graben oder Bauen in Blickrichtung (`direction` muss nicht normiert sein).
    Edit { request: u64, eye: [f32; 3], direction: [f32; 3], action: EditAction },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditAction {
    /// Graben mit Werkzeug-Stufe `tool` (`edits::action::TOOLS`).
    Dig { tool: u8 },
    /// Bauen mit dem Material vom Inventarplatz `slot`. `material` ist, was der Client dort sieht;
    /// liegt dort inzwischen etwas anderes, lehnt der Server ab.
    Build { slot: u8, material: u16 },
    /// Den anvisierten Baum fällen (#18).
    Fell,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Antwort auf `Hello`. `lowest_cave_chunk`: unterster Chunk (y) mit möglichen Höhlen.
    Welcome { version: u32, lowest_cave_chunk: i32 },
    Rejected { reason: String },
    Chunk { request: u64, reply: ChunkReply },
    Edited { request: u64, outcome: EditOutcome },
    Inventory { stacks: Vec<StackInfo> },
    /// Gefällte Bäume (`vegetation::TreeId`: Spalte x/z, Zelle).
    FelledTrees { trees: Vec<(i32, i32, u16)> },
}

/// Ein Inventarplatz, wie der Client ihn anzeigt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StackInfo {
    /// Fester Platz im Inventar (`inventory::SLOTS`); leere Plätze fehlen in der Liste.
    pub slot: u8,
    pub material: u16,
    pub count: u32,
    /// Herkunft (undurchsichtig): gleiche Zahl heißt gleiche Provinz bzw. Region.
    pub origin: u64,
    /// Härte, Dichte, Zähigkeit, Wärmeleitfähigkeit (`properties::Properties`).
    pub properties: [f32; 4],
    /// Bei Erzen: Reinheit (auf 1 % gerundet) und Spurenelemente (`TraceElement` als Zahl, Anteil in
    /// %, auf 0,01 gerundet).
    pub ore: Option<(f32, Vec<(u8, f32)>)>,
}

/// Ergebnis von `Edit`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum EditOutcome {
    /// Nichts verändert, mit Grund (kein Ziel in Reichweite, zu schnell, ungültig …).
    Refused { reason: String },
    /// Gegraben: abgebaute Voxel je Material (`MaterialId`), zu harte Voxel, wie viel davon
    /// keinen Platz im Inventar fand.
    Dug { removed: Vec<(u16, u32)>, too_hard: u32, lost: u32, chunks: Vec<[i32; 3]> },
    /// Gebaut: so viele Voxel aus `material` wurden fest.
    Built { placed: u32, material: u16, chunks: Vec<[i32; 3]> },
    /// Baum gefällt (`vegetation::TreeId`: Spalte x/z, Zelle): Ausbeute und der Chunk, dessen
    /// Spalte neu gebaut werden muss.
    Felled { tree: (i32, i32, u16), material: u16, count: u32, lost: u32, chunk: [i32; 3] },
    /// Fehler auf dem Server mitten in der Änderung: Die Chunks können teilweise geändert sein.
    Failed { chunks: Vec<[i32; 3]> },
}

/// Antwort auf eine Chunk-Anfrage.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ChunkReply {
    /// Kein Mesh nötig (reine Luft oder massiver Fels ohne Hohlraum), nur bei `ChunkKind::Mesh`.
    Empty,
    /// Bitgleich mit der öffentlichen Oberfläche (`DensityChunk::from_column`): Der Client
    /// erzeugt den Chunk selbst.
    Public,
    /// Abweichungen von der öffentlichen Oberfläche (`chunk::diff`), schon gefiltert.
    Diff { runs: Vec<Run> },
}
