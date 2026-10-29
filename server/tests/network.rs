//! Client (`NetworkSource`) und Server über eine echte TCP-Verbindung (#56).

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use glam::IVec3;
use terraforge_core::WorldGen;
use terraforge_core::streaming::{PublicSurface, UndergroundSource};
use terraforge_core::underground::{OreKind, SecretSeed, Underground};
use terraforge_core::volume::{ColumnData, DensityChunk, SAMPLE_MIN, CHUNK_SIZE};
use terraforge_protocol::{ChunkKind, ChunkReply, NetworkSource};
use terraforge_server::{ServerWorld, serve};

const SEED: u32 = 42;

/// Server im selben Prozess auf einem freien Port.
fn start_server(exit_with_client: bool) -> (Arc<ServerWorld>, u16, std::thread::JoinHandle<()>) {
    let world = Arc::new(ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED)));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Arc::clone(&world);
    let handle = std::thread::spawn(move || serve(listener, server, 2, exit_with_client));
    (world, port, handle)
}

fn connect(port: u16) -> NetworkSource {
    NetworkSource::connect(("127.0.0.1", port), SEED, Duration::from_secs(5)).expect("Verbindung")
}

/// Über das Netz kommt genau der gefilterte Chunk des Servers an (oder, bei `Public`, die
/// öffentliche Oberfläche, die laut Server bitgleich ist). Alle Antwortarten kommen vor.
#[test]
fn chunks_over_the_network_match_the_server() {
    let (server, port, _handle) = start_server(false);
    let source = connect(port);
    let world = WorldGen::new(SEED);
    let (mut empty, mut public, mut data) = (0, 0, 0);
    for (cx, cz) in [(0, 0), (1, 0), (0, 1), (-1, -1), (5, -3)] {
        let column = ColumnData::sample(&world, cx, cz);
        for chunk_y in -5..=2 {
            let position = IVec3::new(cx, chunk_y, cz);
            match server.reply(position, ChunkKind::Mesh) {
                ChunkReply::Empty => empty += 1,
                ChunkReply::Public => public += 1,
                ChunkReply::Diff { .. } => data += 1,
            }
            let expected = server.client_chunk(position, ChunkKind::Mesh);
            let got = source.density_chunk(&world, &column, chunk_y);
            match (&expected, &got) {
                (Some(e), Some(g)) => assert!(e.same_as(g), "Mesh-Chunk {position} weicht ab"),
                (None, None) => {}
                _ => panic!("Mesh-Chunk {position}: Server {}, Client {}", expected.is_some(), got.is_some()),
            }
            let expected = server.client_chunk(position, ChunkKind::Full).unwrap();
            assert!(expected.same_as(&source.materials_chunk(&world, &column, chunk_y)), "Full-Chunk {position} weicht ab");
        }
    }
    // Bitgleiche Oberflächen-Chunks gibt es vor allem außerhalb des Höhlenbereichs (Berge).
    for cx in -6..6 {
        for cz in -6..6 {
            let column = ColumnData::sample(&world, cx, cz);
            for chunk_y in column.surface_chunks() {
                public += usize::from(server.reply(IVec3::new(cx, chunk_y, cz), ChunkKind::Mesh) == ChunkReply::Public);
            }
        }
    }
    assert!(empty > 0 && public > 0 && data > 0, "nicht alle Antwortarten: leer {empty}, öffentlich {public}, Daten {data}");
    assert!(!source.failed());
    assert_eq!(source.lowest_cave_chunk(), server.lowest_cave_chunk().into());
}

/// Voller Untergrund zum Vergleichen (wie ihn nur der Server kennt), mit Luft pro Welt-Voxel.
struct Secret {
    world: WorldGen,
    underground: Underground,
    chunks: std::collections::HashMap<IVec3, DensityChunk>,
}

impl Secret {
    fn chunk(&mut self, position: IVec3) -> &DensityChunk {
        let (world, underground) = (&self.world, &self.underground);
        self.chunks.entry(position).or_insert_with(|| {
            let column = ColumnData::sample(world, position.x, position.z);
            underground.materials_chunk(world, &column, position.y)
        })
    }

    fn air(&mut self, voxel: IVec3) -> bool {
        let owner = voxel.div_euclid(IVec3::splat(CHUNK_SIZE));
        let local = voxel - owner * CHUNK_SIZE;
        self.chunk(owner).get(local.x, local.y, local.z) <= 0.0
    }
}

/// Der Server schickt Erze genau dann, wenn sie freiliegen (einer der 26 Nachbarn ist Luft, D-018),
/// sonst an ihrer Stelle das öffentliche Gestein; alles andere unverändert (Review-Fund: vorher nur
/// "weniger Erz als im Untergrund" geprüft).
#[test]
fn hidden_ores_never_leave_the_server() {
    let (_server, port, _handle) = start_server(false);
    let source = connect(port);
    let world = WorldGen::new(SEED);
    let mut secret = Secret { underground: Underground::new(SecretSeed::insecure_dev_seed(SEED), &world.cfg), world: WorldGen::new(SEED), chunks: Default::default() };
    let neighbours: Vec<IVec3> = (0..27).map(|n| IVec3::new(n % 3 - 1, (n / 3) % 3 - 1, n / 9 - 1)).filter(|v| *v != IVec3::ZERO).collect();
    let (mut sent, mut hidden) = (0, 0);
    for cx in -2..=2 {
        let column = ColumnData::sample(&world, cx, 0);
        for chunk_y in -4..=0 {
            let position = IVec3::new(cx, chunk_y, 0);
            let client = source.materials_chunk(&world, &column, chunk_y);
            let public = DensityChunk::from_column(&column, chunk_y);
            let full = secret.chunk(position).clone();
            let origin = full.origin();
            for z in SAMPLE_MIN..=CHUNK_SIZE {
                for y in SAMPLE_MIN..=CHUNK_SIZE {
                    for x in SAMPLE_MIN..=CHUNK_SIZE {
                        assert_eq!(client.get(x, y, z).to_bits(), full.get(x, y, z).to_bits(), "Dichte verändert");
                        let (got, real) = (client.material(x, y, z), full.material(x, y, z));
                        let voxel = origin + IVec3::new(x, y, z);
                        let is_ore = OreKind::from_material(real).is_some();
                        let exposed = is_ore && neighbours.iter().any(|&n| secret.air(voxel + n));
                        if is_ore && exposed {
                            assert_eq!(got, real, "freiliegendes Erz fehlt bei {voxel}");
                            sent += 1;
                        } else if is_ore {
                            assert_eq!(got, public.material(x, y, z), "verborgenes Erz bei {voxel} nicht durch öffentliches Gestein ersetzt");
                            hidden += 1;
                        } else {
                            assert_eq!(got, real, "Material ohne Erz verändert bei {voxel}");
                        }
                    }
                }
            }
        }
    }
    assert!(sent > 0 && hidden > 0, "Testbereich ohne freiliegende ({sent}) oder verborgene ({hidden}) Erze");
}

/// Anfragen außerhalb der Welt beantwortet der Server mit der öffentlichen Oberfläche, ohne
/// abzustürzen (Review-Fund: im Debug-Build lief der Generator über und beendete einen Worker).
#[test]
fn requests_outside_the_world_do_not_break_the_server() {
    use terraforge_protocol::framing::{read_message, write_message};
    use terraforge_protocol::{ClientMessage, ServerMessage, VERSION};
    let (server, port, _handle) = start_server(false);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    // Ohne Antwort nicht ewig warten (so hing der Test, solange die Prüfung selbst überlief).
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write_message(&mut stream, &ClientMessage::Hello { version: VERSION, seed: SEED }).unwrap();
    let _: Option<ServerMessage> = read_message(&mut stream).unwrap();
    // Nach dem `Welcome` schickt der Server den Inventar-Stand, die gefällten Bäume (#18) und die veränderten Chunks (#19).
    let inventory: Option<ServerMessage> = read_message(&mut stream).unwrap();
    assert!(matches!(inventory, Some(ServerMessage::Inventory { .. })), "{inventory:?}");
    let felled: Option<ServerMessage> = read_message(&mut stream).unwrap();
    assert!(matches!(felled, Some(ServerMessage::FelledTrees { .. })), "{felled:?}");
    let touched: Option<ServerMessage> = read_message(&mut stream).unwrap();
    assert!(matches!(touched, Some(ServerMessage::TouchedChunks { .. })), "{touched:?}");
    let dummies: Option<ServerMessage> = read_message(&mut stream).unwrap();
    assert!(matches!(dummies, Some(ServerMessage::Dummies { .. })), "{dummies:?}");
    let far = [[i32::MAX, 0, 0], [0, i32::MIN, 0], [100_000_000, -2, 0], [0, 0, i32::MIN]];
    for (request, position) in far.iter().enumerate() {
        write_message(&mut stream, &ClientMessage::Chunk { request: request as u64, position: *position, kind: ChunkKind::Full }).unwrap();
    }
    for _ in far {
        let reply: Option<ServerMessage> = read_message(&mut stream).unwrap();
        assert!(matches!(reply, Some(ServerMessage::Chunk { reply: ChunkReply::Public, .. })), "{reply:?}");
    }
    // Danach arbeiten alle Worker noch.
    let source = connect(port);
    let world = WorldGen::new(SEED);
    let column = ColumnData::sample(&world, 0, 0);
    let start = std::time::Instant::now();
    for chunk_y in -3..=0 {
        let expected = server.client_chunk(IVec3::new(0, chunk_y, 0), ChunkKind::Full).unwrap();
        assert!(expected.same_as(&source.materials_chunk(&world, &column, chunk_y)));
    }
    assert!(start.elapsed() < Duration::from_secs(5) && !source.failed());
}

#[test]
fn a_client_with_another_seed_is_rejected() {
    let (_server, port, _handle) = start_server(false);
    let error = NetworkSource::connect(("127.0.0.1", port), SEED + 1, Duration::from_secs(5)).err().expect("angenommen");
    assert!(error.to_string().contains("lehnt ab"), "{error}");
}

/// Fällt der Server mitten in der Sitzung weg, liefert die Quelle die öffentliche Oberfläche
/// statt zu hängen, und `failed()` meldet es. Ein Fake-Server begrüßt den Client und schließt dann.
#[test]
fn losing_the_server_falls_back_to_the_public_surface() {
    use terraforge_protocol::framing::{read_message, write_message};
    use terraforge_protocol::{ClientMessage, ServerMessage, VERSION};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let fake = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let hello: Option<ClientMessage> = read_message(&mut stream).unwrap();
        assert!(matches!(hello, Some(ClientMessage::Hello { .. })));
        write_message(&mut stream, &ServerMessage::Welcome { version: VERSION, lowest_cave_chunk: -5 }).unwrap();
        // Erste Anfrage lesen, dann ohne Antwort schließen.
        let _: Option<ClientMessage> = read_message(&mut stream).unwrap();
    });
    let source = connect(port);
    let world = WorldGen::new(SEED);
    let column = ColumnData::sample(&world, 0, 0);
    let start = std::time::Instant::now();
    let chunk = source.materials_chunk(&world, &column, -2);
    fake.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5), "Quelle hing {:?}", start.elapsed());
    assert!(chunk.same_as(&PublicSurface.materials_chunk(&world, &column, -2)));
    assert!(source.failed());
    // Danach öffentliche Oberfläche (ein für immer ausgefallener Server, kein Neuverbinden).
    assert!(source.density_chunk(&world, &column, -2).is_none());
}
