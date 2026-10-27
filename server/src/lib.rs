//! terraforge-server: headless, mit geheimem Untergrund (D-003, D-005, #56).
//!
//! - `world`:      Chunks für Clients: Höhlen, Erze, Erz-Filter, Vergleich mit der Oberfläche
//! - `connection`: eine Client-Verbindung (Anmeldung, Anfragen, Antworten)
//! - `edit`:       Graben und Bauen prüfen und ausführen (#14)
//! - `inventory`:  Inventar des Spielers, Ausbeute und Verbrauch (#18)
//! - `fell`:       Bäume fällen (#18)
//! - `workers`:    Threads, die die Chunks berechnen
//!
//! Vorerst ein lokaler Prozess für den Singleplayer (Godot startet ihn, D-021). Server-autoritative
//! Bewegung, mehrere Spieler und Interest Management kommen mit Phase 6 (#25, #26).

mod connection;
mod edit;
mod fell;
mod inventory;
mod workers;
mod world;

use std::net::TcpListener;
use std::sync::Arc;

pub use edit::Request;
pub use world::ServerWorld;

/// Nimmt Verbindungen an, bis der Listener schließt. Mit `exit_with_client` endet der Server, wenn
/// die erste Verbindung endet (Singleplayer: Spiel geschlossen oder abgestürzt). Dass das Spiel
/// schon vor der Verbindung abstürzt, fängt `main` ab (Ende von stdin).
pub fn serve(listener: TcpListener, world: Arc<ServerWorld>, workers: usize, exit_with_client: bool) {
    let pool = Arc::new(workers::Workers::new(workers));
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("terraforge-server: Verbindung fehlgeschlagen: {error}");
                continue;
            }
        };
        let (world, pool) = (Arc::clone(&world), Arc::clone(&pool));
        let handle = std::thread::spawn(move || connection::handle(stream, &world, &pool));
        if exit_with_client {
            handle.join().ok();
            return;
        }
    }
}
