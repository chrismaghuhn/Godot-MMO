//! Gemeinsames für die Godot-Tests: Godot finden und ein temporäres Projekt bauen, das genau die
//! von `cargo test` gebaute Bibliothek lädt.
//!
//! `cargo test` baut die Bibliothek nur nach `target/<profil>/deps/`. Das temporäre Projekt
//! (Kopie von `client/project.godot` und den Test-Skripten) zeigt mit seiner `.gdextension`
//! genau dorthin; die Bibliothek des Editors (`gdext/target/debug/`) bleibt unberührt, und
//! Target-Ordner oder Profil spielen keine Rolle. `extension_list.cfg` schreibt es selbst, weil
//! der erste Import mit Extension Godot beim Beenden abstürzen lässt (#57).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Laufende Nummer: Tests einer Datei laufen parallel im selben Prozess.
static PROJECTS: AtomicUsize = AtomicUsize::new(0);

pub fn godot() -> PathBuf {
    if let Some(path) = std::env::var_os("GODOT") {
        return path.into();
    }
    let found = Command::new("godot").arg("--version").output().is_ok_and(|out| out.status.success());
    assert!(found, "Godot 4.7 nicht gefunden: GODOT=/pfad/zu/godot setzen");
    "godot".into()
}

/// Die von diesem `cargo test` gebaute Bibliothek (neben dem Test-Programm in `deps/`).
fn built_library() -> PathBuf {
    let deps = std::env::current_exe().expect("Pfad des Tests").parent().expect("deps-Ordner").to_path_buf();
    let name = format!("{}terraforge_gdext{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX);
    let library = deps.join(name);
    assert!(library.exists(), "Bibliothek fehlt: {}", library.display());
    library
}

/// Wo Godot unter Linux `user://`-Ordner anlegt: `XDG_DATA_HOME`, wenn gesetzt und absolut, sonst
/// `~/.local/share`.
pub fn data_home() -> PathBuf {
    match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".local/share"),
    }
}

/// Temporäres Godot-Projekt, das `library` lädt. Wird beim Verwerfen gelöscht, samt eigenem
/// `user://`-Ordner: Sonst teilten sich Tests mit dem echten Spiel („MMO“) den Spielstand (#19).
pub struct TestProject(pub PathBuf, pub PathBuf);

impl TestProject {
    pub fn new() -> Self {
        let library = built_library();
        let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../client");
        let number = PROJECTS.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("terraforge-gdext-test-{}-{number}", std::process::id()));
        std::fs::create_dir_all(dir.join(".godot")).unwrap();
        std::fs::copy(client.join("icon.svg"), dir.join("icon.svg")).unwrap();
        let user_dir = format!("terraforge-gdext-test-{}-{number}", std::process::id());
        let project = std::fs::read_to_string(client.join("project.godot")).unwrap();
        assert!(project.contains("[application]\n"), "project.godot ohne [application]-Abschnitt");
        let project = project.replacen(
            "[application]\n",
            &format!("[application]\n\nconfig/use_custom_user_dir=true\nconfig/custom_user_dir_name=\"{user_dir}\"\n"),
            1,
        );
        std::fs::write(dir.join("project.godot"), project).unwrap();
        let user = data_home().join(user_dir);
        for folder in ["scenes", "scripts", "shaders", "tests"] {
            copy_dir(&client.join(folder), &dir.join(folder));
        }
        let config = std::fs::read_to_string(client.join("terraforge.gdextension")).unwrap();
        let config: String = config
            .lines()
            .map(|line| match line.split_once(" = ") {
                Some((key, _)) if key.starts_with("linux.") => format!("{key} = \"{}\"", library.display()),
                _ => line.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(dir.join("terraforge.gdextension"), config).unwrap();
        std::fs::write(dir.join(".godot/extension_list.cfg"), "res://terraforge.gdextension\n").unwrap();
        Self(dir, user)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
        std::fs::remove_dir_all(&self.1).ok();
    }
}

/// Baut den Server im Haupt-Workspace (einmal pro Test-Programm) und gibt den Pfad zurück.
#[allow(dead_code)] // nur von einigen Test-Programmen genutzt
pub fn server_binary() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
            // Eigener Target-Ordner: Sonst wäre der Debug-Build in `target/debug` der neueste, und
            // das Spiel nähme ihn statt des schnelleren Release-Builds (siehe `server_binary_path`).
            let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-server");
            let status = Command::new(env!("CARGO"))
                .args(["build", "-q", "-p", "terraforge-server", "--manifest-path"])
                .arg(root.join("Cargo.toml"))
                .env("CARGO_TARGET_DIR", &target)
                .status()
                .expect("cargo startet nicht");
            assert!(status.success(), "Server nicht gebaut");
            target.join("debug/terraforge-server").canonicalize().expect("Server-Programm fehlt")
        })
        .clone()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// Startet Godot headless im Projekt und gibt die Ausgabe zurück. Schlägt fehl, wenn Godot nicht
/// mit Exit-Code 0 endet **oder** einen Fehler meldet: Laufzeitfehler in GDScript beenden Godot
/// nicht, der Exit-Code allein verdeckt sie (Review-Fund). `expected`: Teile von Fehlermeldungen,
/// die der Test absichtlich auslöst.
pub fn run(project: &TestProject, args: &[&str], expected: &[&str]) -> String {
    let mut command = Command::new(godot());
    // Einstellungen aus der eigenen Shell (z. B. TERRAFORGE_SHOT, _VIEW_RADIUS) nicht erben.
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("TERRAFORGE_")) {
        command.env_remove(key);
    }
    let output: Output = command.args(["--headless", "--path"]).arg(&project.0).args(args).output().expect("Godot startet nicht");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let is_error = |line: &&str| line.contains("SCRIPT ERROR") || line.trim_start().starts_with("ERROR:");
    let error = stderr.lines().chain(stdout.lines()).filter(is_error).find(|line| !expected.iter().any(|e| line.contains(e)));
    assert!(output.status.success() && error.is_none(), "Godot {args:?} fehlgeschlagen ({:?}):\n{stdout}\n{stderr}", error);
    stdout
}

/// Startet ein Skript des Projekts (`--script`) mit Argumenten nach `--`.
pub fn run_script(project: &TestProject, script: &str, args: &[String], expected: &[&str]) -> String {
    let mut all = vec!["--script", script, "--"];
    all.extend(args.iter().map(String::as_str));
    run(project, &all, expected)
}
