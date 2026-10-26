//! Spawnpunkt: flaches Land nahe am Ursprung und eine Blickrichtung mit Aussicht.
//!
//! Gemeinsam für Viewer und Godot-Client, damit beide an derselben Stelle starten.

use std::f32::consts::TAU;

use glam::Vec3;

use crate::WorldGen;

/// Flache Stelle an Land nahe am Ursprung (Höhe der Oberfläche als `y`).
pub fn find_spawn(world: &WorldGen) -> Vec3 {
    let sea = world.cfg.sea_level;
    let mut best = Vec3::new(0.0, sea + 2.0, 0.0);
    let mut best_score = f32::INFINITY;
    // Bis 6 km Umkreis: Seit #50 sind Meere und Landmassen mehrere Kilometer groß.
    for ring in 0..150 {
        let radius = ring as f32 * 40.0;
        for step in 0..24 {
            let angle = step as f32 / 24.0 * TAU;
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            let height = world.column_height(x, z);
            if height < sea + 4.0 || height > sea + 55.0 {
                continue;
            }
            let slope = (height - world.column_height(x + 6.0, z)).abs()
                + (height - world.column_height(x, z + 6.0)).abs();
            let score = slope * 4.0 + (height - sea) * 0.08 + radius * 0.002;
            if score < best_score {
                best_score = score;
                best = Vec3::new(x, height, z);
            }
        }
    }
    best
}

/// Blickziel vom Spawnpunkt aus: Richtung der tiefsten Stelle in 70 m (meist Aussicht ins Tal).
pub fn spawn_look(world: &WorldGen, spawn: Vec3) -> Vec3 {
    let mut best_angle = 0.0f32;
    let mut best_height = f32::INFINITY;
    for step in 0..16 {
        let angle = step as f32 / 16.0 * TAU;
        let height =
            world.column_height(spawn.x + angle.cos() * 70.0, spawn.z + angle.sin() * 70.0);
        if height < best_height {
            best_height = height;
            best_angle = angle;
        }
    }
    let x = spawn.x + best_angle.cos() * 70.0;
    let z = spawn.z + best_angle.sin() * 70.0;
    Vec3::new(x, best_height.max(spawn.y) + 6.0, z)
}
