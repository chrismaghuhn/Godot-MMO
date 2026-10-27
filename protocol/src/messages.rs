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

use serde::{Deserialize, Serialize};

use crate::chunk::Run;

/// Version des Protokolls; Client und Server müssen gleich sein.
pub const VERSION: u32 = 2;

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
    Build,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Antwort auf `Hello`. `lowest_cave_chunk`: unterster Chunk (y) mit möglichen Höhlen.
    Welcome { version: u32, lowest_cave_chunk: i32 },
    Rejected { reason: String },
    Chunk { request: u64, reply: ChunkReply },
    Edited { request: u64, outcome: EditOutcome },
}

/// Ergebnis von `Edit`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum EditOutcome {
    /// Nichts verändert, mit Grund (kein Ziel in Reichweite, zu schnell, ungültig …).
    Refused { reason: String },
    /// Gegraben: abgebaute Voxel je Material (`MaterialId`), zu harte Voxel.
    Dug { removed: Vec<(u16, u32)>, too_hard: u32, chunks: Vec<[i32; 3]> },
    /// Gebaut: so viele Voxel wurden fest.
    Built { placed: u32, chunks: Vec<[i32; 3]> },
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
