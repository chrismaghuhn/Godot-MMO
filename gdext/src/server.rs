//! Der lokale Server für den Singleplayer (#56, D-021): als Kindprozess starten oder mit einem
//! laufenden verbinden.
//!
//! Der Server gibt nach dem Start `TERRAFORGE_SERVER port=<port>` aus. Mit `--exit-with-client`
//! endet er, wenn die Verbindung endet oder seine stdin-Pipe schließt (auch wenn Godot abstürzt).
//! `Drop` schließt die Pipe und lässt ihm `STOP_TIMEOUT`, den Spielstand zu speichern (#19); erst
//! danach wird er beendet.

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use godot::classes::ProjectSettings;
use godot::prelude::*;
use terraforge_protocol::NetworkSource;

use crate::terrain::TerraforgeTerrain;

/// So lange darf der Server zum Starten brauchen (Welt und Untergrund anlegen).
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// So lange darf der Server zum Speichern und Beenden brauchen.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Warum der Server nicht startet.
pub(crate) enum StartError {
    /// Der Spielstand lässt sich nicht laden oder ist schon in Benutzung (#19).
    Save(String),
    Other(String),
}

pub(crate) struct ServerProcess {
    child: Child,
    /// Offen halten: Schließt sie (Ende oder Absturz von Godot), beendet sich der Server.
    _stdin: Option<ChildStdin>,
    pub(crate) port: u16,
}

impl ServerProcess {
    /// Startet den Server; mit `save` lädt und speichert er den Spielstand dort (#19).
    pub(crate) fn start(binary: &str, seed: u32, save: Option<&str>) -> Result<Self, StartError> {
        let mut command = Command::new(binary);
        command.args(["--seed", &seed.to_string(), "--port", "0", "--exit-with-client"]);
        if let Some(save) = save {
            command.args(["--save", save]);
        }
        // Eigene Prozessgruppe: Ctrl+C im Terminal trifft nur Godot; dessen Ende schließt die Pipe,
        // und der Server speichert (sonst endete er ohne Speichern, Review-Fund #19).
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| StartError::Other(format!("{binary}: {error}")))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout umgeleitet");
        // Erste Zeile in einem eigenen Thread lesen, damit ein hängender Server nicht blockiert.
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).ok();
            sender.send(line).ok();
        });
        let line = receiver.recv_timeout(START_TIMEOUT).unwrap_or_default();
        let port = line.trim().strip_prefix("TERRAFORGE_SERVER port=").and_then(|port| port.parse().ok());
        match port {
            Some(port) => Ok(Self { child, _stdin: stdin, port }),
            None => {
                child.kill().ok();
                child.wait().ok();
                match line.trim().strip_prefix("TERRAFORGE_SERVER error=") {
                    Some(reason) => Err(StartError::Save(reason.to_string())),
                    None => Err(StartError::Other(format!("{binary} meldet keinen Port (Ausgabe: {line:?})"))),
                }
            }
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        // Pipe schließen: Der Server speichert und endet von selbst.
        self._stdin = None;
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

impl TerraforgeTerrain {
    /// Pfad des Server-Programms: `server_binary` oder der zuletzt gebaute aus `target/`.
    fn server_binary_path(&self) -> String {
        let globalize = |path: &str| ProjectSettings::singleton().globalize_path(path).to_string();
        if !self.server_binary.is_empty() {
            return globalize(&self.server_binary.to_string());
        }
        let built = |profile: &str| {
            let path = globalize(&format!("res://../target/{profile}/terraforge-server"));
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            modified.map(|modified| (modified, path))
        };
        match (built("release"), built("debug")) {
            (Some(release), Some(debug)) => release.max(debug).1,
            (Some((_, path)), None) | (None, Some((_, path))) => path,
            (None, None) => globalize("res://../target/release/terraforge-server"),
        }
    }

    fn save_dir_path(&self, seed: u32) -> String {
        let dir = if self.save_dir.is_empty() { format!("user://saves/seed-{seed}") } else { self.save_dir.to_string() };
        ProjectSettings::singleton().globalize_path(&dir).to_string()
    }

    /// Verbindung zum Server nach `server`; `None` heißt: nur öffentliche Oberfläche.
    pub(crate) fn connect_server(&mut self, seed: u32) -> Option<Arc<NetworkSource>> {
        let mode = self.server.to_string();
        self.server_state = "aus";
        let address = match mode.as_str() {
            "" | "off" => return None,
            "auto" => {
                let binary = self.server_binary_path();
                if !std::path::Path::new(&binary).exists() {
                    godot_warn!("Server nicht gefunden ({binary}), ohne Höhlen. Bauen: cargo build -p terraforge-server");
                    self.server_state = "nicht erreichbar";
                    return None;
                }
                let save = self.save_dir_path(seed);
                match ServerProcess::start(&binary, seed, Some(&save)) {
                    Ok(process) => {
                        let address = format!("127.0.0.1:{}", process.port);
                        self.server_process = Some(process);
                        address
                    }
                    Err(StartError::Save(reason)) => {
                        // Nicht still ohne Spielstand weiterspielen: Gegrabenes ginge verloren, und
                        // offline Gegrabenes passte nicht zum Spielstand (Review-Fund).
                        godot_error!("Spielstand: {reason}. Graben und Bauen sind gesperrt.");
                        self.server_state = "Spielstand defekt";
                        self.edits.blocked = Some(format!("Spielstand nicht geladen: {reason}"));
                        return None;
                    }
                    Err(StartError::Other(error)) => {
                        godot_warn!("Server startet nicht ({error}), ohne Höhlen");
                        self.server_state = "nicht erreichbar";
                        return None;
                    }
                }
            }
            address => address.to_string(),
        };
        match NetworkSource::connect(address.as_str(), seed, Duration::from_secs(5)) {
            Ok(network) => {
                let network = Arc::new(network);
                self.network = Some(Arc::clone(&network));
                self.server_state = "verbunden";
                Some(network)
            }
            Err(error) => {
                godot_warn!("Keine Verbindung zum Server {address} ({error}), ohne Höhlen");
                self.server_state = "nicht erreichbar";
                self.server_process = None;
                None
            }
        }
    }

}
