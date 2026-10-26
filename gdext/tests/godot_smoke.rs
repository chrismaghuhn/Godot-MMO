//! Lädt die Extension in einem echten Godot (headless) und vergleicht Höhe und Biom mit
//! `terraforge-core` (#10). Braucht Godot 4.7: Pfad in `GODOT` oder `godot` im `PATH`.

mod common;

use terraforge_core::WorldGen;

#[test]
fn godot_sees_the_same_world_as_the_core() {
    let project = common::TestProject::new();
    let seed = 42;
    let world = WorldGen::new(seed);
    for (x, z) in [(10.0f32, 10.0f32), (-1234.5, 777.25), (5000.0, -3000.0)] {
        let stdout = common::run_script(&project, "res://tests/gdext_smoke.gd", &[seed.to_string(), x.to_string(), z.to_string()],
            // Das Skript prüft absichtlich, dass Seed -1 abgelehnt wird.
            &["Seed liegt außerhalb"],
        );
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
        // Spawnpunkt wie im Viewer (beide nutzen `terraforge_core::spawn`).
        let spawn: Vec<f32> = value("spawn=").split(',').map(|v| v.parse().unwrap()).collect();
        let expected = terraforge_core::spawn::find_spawn(&world);
        assert!((spawn[0] - expected.x).abs() < 1e-3 && (spawn[1] - expected.y).abs() < 1e-3 && (spawn[2] - expected.z).abs() < 1e-3, "Spawn {spawn:?} statt {expected}");
    }
}
