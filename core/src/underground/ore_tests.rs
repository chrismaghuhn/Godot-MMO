//! Tests für die Erz-Platzierung (#45): Regeln, Schutzregel, Menge, Nähte, Reinheit.

use std::collections::HashMap;

use super::*;
use crate::volume::{CHUNK_SIZE, SAMPLE_MIN};

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

/// Spalten über mehrere Provinzen (≈ 900 m) verteilt.
fn columns(world: &WorldGen) -> Vec<ColumnData> {
    let mut result = Vec::new();
    for chunk_z in (-40..40).step_by(13) {
        for chunk_x in (-40..40).step_by(13) {
            result.push(ColumnData::sample(world, chunk_x, chunk_z));
        }
    }
    result
}

/// Lowest surface of the 3×3 neighbourhood, unabhängig von `ShellHeights` berechnet.
fn lowest_neighbour_surface(world: &WorldGen, x: i32, z: i32) -> f32 {
    let mut lowest = f32::INFINITY;
    for dz in -1..=1 {
        for dx in -1..=1 {
            lowest = lowest.min(world.column_height((x + dx) as f32, (z + dz) as f32));
        }
    }
    lowest
}

/// Jedes Erz-Sample erfüllt seine Regel: Provinz hat das Erz, Wirtsgestein, Höhe, Tiefe,
/// Schutzregel; Seifen nur unter Flussbetten im Tiefenband.
#[test]
fn every_ore_sample_follows_its_rule() {
    let (world, underground) = setup(11);
    let mut per_kind: HashMap<OreKind, usize> = HashMap::new();
    for column in columns(&world) {
        for chunk_y in -5..=2 {
            let public = DensityChunk::from_column(&column, chunk_y);
            let full = underground.materials_chunk(&world, &column, chunk_y);
            full.assert_material_invariant();
            let origin = full.origin();
            for_each_sample(|x, y, z| {
                let material = full.material(x, y, z);
                let Some(kind) = OreKind::from_material(material) else {
                    // Höhlen schaffen Luft; alles andere Feste muss das öffentliche Material sein.
                    if !material.is_air() {
                        assert_eq!(material, public.material(x, y, z), "Nicht-Erz-Material verändert");
                    }
                    return;
                };
                *per_kind.entry(kind).or_default() += 1;
                let rule = kind.rule();
                let (wx, wy, wz) = (origin.x + x, (origin.y + y) as f32, origin.z + z);
                let province = underground.province_at(&world, wx as f32, wz as f32);
                assert!(province.deposit(kind).is_some(), "{kind:?} außerhalb seiner Provinzen");
                assert!(rule.hosts.contains(&public.material(x, y, z)), "{kind:?} im falschen Gestein");
                assert!((rule.y_range.0..=rule.y_range.1).contains(&wy), "{kind:?} außerhalb des Höhenbereichs");
                let shell_depth = lowest_neighbour_surface(&world, wx, wz) - wy;
                assert!(shell_depth >= SURFACE_SHELL, "{kind:?} in der Schutz-Schale ({shell_depth})");
                if kind == OreKind::GoldSand {
                    assert!(column.river(x, z) > 0.25, "Seife ohne Fluss");
                    assert!((SURFACE_SHELL + 0.5..SURFACE_SHELL + 4.5).contains(&shell_depth), "Seife außerhalb des Bandes");
                } else {
                    assert!(column.height(x, z) - wy >= rule.min_depth, "{kind:?} zu flach");
                }
            });
        }
    }
    let kinds = per_kind.len();
    eprintln!("Erz-Samples je Art: {per_kind:?}");
    assert!(kinds >= 5, "zu wenige Erzarten im Testgebiet: {per_kind:?}");
}

/// Erz-Anteil am Fels unterhalb der Schale. Grobe Grenzen: fängt "kein Erz" und "alles Erz".
#[test]
fn ore_share_is_reasonable() {
    let (world, underground) = setup(12);
    let (mut rock, mut ore) = (0usize, 0usize);
    for column in columns(&world) {
        for chunk_y in -4..=1 {
            let chunk = underground.materials_chunk(&world, &column, chunk_y);
            let origin = chunk.origin();
            for_each_sample(|x, y, z| {
                let material = chunk.material(x, y, z);
                if material.is_air() || column.height(x, z) - ((origin.y + y) as f32) < SURFACE_SHELL + 8.0 {
                    return;
                }
                rock += 1;
                if OreKind::from_material(material).is_some() {
                    ore += 1;
                }
            });
        }
    }
    let share = ore as f64 / rock as f64;
    eprintln!("Erz-Anteil am Fels: {:.2} %", share * 100.0);
    assert!((0.002..0.08).contains(&share), "Erz-Anteil {share:.4} außerhalb 0,2-8 %");
}

/// `materials_chunk` und `density_chunk` liefern dasselbe, wo beide etwas liefern.
#[test]
fn materials_chunk_matches_density_chunk() {
    let (world, underground) = setup(13);
    let mut compared = 0;
    for column in columns(&world).into_iter().take(12) {
        for chunk_y in -5..=3 {
            let Some(mesh_chunk) = underground.density_chunk(&world, &column, chunk_y) else {
                continue;
            };
            let full = underground.materials_chunk(&world, &column, chunk_y);
            for_each_sample(|x, y, z| {
                assert_eq!(mesh_chunk.get(x, y, z).to_bits(), full.get(x, y, z).to_bits());
                assert_eq!(mesh_chunk.material(x, y, z), full.material(x, y, z));
            });
            compared += 1;
        }
    }
    assert!(compared > 10);
}

/// Nähte mit Erzen: gemeinsame Samples bitgleich in Dichte und Material, auf allen 3 Achsen,
/// auch für massiven Fels (über `materials_chunk`).
#[test]
fn ores_are_seamless_on_all_axes() {
    let (world, underground) = setup(14);
    let mut ore_on_faces = 0;
    for (chunk_x, chunk_z) in [(0, 0), (-13, 26), (26, -13)] {
        let column = ColumnData::sample(&world, chunk_x, chunk_z);
        let east = ColumnData::sample(&world, chunk_x + 1, chunk_z);
        let south = ColumnData::sample(&world, chunk_x, chunk_z + 1);
        for chunk_y in -4..=1 {
            let a = underground.materials_chunk(&world, &column, chunk_y);
            let pairs = [
                (0, underground.materials_chunk(&world, &east, chunk_y)),
                (1, underground.materials_chunk(&world, &column, chunk_y + 1)),
                (2, underground.materials_chunk(&world, &south, chunk_y)),
            ];
            for (axis, b) in pairs {
                let (i, j) = match axis {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                for layer in [-1, 0] {
                    for v in SAMPLE_MIN..=CHUNK_SIZE {
                        for u in SAMPLE_MIN..=CHUNK_SIZE {
                            let (mut pa, mut pb) = ([0; 3], [0; 3]);
                            pa[axis] = CHUNK_SIZE + layer;
                            pb[axis] = layer;
                            (pa[i], pa[j]) = (u, v);
                            (pb[i], pb[j]) = (u, v);
                            let (ma, mb) = (a.material(pa[0], pa[1], pa[2]), b.material(pb[0], pb[1], pb[2]));
                            assert_eq!(a.get(pa[0], pa[1], pa[2]).to_bits(), b.get(pb[0], pb[1], pb[2]).to_bits());
                            assert_eq!(ma, mb, "Material-Naht Achse {axis}, {pa:?}");
                            if OreKind::from_material(ma).is_some() {
                                ore_on_faces += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(ore_on_faces > 0, "kein Erz an einer Chunk-Grenze: Test prüft Erz-Nähte nicht");
}

/// Naht an der Höhlengrenze: Chunk unter `CAVE_MIN_Y` (ohne Untergrund) und der darüber
/// müssen an der gemeinsamen Ebene gleich sein (Review-Fund zu #45).
#[test]
fn cave_floor_boundary_is_seamless() {
    let (world, underground) = setup(11);
    let below = (CAVE_MIN_Y / CHUNK_SIZE as f32).floor() as i32 - 1;
    let mut compared = 0;
    for column in columns(&world) {
        let lower = underground.materials_chunk(&world, &column, below);
        let upper = underground.materials_chunk(&world, &column, below + 1);
        for z in SAMPLE_MIN..=CHUNK_SIZE {
            for x in SAMPLE_MIN..=CHUNK_SIZE {
                for (lower_y, upper_y) in [(CHUNK_SIZE - 1, -1), (CHUNK_SIZE, 0)] {
                    assert_eq!(lower.material(x, lower_y, z), upper.material(x, upper_y, z), "Naht bei {:?}, {x},{z}", column.chunk_x);
                    assert_eq!(lower.get(x, lower_y, z).to_bits(), upper.get(x, upper_y, z).to_bits());
                    compared += 1;
                }
            }
        }
    }
    assert!(compared > 10_000);
}

#[test]
fn purity_exists_only_where_the_province_has_the_ore() {
    let (world, underground) = setup(15);
    let mut with = 0;
    for (x, z) in [(0.0, 0.0), (1500.0, -800.0), (-2600.0, 3100.0), (4200.0, 4200.0)] {
        let province = underground.province_at(&world, x, z);
        for kind in OreKind::ALL {
            let purity = underground.purity(&world, x, z, kind.material());
            assert_eq!(purity.is_some(), province.deposit(kind).is_some(), "{kind:?}");
            if let Some(purity) = purity {
                with += 1;
                assert!((0.05..=1.0).contains(&purity));
                assert_eq!(underground.purity(&world, x, z, kind.material()), Some(purity), "nicht deterministisch");
            }
        }
        assert_eq!(underground.purity(&world, x, z, crate::MaterialId::GRANITE), None, "Gestein hat keine Reinheit");
    }
    assert!(with >= 4);
}

/// Voxel-Belegung über Chunk-Grenzen hinweg (für Filter-Tests).
struct Chunks<'a> {
    world: &'a WorldGen,
    underground: &'a Underground,
    columns: HashMap<(i32, i32), ColumnData>,
    chunks: HashMap<glam::IVec3, DensityChunk>,
}

impl<'a> Chunks<'a> {
    fn new(world: &'a WorldGen, underground: &'a Underground) -> Self {
        Self { world, underground, columns: HashMap::new(), chunks: HashMap::new() }
    }

    fn column(&mut self, x: i32, z: i32) -> &ColumnData {
        let world = self.world;
        self.columns.entry((x, z)).or_insert_with(|| ColumnData::sample(world, x, z))
    }

    fn chunk(&mut self, position: glam::IVec3) -> &DensityChunk {
        if !self.chunks.contains_key(&position) {
            let (world, underground) = (self.world, self.underground);
            let column = self.column(position.x, position.z);
            let chunk = underground.materials_chunk(world, column, position.y);
            self.chunks.insert(position, chunk);
        }
        &self.chunks[&position]
    }

    fn air(&mut self, voxel: glam::IVec3) -> bool {
        let owner = voxel.div_euclid(glam::IVec3::splat(CHUNK_SIZE));
        let local = voxel - owner * CHUNK_SIZE;
        self.chunk(owner).get(local.x, local.y, local.z) <= 0.0
    }

    /// Für Clients gefilterter Chunk.
    fn filtered(&mut self, position: glam::IVec3) -> DensityChunk {
        let (world, underground) = (self.world, self.underground);
        let column = ColumnData::sample(world, position.x, position.z);
        let mut chunk = underground.materials_chunk(world, &column, position.y);
        hide_unexposed_ores(&mut chunk, &column, |v| self.air(v));
        chunk
    }
}

/// #46: Nach dem Filter hat jedes verbleibende Erz einen Luft-Nachbarn; verborgene Erze zeigen
/// genau das öffentliche Gestein; alles andere bleibt unverändert; freiliegende Erze bleiben.
#[test]
fn client_chunks_show_only_exposed_ores() {
    let (world, underground) = setup(21);
    let mut cache = Chunks::new(&world, &underground);
    let (mut hidden, mut exposed) = (0, 0);
    for column in columns(&world).into_iter().take(16) {
        for chunk_y in -4..=1 {
            let position = glam::IVec3::new(column.chunk_x, chunk_y, column.chunk_z);
            let full = underground.materials_chunk(&world, &column, chunk_y);
            let public = DensityChunk::from_column(&column, chunk_y);
            let filtered = cache.filtered(position);
            let origin = full.origin();
            for_each_sample(|x, y, z| {
                let (before, after) = (full.material(x, y, z), filtered.material(x, y, z));
                assert_eq!(full.get(x, y, z).to_bits(), filtered.get(x, y, z).to_bits(), "Dichte verändert");
                let voxel = origin + glam::IVec3::new(x, y, z);
                let has_air = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                    .iter()
                    .any(|&(dx, dy, dz)| cache.air(voxel + glam::IVec3::new(dx, dy, dz)));
                match (OreKind::from_material(before), OreKind::from_material(after)) {
                    (Some(_), Some(_)) => {
                        assert!(has_air, "verborgenes Erz übertragen bei {voxel}");
                        assert_eq!(before, after);
                        exposed += 1;
                    }
                    (Some(_), None) => {
                        assert!(!has_air, "freiliegendes Erz entfernt bei {voxel}");
                        assert_eq!(after, public.material(x, y, z), "verborgenes Erz zeigt nicht das öffentliche Gestein");
                        hidden += 1;
                    }
                    (None, None) => assert_eq!(before, after, "Nicht-Erz verändert"),
                    (None, Some(_)) => panic!("Filter hat Erz erzeugt"),
                }
            });
        }
    }
    eprintln!("Erz-Voxel: {hidden} verborgen, {exposed} freiliegend");
    assert!(hidden > 1000 && exposed > 0, "Testgebiet zu klein: {hidden} verborgen, {exposed} frei");
}

/// Gefilterte Nachbar-Chunks stimmen an gemeinsamen Samples überein (auf allen 3 Achsen).
#[test]
fn filtered_chunks_agree_on_shared_samples() {
    let (world, underground) = setup(22);
    let mut cache = Chunks::new(&world, &underground);
    let mut ore_on_faces = 0;
    for (chunk_x, chunk_z) in [(0, 0), (-13, 26), (26, -13)] {
        for chunk_y in -3..=0 {
            let a = cache.filtered(glam::IVec3::new(chunk_x, chunk_y, chunk_z));
            for axis in 0..3 {
                let mut offset = glam::IVec3::ZERO;
                offset[axis] = 1;
                let b = cache.filtered(glam::IVec3::new(chunk_x, chunk_y, chunk_z) + offset);
                for layer in [-1, 0] {
                    for v in SAMPLE_MIN..=CHUNK_SIZE {
                        for u in SAMPLE_MIN..=CHUNK_SIZE {
                            let (i, j) = match axis {
                                0 => (1, 2),
                                1 => (0, 2),
                                _ => (0, 1),
                            };
                            let (mut pa, mut pb) = ([0; 3], [0; 3]);
                            pa[axis] = CHUNK_SIZE + layer;
                            pb[axis] = layer;
                            (pa[i], pa[j]) = (u, v);
                            (pb[i], pb[j]) = (u, v);
                            let (ma, mb) = (a.material(pa[0], pa[1], pa[2]), b.material(pb[0], pb[1], pb[2]));
                            assert_eq!(ma, mb, "Material-Naht nach Filter, Achse {axis}");
                            if OreKind::from_material(ma).is_some() {
                                ore_on_faces += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(ore_on_faces > 0, "kein freiliegendes Erz an einer Chunk-Grenze im Test");
}
