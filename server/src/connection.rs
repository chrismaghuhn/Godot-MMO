//! Eine Client-Verbindung: Anmeldung prüfen, dann Chunk-Anfragen an die Worker geben und die
//! Antworten in beliebiger Reihenfolge zurückschicken.

use std::net::TcpStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

use glam::IVec3;
use terraforge_protocol::framing::{read_message, write_message};
use terraforge_protocol::{ChunkReply, ClientMessage, ServerMessage, VERSION};

use crate::ServerWorld;
use crate::workers::Workers;

pub fn handle(stream: TcpStream, world: &Arc<ServerWorld>, workers: &Workers) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    if let Err(error) = serve_client(stream, world, workers) {
        eprintln!("terraforge-server: Verbindung {peer} beendet: {error}");
    }
}

/// Größte Chunk-Koordinate (waagerecht ±33.000 km, senkrecht ±8 km). Weiter draußen laufen die
/// Rechnungen des Generators über.
const MAX_CHUNK: u32 = 1 << 20;
const MAX_CHUNK_Y: u32 = 256;

fn in_world(position: IVec3) -> bool {
    // `unsigned_abs`: `abs` liefe bei `i32::MIN` selbst über.
    position.x.unsigned_abs() <= MAX_CHUNK && position.z.unsigned_abs() <= MAX_CHUNK && position.y.unsigned_abs() <= MAX_CHUNK_Y
}

fn serve_client(mut stream: TcpStream, world: &Arc<ServerWorld>, workers: &Workers) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    match read_message::<ClientMessage>(&mut stream)? {
        Some(ClientMessage::Hello { version, seed }) if version == VERSION && seed == world.seed() => {
            let welcome = ServerMessage::Welcome { version: VERSION, lowest_cave_chunk: world.lowest_cave_chunk() };
            write_message(&mut stream, &welcome)?;
        }
        Some(ClientMessage::Hello { version, seed }) => {
            let reason = format!("Client: Protokoll {version}, Seed {seed}; Server: Protokoll {VERSION}, Seed {}", world.seed());
            write_message(&mut stream, &ServerMessage::Rejected { reason })?;
            return Ok(());
        }
        _ => return Ok(()),
    }
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    while let Some(message) = read_message::<ClientMessage>(&mut stream)? {
        match message {
            ClientMessage::Chunk { request, position, kind } => {
                let (world, writer) = (Arc::clone(world), Arc::clone(&writer));
                workers.run(move || {
                    let position = IVec3::from_array(position);
                    // Außerhalb der Welt oder ein Fehler beim Erzeugen: öffentliche Oberfläche statt
                    // keiner Antwort. Sonst wartete der Client vergeblich, und ein Panic beendete
                    // den Worker für immer (Review-Fund).
                    let reply = if !in_world(position) {
                        ChunkReply::Public
                    } else {
                        catch_unwind(AssertUnwindSafe(|| world.reply(position, kind))).unwrap_or_else(|_| {
                            eprintln!("terraforge-server: Fehler beim Erzeugen von Chunk {position}");
                            ChunkReply::Public
                        })
                    };
                    let message = ServerMessage::Chunk { request, reply };
                    write_message(&mut *writer.lock().unwrap_or_else(PoisonError::into_inner), &message).ok();
                });
            }
            ClientMessage::Hello { .. } => {}
        }
    }
    Ok(())
}
