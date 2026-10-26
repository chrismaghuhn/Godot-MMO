//! Szenen-Hash (D-011): prüft, dass ein Umbau ohne Verhaltensänderung die gleiche Szene zeigt.
//!
//! `TERRAFORGE_SCENE_HASH=pfad` wartet, bis sich die Szene 3 s lang nicht mehr ändert, schreibt
//! dann pro Mesh-Entity eine Zeile (Mesh-Inhalt, Transform, Sichtbarkeit; sortiert) und als
//! letzte Zeile den Gesamt-Hash in die Datei und beendet den Viewer. Die Kamera steht als eigene
//! Zeile darin: Bewegt sie sich (Maus/Tastatur im Fenster), sind die
//! Ergebnisse nicht vergleichbar. Vorher und nachher mit denselben Umgebungsvariablen laufen lassen
//! und die Dateien vergleichen. Geprüft wird nur der fertig geladene Zustand, nicht der Weg dorthin
//! (Budgets, Austausch ohne Flackern).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::{app::AppExit, asset::AssetId, prelude::*};

/// Frühester Zeitpunkt und Abstand der Prüfungen (Sekunden).
const START: f32 = 8.0;
const INTERVAL: f32 = 1.0;
/// So viele gleiche Prüfungen hintereinander (3 s) gelten als "fertig geladen".
const STABLE_CHECKS: u32 = 4;

pub struct SceneHashPlugin;

impl Plugin for SceneHashPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(path) = std::env::var("TERRAFORGE_SCENE_HASH") {
            app.insert_resource(SceneHash { path, next: START, last: None, stable: 0, meshes: HashMap::new() })
                .add_systems(Last, scene_hash);
        }
    }
}

#[derive(Resource)]
struct SceneHash {
    path: String,
    next: f32,
    last: Option<u64>,
    stable: u32,
    /// Inhalts-Hash pro Mesh (Bäume teilen sich wenige Meshes).
    meshes: HashMap<AssetId<Mesh>, u64>,
}

fn mesh_hash(mesh: &Mesh) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (attribute, values) in mesh.attributes() {
        attribute.name.hash(&mut hasher);
        values.get_bytes().hash(&mut hasher);
    }
    if let Some(indices) = mesh.indices() {
        indices.iter().for_each(|i| i.hash(&mut hasher));
    }
    hasher.finish()
}

fn scene_hash(
    time: Res<Time>,
    entities: Query<(&Mesh3d, &GlobalTransform, &InheritedVisibility)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    meshes: Res<Assets<Mesh>>,
    mut state: ResMut<SceneHash>,
    mut exit: MessageWriter<AppExit>,
) {
    if time.elapsed_secs() < state.next {
        return;
    }
    state.next = time.elapsed_secs() + INTERVAL;
    let mut lines: Vec<String> = Vec::with_capacity(entities.iter().len());
    for (mesh, transform, visible) in &entities {
        let content = match state.meshes.get(&mesh.id()) {
            Some(&hash) => hash,
            None => {
                // Fehlendes Mesh als eigene Zeile, damit es den Vergleich nicht still verändert.
                let Some(asset) = meshes.get(&mesh.0) else {
                    lines.push("mesh-fehlt".into());
                    continue;
                };
                let hash = mesh_hash(asset);
                state.meshes.insert(mesh.id(), hash);
                hash
            }
        };
        let bits: Vec<u32> = transform.to_matrix().to_cols_array().iter().map(|v| v.to_bits()).collect();
        lines.push(format!("{content:016x} {bits:08x?} {}", visible.get()));
    }
    if let Ok(camera) = camera.single() {
        let bits: Vec<u32> = camera.to_matrix().to_cols_array().iter().map(|v| v.to_bits()).collect();
        lines.push(format!("camera {bits:08x?}"));
    }
    lines.sort_unstable();
    let mut hasher = DefaultHasher::new();
    lines.hash(&mut hasher);
    let total = hasher.finish();

    if state.last == Some(total) {
        state.stable += 1;
    } else {
        state.stable = 0;
        state.last = Some(total);
    }
    if state.stable + 1 < STABLE_CHECKS {
        return;
    }
    lines.push(format!("total {total:016x} ({} Zeilen)", lines.len()));
    if let Err(error) = std::fs::write(&state.path, lines.join("\n") + "\n") {
        error!("Szenen-Hash nicht geschrieben ({}): {error}", state.path);
    }
    info!("Szenen-Hash {total:016x} nach {:.0} s", time.elapsed_secs());
    exit.write(AppExit::Success);
}
