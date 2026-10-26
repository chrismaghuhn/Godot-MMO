//! Client und Haupt-Workspace haben je ein eigenes `Cargo.lock` (D-005, #10). Weichen die
//! Versionen der Abhängigkeiten von `terraforge-core` ab (z. B. `fastnoise-lite` nach einem
//! `cargo update` nur in einem Workspace), erzeugten Client und Viewer/Server still verschiedene
//! Welten. Dieser Test vergleicht sie.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Pakete eines Lockfiles: (Name, Version) → Abhängigkeiten, wie sie im Lockfile stehen.
type Lock = BTreeMap<(String, String), Vec<String>>;

/// Minimaler Leser für `Cargo.lock` (nur `name`, `version`, `dependencies`).
fn read_lock(path: &Path) -> Lock {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lock = Lock::new();
    for block in text.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block.lines().find_map(|l| l.strip_prefix(&format!("{key} = \""))).map(|v| v.trim_end_matches('"').to_string())
        };
        let (Some(name), Some(version)) = (field("name"), field("version")) else { continue };
        let dependencies = block
            .split_once("dependencies = [")
            .map(|(_, rest)| rest.split(']').next().unwrap_or(""))
            .unwrap_or("")
            .split(',')
            .map(|d| d.trim().trim_matches('"').to_string())
            .filter(|d| !d.is_empty())
            .collect();
        lock.insert((name, version), dependencies);
    }
    lock
}

/// Alle Pakete (Name, Version), von denen `root` abhängt, einschließlich `root`.
fn closure(lock: &Lock, root: &str) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut todo: Vec<String> = vec![root.to_string()];
    while let Some(dependency) = todo.pop() {
        // Eintrag "name" oder "name version" (mehrere Versionen) oder "name version (quelle)".
        let mut parts = dependency.split(' ');
        let name = parts.next().unwrap();
        let version = parts.next();
        for ((n, v), deps) in lock {
            if n == name && version.is_none_or(|version| version == v) && found.insert((n.clone(), v.clone())) {
                todo.extend(deps.iter().cloned());
            }
        }
    }
    found
}

#[test]
fn core_dependencies_match_the_main_workspace() {
    let gdext = Path::new(env!("CARGO_MANIFEST_DIR"));
    let client = closure(&read_lock(&gdext.join("Cargo.lock")), "terraforge-core");
    let main = closure(&read_lock(&gdext.join("../Cargo.lock")), "terraforge-core");
    assert!(client.len() > 3, "Abhängigkeiten von terraforge-core nicht gefunden: {client:?}");
    // Im Haupt-Workspace kommen über Bevy weitere Features (und damit Pakete) dazu; jede Version,
    // die der Client für den Core nutzt, muss es dort aber genauso geben.
    let differing: Vec<_> = client.difference(&main).collect();
    assert!(differing.is_empty(), "Versionen weichen ab: {differing:?}; im Client-Workspace `cargo update -p <paket> --precise <version>`");
}
