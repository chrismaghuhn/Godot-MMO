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

mod caves;
mod seed;

pub use caves::{CAVE_MIN_Y, SURFACE_SHELL};
pub use seed::SecretSeed;

use caves::{CaveGrid, CaveNoise};

use crate::volume::{ChunkKind, ColumnData, DensityChunk};
use crate::world::WorldConfig;

/// Erzeugt den Untergrund auf Basis des öffentlichen Dichtefelds.
pub struct Underground {
    seed: SecretSeed,
    sea_level: f32,
    caves: CaveNoise,
}

impl Underground {
    pub fn new(seed: SecretSeed, config: &WorldConfig) -> Self {
        Self {
            seed,
            sea_level: config.sea_level,
            caves: CaveNoise::new(&seed),
        }
    }

    pub fn seed(&self) -> SecretSeed {
        self.seed
    }

    /// Was der Chunk nach Anwendung des Untergrunds enthält.
    ///
    /// Konservativ: `Solid` heißt garantiert ohne Hohlraum. `Surface` kann bei einem
    /// Chunk unter der Oberfläche trotzdem ohne Höhle sein (dann liefert
    /// `density_chunk` `None`), weil nur das grobe Gitter geprüft wird.
    pub fn classify(&self, column: &ColumnData, chunk_y: i32) -> ChunkKind {
        match column.classify(chunk_y) {
            ChunkKind::Solid if self.cave_grid(column, chunk_y).is_some() => ChunkKind::Surface,
            kind => kind,
        }
    }

    /// Vollständiges Dichtefeld eines Chunks: Oberfläche plus Höhlen.
    /// `None`, wenn der Chunk kein Mesh braucht (nur Luft oder nur Fels).
    pub fn density_chunk(&self, column: &ColumnData, chunk_y: i32) -> Option<DensityChunk> {
        let surface_kind = column.classify(chunk_y);
        if surface_kind == ChunkKind::Air {
            return None;
        }
        let Some(grid) = self.cave_grid(column, chunk_y) else {
            return (surface_kind == ChunkKind::Surface)
                .then(|| DensityChunk::from_column(column, chunk_y));
        };
        let mut chunk = DensityChunk::from_column(column, chunk_y);
        let carved_air = caves::carve(&self.caves, &grid, column, &mut chunk, self.sea_level);
        (surface_kind == ChunkKind::Surface || carved_air).then_some(chunk)
    }

    /// Höhlen-Gitter, falls der Chunk Höhlen enthalten kann.
    fn cave_grid(&self, column: &ColumnData, chunk_y: i32) -> Option<CaveGrid> {
        if !CaveGrid::in_cave_range(chunk_y) {
            return None;
        }
        let grid = CaveGrid::build(&self.caves, column.chunk_x, chunk_y, column.chunk_z);
        grid.may_have_caves().then_some(grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldGen;
    use crate::mesh::surface_nets;
    use crate::volume::{CHUNK_SIZE, SAMPLE_MIN};

    const COLUMNS: [(i32, i32); 5] = [(0, 0), (8, -3), (-15, 11), (4, 4), (-6, -9)];

    fn setup(secret: u64) -> (WorldGen, Underground) {
        let world = WorldGen::new(42);
        let underground = Underground::new(SecretSeed::from_u64(secret), &world.cfg);
        (world, underground)
    }

    fn for_each_sample(mut f: impl FnMut(i32, i32, i32)) {
        for z in SAMPLE_MIN..=CHUNK_SIZE {
            for y in SAMPLE_MIN..=CHUNK_SIZE {
                for x in SAMPLE_MIN..=CHUNK_SIZE {
                    f(x, y, z);
                }
            }
        }
    }

    /// D-005: Luft und die Schutz-Schale unter dem Boden sind bitgenau unabhängig vom
    /// geheimen Seed, außer an Eingängen. Sonst würde die Oberfläche Höhlen verraten.
    #[test]
    fn air_and_shell_unchanged_without_entrance() {
        let mut checked_shell = 0usize;
        for secret in [1u64, 0xFFFF_0000_1234] {
            let (world, underground) = setup(secret);
            for (chunk_x, chunk_z) in COLUMNS {
                let column = ColumnData::sample(&world, chunk_x, chunk_z);
                for chunk_y in column.surface_chunks() {
                    let public = DensityChunk::from_column(&column, chunk_y);
                    let full = underground.density_chunk(&column, chunk_y).expect("Oberflächen-Chunk fehlt");
                    let origin = public.origin();
                    for_each_sample(|x, y, z| {
                        let before = public.get(x, y, z);
                        let after = full.get(x, y, z);
                        if before <= 0.0 {
                            assert_eq!(before.to_bits(), after.to_bits(), "Luft verändert");
                            return;
                        }
                        let height = column.height(x, z);
                        let depth = height - (origin.y + y) as f32;
                        let entrance = underground.caves.entrance(
                            (origin.x + x) as f32,
                            (origin.z + z) as f32,
                            height,
                            world.cfg.sea_level,
                        );
                        if depth < SURFACE_SHELL && entrance <= 0.0 {
                            checked_shell += 1;
                            assert_eq!(before.to_bits(), after.to_bits(), "Schale verändert");
                        }
                    });
                }
            }
        }
        assert!(checked_shell > 10_000, "zu wenige Schalen-Samples geprüft: {checked_shell}");
    }

    /// Anteil Hohlraum unterhalb der Schale. Bewusst grobe Grenzen: fängt "gar keine Höhlen"
    /// und "alles hohl" ab, ohne bei jeder Feinjustierung zu brechen.
    #[test]
    fn caves_have_a_reasonable_volume() {
        let (world, underground) = setup(7);
        let (mut total, mut hollow) = (0usize, 0usize);
        for chunk_z in -3..3 {
            for chunk_x in -3..3 {
                let column = ColumnData::sample(&world, chunk_x, chunk_z);
                for chunk_y in -4..=1 {
                    let public = DensityChunk::from_column(&column, chunk_y);
                    let full = underground.density_chunk(&column, chunk_y);
                    let origin = public.origin();
                    for_each_sample(|x, y, z| {
                        let world_y = (origin.y + y) as f32;
                        let deep_enough = column.height(x, z) - world_y > SURFACE_SHELL + 8.0;
                        if public.get(x, y, z) <= 0.0 || !deep_enough || world_y < CAVE_MIN_Y + 20.0 {
                            return;
                        }
                        total += 1;
                        if full.as_ref().is_some_and(|chunk| chunk.get(x, y, z) <= 0.0) {
                            hollow += 1;
                        }
                    });
                }
            }
        }
        let share = hollow as f64 / total as f64;
        eprintln!("Hohlraum-Anteil unter der Schale: {:.2} %", share * 100.0);
        assert!((0.01..0.30).contains(&share), "Hohlraum-Anteil {share:.3} außerhalb 1-30 %");
    }

    #[test]
    fn different_secrets_give_different_caves() {
        let world = WorldGen::new(42);
        let hollow_mask = |secret: u64| {
            let underground = Underground::new(SecretSeed::from_u64(secret), &world.cfg);
            let mut mask = Vec::new();
            for (chunk_x, chunk_z) in COLUMNS {
                let column = ColumnData::sample(&world, chunk_x, chunk_z);
                for chunk_y in -3..=0 {
                    let chunk = underground.density_chunk(&column, chunk_y);
                    for_each_sample(|x, y, z| {
                        mask.push(chunk.as_ref().is_some_and(|c| c.get(x, y, z) <= 0.0));
                    });
                }
            }
            mask
        };
        let (a, b) = (hollow_mask(1), hollow_mask(2));
        assert_eq!(a, hollow_mask(1), "nicht deterministisch");
        let differing = a.iter().zip(&b).filter(|(p, q)| p != q).count();
        assert!(differing > 1000, "Höhlen hängen kaum vom Seed ab: {differing}");
    }

    /// `classify == Solid` muss garantieren, dass es keinen Hohlraum gibt.
    #[test]
    fn classification_is_conservative() {
        let (world, underground) = setup(3);
        let (mut solid, mut caves) = (0, 0);
        for (chunk_x, chunk_z) in COLUMNS {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            for chunk_y in -6..=4 {
                let kind = underground.classify(&column, chunk_y);
                let chunk = underground.density_chunk(&column, chunk_y);
                match kind {
                    ChunkKind::Solid => {
                        solid += 1;
                        assert!(chunk.is_none(), "Solid, aber Dichtefeld bei {chunk_x},{chunk_y},{chunk_z}");
                    }
                    ChunkKind::Air => assert!(chunk.is_none()),
                    ChunkKind::Surface => {}
                }
                if column.classify(chunk_y) == ChunkKind::Solid
                    && let Some(chunk) = chunk
                {
                    caves += 1;
                    let mut has_air = false;
                    for_each_sample(|x, y, z| has_air |= chunk.get(x, y, z) <= 0.0);
                    assert!(has_air, "Höhlen-Chunk ohne Luft bei {chunk_x},{chunk_y},{chunk_z}");
                }
            }
        }
        assert!(solid > 0 && caves > 0, "Testabdeckung zu klein: solid={solid}, caves={caves}");
    }

    /// Höhlen-Chunks müssen wie Oberflächen-Chunks nahtlos aneinanderpassen.
    #[test]
    fn cave_chunks_are_seamless() {
        let (world, underground) = setup(5);
        let size = CHUNK_SIZE as f32;
        let mut compared = 0usize;
        for chunk_z in -2..2 {
            for chunk_x in -3..3 {
                let column_a = ColumnData::sample(&world, chunk_x, chunk_z);
                let column_b = ColumnData::sample(&world, chunk_x + 1, chunk_z);
                for chunk_y in -4..=-1 {
                    let (Some(a), Some(b)) = (
                        underground.density_chunk(&column_a, chunk_y),
                        underground.density_chunk(&column_b, chunk_y),
                    ) else {
                        continue;
                    };
                    let (a, b) = (surface_nets(&a), surface_nets(&b));
                    let border = |positions: &[[f32; 3]], low: f32, shift: f32| {
                        let mut points: Vec<[f32; 3]> = positions
                            .iter()
                            .filter(|p| p[0] >= low && p[0] <= low + 1.0)
                            .map(|p| [p[0] + shift, p[1], p[2]])
                            .collect();
                        points.sort_by(|p, q| {
                            p[2].total_cmp(&q[2]).then(p[1].total_cmp(&q[1])).then(p[0].total_cmp(&q[0]))
                        });
                        points
                    };
                    let (border_a, border_b) = (border(&a.positions, size - 1.0, 0.0), border(&b.positions, -1.0, size));
                    assert_eq!(border_a.len(), border_b.len(), "Naht bei {chunk_x},{chunk_y},{chunk_z}");
                    for (p, q) in border_a.iter().zip(&border_b) {
                        let distance = glam::Vec3::from(*p).distance(glam::Vec3::from(*q));
                        assert!(distance < 1e-4, "Naht bei {chunk_x},{chunk_y},{chunk_z}: {p:?} vs {q:?}");
                    }
                    compared += border_a.len();
                }
            }
        }
        assert!(compared > 100, "zu wenige Höhlen-Randvertices verglichen: {compared}");
    }

    #[test]
    fn entrances_only_on_land() {
        let (world, underground) = setup(9);
        let sea = world.cfg.sea_level;
        let mut underwater = 0;
        for z in (-2000..2000).step_by(37) {
            for x in (-2000..2000).step_by(37) {
                let height = world.column_height(x as f32, z as f32);
                if height < sea + 3.0 {
                    underwater += 1;
                    assert_eq!(underground.caves.entrance(x as f32, z as f32, height, sea), 0.0);
                }
            }
        }
        assert!(underwater > 100);
    }

    #[test]
    fn chunks_without_surface_or_caves_need_no_density() {
        let (world, underground) = setup(3);
        let column = ColumnData::sample(&world, 0, 0);
        assert!(underground.density_chunk(&column, 20).is_none(), "hoch in der Luft");
        assert!(underground.density_chunk(&column, -20).is_none(), "unterhalb der Höhlen");
    }
}
