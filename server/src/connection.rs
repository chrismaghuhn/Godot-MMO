//! Eine Client-Verbindung: Anmeldung prüfen, dann Chunk- und Edit-Anfragen an die Worker geben und
//! die Antworten in beliebiger Reihenfolge zurückschicken.

use std::net::TcpStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

use glam::IVec3;
use terraforge_protocol::framing::{read_message, write_message};
use terraforge_protocol::{ChunkReply, ClientMessage, EditOutcome, ServerMessage, VERSION};

use crate::ServerWorld;
use crate::world::in_world;
use crate::workers::Workers;

pub fn handle(stream: TcpStream, world: &Arc<ServerWorld>, workers: &Workers) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    if let Err(error) = serve_client(stream, world, workers) {
        eprintln!("terraforge-server: Verbindung {peer} beendet: {error}");
    }
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
    let mut last_edit = None;
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
            ClientMessage::Edit { request, eye, direction, action } => {
                let (world, writer) = (Arc::clone(world), Arc::clone(&writer));
                let checked = crate::edit::check(eye, direction, action, &mut last_edit);
                workers.run(move || {
                    let outcome = match checked {
                        Ok((eye, direction, action)) => catch_unwind(AssertUnwindSafe(|| world.edit(eye, direction, action))).unwrap_or_else(|_| {
                            eprintln!("terraforge-server: Fehler beim Graben/Bauen bei {eye}");
                            EditOutcome::Refused { reason: "Fehler auf dem Server".into() }
                        }),
                        Err(reason) => EditOutcome::Refused { reason },
                    };
                    let message = ServerMessage::Edited { request, outcome };
                    write_message(&mut *writer.lock().unwrap_or_else(PoisonError::into_inner), &message).ok();
                });
            }
            ClientMessage::Hello { .. } => {}
        }
    }
    Ok(())
}
