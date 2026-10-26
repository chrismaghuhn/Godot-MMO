//! Geheimer Untergrund: alles unter der Oberfläche, was ein Client nicht vorab kennen darf (D-005).
//!
//! Dieses Modul gibt es nur mit dem Cargo-Feature `underground`. Der Spiel-Client baut ohne
//! das Feature und kann den Untergrund deshalb nicht selbst erzeugen; er bekommt
//! Untergrund-Chunks später vom Server (#26).
//!
//! Aufbau (geplant):
//! - `seed`:  der geheime Seed
//! - `caves`: Höhlen (#4)
//! - `ores`:  Gestein und Erze (#5)
//!
//! Regel: Der öffentliche Teil (`world`, `volume`, …) importiert nie etwas aus diesem Modul.
//! Der Compiler erzwingt das, weil der öffentliche Teil auch ohne das Feature bauen muss.

mod seed;

pub use seed::SecretSeed;

use crate::volume::{ChunkKind, ColumnData, DensityChunk};

/// Erzeugt den Untergrund auf Basis des öffentlichen Dichtefelds.
pub struct Underground {
    seed: SecretSeed,
}

impl Underground {
    pub fn new(seed: SecretSeed) -> Self {
        Self { seed }
    }

    pub fn seed(&self) -> SecretSeed {
        self.seed
    }

    /// Was der Chunk nach Anwendung des Untergrunds enthält.
    ///
    /// Aktuell gleich der Oberflächen-Klassifizierung. Mit Höhlen (#4) können
    /// `Solid`-Chunks zu `Surface` werden, weil sie Hohlräume enthalten.
    pub fn classify(&self, column: &ColumnData, chunk_y: i32) -> ChunkKind {
        column.classify(chunk_y)
    }

    /// Vollständiges Dichtefeld eines Chunks: Oberfläche plus Untergrund.
    /// `None`, wenn der Chunk kein Mesh braucht (nur Luft oder nur Fels).
    pub fn density_chunk(&self, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        match self.classify(column, chunk_y) {
            ChunkKind::Surface => Some(DensityChunk::from_column(column, chunk_y)),
            ChunkKind::Air | ChunkKind::Solid => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldGen;

    fn same_density(a: &DensityChunk, b: &DensityChunk) -> bool {
        use crate::volume::{CHUNK_SIZE, SAMPLE_MIN};
        (SAMPLE_MIN..=CHUNK_SIZE).all(|z| {
            (SAMPLE_MIN..=CHUNK_SIZE)
                .all(|y| (SAMPLE_MIN..=CHUNK_SIZE).all(|x| a.get(x, y, z) == b.get(x, y, z)))
        })
    }

    /// Oberhalb des Bodens darf der Untergrund nichts verändern: Der Client muss die
    /// sichtbare Welt ohne den geheimen Seed exakt gleich erzeugen.
    /// Solange es keine Höhlen gibt, gilt das für den ganzen Chunk.
    #[test]
    fn surface_is_independent_of_the_secret_seed() {
        let world = WorldGen::new(42);
        let secrets = [SecretSeed::from_u64(1), SecretSeed::from_u64(0xFFFF_0000_1234)];
        for (chunk_x, chunk_z) in [(0, 0), (8, -3), (-15, 11)] {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            for chunk_y in column.surface_chunks() {
                let public = DensityChunk::from_column(&column, chunk_y);
                for secret in secrets {
                    let full = Underground::new(secret)
                        .density_chunk(&column, chunk_y)
                        .expect("Oberflächen-Chunk fehlt");
                    assert!(same_density(&public, &full));
                }
            }
        }
    }

    #[test]
    fn chunks_without_surface_need_no_density() {
        let world = WorldGen::new(42);
        let underground = Underground::new(SecretSeed::from_u64(3));
        let column = ColumnData::sample(&world, 0, 0);
        assert!(underground.density_chunk(&column, 20).is_none(), "hoch in der Luft");
        assert!(underground.density_chunk(&column, -20).is_none(), "tief im Fels");
    }
}
