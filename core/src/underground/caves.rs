//! Höhlen (#4): große Kavernen und lange Tunnel im geheimen Untergrund.
//!
//! - **Kavernen** ("Cheese"): 3D-Noise, hohl wo der Wert über einer Schwelle liegt.
//! - **Tunnel** ("Spaghetti"): zwei 3D-Noises, hohl wo beide nahe 0 sind (Schnittlinie).
//! - **Regionen**: 2D-Noise, Gebiete mit vielen bzw. ohne Höhlen.
//! - **Eingänge**: 2D-Noise, nur an Land; dort dürfen Höhlen die Oberfläche anschneiden.
//!
//! Die Höhlen-Dichte wird nur auf einem 4-m-Gitter berechnet und trilinear interpoliert.
//! Weil Interpolation nie unter den kleinsten Gitterwert fällt, ist `CaveGrid::may_have_caves`
//! eine sichere Prüfung, ob ein Chunk überhaupt Höhlen enthalten kann.
//!
//! **Schutz-Schale (D-005):** Luft wird nie verändert, und in den obersten `SURFACE_SHELL`
//! Metern unter dem Boden bleibt die Dichte bitgenau gleich, außer an Eingängen. Sonst
//! würde die sichtbare Oberfläche je nach geheimem Seed minimal anders aussehen und
//! flache Höhlen verraten.

use crate::noise::Layer;
use crate::volume::{CHUNK_SIZE, ColumnData, DensityChunk, SAMPLE_MIN};
use crate::world::smoothstep;

use super::SecretSeed;

/// Unterhalb davon gibt es keine Höhlen (Welt-y). Begrenzt auch das vertikale Streaming.
pub const CAVE_MIN_Y: f32 = -160.0;
/// Mindesttiefe unter dem Boden, ab der Höhlen ohne Eingang beginnen dürfen.
pub const SURFACE_SHELL: f32 = 4.0;
/// Übergangszone unter der Schale, in der Höhlen langsam auf volle Größe anwachsen.
const SHELL_FADE: f32 = 8.0;

const GRID_STEP: i32 = 4;
/// Lokale Koordinate des ersten Gitterpunkts (deckt das Rand-Sample -1 ab).
const GRID_MIN: i32 = -GRID_STEP;
/// Gitterpunkte pro Achse: lokal `-4, 0, 4, …, CHUNK_SIZE`.
const GRID_POINTS: usize = ((CHUNK_SIZE - GRID_MIN) / GRID_STEP + 1) as usize;

const CHEESE_THRESHOLD: f32 = 0.42;
const TUNNEL_RADIUS: f32 = 0.11;
/// Skalierung auf ungefähr "Meter", damit die Dichte zur Oberfläche (1 pro Meter) passt.
const CHEESE_SCALE: f32 = 40.0;
const TUNNEL_SCALE: f32 = 22.0;
/// Zuschlag, der in höhlenarmen Regionen Hohlräume verhindert.
const REGION_SEAL: f32 = 24.0;
/// Zuschlag im Übergang unter der Schale (bei `allowed = 0` greift die Schale vollständig).
const FADE_SEAL: f32 = 40.0;
/// Praktisch "unendlich fest" unterhalb von `CAVE_MIN_Y`.
const DEEP_SEAL: f32 = 1000.0;

pub(super) struct CaveNoise {
    cheese: Layer,
    tunnel_a: Layer,
    tunnel_b: Layer,
    region: Layer,
    entrance: Layer,
}

impl CaveNoise {
    pub(super) fn new(seed: &SecretSeed) -> Self {
        Self {
            cheese: Layer::new(seed.layer(0xC4EE5E), 0.011, 2),
            tunnel_a: Layer::new(seed.layer(0x7A), 0.018, 1),
            tunnel_b: Layer::new(seed.layer(0x7B), 0.018, 1),
            region: Layer::new(seed.layer(0x4E6), 0.0030, 2),
            entrance: Layer::new(seed.layer(0xE7), 0.0090, 2),
        }
    }

    /// Höhlen-Dichte an einer Weltposition (positiv = fest), ohne Schutz-Schale.
    /// `region` ist der 2D-Faktor dieser Spalte (0 = keine Höhlen, 1 = volle Höhlen).
    fn raw_density(&self, x: f32, y: f32, z: f32, region: f32) -> f32 {
        let cheese = self.cheese.get3(x, y * 1.6, z);
        let cheese_density = (CHEESE_THRESHOLD - cheese) * CHEESE_SCALE;

        let a = self.tunnel_a.get3(x, y * 1.3, z);
        let b = self.tunnel_b.get3(x, y * 1.3, z);
        let tunnel_density = ((a * a + b * b).sqrt() - TUNNEL_RADIUS) * TUNNEL_SCALE;

        let deep = (1.0 - smoothstep(CAVE_MIN_Y, CAVE_MIN_Y + 20.0, y)) * DEEP_SEAL;
        cheese_density.min(tunnel_density) + (1.0 - region) * REGION_SEAL + deep
    }

    fn region(&self, x: f32, z: f32) -> f32 {
        smoothstep(-0.35, 0.35, self.region.get(x, z))
    }

    /// 0..1: Wie stark dürfen Höhlen an dieser Stelle bis an die Oberfläche reichen?
    /// Nur an Land, damit keine Löcher in Meeres- oder Flussböden entstehen.
    pub(super) fn entrance(&self, x: f32, z: f32, surface_height: f32, sea_level: f32) -> f32 {
        let land = smoothstep(sea_level + 3.0, sea_level + 6.0, surface_height);
        if land <= 0.0 {
            return 0.0;
        }
        smoothstep(0.60, 0.80, self.entrance.get(x, z)) * land
    }
}

/// Höhlen-Dichte eines Chunks auf dem groben Gitter.
pub(super) struct CaveGrid {
    values: Vec<f32>,
    min: f32,
}

impl CaveGrid {
    /// Kann ein Chunk in Höhe `chunk_y` überhaupt Höhlen haben? (ohne Noise)
    pub(super) fn in_cave_range(chunk_y: i32) -> bool {
        ((chunk_y * CHUNK_SIZE + CHUNK_SIZE) as f32) > CAVE_MIN_Y
    }

    pub(super) fn build(noise: &CaveNoise, chunk_x: i32, chunk_y: i32, chunk_z: i32) -> Self {
        let n = GRID_POINTS;
        let mut values = vec![0.0; n * n * n];
        let mut min = f32::INFINITY;
        let origin = [chunk_x * CHUNK_SIZE, chunk_y * CHUNK_SIZE, chunk_z * CHUNK_SIZE];
        let world = |axis: usize, index: usize| (origin[axis] + GRID_MIN + index as i32 * GRID_STEP) as f32;
        for iz in 0..n {
            for ix in 0..n {
                let (x, z) = (world(0, ix), world(2, iz));
                let region = noise.region(x, z);
                for iy in 0..n {
                    let value = noise.raw_density(x, world(1, iy), z, region);
                    values[(iz * n + iy) * n + ix] = value;
                    min = min.min(value);
                }
            }
        }
        Self { values, min }
    }

    /// Sicher: `false` bedeutet, dass der Chunk garantiert keine Höhle enthält.
    pub(super) fn may_have_caves(&self) -> bool {
        self.min <= 0.0
    }

    /// Trilinear interpolierte Höhlen-Dichte an lokaler Sample-Koordinate.
    fn sample(&self, x: i32, y: i32, z: i32) -> f32 {
        let n = GRID_POINTS;
        let cell = |v: i32| {
            let offset = v - GRID_MIN;
            let index = ((offset / GRID_STEP) as usize).min(n - 2);
            let t = (offset - index as i32 * GRID_STEP) as f32 / GRID_STEP as f32;
            (index, t)
        };
        let (ix, tx) = cell(x);
        let (iy, ty) = cell(y);
        let (iz, tz) = cell(z);
        let at = |dx: usize, dy: usize, dz: usize| self.values[((iz + dz) * n + iy + dy) * n + ix + dx];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let x00 = lerp(at(0, 0, 0), at(1, 0, 0), tx);
        let x10 = lerp(at(0, 1, 0), at(1, 1, 0), tx);
        let x01 = lerp(at(0, 0, 1), at(1, 0, 1), tx);
        let x11 = lerp(at(0, 1, 1), at(1, 1, 1), tx);
        lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
    }
}

/// Schneidet die Höhlen in ein Dichtefeld. Gibt zurück, ob dabei Luft entstanden ist.
pub(super) fn carve(
    noise: &CaveNoise,
    grid: &CaveGrid,
    column: &ColumnData,
    chunk: &mut DensityChunk,
    sea_level: f32,
) -> bool {
    let origin = chunk.origin();
    let mut carved_air = false;
    for z in SAMPLE_MIN..=CHUNK_SIZE {
        for x in SAMPLE_MIN..=CHUNK_SIZE {
            let height = column.height(x, z);
            let entrance = noise.entrance(
                (origin.x + x) as f32,
                (origin.z + z) as f32,
                height,
                sea_level,
            );
            for y in SAMPLE_MIN..=CHUNK_SIZE {
                let surface = chunk.get(x, y, z);
                // Luft bleibt immer unverändert.
                if surface <= 0.0 {
                    continue;
                }
                let depth = height - (origin.y + y) as f32;
                let allowed = smoothstep(SURFACE_SHELL, SURFACE_SHELL + SHELL_FADE, depth).max(entrance);
                // Schutz-Schale: bitgenau unverändert.
                if allowed <= 0.0 {
                    continue;
                }
                let cave = grid.sample(x, y, z) + (1.0 - allowed) * FADE_SEAL;
                if cave < surface {
                    chunk.set(x, y, z, cave);
                    carved_air |= cave <= 0.0;
                }
            }
        }
    }
    carved_air
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise() -> CaveNoise {
        CaveNoise::new(&SecretSeed::from_u64(0xCAFE))
    }

    #[test]
    fn interpolation_hits_grid_points_exactly() {
        let grid = CaveGrid::build(&noise(), 3, -2, -5);
        let n = GRID_POINTS;
        for (ix, iy, iz) in [(0, 0, 0), (1, 2, 3), (n - 1, n - 1, n - 1), (4, 9, 0)] {
            let local = |i: usize| GRID_MIN + i as i32 * GRID_STEP;
            let expected = grid.values[(iz * n + iy) * n + ix];
            assert_eq!(grid.sample(local(ix), local(iy), local(iz)), expected);
        }
    }

    /// Wenn das Gitter keine Höhle meldet, darf auch kein interpolierter Wert <= 0 sein.
    #[test]
    fn may_have_caves_is_a_safe_check() {
        let noise = noise();
        let (mut checked, mut total) = (0, 0);
        for chunk_z in -4..4 {
            for chunk_x in -4..4 {
                for chunk_y in -4..=0 {
                    total += 1;
                    let grid = CaveGrid::build(&noise, chunk_x, chunk_y, chunk_z);
                    if grid.may_have_caves() {
                        continue;
                    }
                    checked += 1;
                    for z in SAMPLE_MIN..=CHUNK_SIZE {
                        for y in SAMPLE_MIN..=CHUNK_SIZE {
                            for x in SAMPLE_MIN..=CHUNK_SIZE {
                                assert!(grid.sample(x, y, z) > 0.0);
                            }
                        }
                    }
                }
            }
        }
        eprintln!("höhlenfreie Chunks laut Gitter: {checked}/{total}");
        assert!(checked > 0, "kein höhlenfreier Chunk gefunden");
    }

    #[test]
    fn no_caves_below_the_minimum() {
        let lowest = (CAVE_MIN_Y / CHUNK_SIZE as f32).floor() as i32 - 1;
        assert!(!CaveGrid::in_cave_range(lowest));
        // Auch wenn jemand das Gitter dort trotzdem baut: alles fest.
        let grid = CaveGrid::build(&noise(), 0, lowest, 0);
        assert!(!grid.may_have_caves());
        assert!(grid.values.iter().all(|&v| v > 100.0));
    }
}
