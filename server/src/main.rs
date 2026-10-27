//! terraforge-server: lokaler Server für den Godot-Client (#56).
//!
//! `terraforge-server [--seed 42] [--port 0] [--exit-with-client]`
//!
//! Gibt nach dem Start `TERRAFORGE_SERVER port=<port>` aus (Port 0 = frei wählen). Der geheime
//! Untergrund-Seed kommt aus `TERRAFORGE_SECRET_SEED`, sonst der unsichere Entwickler-Seed.
//!
//! `--exit-with-client`: Der Server endet mit der ersten Verbindung, und auch, wenn stdin endet. So
//! bleibt er nicht übrig, wenn das Spiel abstürzt, bevor es sich verbunden hat (Review-Fund; Godot
//! hält dafür eine Pipe auf stdin offen).

use std::io::Write;
use std::net::TcpListener;
use std::sync::Arc;

use terraforge_core::underground::SecretSeed;
use terraforge_server::{ServerWorld, serve};

fn main() {
    let mut seed: u32 = 42;
    let mut port: u16 = 0;
    let mut exit_with_client = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| fail(&format!("{arg} braucht einen Wert")));
        match arg.as_str() {
            "--seed" => seed = value().parse().unwrap_or_else(|_| fail("ungültiger Seed")),
            "--port" => port = value().parse().unwrap_or_else(|_| fail("ungültiger Port")),
            "--exit-with-client" => exit_with_client = true,
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
    // Nur lokal erreichbar: Der Server ist vorerst für den Singleplayer (D-021).
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| fail(&format!("Port {port}: {e}")));
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    println!("TERRAFORGE_SERVER port={port}");
    std::io::stdout().flush().ok();
    if exit_with_client {
        std::thread::spawn(|| {
            std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).ok();
            std::process::exit(0);
        });
    }
    let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1));
    serve(listener, world, workers, exit_with_client);
}

fn fail(message: &str) -> ! {
    eprintln!("terraforge-server: {message}");
    std::process::exit(2);
}
