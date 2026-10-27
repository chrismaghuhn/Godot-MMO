//! `TerraforgeWorld`: die öffentliche Weltgenerierung für GDScript.
//!
//! Höhe, Biom, Spawnpunkt und die Reichweiten des Streamings (für Nebel, Kamera, Wasser).

use std::sync::Arc;

use godot::prelude::*;
use terraforge_core::{WorldGen, spawn, streaming, vegetation, volume};

/// Öffentliche Welt eines Seeds. In GDScript: `TerraforgeWorld.create(42).height_at(x, z)`, oder
/// von einem Terrain-Node: `terrain.get_world()`.
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub(crate) struct TerraforgeWorld {
    world: Arc<WorldGen>,
}

impl TerraforgeWorld {
    pub(crate) fn wrap(world: Arc<WorldGen>) -> Gd<Self> {
        Gd::from_object(Self { world })
    }

    /// Seed aus GDScript (`int`) als `u32`; außerhalb davon ein Fehler statt still einer anderen Welt.
    pub(crate) fn checked_seed(seed: i64) -> Option<u32> {
        let seed = u32::try_from(seed).ok();
        if seed.is_none() {
            godot_error!("Seed liegt außerhalb von 0..=4294967295");
        }
        seed
    }
}

fn vector(v: glam::Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

#[godot_api]
impl TerraforgeWorld {
    /// Erzeugt die Welt für einen öffentlichen Seed (0 bis 2³²-1). Außerhalb davon `null`: Ein
    /// umgerechneter Seed ergäbe still eine andere Welt als beim Server.
    #[func]
    fn create(seed: i64) -> Option<Gd<Self>> {
        Self::checked_seed(seed).map(|seed| Self::wrap(Arc::new(WorldGen::new(seed))))
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

    /// Flache Stelle an Land nahe am Ursprung (wie im Viewer); `y` ist die Höhe der Oberfläche.
    #[func]
    fn spawn_point(&self) -> Vector3 {
        vector(spawn::find_spawn(&self.world))
    }

    /// Blickziel vom Spawnpunkt aus (Aussicht).
    #[func]
    fn spawn_look(&self, spawn_point: Vector3) -> Vector3 {
        vector(spawn::spawn_look(&self.world, glam::Vec3::new(spawn_point.x, spawn_point.y, spawn_point.z)))
    }

    /// Fußpunkte der Bäume einer Spalte (Weltkoordinaten) auf der öffentlichen Oberfläche, mit dem
    /// freien Kreis um den Spawn. Ohne Höhlen und Spieler-Änderungen; für Tests und Debug.
    #[func]
    fn tree_bases(&self, chunk_x: i32, chunk_z: i32) -> PackedVector3Array {
        let column = volume::ColumnData::sample(&self.world, chunk_x, chunk_z);
        let clearing = Some((glam::Vec3Swizzles::xz(spawn::find_spawn(&self.world)), vegetation::SPAWN_CLEARING));
        let size = volume::CHUNK_SIZE as f32;
        let origin = glam::Vec3::new(chunk_x as f32 * size, 0.0, chunk_z as f32 * size);
        let density = |voxel: glam::IVec3| column.height(voxel.x - chunk_x * volume::CHUNK_SIZE, voxel.z - chunk_z * volume::CHUNK_SIZE) - voxel.y as f32;
        vegetation::column_trees(&self.world, &column, clearing, density).iter().map(|tree| vector(origin + tree.position)).collect()
    }

    /// Kantenlänge eines Chunks in Metern.
    #[func]
    fn chunk_size() -> f32 {
        volume::CHUNK_SIZE as f32
    }

    /// Sichtweite des Fernterrains von der Feldmitte (Kamera-Sichtweite, Wasserfläche).
    #[func]
    fn far_view() -> f32 {
        streaming::FAR_VIEW
    }

    /// Garantierte Reichweite des Fernterrains von jeder Kameraposition: Nebel muss davor enden.
    #[func]
    fn far_guaranteed() -> f32 {
        streaming::FAR_GUARANTEED
    }
}
