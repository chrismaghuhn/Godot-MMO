//! `TerraforgeTerrain`: Chunk-Streaming um die aktive Kamera (#11).
//!
//! Der Core-Streamer (`terraforge_core::streaming`, D-021) entscheidet, welche Chunks gebraucht
//! werden, und baut sie in eigenen Threads; seine Worker bereiten die Meshes schon im Godot-Format
//! vor (`MeshData`). Dieser Node setzt nur die Events um: Spalten sind `Node3D` mit ihren Chunks
//! als `MeshInstance3D`, Höhlen-Chunks und Fernkacheln eigene `MeshInstance3D`. Wie viele Meshes
//! pro Frame hochgeladen werden, begrenzt der Streamer (wie im Viewer). Bäume hängen als
//! MultiMesh an ihrer Spalte (`trees`, #12).
//!
//! Ohne Server gibt es nur die öffentliche Oberfläche (`PublicSurface`): Höhlen kommen mit #56.
//!
//! Seed, Sichtradius, Fernterrain und `spawn_clearing` gelten ab `_ready`; spätere Änderungen
//! wirken nicht. Der Node darf verschoben, aber nicht gedreht oder skaliert werden (die Kamera wird
//! in seine lokalen Koordinaten umgerechnet, die Chunks liegen achsparallel darin).

// Das `GodotClass`-Makro erzeugt für `#[class(init)]` Code wie `base: base`.
#![allow(clippy::redundant_field_names)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use glam::{IVec2, IVec3, Vec3Swizzles};
use godot::classes::base_material_3d::{Flags, ShadingMode};
use godot::classes::{INode3D, MeshInstance3D, Node3D, StandardMaterial3D};
use godot::prelude::*;
use terraforge_core::WorldGen;
use terraforge_core::spawn::find_spawn;
use terraforge_core::edits::WorldEdits;
use terraforge_core::streaming::{Config, Event, FarKey, JobKind, PublicSurface, Streamer};
use terraforge_core::volume::CHUNK_SIZE;

use crate::mesh::MeshData;
use crate::trees::TreeAssets;
use crate::world::TerraforgeWorld;

/// So viele Job-Dauern pro Art fließen in die Durchschnitte von `get_stats` ein.
const RECENT_JOBS: usize = 256;

#[derive(GodotClass)]
#[class(init, base = Node3D)]
pub(crate) struct TerraforgeTerrain {
    /// Öffentlicher Seed der Welt (0 bis 2³²-1).
    #[export]
    #[init(val = 42)]
    seed: i64,
    /// Sichtradius in Spalten (32 m), höchstens 30.
    #[export]
    #[init(val = 8)]
    view_radius: i32,
    /// Grobes Fernterrain bis ~3,5 km (D-015).
    #[export]
    #[init(val = true)]
    far_terrain: bool,
    /// Alle Job-Dauern für `take_job_times` sammeln (Benchmark). Vor dem Start setzen.
    #[export]
    record_job_times: bool,
    /// Baumfreier Kreis (m) um den Spawnpunkt, wie im Viewer; 0 = überall Bäume.
    #[export]
    #[init(val = 17.0)]
    spawn_clearing: f32,

    world: Option<Arc<WorldGen>>,
    streamer: Option<Streamer<MeshData>>,
    material: Option<Gd<StandardMaterial3D>>,
    trees: Option<TreeAssets>,
    columns: HashMap<IVec2, Column>,
    caves: HashMap<IVec3, Gd<MeshInstance3D>>,
    far: HashMap<FarKey, Gd<MeshInstance3D>>,
    /// Letzte Job-Dauern pro Art (Spalte, Höhle, Fern) für die Anzeige.
    recent: [VecDeque<f32>; 3],
    /// Alle Job-Dauern seit dem letzten `take_job_times` (nur mit `record_job_times`).
    recorded: [Vec<f32>; 3],
    /// Zeit für das Umsetzen der Events im letzten Frame (Mesh-Upload, Nodes).
    upload: Duration,
    base: Base<Node3D>,
}

/// Eine geladene Spalte: Node mit Chunks und Bäumen als Kinder.
struct Column {
    node: Gd<Node3D>,
    chunks: usize,
    trees: usize,
}

fn kind_index(kind: JobKind) -> usize {
    match kind {
        JobKind::Column => 0,
        JobKind::Cave => 1,
        JobKind::Far => 2,
    }
}

#[godot_api]
impl INode3D for TerraforgeTerrain {
    fn ready(&mut self) {
        // `_ready` kann erneut laufen (`request_ready`): Der Streamer und seine Nodes bleiben.
        if self.streamer.is_some() {
            return;
        }
        let Some(seed) = TerraforgeWorld::checked_seed(self.seed) else {
            return;
        };
        let world = Arc::new(WorldGen::new(seed));
        let mut config = Config::new(self.view_radius);
        config.far = self.far_terrain;
        config.record_job_times = true;
        if self.spawn_clearing > 0.0 {
            config.clearing = Some((find_spawn(&world).xz(), self.spawn_clearing));
        }
        let edits = Arc::new(RwLock::new(WorldEdits::default()));
        self.streamer = Some(Streamer::new(Arc::clone(&world), Arc::new(PublicSurface), edits, config, MeshData::from_core));
        self.world = Some(world);

        let mut material = StandardMaterial3D::new_gd();
        material.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        material.set_flag(Flags::SRGB_VERTEX_COLOR, true);
        material.set_shading_mode(ShadingMode::PER_PIXEL);
        material.set_roughness(0.93);
        material.set_metallic(0.0);
        material.set_specular(0.0);
        self.material = Some(material);
        self.trees = Some(TreeAssets::new());
    }

    fn process(&mut self, _delta: f64) {
        let Some(camera) = self.base().get_viewport().and_then(|viewport| viewport.get_camera_3d()) else {
            return;
        };
        let position = self.base().to_local(camera.get_global_position());
        let Some(streamer) = self.streamer.as_mut() else {
            return;
        };
        streamer.update(glam::Vec3::new(position.x, position.y, position.z));
        for (kind, duration) in streamer.take_job_times() {
            let ms = duration.as_secs_f32() * 1000.0;
            let recent = &mut self.recent[kind_index(kind)];
            if recent.len() == RECENT_JOBS {
                recent.pop_front();
            }
            recent.push_back(ms);
            if self.record_job_times {
                self.recorded[kind_index(kind)].push(ms);
            }
        }
        let start = Instant::now();
        let events: Vec<Event<MeshData>> = streamer.drain_events().collect();
        for event in events {
            self.apply(event);
        }
        self.upload = start.elapsed();
    }
}

impl TerraforgeTerrain {
    fn mesh_instance(&self, data: MeshData, position: Vector3) -> Gd<MeshInstance3D> {
        let mut instance = MeshInstance3D::new_alloc();
        instance.set_mesh(&data.into_array_mesh());
        if let Some(material) = &self.material {
            instance.set_material_override(material);
        }
        instance.set_position(position);
        instance
    }

    fn apply(&mut self, event: Event<MeshData>) {
        let size = CHUNK_SIZE as f32;
        match event {
            Event::ShowColumn { column, chunks, trees } => {
                let mut node = Node3D::new_alloc();
                node.set_name(&format!("Spalte {} {}", column.x, column.y));
                node.set_position(Vector3::new(column.x as f32 * size, 0.0, column.y as f32 * size));
                let chunk_count = chunks.len();
                for (chunk_y, data) in chunks {
                    let instance = self.mesh_instance(data, Vector3::new(0.0, chunk_y as f32 * size, 0.0));
                    node.add_child(&instance);
                }
                let tree_count = self.trees.as_ref().map_or(0, |assets| assets.add_to(&mut node, &trees));
                self.base_mut().add_child(&node);
                let node = Column { node, chunks: chunk_count, trees: tree_count };
                // Neubau: die alte Spalte erst jetzt entfernen, wo die neue da ist (kein Flackern).
                if let Some(mut old) = self.columns.insert(column, node) {
                    old.node.queue_free();
                }
            }
            Event::RemoveColumn { column } => {
                if let Some(mut old) = self.columns.remove(&column) {
                    old.node.queue_free();
                }
            }
            Event::ShowCave { chunk, mesh } => {
                let instance = mesh.map(|data| {
                    let instance = self.mesh_instance(data, Vector3::new(chunk.x as f32, chunk.y as f32, chunk.z as f32) * size);
                    self.base_mut().add_child(&instance);
                    instance
                });
                let old = match instance {
                    Some(instance) => self.caves.insert(chunk, instance),
                    None => self.caves.remove(&chunk),
                };
                if let Some(mut old) = old {
                    old.queue_free();
                }
            }
            Event::RemoveCave { chunk } => {
                if let Some(mut instance) = self.caves.remove(&chunk) {
                    instance.queue_free();
                }
            }
            Event::CaveVisibility { chunk, visible } => {
                if let Some(instance) = self.caves.get_mut(&chunk) {
                    instance.set_visible(visible);
                }
            }
            Event::ShowFar { tile, mesh } => {
                let origin = tile.origin();
                let instance = mesh.map(|data| {
                    let instance = self.mesh_instance(data, Vector3::new(origin.x, origin.y, origin.z));
                    self.base_mut().add_child(&instance);
                    instance
                });
                // Alte Version derselben Kachel erst jetzt ersetzen (keine Lücke).
                let old = match instance {
                    Some(instance) => self.far.insert(tile, instance),
                    None => self.far.remove(&tile),
                };
                if let Some(mut old) = old {
                    old.queue_free();
                }
            }
            Event::RemoveFar { tile } => {
                if let Some(mut instance) = self.far.remove(&tile) {
                    instance.queue_free();
                }
            }
        }
    }
}

fn average(values: &VecDeque<f32>) -> f32 {
    if values.is_empty() { 0.0 } else { values.iter().sum::<f32>() / values.len() as f32 }
}

#[godot_api]
impl TerraforgeTerrain {
    /// Die Welt dieses Terrains (nach `_ready`), z. B. für Spawnpunkt und Höhe.
    #[func]
    fn get_world(&self) -> Option<Gd<TerraforgeWorld>> {
        self.world.as_ref().map(|world| TerraforgeWorld::wrap(Arc::clone(world)))
    }

    /// Zahlen für Overlay und Tests: geladene Spalten, Jobs, Fernkacheln, Höhlen, Zeiten in ms.
    #[func]
    fn get_stats(&self) -> VarDictionary {
        let mut stats = VarDictionary::new();
        let Some(streamer) = &self.streamer else {
            return stats;
        };
        let s = streamer.stats();
        let mut set = |key: &str, value: Variant| {
            stats.set(key, &value);
        };
        set("columns", (s.columns as i64).to_variant());
        set("column_jobs", (s.column_jobs as i64).to_variant());
        set("columns_ready", (s.columns_ready as i64).to_variant());
        set("cave_jobs", (s.cave_jobs as i64).to_variant());
        set("cave_meshes", (s.cave_meshes as i64).to_variant());
        set("caves_shown", (s.cull.shown as i64).to_variant());
        set("far_jobs", (s.far_jobs as i64).to_variant());
        set("far_tiles", (s.far_tiles as i64).to_variant());
        let column_meshes: usize = self.columns.values().map(|column| column.chunks).sum();
        set("meshes", ((column_meshes + self.caves.len() + self.far.len()) as i64).to_variant());
        set("trees", (self.columns.values().map(|column| column.trees).sum::<usize>() as i64).to_variant());
        let idle = s.column_jobs == 0 && s.columns_ready == 0 && s.cave_jobs == 0 && s.caves_ready == 0 && s.far_jobs == 0 && s.far_ready == 0;
        set("idle", idle.to_variant());
        set("upload_ms", (self.upload.as_secs_f32() * 1000.0).to_variant());
        set("column_job_ms", average(&self.recent[0]).to_variant());
        set("cave_job_ms", average(&self.recent[1]).to_variant());
        set("far_job_ms", average(&self.recent[2]).to_variant());
        stats
    }

    /// Alle Job-Dauern (ms) seit dem letzten Aufruf, je Art; nur mit `record_job_times`.
    #[func]
    fn take_job_times(&mut self) -> VarDictionary {
        let mut times = VarDictionary::new();
        for (key, index) in [("column", 0), ("cave", 1), ("far", 2)] {
            let values = std::mem::take(&mut self.recorded[index]);
            times.set(key, &PackedFloat32Array::from(values).to_variant());
        }
        times
    }
}
