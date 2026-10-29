//! terraforge-server: lokaler Server für den Godot-Client (#56).
//!
//! `terraforge-server [--seed 42] [--port 0] [--exit-with-client] [--save ORDNER]`
//!
//! Gibt nach dem Start `TERRAFORGE_SERVER port=<port>` aus (Port 0 = frei wählen). Der geheime
//! Untergrund-Seed kommt aus `TERRAFORGE_SECRET_SEED`, sonst der unsichere Entwickler-Seed.
//!
//! `--exit-with-client`: Der Server endet mit der ersten Verbindung, und auch, wenn stdin endet. So
//! bleibt er nicht übrig, wenn das Spiel abstürzt, bevor es sich verbunden hat (Review-Fund; Godot
//! hält dafür eine Pipe auf stdin offen).
//!
//! `--save ORDNER` (#19): lädt den Spielstand von dort und speichert alle `SAVE_INTERVAL`, wenn sich
//! etwas geändert hat, und beim Ende (Verbindung oder stdin zu). Lässt sich ein vorhandener
//! Spielstand nicht laden oder benutzt ihn schon ein anderer Server, gibt er
//! `TERRAFORGE_SERVER error=<Grund>` aus und startet nicht, statt ihn zu überschreiben. Bei Ctrl+C
//! auf den Server selbst gilt der letzte regelmäßige Stand (Godot startet ihn in einer eigenen
//! Prozessgruppe, Ctrl+C auf Godot schließt nur dessen Pipe).

use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use terraforge_core::underground::SecretSeed;
use terraforge_server::{ServerWorld, serve};

fn main() {
    let mut seed: u32 = 42;
    let mut port: u16 = 0;
    let mut exit_with_client = false;
    let mut save: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| fail(&format!("{arg} braucht einen Wert")));
        match arg.as_str() {
            "--seed" => seed = value().parse().unwrap_or_else(|_| fail("ungültiger Seed")),
            "--port" => port = value().parse().unwrap_or_else(|_| fail("ungültiger Port")),
            "--exit-with-client" => exit_with_client = true,
            "--save" => save = Some(PathBuf::from(value())),
            other => fail(&format!("unbekanntes Argument {other}")),
        }
    }
    let secret = match std::env::var("TERRAFORGE_SECRET_SEED").ok().and_then(|v| v.parse().ok()) {
        Some(value) => SecretSeed::from_u64(value),
        None => {
            eprintln!("terraforge-server: unsicherer Entwickler-Seed (TERRAFORGE_SECRET_SEED nicht gesetzt)");
            SecretSeed::insecure_dev_seed(seed)
        }
    };
    let world = Arc::new(ServerWorld::new(seed, secret));
    // Hält den Ordner für diesen Prozess: Zwei Server auf demselben Spielstand überschrieben sich
    // gegenseitig.
    let _lock = save.as_ref().map(|dir| lock_save(dir));
    if let Some(dir) = &save {
        match world.load(dir) {
            Ok(true) => eprintln!("terraforge-server: Spielstand geladen ({})", dir.display()),
            Ok(false) => eprintln!("terraforge-server: neuer Spielstand ({})", dir.display()),
            Err(error) => save_error(&format!("Spielstand nicht geladen, Server startet nicht: {error}")),
        }
        let (world, dir) = (Arc::clone(&world), dir.clone());
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(SAVE_INTERVAL);
                save_now(&world, &dir, false);
            }
        });
    }
    // Nur lokal erreichbar: Der Server ist vorerst für den Singleplayer (D-021).
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| fail(&format!("Port {port}: {e}")));
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    println!("TERRAFORGE_SERVER port={port}");
    std::io::stdout().flush().ok();
    if exit_with_client {
        let (world, save) = (Arc::clone(&world), save.clone());
        std::thread::spawn(move || {
            std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).ok();
            if let Some(dir) = &save {
                save_now(&world, dir, true);
            }
            std::process::exit(0);
        });
    }
    let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1));
    serve(listener, Arc::clone(&world), workers, exit_with_client);
    if let Some(dir) = &save {
        save_now(&world, dir, true);
    }
}

/// Abstand der regelmäßigen Speicherungen.
const SAVE_INTERVAL: Duration = Duration::from_secs(30);

fn save_now(world: &ServerWorld, dir: &std::path::Path, last: bool) {
    match world.save_if_changed(dir) {
        Ok(true) if last => eprintln!("terraforge-server: Spielstand gespeichert"),
        Ok(_) => {}
        Err(error) => eprintln!("terraforge-server: Spielstand nicht gespeichert ({}): {error}", dir.display()),
    }
}

/// Sperrt den Spielstand-Ordner für diesen Prozess.
fn lock_save(dir: &std::path::Path) -> std::fs::File {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| save_error(&format!("{}: {e}", dir.display())));
    let file = std::fs::File::create(dir.join("lock")).unwrap_or_else(|e| save_error(&format!("{}: {e}", dir.display())));
    if file.try_lock().is_err() {
        save_error(&format!("{} wird schon von einem anderen Server benutzt", dir.display()));
    }
    file
}

/// Ein Problem mit dem Spielstand: auch auf stdout melden, damit Godot es anzeigen kann (statt nur
/// „kein Port“), dann beenden, ohne den Spielstand anzufassen.
fn save_error(message: &str) -> ! {
    println!("TERRAFORGE_SERVER error={message}");
    std::io::stdout().flush().ok();
    fail(message)
}

fn fail(message: &str) -> ! {
    eprintln!("terraforge-server: {message}");
    std::process::exit(2);
}
