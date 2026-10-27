//! Der lokale Server für den Singleplayer (#56, D-021): als Kindprozess starten oder mit einem
//! laufenden verbinden.
//!
//! Der Server gibt nach dem Start `TERRAFORGE_SERVER port=<port>` aus. Mit `--exit-with-client`
//! endet er, wenn die Verbindung endet oder seine stdin-Pipe schließt (auch wenn Godot abstürzt);
//! zusätzlich beendet `Drop` den Prozess, wenn das Terrain freigegeben wird.

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

/// So lange darf der Server zum Starten brauchen (Welt und Untergrund anlegen).
const START_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct ServerProcess {
    child: Child,
    /// Offen halten: Schließt sie (Ende oder Absturz von Godot), beendet sich der Server.
    _stdin: Option<ChildStdin>,
    pub(crate) port: u16,
}

impl ServerProcess {
    pub(crate) fn start(binary: &str, seed: u32) -> Result<Self, String> {
        let mut child = Command::new(binary)
            .args(["--seed", &seed.to_string(), "--port", "0", "--exit-with-client"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{binary}: {error}"))?;
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
                Err(format!("{binary} meldet keinen Port (Ausgabe: {line:?})"))
            }
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}
