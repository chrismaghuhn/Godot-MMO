//! Spieler-Änderungen am Terrain (#6): Graben und Bauen.
//!
//! Änderungen werden **pro Welt-Voxel** gespeichert (Dichte + Material), als Überschreibung des
//! generierten Werts. Ein Voxel an einer Chunk-Grenze ist Sample in bis zu 8 Chunks; weil alle
//! denselben gespeicherten Wert lesen, passen Meshes nach einer Änderung weiter nahtlos.
//!
//! Graben und Bauen arbeiten mit einer Kugel als Distanzfeld: Graben nimmt
//! `min(alt, abstand - radius)`, Bauen `max(alt, radius - abstand)`. So entstehen glatte Formen.
//! Graben hat eine Härte-Grenze (Werkzeug, später #21) und meldet die Ausbeute pro Material
//! (für das Inventar, #18).
//!
//! Dieses Modul kennt keinen Generator: Der aktuelle, ungeänderte Wert eines Voxels kommt über
//! eine Funktion (`source`) – beim Server mit Untergrund.
//!
//! **Hinweis für den Server (D-005):** Die gespeicherten Änderungen allein reichen einem Client
//! mit nur öffentlichen Daten nicht: Für Voxel, die im geheimen Feld schon Luft (Höhlen) oder zu
//! hart sind, wird nichts gespeichert. Der Server muss für geänderte Chunks die fertigen
//! Voxeldaten senden, nicht nur die Änderungen. Was dadurch sichtbar wird, sind nur Wände, die
//! ohnehin freiliegen.

use std::collections::{BTreeMap, HashMap, HashSet};

use glam::{IVec3, Vec3};

use crate::material::{MaterialId, MaterialRegistry};
use crate::vegetation::TreeId;
use crate::volume::{CHUNK_SIZE, DensityChunk, SAMPLE_MIN};

/// Dichte und Material eines Voxels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voxel {
    pub density: f32,
    pub material: MaterialId,
}

/// Alle Änderungen der Welt.
#[derive(Default, Debug, Clone)]
pub struct WorldEdits {
    /// Nach Besitzer-Chunk (der Chunk, in dem der Voxel lokal in `0..CHUNK_SIZE` liegt).
    by_owner: HashMap<IVec3, HashMap<IVec3, Voxel>>,
    /// Chunks, deren Sample-Bereich (`-1..=CHUNK_SIZE`) mindestens eine Änderung enthält.
    touched: HashSet<IVec3>,
    /// Gefällte Bäume (#7; Axt und Holz-Ausbeute später, #21).
    felled: HashSet<TreeId>,
}

/// Ergebnis von `dig_sphere`.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct DigResult {
    /// Voxel, die von fest zu Luft wurden, je Material (≈ m³).
    pub removed: BTreeMap<MaterialId, u32>,
    /// Feste Voxel in der Kugel, die für das Werkzeug zu hart waren.
    pub too_hard: u32,
    /// Chunks, die neu vermascht werden müssen.
    pub affected: HashSet<IVec3>,
}

/// Ergebnis von `place_sphere`.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct PlaceResult {
    /// Voxel, die von Luft zu fest wurden (≈ verbrauchtes Material in m³).
    pub placed: u32,
    pub affected: HashSet<IVec3>,
}

/// Chunk, in dem ein Voxel lokal in `0..CHUNK_SIZE` liegt.
pub fn owner_chunk(voxel: IVec3) -> IVec3 {
    voxel.div_euclid(IVec3::splat(CHUNK_SIZE))
}

/// Alle Chunks, die diesen Voxel als Sample enthalten (lokal `-1..=CHUNK_SIZE`, 1-8 Stück).
pub fn chunks_containing(voxel: IVec3) -> impl Iterator<Item = IVec3> {
    let owner = owner_chunk(voxel);
    let local = voxel - owner * CHUNK_SIZE;
    let axis = |l: i32| -> &'static [i32] {
        match l {
            0 => &[0, -1],
            l if l == CHUNK_SIZE - 1 => &[0, 1],
            _ => &[0],
        }
    };
    let (ax, ay, az) = (axis(local.x), axis(local.y), axis(local.z));
    ax.iter().flat_map(move |&dx| {
        ay.iter().flat_map(move |&dy| az.iter().map(move |&dz| owner + IVec3::new(dx, dy, dz)))
    })
}

impl WorldEdits {
    pub fn is_empty(&self) -> bool {
        self.by_owner.is_empty() && self.felled.is_empty()
    }

    /// Fällt einen Baum. `false`, wenn er schon gefällt war.
    pub fn fell_tree(&mut self, id: TreeId) -> bool {
        self.felled.insert(id)
    }

    pub fn is_felled(&self, id: TreeId) -> bool {
        self.felled.contains(&id)
    }

    /// Anzahl geänderter Voxel.
    pub fn len(&self) -> usize {
        self.by_owner.values().map(HashMap::len).sum()
    }

    pub fn get(&self, voxel: IVec3) -> Option<Voxel> {
        self.by_owner.get(&owner_chunk(voxel))?.get(&voxel).copied()
    }

    /// Hat der Chunk (in seinem Sample-Bereich) irgendeine Änderung?
    pub fn touches(&self, chunk: IVec3) -> bool {
        self.touched.contains(&chunk)
    }

    /// Alle Chunks mit Änderungen im Sample-Bereich.
    pub fn touched_chunks(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.touched.iter().copied()
    }

    /// Geänderte Chunks einer Spalte (x/z), nach Höhe sortiert.
    pub fn touched_in_column(&self, chunk_x: i32, chunk_z: i32) -> Vec<i32> {
        let mut heights: Vec<i32> =
            self.touched.iter().filter(|c| c.x == chunk_x && c.z == chunk_z).map(|c| c.y).collect();
        heights.sort_unstable();
        heights
    }

    fn set(&mut self, voxel: IVec3, value: Voxel) {
        self.by_owner.entry(owner_chunk(voxel)).or_default().insert(voxel, value);
        self.touched.extend(chunks_containing(voxel));
    }

    /// Wendet alle Änderungen im Sample-Bereich des Chunks an.
    pub fn apply(&self, chunk: &mut DensityChunk) {
        if !self.touches(chunk.position) {
            return;
        }
        let origin = chunk.origin();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(voxels) = self.by_owner.get(&(chunk.position + IVec3::new(dx, dy, dz))) else {
                        continue;
                    };
                    for (&voxel, value) in voxels {
                        let local = voxel - origin;
                        let inside = (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.x)
                            && (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.y)
                            && (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.z);
                        if inside {
                            chunk.set_voxel(local.x, local.y, local.z, value.density, value.material);
                        }
                    }
                }
            }
        }
    }

    /// Aktueller Wert eines Voxels: Änderung, sonst der generierte Wert aus `source`.
    fn current(&self, voxel: IVec3, source: &mut impl FnMut(IVec3) -> Voxel) -> Voxel {
        self.get(voxel).unwrap_or_else(|| source(voxel))
    }

    /// Gräbt eine Kugel. Voxel mit Härte über `max_hardness` bleiben unverändert.
    pub fn dig_sphere(
        &mut self,
        materials: &MaterialRegistry,
        mut source: impl FnMut(IVec3) -> Voxel,
        center: Vec3,
        radius: f32,
        max_hardness: f32,
    ) -> DigResult {
        let mut result = DigResult::default();
        for voxel in sphere_voxels(center, radius + 1.0) {
            let old = self.current(voxel, &mut source);
            if old.density <= 0.0 {
                continue;
            }
            let carve = voxel.as_vec3().distance(center) - radius;
            if carve >= old.density {
                continue;
            }
            let hardness = materials.get(old.material).map_or(f32::INFINITY, |m| m.hardness);
            if hardness > max_hardness {
                if carve <= 0.0 {
                    result.too_hard += 1;
                }
                continue;
            }
            let new = Voxel { density: carve, material: if carve <= 0.0 { MaterialId::AIR } else { old.material } };
            if carve <= 0.0 {
                *result.removed.entry(old.material).or_insert(0) += 1;
            }
            self.set(voxel, new);
            result.affected.extend(chunks_containing(voxel));
        }
        result
    }

    /// Baut eine Kugel aus `material`. Bestehendes festes Material bleibt erhalten.
    pub fn place_sphere(
        &mut self,
        mut source: impl FnMut(IVec3) -> Voxel,
        center: Vec3,
        radius: f32,
        material: MaterialId,
    ) -> PlaceResult {
        assert!(!material.is_air(), "Luft kann nicht gebaut werden");
        let mut result = PlaceResult::default();
        for voxel in sphere_voxels(center, radius + 1.0) {
            let old = self.current(voxel, &mut source);
            let fill = radius - voxel.as_vec3().distance(center);
            if fill <= old.density {
                continue;
            }
            // Invariante auch im Speicher: Dichte <= 0 ⇔ Luft (Review-Fund; `get` ist öffentlich).
            let new_material = if fill <= 0.0 {
                MaterialId::AIR
            } else if old.density > 0.0 {
                old.material
            } else {
                material
            };
            let new = Voxel { density: fill, material: new_material };
            if fill > 0.0 && old.density <= 0.0 {
                result.placed += 1;
            }
            self.set(voxel, new);
            result.affected.extend(chunks_containing(voxel));
        }
        result
    }
}

/// Voxel in einer Kugel (Weltkoordinaten), deterministische Reihenfolge.
fn sphere_voxels(center: Vec3, radius: f32) -> impl Iterator<Item = IVec3> {
    let min = (center - Vec3::splat(radius)).floor().as_ivec3();
    let max = (center + Vec3::splat(radius)).ceil().as_ivec3();
    (min.z..=max.z).flat_map(move |z| {
        (min.y..=max.y).flat_map(move |y| {
            (min.x..=max.x)
                .map(move |x| IVec3::new(x, y, z))
                .filter(move |v| v.as_vec3().distance(center) <= radius)
        })
    })
}

#[cfg(test)]
mod tests;
