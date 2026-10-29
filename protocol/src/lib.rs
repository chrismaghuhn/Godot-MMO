//! terraforge-protocol: Nachrichten zwischen Client und Server (#56, D-021).
//!
//! - `messages`: was Client und Server einander schicken
//! - `chunk`:    Chunks als Unterschied zur öffentlichen Oberfläche
//! - `framing`:  Nachrichten mit Längen-Präfix über einen Byte-Strom (TCP)
//! - `combat`:   Treffer des Kampfs (#62)
//! - `inventory`: Inventar-Stapel für die Anzeige im Client (#18)
//! - `client`:   `NetworkSource`, holt Untergrund-Chunks für den Streamer vom Server
//!
//! Transport vorerst TCP auf localhost; die endgültige Netzwerk-Bibliothek kommt mit Phase 6
//! (#25). Das Protokoll kennt den geheimen Untergrund nicht (D-005): Der Server schickt nur
//! fertige, gefilterte Chunks (Erze nur, wenn sie freiliegen, D-018).

pub mod chunk;
pub mod combat;
pub mod client;
pub mod framing;
pub mod inventory;
pub mod messages;

pub use client::NetworkSource;
pub use messages::{AttackOutcome, ChunkKind, ChunkReply, ClientMessage, EditAction, EditOutcome, HitInfo, ServerMessage, StackInfo, VERSION};
