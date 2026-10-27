//! Chunks für Clients (D-005, D-018): mit Höhlen und Erzen, verborgene Erze durch öffentliches
//! Gestein ersetzt. Geschickt wird nur, was von der öffentlichen Oberfläche abweicht (`Diff`, bzw.
//! `Public`, wenn nichts abweicht); die Oberfläche erzeugt der Client selbst.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, PoisonError};

use glam::{IVec2, IVec3};
use terraforge_core::WorldGen;
use terraforge_core::streaming::UndergroundSource;
use terraforge_core::underground::{SecretSeed, Underground, hide_unexposed_ores};
use terraforge_core::volume::{CHUNK_SIZE, ColumnData, DensityChunk};
use terraforge_protocol::chunk::diff;
use terraforge_protocol::{ChunkKind, ChunkReply};

/// Höchstens so viele Spalten bzw. vollständige Chunks im Speicher (ein Chunk ≈ 240 KB).
const COLUMN_CACHE: usize = 1024;
const CHUNK_CACHE: usize = 128;

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
    world: WorldGen,
    underground: Underground,
    columns: Cache<IVec2, Arc<ColumnData>>,
    chunks: Cache<IVec3, Arc<DensityChunk>>,
}

impl ServerWorld {
    pub fn new(seed: u32, secret: SecretSeed) -> Self {
        let world = WorldGen::new(seed);
        let underground = Underground::new(secret, &world.cfg);
        Self { world, underground, columns: Cache::new(COLUMN_CACHE), chunks: Cache::new(CHUNK_CACHE) }
    }

    pub fn seed(&self) -> u32 {
        self.world.cfg.seed
    }

    pub fn lowest_cave_chunk(&self) -> i32 {
        self.underground.lowest_cave_chunk().expect("Untergrund hat Höhlen")
    }

    fn column(&self, position: IVec2) -> Arc<ColumnData> {
        self.columns.get(position, || Arc::new(ColumnData::sample(&self.world, position.x, position.y)))
    }

    /// Vollständiger, ungefilterter Chunk (für die Nachbarn im Erz-Filter).
    fn full_chunk(&self, position: IVec3) -> Arc<DensityChunk> {
        self.chunks.get(position, || {
            let column = self.column(IVec2::new(position.x, position.z));
            Arc::new(self.underground.materials_chunk(&self.world, &column, position.y))
        })
    }

    fn air(&self, voxel: IVec3) -> bool {
        let owner = voxel.div_euclid(IVec3::splat(CHUNK_SIZE));
        let local = voxel - owner * CHUNK_SIZE;
        self.full_chunk(owner).get(local.x, local.y, local.z) <= 0.0
    }

    /// Der Chunk, wie ihn ein Client sehen darf (Erze nur, wenn sie freiliegen); `None` bei
    /// `ChunkKind::Mesh` ohne Mesh.
    pub fn client_chunk(&self, position: IVec3, kind: ChunkKind) -> Option<DensityChunk> {
        let column = self.column(IVec2::new(position.x, position.z));
        let mut chunk = match kind {
            ChunkKind::Mesh => self.underground.density_chunk(&self.world, &column, position.y)?,
            ChunkKind::Full => (*self.full_chunk(position)).clone(),
        };
        hide_unexposed_ores(&mut chunk, &column, |voxel| self.air(voxel));
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
