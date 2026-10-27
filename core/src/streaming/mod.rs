//! Chunk-Streaming um die Kamera, engine-unabhängig (#55, D-021).
//!
//! Der `Streamer` entscheidet, welche Chunks gebraucht werden, baut sie in Worker-Threads und
//! meldet das Ergebnis als `Event`s. Er kennt keine Entities oder Nodes: Viewer und Godot-Client
//! setzen die Events nur um. Meshes wandelt eine Funktion des Aufrufers schon im Worker-Thread in
//! das Format der Engine um (`M`).
//!
//! - `wanted`:  welche Spalten und Chunks um die Kamera gebraucht werden
//! - `columns`: Spalten im Sichtradius; pro Spalte die Oberflächen-Chunks und Bäume
//! - `caves`:   reine Höhlen-Chunks unter der Oberfläche, nur nahe der Kamera (#4)
//! - `culling`: Cave Culling über Seiten-Verbindungen (#8, D-018)
//! - `far`:     grobes Fernterrain bis ~3,5 km (#33)
//! - `dirty`:   durch Graben/Bauen veränderte Chunks (#6)
//! - `field`:   Dichtefeld nahe der Kamera für Kollision und Bewegung (#13)
//! - `jobs`:    Worker-Threads (einziger Ort mit Threads)
//! - `source`:  woher die Dichtefelder kommen (lokaler Untergrund oder nur Oberfläche, D-005)
//!
//! Pro Frame: `update(kamera)`, danach `drain_events()` umsetzen. Die Reihenfolge der Events
//! ist wichtig (z. B. erst `ShowCave`, dann `CaveVisibility` für denselben Chunk).

mod caves;
mod columns;
mod culling;
mod dirty;
mod far;
mod field;
mod jobs;
mod source;
mod wanted;

#[cfg(test)]
mod streamer_tests;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use glam::{IVec2, IVec3, Vec2, Vec3};

use crate::WorldGen;
use crate::edits::WorldEdits;
use crate::mesh::ChunkMesh;
use crate::occlusion::FaceLinks;
use crate::vegetation::TreePlacement;
use crate::volume::ColumnData;

pub use far::{FAR_GUARANTEED, FAR_VIEW, FarKey, MAX_VIEW_RADIUS};
pub use jobs::JobKind;
pub use source::{PublicSurface, UndergroundSource};
pub use wanted::camera_chunk;

/// Grenzen pro Chunk-Art: laufende Jobs, Einfügen und Entfernen pro `update`.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub jobs: usize,
    pub inserts: usize,
    pub removals: usize,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Sichtradius in Spalten (Chebyshev). Höchstens `MAX_VIEW_RADIUS`, sonst ragt die Nahzone
    /// aus der feinen Fernstufe heraus.
    pub view_radius: i32,
    /// Radius (Chunks, Chebyshev in 3D) um die Kamera, in dem Höhlen geladen werden.
    pub cave_radius: i32,
    pub culling: bool,
    pub far: bool,
    /// Baumfreier Kreis (Mittelpunkt x/z, Radius), z. B. um den Spawnpunkt.
    pub clearing: Option<(Vec2, f32)>,
    pub threads: usize,
    /// Dauer der Jobs sammeln (`take_job_times`), sonst wüchse die Liste die ganze Sitzung.
    pub record_job_times: bool,
    pub columns: Limits,
    pub caves: Limits,
    pub far_limits: Limits,
}

impl Config {
    pub fn new(view_radius: i32) -> Self {
        // Wie Bevys Pool für Hintergrund-Jobs: ein Viertel der Kerne (gerundet), 1 bis 4.
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            view_radius: view_radius.clamp(1, MAX_VIEW_RADIUS),
            cave_radius: 3,
            culling: true,
            far: true,
            clearing: None,
            threads: ((cores + 2) / 4).clamp(1, 4),
            record_job_times: false,
            columns: Limits { jobs: 8, inserts: 3, removals: 4 },
            caves: Limits { jobs: 6, inserts: 4, removals: 8 },
            // Eine Kachel pro Frame: mehr führte bei Aussparungs-Wechseln zu Frames > 20 ms.
            far_limits: Limits { jobs: 4, inserts: 1, removals: usize::MAX },
        }
    }
}

/// Was die Engine umsetzen muss. Positionen sind Chunk-Koordinaten; Meshes sind lokal zum
/// Chunk-Ursprung (`chunk * 32`), Bäume lokal zum Ursprung der Spalte.
pub enum Event<M> {
    /// Spalte (neu) gebaut: ersetzt den bisherigen Inhalt der Spalte vollständig.
    /// `chunks`: (Chunk-Höhe, Mesh) der Oberflächen-Chunks.
    ShowColumn { column: IVec2, chunks: Vec<(i32, M)>, trees: Vec<TreePlacement> },
    RemoveColumn { column: IVec2 },
    /// Höhlen-Chunk (neu) gebaut, zunächst sichtbar: ersetzt den bisherigen; ohne Mesh entfernt er ihn.
    ShowCave { chunk: IVec3, mesh: Option<M> },
    RemoveCave { chunk: IVec3 },
    /// Cave Culling: nur Änderungen.
    CaveVisibility { chunk: IVec3, visible: bool },
    /// Fernkachel (neu) gebaut, Ursprung `FarKey::origin`: ersetzt die bisherige.
    ShowFar { tile: FarKey, mesh: Option<M> },
    RemoveFar { tile: FarKey },
}

/// Zahlen für Overlays.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub columns: usize,
    pub column_jobs: usize,
    /// Fertige Spalten, die noch auf das Einfügen warten (Budget pro Frame).
    pub columns_ready: usize,
    pub cave_jobs: usize,
    /// Fertige Höhlen-Chunks, die noch auf das Einfügen warten.
    pub caves_ready: usize,
    pub cave_meshes: usize,
    pub far_jobs: usize,
    /// Fertige Fernkacheln, die noch auf das Einfügen warten (eine pro Frame).
    pub far_ready: usize,
    pub far_tiles: usize,
    /// Chunks mit Dichtefeld für die Kollision und laufende Jobs dafür.
    pub field_chunks: usize,
    pub field_jobs: usize,
    pub cull: CullStats,
}

/// Sichtbare und ausgeblendete Höhlen-Chunks (mit Mesh).
#[derive(Clone, Copy, Debug, Default)]
pub struct CullStats {
    pub shown: usize,
    pub hidden: usize,
}

/// Was die Jobs brauchen; günstig zu klonen.
struct Context<M> {
    world: Arc<WorldGen>,
    source: Arc<dyn UndergroundSource>,
    edits: Arc<RwLock<WorldEdits>>,
    convert: Arc<dyn Fn(ChunkMesh) -> M + Send + Sync>,
    config: Arc<Config>,
}

impl<M> Clone for Context<M> {
    fn clone(&self) -> Self {
        Self {
            world: Arc::clone(&self.world),
            source: Arc::clone(&self.source),
            edits: Arc::clone(&self.edits),
            convert: Arc::clone(&self.convert),
            config: Arc::clone(&self.config),
        }
    }
}

enum Built<M> {
    Column(columns::ColumnBuild<M>),
    Cave(caves::CaveBuild<M>),
    Far(far::FarBuild<M>),
    Field(field::FieldBuild),
}

/// Was ein Schritt der Chunk-Arten gemeinsam nutzt.
struct Frame<'a, M> {
    ctx: &'a Context<M>,
    pool: &'a mut jobs::JobPool<Built<M>>,
    /// Höhen und Biome der geladenen Spalten, geteilt mit den Höhlen-Jobs.
    cache: &'a mut HashMap<IVec2, Arc<ColumnData>>,
    /// Seiten-Verbindungen der von Spalten gebauten Chunks (Cave Culling). Fehlt ein Eintrag,
    /// gilt der Chunk als offen (vorsichtig).
    surface_links: &'a mut HashMap<IVec3, FaceLinks>,
    dirty: &'a mut dirty::DirtyChunks,
    field: &'a mut field::DensityField,
    events: &'a mut Vec<Event<M>>,
    camera: Vec3,
}

pub struct Streamer<M> {
    ctx: Context<M>,
    pool: jobs::JobPool<Built<M>>,
    columns: columns::Columns<M>,
    caves: caves::Caves<M>,
    far: far::Far<M>,
    cache: HashMap<IVec2, Arc<ColumnData>>,
    surface_links: HashMap<IVec3, FaceLinks>,
    dirty: dirty::DirtyChunks,
    field: field::DensityField,
    events: Vec<Event<M>>,
    job_times: Vec<(JobKind, Duration)>,
    culling: bool,
    cull: CullStats,
}

impl<M: Send + 'static> Streamer<M> {
    pub fn new(
        world: Arc<WorldGen>,
        source: Arc<dyn UndergroundSource>,
        edits: Arc<RwLock<WorldEdits>>,
        config: Config,
        convert: impl Fn(ChunkMesh) -> M + Send + Sync + 'static,
    ) -> Self {
        let pool = jobs::JobPool::new(config.threads);
        let culling = config.culling;
        Self {
            ctx: Context { world, source, edits, convert: Arc::new(convert), config: Arc::new(config) },
            pool,
            columns: Default::default(),
            caves: Default::default(),
            far: Default::default(),
            cache: HashMap::new(),
            surface_links: HashMap::new(),
            dirty: Default::default(),
            field: Default::default(),
            events: Vec::new(),
            job_times: Vec::new(),
            culling,
            cull: CullStats::default(),
        }
    }

    /// Ein Schritt (pro Frame): fertige Jobs übernehmen, Chunks einfügen/entfernen, neue Jobs starten.
    ///
    /// Panict ein Job, panict auch `update`, aber erst nachdem alle anderen Ergebnisse übernommen
    /// und der Job vergessen ist: Fängt die Engine den Panic (gdext tut das), läuft das Streaming
    /// weiter und versucht die Position erneut.
    pub fn update(&mut self, camera: Vec3) {
        let mut panic = None;
        for done in self.pool.poll() {
            if self.ctx.config.record_job_times {
                self.job_times.push((done.kind, done.duration));
            }
            match done.result {
                Ok(Built::Column(build)) => self.columns.accept(done.id, build),
                Ok(Built::Cave(build)) => self.caves.accept(done.id, build),
                Ok(Built::Far(build)) => self.far.accept(done.id, build),
                Ok(Built::Field(build)) => self.field.accept(done.id, build),
                Err(payload) => {
                    self.columns.forget(done.id);
                    self.caves.forget(done.id);
                    self.far.forget(done.id);
                    self.field.forget(done.id);
                    panic.get_or_insert(payload);
                }
            }
        }
        if let Some(payload) = panic {
            std::panic::resume_unwind(payload);
        }
        let mut frame = Frame {
            ctx: &self.ctx,
            pool: &mut self.pool,
            cache: &mut self.cache,
            surface_links: &mut self.surface_links,
            dirty: &mut self.dirty,
            field: &mut self.field,
            events: &mut self.events,
            camera,
        };
        self.columns.update(&mut frame);
        self.caves.update(&mut frame);
        self.cull = self.caves.cull(&mut frame, self.culling);
        if self.ctx.config.far {
            self.far.update(&mut frame, &self.columns);
        }
        self.field.update(&self.ctx, &mut self.pool, &self.cache, camera_chunk(camera));
    }

    /// Dichte am Welt-Voxel (> 0 fest): aus den geladenen Chunks nahe der Kamera, sonst die
    /// öffentliche Höhe (siehe `field`). Für Kollision und Bewegung (#13).
    pub fn density_at(&self, voxel: IVec3) -> f32 {
        self.field.density(voxel, &self.cache, &self.ctx.world)
    }

    /// Die Events seit dem letzten Aufruf, in der Reihenfolge, in der sie umzusetzen sind.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, Event<M>> {
        self.events.drain(..)
    }

    /// Markiert veränderte Chunks (Graben/Bauen) zum Neubau. Vor dem nächsten `update` aufrufen.
    pub fn mark_changed(&mut self, chunks: impl IntoIterator<Item = IVec3>) {
        let chunks: Vec<IVec3> = chunks.into_iter().collect();
        for &chunk in &chunks {
            self.field.invalidate(chunk);
        }
        self.dirty.mark(chunks, &self.cache);
    }

    /// Baut eine geladene Spalte neu (z. B. nach dem Fällen eines Baums, #18); nicht geladene
    /// bekommen den Stand ohnehin beim Laden.
    pub fn mark_column_changed(&mut self, column: IVec2) {
        self.dirty.columns.insert(column);
    }

    /// Cave Culling ein- oder ausschalten (z. B. aus im Röntgenmodus).
    pub fn set_culling(&mut self, enabled: bool) {
        self.culling = enabled;
    }

    /// Höhen und Biome einer geladenen Spalte.
    pub fn column(&self, position: IVec2) -> Option<&Arc<ColumnData>> {
        self.cache.get(&position)
    }

    pub fn config(&self) -> &Config {
        &self.ctx.config
    }

    pub fn stats(&self) -> Stats {
        Stats {
            columns: self.columns.loaded_count(),
            column_jobs: self.columns.pending_count(),
            columns_ready: self.columns.ready_count(),
            cave_jobs: self.caves.pending_count(),
            caves_ready: self.caves.ready_count(),
            cave_meshes: self.caves.mesh_count(),
            far_jobs: self.far.pending_count(),
            far_ready: self.far.ready_count(),
            far_tiles: self.far.tile_count(),
            field_chunks: self.field.len(),
            field_jobs: self.field.pending_count(),
            cull: self.cull,
        }
    }

    /// Dauer der seit dem letzten Aufruf fertigen Jobs (nur mit `Config::record_job_times`).
    pub fn take_job_times(&mut self) -> Vec<(JobKind, Duration)> {
        std::mem::take(&mut self.job_times)
    }
}
