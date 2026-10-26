//! Verbindungen zwischen den Seiten eines Chunks durch Luft (#8, D-006), Grundlage für
//! Cave Culling: Ein Chunk kann nur durch Seiten "hindurchgesehen" werden, die über
//! zusammenhängende Luft verbunden sind.
//!
//! Betrachtet werden die Samples `0..=CHUNK_SIZE` je Achse (6er-Nachbarschaft) – genau die, aus
//! denen der Chunk seine Geometrie baut (Surface Nets erzeugt auch die Kanten 31→32). Die Lage
//! `CHUNK_SIZE` ist dieselbe wie Lage 0 des Nachbarn, deshalb sehen beide Chunks an der gemeinsamen
//! Seite dieselbe Luft (Review-Fund: sonst wurden Wände eines Chunks an der Höhle des Nachbarn
//! weggecullt). Eine Luft-Komponente berührt eine Seite, wenn sie ein Sample in deren Lage hat.

use std::collections::VecDeque;

use crate::volume::{CHUNK_SIZE, DensityChunk};

/// Seiten eines Chunks: -x, +x, -y, +y, -z, +z.
pub const FACES: usize = 6;

/// Für jede Seite die Menge der Seiten, die von ihr aus durch Luft erreichbar sind (Bitmaske,
/// Bit `f` = Seite `f`). Eine Seite mit Luft ist mit sich selbst verbunden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceLinks(pub [u8; FACES]);

impl FaceLinks {
    /// Alle Seiten miteinander verbunden (z. B. reine Luft, oder vorsichtige Annahme).
    pub const ALL: FaceLinks = FaceLinks([0b11_1111; FACES]);
    /// Keine Verbindung (massiver Fels).
    pub const NONE: FaceLinks = FaceLinks([0; FACES]);

    pub fn connects(&self, from: usize, to: usize) -> bool {
        self.0[from] & (1 << to) != 0
    }

    /// Hat die Seite überhaupt Luft?
    pub fn open(&self, face: usize) -> bool {
        self.0[face] != 0
    }
}

/// Gegenüberliegende Seite.
pub fn opposite(face: usize) -> usize {
    face ^ 1
}

/// Richtung (Chunk-Versatz) einer Seite.
pub fn face_offset(face: usize) -> [i32; 3] {
    let mut offset = [0; 3];
    offset[face / 2] = if face.is_multiple_of(2) { -1 } else { 1 };
    offset
}

/// Verbindungen der Seiten eines Chunks durch Luft.
pub fn face_links(chunk: &DensityChunk) -> FaceLinks {
    // Samples je Achse: 0..=CHUNK_SIZE.
    let n = CHUNK_SIZE as usize + 1;
    let index = |x: usize, y: usize, z: usize| (z * n + y) * n + x;
    let mut seen = vec![false; n * n * n];
    let mut links = FaceLinks::NONE;
    let mut queue = VecDeque::new();
    for start in 0..n * n * n {
        let (x, y, z) = (start % n, (start / n) % n, start / (n * n));
        if seen[start] || chunk.get(x as i32, y as i32, z as i32) > 0.0 {
            continue;
        }
        seen[start] = true;
        queue.push_back((x, y, z));
        let mut touched = 0u8;
        while let Some((x, y, z)) = queue.pop_front() {
            for (axis, value) in [x, y, z].into_iter().enumerate() {
                if value == 0 {
                    touched |= 1 << (axis * 2);
                }
                if value == n - 1 {
                    touched |= 1 << (axis * 2 + 1);
                }
            }
            let neighbours = [
                (x > 0).then(|| (x - 1, y, z)),
                (x + 1 < n).then(|| (x + 1, y, z)),
                (y > 0).then(|| (x, y - 1, z)),
                (y + 1 < n).then(|| (x, y + 1, z)),
                (z > 0).then(|| (x, y, z - 1)),
                (z + 1 < n).then(|| (x, y, z + 1)),
            ];
            for (nx, ny, nz) in neighbours.into_iter().flatten() {
                let i = index(nx, ny, nz);
                if !seen[i] && chunk.get(nx as i32, ny as i32, nz as i32) <= 0.0 {
                    seen[i] = true;
                    queue.push_back((nx, ny, nz));
                }
            }
        }
        for face in 0..FACES {
            if touched & (1 << face) != 0 {
                links.0[face] |= touched;
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume::ColumnData;
    use crate::world::WorldGen;

    /// Künstliches Dichtefeld über die Spalte: Höhen so setzen, dass der Chunk passt.
    fn chunk_with(f: impl Fn(i32, i32, i32) -> f32) -> DensityChunk {
        let world = WorldGen::new(1);
        let column = ColumnData::sample(&world, 0, 0);
        let mut chunk = DensityChunk::from_column(&column, 100); // hoch in der Luft: alles Luft
        for z in -1..=CHUNK_SIZE {
            for y in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    let d = f(x, y, z);
                    chunk.set_voxel(x, y, z, d, if d > 0.0 { crate::MaterialId::GRANITE } else { crate::MaterialId::AIR });
                }
            }
        }
        chunk
    }

    #[test]
    fn air_and_rock_extremes() {
        assert_eq!(face_links(&chunk_with(|_, _, _| -1.0)), FaceLinks::ALL);
        assert_eq!(face_links(&chunk_with(|_, _, _| 1.0)), FaceLinks::NONE);
    }

    #[test]
    fn straight_tunnel_connects_only_its_two_ends() {
        // Tunnel entlang x durch die Mitte.
        let links = face_links(&chunk_with(|_, y, z| if (14..18).contains(&y) && (14..18).contains(&z) { -1.0 } else { 1.0 }));
        assert!(links.connects(0, 1) && links.connects(1, 0));
        for face in 2..6 {
            assert!(!links.open(face), "Seite {face} sollte zu sein");
        }
    }

    #[test]
    fn separate_pockets_do_not_connect() {
        // Zwei getrennte Hohlräume: einer an -x, einer an +y.
        let links = face_links(&chunk_with(|x, y, z| {
            let a = x < 4 && (10..14).contains(&y) && (10..14).contains(&z);
            let b = y > 27 && (20..24).contains(&x) && (20..24).contains(&z);
            if a || b { -1.0 } else { 1.0 }
        }));
        assert!(links.open(0) && links.open(3));
        assert!(!links.connects(0, 3) && !links.connects(3, 0), "getrennte Hohlräume verbunden");
        // Ein Hohlraum ganz im Inneren berührt keine Seite.
        let inner = face_links(&chunk_with(|x, y, z| if (10..20).contains(&x) && (10..20).contains(&y) && (10..20).contains(&z) { -1.0 } else { 1.0 }));
        assert_eq!(inner, FaceLinks::NONE);
    }

    /// Luft nur in der gemeinsamen Lage mit dem +x-Nachbarn: Der Chunk baut dort Wände, also muss
    /// er von dieser Seite aus erreichbar sein.
    #[test]
    fn shared_border_layer_counts() {
        let links = face_links(&chunk_with(|x, _, _| if x == CHUNK_SIZE { -1.0 } else { 1.0 }));
        assert!(links.open(1), "Seite +x mit Luft in Lage 32 gilt als zu");
        assert!(!links.open(0));
        let links = face_links(&chunk_with(|_, y, _| if y == 0 { -1.0 } else { 1.0 }));
        assert!(links.open(2) && !links.open(3));
    }

    #[test]
    fn faces_and_offsets() {
        for face in 0..FACES {
            assert_eq!(opposite(opposite(face)), face);
            let (a, b) = (face_offset(face), face_offset(opposite(face)));
            assert_eq!([a[0] + b[0], a[1] + b[1], a[2] + b[2]], [0, 0, 0]);
        }
    }
}
