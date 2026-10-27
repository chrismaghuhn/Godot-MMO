//! Nachrichten zwischen Client und Server.
//!
//! Ablauf: Client `Hello` → Server `Welcome` (oder `Rejected`), danach beliebig viele
//! `Chunk`-Anfragen, die der Server in beliebiger Reihenfolge beantwortet (zugeordnet über
//! `request`).

use serde::{Deserialize, Serialize};

use crate::chunk::Run;

/// Version des Protokolls; Client und Server müssen gleich sein.
pub const VERSION: u32 = 1;

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
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Antwort auf `Hello`. `lowest_cave_chunk`: unterster Chunk (y) mit möglichen Höhlen.
    Welcome { version: u32, lowest_cave_chunk: i32 },
    Rejected { reason: String },
    Chunk { request: u64, reply: ChunkReply },
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
