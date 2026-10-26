//! Volumen-Terrain: Dichtefeld in 3D-Chunks (D-004).
//!
//! Konvention: `dichte > 0` ist fest (Fels/Erde), `dichte <= 0` ist Luft.
//! Die Oberfläche liegt bei `dichte == 0`. Aktuell kommt die Dichte nur aus der
//! Höhenfunktion (`dichte = höhe(x,z) - y`); Höhlen ziehen später davon ab (#4).

use glam::IVec3;

use crate::world::{BiomeRef, WorldGen};

/// Kantenlänge eines Chunks in Zellen (= Meter).
pub const CHUNK_SIZE: i32 = 32;

/// Samples pro Achse in einer Spalte: lokale Koordinaten `-1..=CHUNK_SIZE + 1`.
/// Das Extra-Sample am oberen Rand braucht die Baumplanung (Zellen mit 4 Eckhöhen).
pub const COLUMN_SAMPLES: usize = CHUNK_SIZE as usize + 3;

/// Samples pro Achse in einem Dichte-Chunk: lokale Koordinaten `-1..=CHUNK_SIZE`.
/// Die Lage `-1` gehört zum Nachbarn und sorgt dafür, dass Meshes nahtlos aneinanderpassen.
pub const DENSITY_SAMPLES: usize = CHUNK_SIZE as usize + 2;

/// Lokale Koordinate des ersten Samples (Rand zum Nachbarn).
pub const SAMPLE_MIN: i32 = -1;

/// Höhen und Biome einer Chunk-Spalte (x/z), Grundlage für alle Chunks darüber und darunter.
pub struct ColumnData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// `heights[z][x]` für lokale Koordinaten `x, z` in `-1..=CHUNK_SIZE + 1` (Index = lokal + 1).
    pub heights: [[f32; COLUMN_SAMPLES]; COLUMN_SAMPLES],
    /// Biome mit derselben Indizierung wie `heights`.
    pub biomes: [[BiomeRef; COLUMN_SAMPLES]; COLUMN_SAMPLES],
    min_height: f32,
    max_height: f32,
}

/// Was ein Chunk enthält. Nur `Surface` braucht Dichte-Daten und ein Mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkKind {
    Air,
    Solid,
    Surface,
}

impl ColumnData {
    pub fn sample(world: &WorldGen, chunk_x: i32, chunk_z: i32) -> Self {
        let default_biome = world.biome_at(&world.sample(0.0, 0.0));
        let mut column = Self {
            chunk_x,
            chunk_z,
            heights: [[0.0; COLUMN_SAMPLES]; COLUMN_SAMPLES],
            biomes: [[default_biome; COLUMN_SAMPLES]; COLUMN_SAMPLES],
            min_height: f32::INFINITY,
            max_height: f32::NEG_INFINITY,
        };
        let origin_x = chunk_x * CHUNK_SIZE;
        let origin_z = chunk_z * CHUNK_SIZE;
        for iz in 0..COLUMN_SAMPLES {
            for ix in 0..COLUMN_SAMPLES {
                let world_x = (origin_x + ix as i32 + SAMPLE_MIN) as f32;
                let world_z = (origin_z + iz as i32 + SAMPLE_MIN) as f32;
                let sample = world.sample(world_x, world_z);
                column.heights[iz][ix] = sample.height;
                column.biomes[iz][ix] = world.biome_at(&sample);
            }
        }
        // Min/Max nur über den Bereich, den Dichte-Chunks nutzen (-1..=CHUNK_SIZE).
        for row in column.heights.iter().take(DENSITY_SAMPLES) {
            for &height in row.iter().take(DENSITY_SAMPLES) {
                column.min_height = column.min_height.min(height);
                column.max_height = column.max_height.max(height);
            }
        }
        column
    }

    /// Höhe an lokaler Spaltenkoordinate (`-1..=CHUNK_SIZE + 1`).
    #[inline]
    pub fn height(&self, local_x: i32, local_z: i32) -> f32 {
        self.heights[(local_z - SAMPLE_MIN) as usize][(local_x - SAMPLE_MIN) as usize]
    }

    /// Biom an lokaler Spaltenkoordinate (`-1..=CHUNK_SIZE + 1`).
    #[inline]
    pub fn biome(&self, local_x: i32, local_z: i32) -> BiomeRef {
        self.biomes[(local_z - SAMPLE_MIN) as usize][(local_x - SAMPLE_MIN) as usize]
    }

    /// Klassifiziert den Chunk in Höhe `chunk_y`, ohne das Dichtefeld zu erzeugen.
    pub fn classify(&self, chunk_y: i32) -> ChunkKind {
        let bottom = (chunk_y * CHUNK_SIZE + SAMPLE_MIN) as f32;
        let top = (chunk_y * CHUNK_SIZE + CHUNK_SIZE) as f32;
        // Fest heißt: jedes Sample hat dichte = höhe - y > 0, also höhe > y für das höchste y.
        if self.min_height > top {
            ChunkKind::Solid
        } else if self.max_height <= bottom {
            ChunkKind::Air
        } else {
            ChunkKind::Surface
        }
    }

    /// Alle Chunk-Höhen, in denen diese Spalte eine Oberfläche haben kann.
    pub fn surface_chunks(&self) -> impl Iterator<Item = i32> + '_ {
        let lowest = ((self.min_height - CHUNK_SIZE as f32) / CHUNK_SIZE as f32).floor() as i32;
        let highest = ((self.max_height + 1.0) / CHUNK_SIZE as f32).floor() as i32;
        (lowest..=highest).filter(|&chunk_y| self.classify(chunk_y) == ChunkKind::Surface)
    }
}

/// Dichtefeld eines Chunks inklusive einer Lage Rand-Samples zum Nachbarn.
pub struct DensityChunk {
    pub position: IVec3,
    values: Vec<f32>,
}

impl DensityChunk {
    pub fn from_column(column: &ColumnData, chunk_y: i32) -> Self {
        let n = DENSITY_SAMPLES;
        let mut values = vec![0.0; n * n * n];
        let origin_y = chunk_y * CHUNK_SIZE;
        for iz in 0..n {
            for ix in 0..n {
                let height = column.heights[iz][ix];
                for iy in 0..n {
                    let world_y = (origin_y + iy as i32 + SAMPLE_MIN) as f32;
                    values[(iz * n + iy) * n + ix] = height - world_y;
                }
            }
        }
        Self {
            position: IVec3::new(column.chunk_x, chunk_y, column.chunk_z),
            values,
        }
    }

    /// Dichte an lokaler Koordinate `-1..=CHUNK_SIZE` je Achse.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> f32 {
        let n = DENSITY_SAMPLES;
        let ix = (x - SAMPLE_MIN) as usize;
        let iy = (y - SAMPLE_MIN) as usize;
        let iz = (z - SAMPLE_MIN) as usize;
        self.values[(iz * n + iy) * n + ix]
    }

    /// Setzt die Dichte an lokaler Koordinate `-1..=CHUNK_SIZE` je Achse.
    /// Nur innerhalb des Cores (Untergrund), damit Chunks nicht beliebig verändert werden.
    #[cfg_attr(not(feature = "underground"), allow(dead_code))]
    #[inline]
    pub(crate) fn set(&mut self, x: i32, y: i32, z: i32, value: f32) {
        let n = DENSITY_SAMPLES;
        let ix = (x - SAMPLE_MIN) as usize;
        let iy = (y - SAMPLE_MIN) as usize;
        let iz = (z - SAMPLE_MIN) as usize;
        self.values[(iz * n + iy) * n + ix] = value;
    }

    /// Weltposition der lokalen Koordinate (0, 0, 0).
    pub fn origin(&self) -> IVec3 {
        self.position * CHUNK_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_changes(chunk: &DensityChunk) -> bool {
        let mut solid = false;
        let mut air = false;
        for z in SAMPLE_MIN..=CHUNK_SIZE {
            for y in SAMPLE_MIN..=CHUNK_SIZE {
                for x in SAMPLE_MIN..=CHUNK_SIZE {
                    if chunk.get(x, y, z) > 0.0 {
                        solid = true;
                    } else {
                        air = true;
                    }
                }
            }
        }
        solid && air
    }

    #[test]
    fn classification_matches_density_field() {
        let world = WorldGen::new(42);
        let mut counts = [0usize; 3];
        // Spalten quer durch Meer, Ebene und Gebirge.
        for (chunk_x, chunk_z) in [(0, 0), (5, -3), (-12, 7), (20, 20), (-30, -8), (40, -25)] {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            for chunk_y in -4..10 {
                let kind = column.classify(chunk_y);
                let chunk = DensityChunk::from_column(&column, chunk_y);
                let mixed = sign_changes(&chunk);
                match kind {
                    ChunkKind::Surface => {
                        counts[0] += 1;
                        assert!(mixed, "Surface ohne Vorzeichenwechsel bei {chunk_x},{chunk_y},{chunk_z}");
                    }
                    ChunkKind::Solid | ChunkKind::Air => {
                        counts[if kind == ChunkKind::Solid { 1 } else { 2 }] += 1;
                        assert!(!mixed, "{kind:?} mit Vorzeichenwechsel bei {chunk_x},{chunk_y},{chunk_z}");
                    }
                }
            }
        }
        assert!(counts.iter().all(|&count| count > 0), "alle Arten sollten vorkommen: {counts:?}");
    }

    #[test]
    fn surface_chunks_lists_exactly_the_surface_chunks() {
        let world = WorldGen::new(7);
        for (chunk_x, chunk_z) in [(0, 0), (3, 9), (-17, 4)] {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            let listed: Vec<i32> = column.surface_chunks().collect();
            let expected: Vec<i32> = (-10..20)
                .filter(|&chunk_y| column.classify(chunk_y) == ChunkKind::Surface)
                .collect();
            assert_eq!(listed, expected);
            assert!(!listed.is_empty());
        }
    }

    #[test]
    fn column_matches_world_heights() {
        let world = WorldGen::new(42);
        let column = ColumnData::sample(&world, 2, -1);
        for (x, z) in [(-1, -1), (0, 0), (17, 5), (32, 32), (33, 33)] {
            let expected = world.column_height((2 * CHUNK_SIZE + x) as f32, (-CHUNK_SIZE + z) as f32);
            assert_eq!(column.height(x, z), expected);
        }
    }
}
