//! Lädt die Extension in einem echten Godot (headless) und vergleicht Höhe und Biom mit
//! `terraforge-core` (#10).
//!
//! Braucht Godot 4.7: Pfad in `GODOT` oder `godot` im `PATH`, sonst schlägt der Test fehl.
//!
//! Der Test lädt genau die Bibliothek, die `cargo test` gerade gebaut hat: Er legt ein
//! temporäres Projekt an (Kopie von `client/project.godot` und dem Test-Skript), dessen
//! `.gdextension` auf diese Datei zeigt. Die Bibliothek des Editors (`gdext/target/debug/`) bleibt
//! unberührt, und Target-Ordner oder Profil spielen keine Rolle. `extension_list.cfg` schreibt der
//! Test selbst, weil der erste Import mit Extension Godot beim Beenden abstürzen lässt (#57).

use std::path::{Path, PathBuf};
use std::process::Command;

use terraforge_core::WorldGen;

fn godot() -> PathBuf {
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

/// Temporäres Godot-Projekt, das `library` lädt. Wird beim Verwerfen gelöscht.
struct TestProject(PathBuf);

impl TestProject {
    fn new(library: &Path) -> Self {
        let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../client");
        let dir = std::env::temp_dir().join(format!("terraforge-gdext-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::create_dir_all(dir.join(".godot")).unwrap();
        for file in ["project.godot", "icon.svg", "tests/gdext_smoke.gd"] {
            std::fs::copy(client.join(file), dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
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
        Self(dir)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[test]
fn godot_sees_the_same_world_as_the_core() {
    let godot = godot();
    let project = TestProject::new(&built_library());

    let seed = 42;
    let world = WorldGen::new(seed);
    for (x, z) in [(10.0f32, 10.0f32), (-1234.5, 777.25), (5000.0, -3000.0)] {
        let output = Command::new(&godot)
            .args(["--headless", "--path"])
            .arg(&project.0)
            .args(["--script", "res://tests/gdext_smoke.gd", "--"])
            .args([seed.to_string(), x.to_string(), z.to_string()])
            .output()
            .expect("Godot startet nicht");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "Smoke-Test fehlgeschlagen:\n{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
        let line = stdout.lines().find_map(|l| l.strip_prefix("TERRAFORGE ")).expect("keine Ergebniszeile");

        let value = |key: &str| line.split(' ').find_map(|part| part.strip_prefix(key)).unwrap_or_else(|| panic!("{key} fehlt in {line}"));
        let height: f32 = value("height=").parse().unwrap();
        let expected = world.column_height(x, z);
        assert!((height - expected).abs() < 1e-4, "Höhe bei ({x}, {z}): Godot {height}, Core {expected}");
        let biome = world.biome_name(world.biome_at(&world.sample(x, z)));
        // Biom-Namen können Leerzeichen enthalten: alles zwischen "biome=" und " sea=".
        let godot_biome = line.split_once("biome=").and_then(|(_, rest)| rest.rsplit_once(" sea=")).map(|(name, _)| name);
        assert_eq!(godot_biome, Some(biome), "Biom bei ({x}, {z})");
        let sea: f32 = value("sea=").parse().unwrap();
        assert_eq!(sea, world.cfg.sea_level);
    }
}
