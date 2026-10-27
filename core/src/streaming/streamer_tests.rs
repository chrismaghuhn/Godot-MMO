//! Tests des ganzen Streamers (nur mit der öffentlichen Oberfläche, damit sie auch ohne
//! `underground` laufen): Laden, Folgen, Graben, Fernkacheln, Panic im Job, freier Kreis.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use glam::{IVec2, IVec3, Vec2, Vec3, Vec3Swizzles};

use super::{Config, Event, FarKey, PublicSurface, Streamer, UndergroundSource, camera_chunk};
use crate::WorldGen;
use crate::edits::{Voxel, WorldEdits, owner_chunk};
use crate::volume::{CHUNK_SIZE, ChunkKind, ColumnData, DensityChunk};

/// Was die Tests vom Streamer sehen: geladene Spalten (Zahl der Chunks) und Höhlen-Chunks.
#[derive(Default)]
struct Scene {
    columns: HashMap<IVec2, usize>,
    caves: HashSet<IVec3>,
    far: HashSet<FarKey>,
    shown: Vec<IVec2>,
    /// Bäume pro Spalte in Weltkoordinaten (x, z).
    trees: HashMap<IVec2, Vec<Vec2>>,
}

impl Scene {
    fn apply(&mut self, event: Event<usize>) {
        match event {
            Event::ShowColumn { column, chunks, trees } => {
                assert!(chunks.iter().all(|(_, vertices)| *vertices > 0), "leeres Mesh gemeldet");
                let origin = column.as_vec2() * CHUNK_SIZE as f32;
                self.trees.insert(column, trees.iter().map(|tree| origin + tree.position.xz()).collect());
                self.columns.insert(column, chunks.len());
                self.shown.push(column);
            }
            Event::RemoveColumn { column } => {
                assert!(self.columns.remove(&column).is_some(), "nicht geladene Spalte {column} entfernt");
                self.trees.remove(&column);
            }
            Event::ShowCave { chunk, mesh } => {
                if mesh.is_some() {
                    self.caves.insert(chunk);
                } else {
                    self.caves.remove(&chunk);
                }
            }
            Event::RemoveCave { chunk } => {
                assert!(self.caves.remove(&chunk), "nicht geladener Höhlen-Chunk entfernt");
            }
            Event::CaveVisibility { chunk, .. } => assert!(self.caves.contains(&chunk)),
            Event::ShowFar { tile, mesh } => {
                if mesh.is_some() {
                    self.far.insert(tile);
                } else {
                    self.far.remove(&tile);
                }
            }
            Event::RemoveFar { tile } => {
                assert!(self.far.remove(&tile), "nicht geladene Fernkachel entfernt");
            }
        }
    }
}

fn streamer(world: &Arc<WorldGen>, edits: &Arc<RwLock<WorldEdits>>, view_radius: i32) -> Streamer<usize> {
    streamer_with(world, edits, view_radius, false, Arc::new(PublicSurface))
}

fn config(view_radius: i32, far: bool) -> Config {
    let mut config = Config::new(view_radius);
    config.far = far;
    config.threads = 2;
    config
}

fn streamer_with(world: &Arc<WorldGen>, edits: &Arc<RwLock<WorldEdits>>, view_radius: i32, far: bool, source: Arc<dyn UndergroundSource>) -> Streamer<usize> {
    streamer_from(world, edits, config(view_radius, far), source)
}

fn streamer_from(world: &Arc<WorldGen>, edits: &Arc<RwLock<WorldEdits>>, config: Config, source: Arc<dyn UndergroundSource>) -> Streamer<usize> {
    Streamer::new(Arc::clone(world), source, Arc::clone(edits), config, |mesh| mesh.positions.len())
}

/// Graben wie im Viewer, Voxel aus der öffentlichen Oberfläche; gibt die betroffenen Chunks zurück.
fn dig(world: &WorldGen, edits: &RwLock<WorldEdits>, center: Vec3, radius: f32) -> HashSet<IVec3> {
    let mut voxel_at = |voxel: IVec3| {
        let owner = owner_chunk(voxel);
        let chunk = DensityChunk::from_column(&ColumnData::sample(world, owner.x, owner.z), owner.y);
        let local = voxel - owner * CHUNK_SIZE;
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    };
    let result = edits.write().unwrap().dig_sphere(&world.materials, &mut voxel_at, center, radius, 10.0);
    assert!(!result.removed.is_empty(), "nichts gegraben");
    result.affected
}

/// Läuft, bis keine Jobs mehr offen sind und mehrere Schritte nichts mehr passiert.
fn settle(streamer: &mut Streamer<usize>, scene: &mut Scene, camera: Vec3) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut quiet = 0;
    while quiet < 5 {
        assert!(Instant::now() < deadline, "Streamer kommt nicht zur Ruhe");
        streamer.update(camera);
        let events: Vec<_> = streamer.drain_events().collect();
        let stats = streamer.stats();
        let idle = stats.column_jobs == 0 && stats.columns_ready == 0 && stats.cave_jobs == 0 && stats.caves_ready == 0 && stats.far_jobs == 0 && stats.far_ready == 0 && stats.field_jobs == 0;
        quiet = if events.is_empty() && idle { quiet + 1 } else { 0 };
        events.into_iter().for_each(|event| scene.apply(event));
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn streamer_loads_the_view_and_follows_the_camera() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer(&world, &edits, 2);
    let mut scene = Scene::default();

    let start = Vec3::new(10.0, world.column_height(10.0, 10.0) + 2.0, 10.0);
    settle(&mut streamer, &mut scene, start);
    let center = camera_chunk(start).xz();
    let expected: HashSet<IVec2> = (-2..=2).flat_map(|dz| (-2..=2).map(move |dx| center + IVec2::new(dx, dz))).collect();
    assert_eq!(scene.columns.keys().copied().collect::<HashSet<_>>(), expected);
    assert_eq!(streamer.stats().columns, expected.len());
    assert!(scene.columns.values().sum::<usize>() >= expected.len(), "Spalten ohne Terrain");
    assert!(scene.caves.is_empty(), "Höhlen ohne Untergrund");
    for column in &expected {
        assert!(streamer.column(*column).is_some(), "Höhen der Spalte {column} fehlen");
    }

    // 10 Spalten weiter: alte Spalten verschwinden (Budget pro Schritt), neue kommen.
    let moved = start + Vec3::new(10.0 * CHUNK_SIZE as f32, 0.0, 0.0);
    settle(&mut streamer, &mut scene, moved);
    let center = camera_chunk(moved).xz();
    assert!(scene.columns.keys().all(|c| (*c - center).abs().max_element() <= 3), "alte Spalten geblieben");
    assert_eq!(scene.columns.len(), 25);
}

#[test]
fn digging_rebuilds_the_column() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer(&world, &edits, 1);
    let mut scene = Scene::default();
    let ground = world.column_height(40.0, 40.0);
    let camera = Vec3::new(40.0, ground + 2.0, 40.0);
    settle(&mut streamer, &mut scene, camera);

    let center = Vec3::new(40.0, ground - 1.0, 40.0);
    let affected = dig(&world, &edits, center, 2.0);
    let column = camera_chunk(center).xz();
    assert!(affected.iter().any(|c| c.xz() == column));

    scene.shown.clear();
    streamer.mark_changed(affected);
    settle(&mut streamer, &mut scene, camera);
    assert!(scene.shown.contains(&column), "Spalte nach dem Graben nicht neu gebaut");
}

/// Ein Hohlraum tief im Fels: Mit `PublicSurface` gibt es keine Höhlen, der veränderte massive
/// Chunk wird trotzdem als Höhlen-Chunk geladen und durch das Culling geführt.
#[test]
fn digging_into_solid_rock_shows_a_cave_chunk() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer(&world, &edits, 1);
    let mut scene = Scene::default();
    let camera = Vec3::new(40.0, world.column_height(40.0, 40.0) + 2.0, 40.0);
    settle(&mut streamer, &mut scene, camera);

    // Mitte eines Chunks, der laut Oberfläche massiv ist.
    let column = streamer.column(camera_chunk(camera).xz()).unwrap().clone();
    let chunk_y = (-8..0).rev().find(|&y| column.classify(y) == ChunkKind::Solid && column.classify(y + 1) == ChunkKind::Solid).unwrap();
    let center = Vec3::new(48.0, (chunk_y * CHUNK_SIZE + CHUNK_SIZE / 2) as f32, 48.0);
    let affected = dig(&world, &edits, center, 3.0);
    let cave = camera_chunk(center);
    assert!(affected.contains(&cave));

    streamer.mark_changed(affected);
    settle(&mut streamer, &mut scene, camera);
    assert!(scene.caves.contains(&cave), "Hohlraum im Fels nicht gezeigt");
    assert_eq!(streamer.stats().cave_meshes, scene.caves.len());
    // Von der Oberfläche aus abgeschlossen: ausgeblendet, aber jenseits der Suchgrenze sichtbar.
    let stats = streamer.stats().cull;
    assert_eq!(stats.shown + stats.hidden, scene.caves.len());
}

#[test]
fn far_tiles_follow_the_view() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer_with(&world, &edits, 5, true, Arc::new(PublicSurface));
    let mut scene = Scene::default();
    let start = Vec3::new(10.0, 80.0, 10.0);
    settle(&mut streamer, &mut scene, start);
    // 3×3 Stufe-2-Felder mit je 4×4 Stufe-1-Kacheln, dazu 40 Stufe-2-Kacheln außen.
    assert_eq!(scene.far.len(), 9 * 16 + 40);
    assert_eq!(streamer.stats().far_tiles, scene.far.len());

    // Ein Stufe-2-Feld weiter: alte Kacheln werden ersetzt, nicht doppelt gezeigt.
    settle(&mut streamer, &mut scene, start + Vec3::new(1024.0, 0.0, 0.0));
    assert_eq!(scene.far.len(), 9 * 16 + 40);
}

/// Quelle, deren erster Aufruf panict.
struct PanicOnce(AtomicBool);

impl UndergroundSource for PanicOnce {
    fn density_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        if !self.0.swap(true, Ordering::SeqCst) {
            panic!("Test-Panic im Job");
        }
        PublicSurface.density_chunk(world, column, chunk_y)
    }

    fn materials_chunk(&self, world: &WorldGen, column: &ColumnData, chunk_y: i32) -> DensityChunk {
        PublicSurface.materials_chunk(world, column, chunk_y)
    }

    fn lowest_cave_chunk(&self) -> Option<i32> {
        None
    }
}

/// Ein Panic im Job erreicht den Aufrufer, die Position bleibt aber nicht für immer "in Arbeit"
/// (Review-Fund: gdext fängt Panics, das Streaming läuft dann weiter).
#[test]
fn streaming_recovers_after_a_panic_in_a_job() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer_with(&world, &edits, 1, false, Arc::new(PanicOnce(AtomicBool::new(false))));
    let mut scene = Scene::default();
    let camera = Vec3::new(10.0, world.column_height(10.0, 10.0) + 2.0, 10.0);
    let deadline = Instant::now() + Duration::from_secs(30);
    let panicked = loop {
        assert!(Instant::now() < deadline, "Panic kam nie an");
        let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| streamer.update(camera)));
        let events: Vec<_> = streamer.drain_events().collect();
        events.into_iter().for_each(|event| scene.apply(event));
        if step.is_err() {
            break true;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(panicked);
    settle(&mut streamer, &mut scene, camera);
    assert_eq!(scene.columns.len(), 9, "Spalte des Panic-Jobs nie geladen");
}

/// `Config::clearing` lässt um den Spawnpunkt keine Bäume stehen (Viewer und Godot nutzen das).
#[test]
fn clearing_keeps_trees_away_from_the_spawn() {
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let spawn = crate::spawn::find_spawn(&world);
    let camera = spawn + Vec3::Y * 4.5;
    let near = |scene: &Scene| scene.trees.values().flatten().filter(|tree| tree.distance(spawn.xz()) <= 17.0).count();

    let mut scene = Scene::default();
    settle(&mut streamer(&world, &edits, 1), &mut scene, camera);
    assert!(near(&scene) > 0, "ohne freien Kreis keine Bäume nahe am Spawnpunkt (Test sinnlos)");

    let mut config = config(1, false);
    config.clearing = Some((spawn.xz(), 17.0));
    let mut cleared = streamer_from(&world, &edits, config, Arc::new(PublicSurface));
    let mut scene = Scene::default();
    settle(&mut cleared, &mut scene, camera);
    assert_eq!(near(&scene), 0, "Bäume im freien Kreis");
    assert!(scene.trees.values().flatten().count() > 0, "gar keine Bäume");
}

/// Graben direkt neben dem Körper: Bis der neue Stand da ist, gilt der alte, Gegrabenes wird nicht
/// kurz massiv (Review-Fund: der Körper wurde 8,7 m aus einer Grube geschoben).
#[test]
fn digging_keeps_the_old_density_until_the_new_one_arrives() {
    use crate::movement::DensitySource;
    let world = Arc::new(WorldGen::new(42));
    let edits = Arc::new(RwLock::new(WorldEdits::default()));
    let mut streamer = streamer(&world, &edits, 1);
    let mut scene = Scene::default();
    let ground = world.column_height(40.0, 40.0);
    let camera = Vec3::new(40.0, ground - 6.0, 40.0);
    // Grube graben und laden lassen.
    let pit = Vec3::new(40.0, ground - 5.0, 40.0);
    streamer.mark_changed(dig(&world, &edits, pit, 3.0));
    settle(&mut streamer, &mut scene, camera);
    let inside = pit.round().as_ivec3();
    assert!(DensitySource::density(&streamer, inside) <= 0.0, "Grube nicht im Feld");
    // Daneben graben: Bis zum nächsten `update` bleibt die Grube Luft.
    streamer.mark_changed(dig(&world, &edits, pit + Vec3::new(2.5, 0.0, 0.0), 2.0));
    assert!(DensitySource::density(&streamer, inside) <= 0.0, "Grube kurz massiv");
    settle(&mut streamer, &mut scene, camera);
    let beside = (pit + Vec3::new(2.5, 0.0, 0.0)).round().as_ivec3();
    assert!(DensitySource::density(&streamer, beside) <= 0.0, "neues Loch nicht im Feld");
}
