//! Gemeinsame Hilfen der Server-Tests über TCP.
#![allow(dead_code)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec3;
use terraforge_core::WorldGen;
use terraforge_core::underground::SecretSeed;
use terraforge_protocol::{EditAction, EditOutcome, NetworkSource, StackInfo};
use terraforge_server::{ServerWorld, serve};

pub const SEED: u32 = 42;

pub fn start_server() -> (Arc<ServerWorld>, NetworkSource) {
    let (world, port) = start_server_port();
    (world, connect(port))
}

/// Server ohne Verbindung; mit `connect` verbinden (auch mehrmals).
pub fn start_server_port() -> (Arc<ServerWorld>, u16) {
    let world = Arc::new(ServerWorld::new(SEED, SecretSeed::insecure_dev_seed(SEED)));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Arc::clone(&world);
    std::thread::spawn(move || serve(listener, server, 2, false));
    (world, port)
}

pub fn connect(port: u16) -> NetworkSource {
    NetworkSource::connect(("127.0.0.1", port), SEED, Duration::from_secs(5)).expect("Verbindung")
}

/// Schickt eine Aktion und wartet auf die Antwort. Wartet vorher das Mindest-Intervall ab.
pub fn edit(source: &NetworkSource, eye: Vec3, direction: Vec3, action: EditAction) -> EditOutcome {
    std::thread::sleep(Duration::from_millis(110));
    let request = source.send_edit(eye.to_array(), direction.to_array(), action).expect("gesendet");
    let start = Instant::now();
    loop {
        if let Some((_, outcome)) = source.take_edits().into_iter().find(|(r, _)| *r == request) {
            return outcome;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "keine Antwort");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Auge 1,65 m über der Oberfläche an (x, z).
pub fn eye(world: &WorldGen, x: f32, z: f32) -> Vec3 {
    Vec3::new(x, world.column_height(x, z) + 1.65, z)
}


/// Wartet auf den nächsten Inventar-Stand vom Server.
pub fn inventory(source: &NetworkSource) -> Vec<StackInfo> {
    let start = Instant::now();
    loop {
        if let Some(stacks) = source.take_inventory() {
            return stacks;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "kein Inventar");
        std::thread::sleep(Duration::from_millis(5));
    }
}
