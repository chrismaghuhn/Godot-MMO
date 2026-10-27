//! `NetworkSource`: der Untergrund für den Streamer kommt vom Server (D-005, #56).
//!
//! Die Worker-Threads des Streamers fragen Chunks an und warten auf die Antwort; ein eigener
//! Thread liest die Antworten und ordnet sie über die Anfrage-Nummer zu. Mehrere Anfragen laufen
//! gleichzeitig über eine Verbindung.
//!
//! Fällt der Server aus, liefert die Quelle die öffentliche Oberfläche (`PublicSurface`): Das
//! Spiel läuft weiter, nur ohne Höhlen. `failed()` meldet das dem Client.

use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use glam::IVec3;
use terraforge_core::WorldGen;
use terraforge_core::streaming::{PublicSurface, UndergroundSource};
use terraforge_core::volume::{ColumnData, DensityChunk};

use crate::framing::{read_message, write_message};
use crate::chunk::apply;
use crate::messages::{ChunkKind, ChunkReply, ClientMessage, ServerMessage, VERSION};

/// So lange wartet ein Worker höchstens auf einen Chunk.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

type Pending = Arc<Mutex<HashMap<u64, Sender<ChunkReply>>>>;

pub struct NetworkSource {
    lowest_cave_chunk: i32,
    writer: Mutex<TcpStream>,
    pending: Pending,
    next_request: AtomicU64,
    failed: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl NetworkSource {
    /// Verbindet und meldet sich mit dem öffentlichen Seed an. Fehler, wenn der Server nicht
    /// erreichbar ist, eine andere Version spricht oder einen anderen Seed hat.
    pub fn connect(address: impl ToSocketAddrs, seed: u32, timeout: Duration) -> io::Result<Self> {
        let address = address.to_socket_addrs()?.next().ok_or_else(|| io::Error::other("keine Adresse"))?;
        let mut stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(timeout))?;
        write_message(&mut stream, &ClientMessage::Hello { version: VERSION, seed })?;
        let lowest_cave_chunk = match read_message::<ServerMessage>(&mut stream)? {
            Some(ServerMessage::Welcome { version, lowest_cave_chunk }) if version == VERSION => lowest_cave_chunk,
            Some(ServerMessage::Welcome { version, .. }) => return Err(io::Error::other(format!("Server spricht Protokoll {version}, Client {VERSION}"))),
            Some(ServerMessage::Rejected { reason }) => return Err(io::Error::other(format!("Server lehnt ab: {reason}"))),
            other => return Err(io::Error::other(format!("unerwartete Antwort: {other:?}"))),
        };
        stream.set_read_timeout(None)?;

        let pending: Pending = Arc::default();
        let failed = Arc::new(AtomicBool::new(false));
        let mut reader_stream = stream.try_clone()?;
        let reader = {
            let (pending, failed) = (Arc::clone(&pending), Arc::clone(&failed));
            std::thread::Builder::new().name("terraforge-net".into()).spawn(move || {
                loop {
                    match read_message::<ServerMessage>(&mut reader_stream) {
                        Ok(Some(ServerMessage::Chunk { request, reply })) => {
                            if let Some(sender) = pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&request) {
                                sender.send(reply).ok();
                            }
                        }
                        Ok(Some(other)) => eprintln!("terraforge: unerwartete Nachricht vom Server: {other:?}"),
                        Ok(None) | Err(_) => break,
                    }
                }
                // Verbindung weg: Wartende Worker bekommen keine Antwort mehr.
                failed.store(true, Ordering::Relaxed);
                pending.lock().unwrap_or_else(PoisonError::into_inner).clear();
            })?
        };
        Ok(Self { lowest_cave_chunk, writer: Mutex::new(stream), pending, next_request: AtomicU64::new(0), failed, reader: Some(reader) })
    }

    /// Ob die Verbindung ausgefallen ist (ab dann nur noch öffentliche Oberfläche).
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Fragt einen Chunk an und wartet auf die Antwort; `None`, wenn keine kommt.
    fn request(&self, position: IVec3, kind: ChunkKind) -> Option<ChunkReply> {
        if self.failed() {
            return None;
        }
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = channel();
        self.pending.lock().unwrap_or_else(PoisonError::into_inner).insert(request, sender);
        let message = ClientMessage::Chunk { request, position: position.to_array(), kind };
        let sent = write_message(&mut *self.writer.lock().unwrap_or_else(PoisonError::into_inner), &message);
        let reply = sent.ok().and_then(|()| receiver.recv_timeout(REQUEST_TIMEOUT).ok());
        if reply.is_none() {
            self.pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&request);
            if !self.failed.swap(true, Ordering::Relaxed) {
                eprintln!("terraforge: Server antwortet nicht, weiter ohne Untergrund");
            }
        }
        reply
    }

    /// Chunk aus einer Antwort mit Inhalt (`Public` oder `Diff`); `None` bei ungültigen Daten.
    fn decode(column: &ColumnData, chunk_y: i32, reply: ChunkReply) -> Option<DensityChunk> {
        let base = DensityChunk::from_column(column, chunk_y);
        match reply {
            ChunkReply::Public => Some(base),
            ChunkReply::Diff { runs } => apply(base, runs),
            ChunkReply::Empty => None,
        }
    }
}

impl UndergroundSource for NetworkSource {
    fn density_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        let position = IVec3::new(column.chunk_x, chunk_y, column.chunk_z);
        match self.request(position, ChunkKind::Mesh) {
            Some(ChunkReply::Empty) => None,
            Some(reply) => Self::decode(column, chunk_y, reply).or_else(|| PublicSurface.density_chunk(world, column, chunk_y)),
            None => PublicSurface.density_chunk(world, column, chunk_y),
        }
    }

    fn materials_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk {
        let position = IVec3::new(column.chunk_x, chunk_y, column.chunk_z);
        self.request(position, ChunkKind::Full)
            .and_then(|reply| Self::decode(column, chunk_y, reply))
            .unwrap_or_else(|| PublicSurface.materials_chunk(world, column, chunk_y))
    }

    fn lowest_cave_chunk(&self) -> Option<i32> {
        Some(self.lowest_cave_chunk)
    }
}

impl Drop for NetworkSource {
    fn drop(&mut self) {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner).shutdown(Shutdown::Both).ok();
        if let Some(reader) = self.reader.take() {
            reader.join().ok();
        }
    }
}
