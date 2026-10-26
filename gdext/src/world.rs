//! `TerraforgeWorld`: die öffentliche Weltgenerierung für GDScript.
//!
//! Erster Test der Anbindung (#10): Höhe und Biom an einer Position. Chunks und Streaming
//! folgen mit #11.

use godot::prelude::*;
use terraforge_core::WorldGen;

/// Öffentliche Welt eines Seeds. In GDScript: `TerraforgeWorld.create(42).height_at(x, z)`.
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub(crate) struct TerraforgeWorld {
    world: WorldGen,
}

#[godot_api]
impl TerraforgeWorld {
    /// Erzeugt die Welt für einen öffentlichen Seed (0 bis 2³²-1). Außerhalb davon `null`: Ein
    /// umgerechneter Seed ergäbe still eine andere Welt als beim Server.
    #[func]
    fn create(seed: i64) -> Option<Gd<Self>> {
        let Ok(seed) = u32::try_from(seed) else {
            godot_error!("Seed {seed} liegt außerhalb von 0..=4294967295");
            return None;
        };
        Some(Gd::from_object(Self { world: WorldGen::new(seed) }))
    }

    #[func]
    fn seed(&self) -> i64 {
        self.world.cfg.seed.into()
    }

    #[func]
    fn sea_level(&self) -> f32 {
        self.world.cfg.sea_level
    }

    /// Geplante Höhe der Oberfläche an einer Weltposition (ohne Spieler-Änderungen).
    #[func]
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.world.column_height(x, z)
    }

    /// Name des Bioms an einer Weltposition.
    #[func]
    fn biome_at(&self, x: f32, z: f32) -> GString {
        let sample = self.world.sample(x, z);
        self.world.biome_name(self.world.biome_at(&sample)).into()
    }
}
