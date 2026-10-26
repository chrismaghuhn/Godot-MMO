//! Grobes 4-m-Gitter für teure 3D-Noise (Höhlen, Erze), trilinear auf die Samples interpoliert.
//!
//! Gitterpunkte liegen auf Weltkoordinaten, die Vielfache von `GRID_STEP` sind. Benachbarte
//! Chunks berechnen gemeinsame Gitterpunkte deshalb identisch, und die Interpolation ist an
//! gemeinsamen Samples bitgleich (siehe `sample`).

use crate::volume::CHUNK_SIZE;

pub(super) const GRID_STEP: i32 = 4;
/// Lokale Koordinate des ersten Gitterpunkts (deckt das Rand-Sample -1 ab).
const GRID_MIN: i32 = -GRID_STEP;
/// Gitterpunkte pro Achse: lokal `-4, 0, 4, …, CHUNK_SIZE`.
pub(super) const GRID_POINTS: usize = ((CHUNK_SIZE - GRID_MIN) / GRID_STEP + 1) as usize;

pub(super) struct CoarseGrid {
    pub(super) values: Vec<f32>,
    min: f32,
}

impl CoarseGrid {
    /// Baut das Gitter eines Chunks. `column(x, z)` liefert einmal pro Gitterspalte einen
    /// Kontext (z. B. 2D-Noise), `value(kontext, x, y, z)` den Wert am Gitterpunkt.
    pub(super) fn build<C>(
        chunk_x: i32,
        chunk_y: i32,
        chunk_z: i32,
        mut column: impl FnMut(f32, f32) -> C,
        mut value: impl FnMut(&C, f32, f32, f32) -> f32,
    ) -> Self {
        let n = GRID_POINTS;
        let mut values = vec![0.0; n * n * n];
        let mut min = f32::INFINITY;
        let origin = [chunk_x * CHUNK_SIZE, chunk_y * CHUNK_SIZE, chunk_z * CHUNK_SIZE];
        let world = |axis: usize, index: usize| (origin[axis] + GRID_MIN + index as i32 * GRID_STEP) as f32;
        for iz in 0..n {
            for ix in 0..n {
                let (x, z) = (world(0, ix), world(2, iz));
                let context = column(x, z);
                for iy in 0..n {
                    let v = value(&context, x, world(1, iy), z);
                    values[(iz * n + iy) * n + ix] = v;
                    min = min.min(v);
                }
            }
        }
        Self { values, min }
    }

    /// Kleinster Gitterwert. Kein interpolierter Wert ist kleiner.
    pub(super) fn min(&self) -> f32 {
        self.min
    }

    /// Trilinear interpolierter Wert an lokaler Sample-Koordinate (`-1..=CHUNK_SIZE`).
    pub(super) fn sample(&self, x: i32, y: i32, z: i32) -> f32 {
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
        // Gewichtete Form statt `a + (b - a) * t`: exakt `a` bei t = 0 und `b` bei t = 1, damit
        // benachbarte Chunks an gemeinsamen Samples bitgleich rechnen. Mit Gewichten >= 0 bleibt
        // das Ergebnis > 0, wenn beide Werte > 0 sind (Voraussetzung für sichere Prüfungen mit `min`).
        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
        let x00 = lerp(at(0, 0, 0), at(1, 0, 0), tx);
        let x10 = lerp(at(0, 1, 0), at(1, 1, 0), tx);
        let x01 = lerp(at(0, 0, 1), at(1, 0, 1), tx);
        let x11 = lerp(at(0, 1, 1), at(1, 1, 1), tx);
        lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> CoarseGrid {
        CoarseGrid::build(3, -2, -5, |x, z| x * 0.01 + z * 0.02, |c, x, y, z| (c + y * 0.03 + (x * z).sin()).cos())
    }

    #[test]
    fn interpolation_hits_grid_points_exactly() {
        let grid = grid();
        let n = GRID_POINTS;
        for (ix, iy, iz) in [(0, 0, 0), (1, 2, 3), (n - 1, n - 1, n - 1), (4, 9, 0)] {
            let local = |i: usize| GRID_MIN + i as i32 * GRID_STEP;
            let expected = grid.values[(iz * n + iy) * n + ix];
            assert_eq!(grid.sample(local(ix), local(iy), local(iz)), expected);
        }
    }

    #[test]
    fn min_is_a_lower_bound_of_every_sample() {
        let grid = grid();
        for z in -1..=CHUNK_SIZE {
            for y in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    assert!(grid.sample(x, y, z) >= grid.min());
                }
            }
        }
    }
}
