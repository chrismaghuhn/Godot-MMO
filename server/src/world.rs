//! Chunks für Clients (D-005, D-018): mit Höhlen und Erzen, verborgene Erze durch öffentliches
//! Gestein ersetzt. Geschickt wird nur, was von der öffentlichen Oberfläche abweicht (`Diff`, bzw.
//! `Public`, wenn nichts abweicht); die Oberfläche erzeugt der Client selbst.
//!
//! Spieler-Änderungen (#14, `edit.rs`) hält der Server; geänderte Chunks schickt er fertig, mit dem
//! Erz-Filter auf dem geänderten Stand (frisch freigelegte Erze werden sichtbar).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use glam::{IVec2, IVec3, Vec2, Vec3Swizzles};
use terraforge_core::WorldGen;
use terraforge_core::edits::{Voxel, WorldEdits, owner_chunk};
use terraforge_core::inventory::Holdings;
use terraforge_core::spawn::find_spawn;
use terraforge_core::vegetation::SPAWN_CLEARING;
use terraforge_core::streaming::UndergroundSource;
use terraforge_core::underground::{SecretSeed, Underground, hide_unexposed_ores};
use terraforge_core::volume::{CHUNK_SIZE, ColumnData, DensityChunk};
use terraforge_protocol::chunk::diff;
use terraforge_protocol::{ChunkKind, ChunkReply};

/// Höchstens so viele Spalten bzw. vollständige Chunks im Speicher (ein Chunk ≈ 240 KB).
const COLUMN_CACHE: usize = 1024;
const CHUNK_CACHE: usize = 128;

/// Größte Chunk-Koordinate (waagerecht ±33.000 km, senkrecht ±8 km). Weiter draußen laufen die
/// Rechnungen des Generators über.
const MAX_CHUNK: u32 = 1 << 20;
const MAX_CHUNK_Y: u32 = 256;

pub(crate) fn in_world(position: IVec3) -> bool {
    // `unsigned_abs`: `abs` liefe bei `i32::MIN` selbst über.
    position.x.unsigned_abs() <= MAX_CHUNK && position.z.unsigned_abs() <= MAX_CHUNK && position.y.unsigned_abs() <= MAX_CHUNK_Y
}

/// Einfacher Cache: Wird er zu groß, fängt er von vorn an (wie im Viewer beim Graben).
struct Cache<K, V>(Mutex<HashMap<K, V>>, usize);

impl<K: Eq + Hash + Copy, V: Clone> Cache<K, V> {
    fn new(limit: usize) -> Self {
        Self(Mutex::new(HashMap::new()), limit)
    }

    /// Wert aus dem Cache oder neu berechnet (ohne Lock, damit Threads sich nicht blockieren).
    fn get(&self, key: K, compute: impl FnOnce() -> V) -> V {
        if let Some(value) = self.0.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
            return value.clone();
        }
        let value = compute();
        let mut map = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if map.len() >= self.1 {
            map.clear();
        }
        map.insert(key, value.clone());
        value
    }
}

pub struct ServerWorld {
    pub(crate) world: WorldGen,
    pub(crate) underground: Underground,
    columns: Cache<IVec2, Arc<ColumnData>>,
    /// Generierte Chunks ohne Spieler-Änderungen.
    chunks: Cache<IVec3, Arc<DensityChunk>>,
    pub(crate) edits: RwLock<WorldEdits>,
    /// Graben und Bauen nacheinander (`edit.rs`).
    pub(crate) edit_lock: Mutex<()>,
    /// Inventar des Spielers (vorerst nur einer, D-021) und gebaute Voxel (`inventory.rs`, #18).
    pub(crate) holdings: Mutex<Holdings>,
    /// Baumfreier Kreis um den Spawn (wie beim Client), einmal berechnet (`fell.rs`).
    pub(crate) clearing: (Vec2, f32),
    /// Seit dem letzten Speichern geändert; nur eine Speicherung zugleich (`save.rs`).
    pub(crate) unsaved: AtomicBool,
    pub(crate) save_lock: Mutex<()>,
}

impl ServerWorld {
    pub fn new(seed: u32, secret: SecretSeed) -> Self {
        let world = WorldGen::new(seed);
        let underground = Underground::new(secret, &world.cfg);
        let clearing = (find_spawn(&world).xz(), SPAWN_CLEARING);
        Self { world, underground, columns: Cache::new(COLUMN_CACHE), chunks: Cache::new(CHUNK_CACHE), edits: RwLock::default(), edit_lock: Mutex::default(), holdings: Mutex::default(), clearing, unsaved: AtomicBool::new(false), save_lock: Mutex::default() }
    }

    pub fn seed(&self) -> u32 {
        self.world.cfg.seed
    }

    pub fn lowest_cave_chunk(&self) -> i32 {
        self.underground.lowest_cave_chunk().expect("Untergrund hat Höhlen")
    }

    pub(crate) fn column(&self, position: IVec2) -> Arc<ColumnData> {
        self.columns.get(position, || Arc::new(ColumnData::sample(&self.world, position.x, position.y)))
    }

    /// Vollständiger, ungefilterter Chunk ohne Spieler-Änderungen.
    pub(crate) fn full_chunk(&self, position: IVec3) -> Arc<DensityChunk> {
        self.chunks.get(position, || {
            let column = self.column(IVec2::new(position.x, position.z));
            Arc::new(self.underground.materials_chunk(&self.world, &column, position.y))
        })
    }

    /// Generierter Voxel ohne Spieler-Änderungen.
    pub(crate) fn generated(&self, voxel: IVec3) -> Voxel {
        let owner = owner_chunk(voxel);
        let local = voxel - owner * CHUNK_SIZE;
        let chunk = self.full_chunk(owner);
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    }

    /// Aktueller Voxel mit Spieler-Änderungen. Hält das Lock nur kurz (nicht während des
    /// Erzeugens), damit Graben nicht auf andere Anfragen wartet.
    pub fn voxel(&self, voxel: IVec3) -> Voxel {
        let edited = self.edits.read().unwrap_or_else(PoisonError::into_inner).get(voxel);
        edited.unwrap_or_else(|| self.generated(voxel))
    }

    /// Der Chunk, wie ihn ein Client sehen darf (Erze nur, wenn sie freiliegen); `None` bei
    /// `ChunkKind::Mesh` ohne Mesh.
    pub fn client_chunk(&self, position: IVec3, kind: ChunkKind) -> Option<DensityChunk> {
        let column = self.column(IVec2::new(position.x, position.z));
        let touched = self.edits.read().unwrap_or_else(PoisonError::into_inner).touches(position);
        let mut chunk = if kind == ChunkKind::Mesh && !touched {
            self.underground.density_chunk(&self.world, &column, position.y)?
        } else {
            // Geänderte Chunks immer vollständig: Ob sie ein Mesh brauchen, weiß nur der neue Stand.
            let mut chunk = (*self.full_chunk(position)).clone();
            self.edits.read().unwrap_or_else(PoisonError::into_inner).apply(&mut chunk);
            chunk
        };
        hide_unexposed_ores(&mut chunk, &column, |voxel| self.voxel(voxel).density <= 0.0);
        Some(chunk)
    }

    /// Antwort auf eine Chunk-Anfrage.
    pub fn reply(&self, position: IVec3, kind: ChunkKind) -> ChunkReply {
        let Some(chunk) = self.client_chunk(position, kind) else {
            return ChunkReply::Empty;
        };
        // Der Client erzeugt die öffentliche Oberfläche selbst; geschickt wird nur, was abweicht.
        let column = self.column(IVec2::new(position.x, position.z));
        let runs = diff(&DensityChunk::from_column(&column, position.y), &chunk);
        if runs.is_empty() { ChunkReply::Public } else { ChunkReply::Diff { runs } }
    }
}
