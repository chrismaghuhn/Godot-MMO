//! Tests für den Untergrund: Schutz-Schale, Klassifizierung, Nähte, Höhlen-Eigenschaften.

use super::*;
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

/// Dichtefeld so, wie es Nachbarn sehen: mit Höhlen, oder das öffentliche, wenn kein
/// Chunk nötig ist.
fn effective(world: &WorldGen, underground: &Underground, column: &ColumnData, chunk_y: i32) -> (DensityChunk, bool) {
    match underground.density_chunk(world, column, chunk_y) {
        Some(chunk) => (chunk, true),
        None => (DensityChunk::from_column(column, chunk_y), false),
    }
}

/// D-005: Luft wird nie verändert, und jede Zelle, die Luft berührt, bleibt bitgenau
/// gleich (außer an Eingängen). Ein Surface-Nets-Vertex hängt nur von den 8 Ecken
/// seiner Zelle ab, also ist die sichtbare Oberfläche dann unabhängig vom Seed.
#[test]
fn surface_cells_unchanged_without_entrance() {
    // Zusätzlich ein Block mit Steilhängen (dort fand das Review den Fehler der
    // rein senkrechten Tiefe, z. B. Chunk 0,1,-40).
    let mut columns: Vec<(i32, i32)> = COLUMNS.to_vec();
    for chunk_z in -42..=-38 {
        for chunk_x in -3..=3 {
            columns.push((chunk_x, chunk_z));
        }
    }
    let (mut cells, mut steep_cells) = (0usize, 0usize);
    for secret in [1u64, 0xFFFF_0000_1234] {
        let (world, underground) = setup(secret);
        let sea = world.cfg.sea_level;
        for &(chunk_x, chunk_z) in &columns {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            for chunk_y in column.surface_chunks() {
                let public = DensityChunk::from_column(&column, chunk_y);
                let full = underground.density_chunk(&world, &column, chunk_y).expect("Oberflächen-Chunk fehlt");
                full.assert_material_invariant();
                let origin = public.origin();
                let entrance = |x: i32, z: i32| {
                    underground.caves.entrance((origin.x + x) as f32, (origin.z + z) as f32, column.height(x, z), sea) > 0.0
                };
                for_each_sample(|x, y, z| {
                    if public.get(x, y, z) <= 0.0 {
                        assert_eq!(public.get(x, y, z).to_bits(), full.get(x, y, z).to_bits(), "Luft verändert");
                    }
                });
                for z in SAMPLE_MIN..CHUNK_SIZE {
                    for y in SAMPLE_MIN..CHUNK_SIZE {
                        for x in SAMPLE_MIN..CHUNK_SIZE {
                            let corners = [
                                (x, y, z), (x + 1, y, z), (x, y + 1, z), (x + 1, y + 1, z),
                                (x, y, z + 1), (x + 1, y, z + 1), (x, y + 1, z + 1), (x + 1, y + 1, z + 1),
                            ];
                            let touches_air = corners.iter().any(|&(cx, cy, cz)| public.get(cx, cy, cz) <= 0.0);
                            if !touches_air || entrance(x, z) || entrance(x + 1, z) || entrance(x, z + 1) || entrance(x + 1, z + 1) {
                                continue;
                            }
                            cells += 1;
                            // Steiler Fall: eine Ecke liegt senkrecht tief, die Zelle berührt trotzdem Luft.
                            if corners.iter().any(|&(cx, cy, cz)| column.height(cx, cz) - (origin.y + cy) as f32 > SURFACE_SHELL + 4.0) {
                                steep_cells += 1;
                            }
                            for (cx, cy, cz) in corners {
                                assert_eq!(
                                    public.get(cx, cy, cz).to_bits(),
                                    full.get(cx, cy, cz).to_bits(),
                                    "Oberflächen-Zelle verändert: Chunk {chunk_x},{chunk_y},{chunk_z}, Zelle {x},{y},{z}"
                                );
                                assert_eq!(
                                    public.material(cx, cy, cz),
                                    full.material(cx, cy, cz),
                                    "Material einer Oberflächen-Zelle verändert: Chunk {chunk_x},{chunk_y},{chunk_z}, Zelle {x},{y},{z}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(cells > 50_000, "zu wenige Oberflächen-Zellen geprüft: {cells}");
    assert!(steep_cells > 100, "Steilhänge nicht abgedeckt: {steep_cells}");
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
                let full = underground.density_chunk(&world, &column, chunk_y);
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
                let chunk = underground.density_chunk(&world, &column, chunk_y);
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

/// `classify == Solid` muss garantieren, dass es keinen Hohlraum gibt. Geprüft gegen ein
/// Dichtefeld, das unabhängig von der Klassifizierung immer bearbeitet wird.
#[test]
fn classification_is_conservative() {
    let (world, underground) = setup(3);
    let (mut solid, mut caves) = (0, 0);
    for (chunk_x, chunk_z) in COLUMNS {
        let column = ColumnData::sample(&world, chunk_x, chunk_z);
        let shell = ShellHeights::new(&world, &column);
        for chunk_y in -7..=4 {
            let mut forced = DensityChunk::from_column(&column, chunk_y);
            let grid = CaveGrid::build(&underground.caves, chunk_x, chunk_y, chunk_z);
            caves::carve(&underground.caves, &grid, &column, &shell, &mut forced, world.cfg.sea_level);
            forced.assert_material_invariant();
            let mut has_air = false;
            for_each_sample(|x, y, z| has_air |= forced.get(x, y, z) <= 0.0);

            match underground.classify(&column, chunk_y) {
                ChunkKind::Solid => {
                    solid += 1;
                    assert!(!has_air, "Solid, aber Hohlraum bei {chunk_x},{chunk_y},{chunk_z}");
                    assert!(underground.density_chunk(&world, &column, chunk_y).is_none());
                }
                ChunkKind::Air => assert!(underground.density_chunk(&world, &column, chunk_y).is_none()),
                ChunkKind::Surface => {}
            }
            if column.classify(chunk_y) == ChunkKind::Solid && has_air {
                caves += 1;
                assert!(
                    underground.density_chunk(&world, &column, chunk_y).is_some(),
                    "Höhle ohne Chunk bei {chunk_x},{chunk_y},{chunk_z}"
                );
            }
        }
    }
    assert!(solid > 0 && caves > 0, "Testabdeckung zu klein: solid={solid}, caves={caves}");
}

/// Gemeinsame Samples benachbarter Chunks müssen bitgleich sein (wenn beide einen Chunk
/// haben) bzw. dasselbe Vorzeichen haben (wenn einer keinen braucht), auf allen 3 Achsen.
#[test]
fn neighbouring_chunks_agree_on_shared_samples() {
    let (world, underground) = setup(5);
    let mut compared = 0usize;
    let mut columns = std::collections::HashMap::new();
    for chunk_z in -2..=2 {
        for chunk_x in -3..=3 {
            columns.insert((chunk_x, chunk_z), ColumnData::sample(&world, chunk_x, chunk_z));
        }
    }
    for chunk_z in -2..2 {
        for chunk_x in -3..3 {
            for chunk_y in -4..=1 {
                let (a, a_some) = effective(&world, &underground, &columns[&(chunk_x, chunk_z)], chunk_y);
                let neighbours = [
                    (0, effective(&world, &underground, &columns[&(chunk_x + 1, chunk_z)], chunk_y)),
                    (1, effective(&world, &underground, &columns[&(chunk_x, chunk_z)], chunk_y + 1)),
                    (2, effective(&world, &underground, &columns[&(chunk_x, chunk_z + 1)], chunk_y)),
                ];
                for (axis, (b, b_some)) in neighbours {
                    // A lokal 31/32 entspricht B lokal -1/0 auf der Achse.
                    for layer in [-1, 0] {
                        for v in SAMPLE_MIN..=CHUNK_SIZE {
                            for u in SAMPLE_MIN..=CHUNK_SIZE {
                                // u/v laufen über die beiden anderen Achsen.
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
                                let va = a.get(pa[0], pa[1], pa[2]);
                                let vb = b.get(pb[0], pb[1], pb[2]);
                                if a_some && b_some {
                                    assert_eq!(va.to_bits(), vb.to_bits(), "Achse {axis}, Chunk {chunk_x},{chunk_y},{chunk_z}, {pa:?}");
                                    assert_eq!(
                                        a.material(pa[0], pa[1], pa[2]),
                                        b.material(pb[0], pb[1], pb[2]),
                                        "Material Achse {axis}, Chunk {chunk_x},{chunk_y},{chunk_z}, {pa:?}"
                                    );
                                } else {
                                    assert_eq!(va > 0.0, vb > 0.0, "Vorzeichen Achse {axis}, Chunk {chunk_x},{chunk_y},{chunk_z}, {pa:?}");
                                }
                                compared += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    // 24 Spaltenpaare × 6 Höhen × 3 Achsen × 2 Lagen × 34² = 998.784
    assert!(compared > 900_000, "zu wenige Samples verglichen: {compared}");
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
    assert!(underground.density_chunk(&world, &column, 20).is_none(), "hoch in der Luft");
    assert!(underground.density_chunk(&world, &column, -20).is_none(), "unterhalb der Höhlen");
}

/// D-013 als Eigenschaft, auch an Höhleneingängen: Jede feste Ecke einer Zelle, die öffentliche
/// Luft berührt, behält ihr öffentliches Material. Die Dichte darf sich an Eingängen ändern
/// (Höhlen öffnen sich), aber ein Erz darf nie an der sichtbaren Oberfläche erscheinen.
#[test]
fn surface_materials_unchanged_even_at_entrances() {
    let world = WorldGen::new(42);
    let underground = Underground::new(SecretSeed::insecure_dev_seed(42), &world.cfg);
    let sea = world.cfg.sea_level;
    // Große Eingänge aus einer Messung mit diesem Seed (siehe #4/#45).
    let spots = [(-1435.9, -1254.7), (-648.2, 979.9), (581.3, 1343.3), (-1585.2, 686.4), (33.9, -1042.3)];
    let (mut cells, mut entrance_cells, mut opened_cells) = (0usize, 0usize, 0usize);
    for (x, z) in spots {
        let (chunk_x, chunk_z) = ((x / CHUNK_SIZE as f32).floor() as i32, (z / CHUNK_SIZE as f32).floor() as i32);
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)] {
            let column = ColumnData::sample(&world, chunk_x + dx, chunk_z + dz);
            for chunk_y in column.surface_chunks() {
                let public = DensityChunk::from_column(&column, chunk_y);
                let full = underground.materials_chunk(&world, &column, chunk_y);
                let origin = public.origin();
                for cz in SAMPLE_MIN..CHUNK_SIZE {
                    for cy in SAMPLE_MIN..CHUNK_SIZE {
                        for cx in SAMPLE_MIN..CHUNK_SIZE {
                            let corners = [
                                (cx, cy, cz), (cx + 1, cy, cz), (cx, cy + 1, cz), (cx + 1, cy + 1, cz),
                                (cx, cy, cz + 1), (cx + 1, cy, cz + 1), (cx, cy + 1, cz + 1), (cx + 1, cy + 1, cz + 1),
                            ];
                            if !corners.iter().any(|&(a, b, c)| public.get(a, b, c) <= 0.0) {
                                continue;
                            }
                            cells += 1;
                            let at_entrance = [(cx, cz), (cx + 1, cz), (cx, cz + 1), (cx + 1, cz + 1)].iter().any(|&(a, c)| {
                                underground.caves.entrance((origin.x + a) as f32, (origin.z + c) as f32, column.height(a, c), sea) > 0.0
                            });
                            if at_entrance {
                                entrance_cells += 1;
                            }
                            // Echte Öffnung: Höhle hat hier öffentlichen Fels zu Luft gemacht.
                            if corners.iter().any(|&(a, b, c)| full.get(a, b, c) <= 0.0 && public.get(a, b, c) > 0.0) {
                                opened_cells += 1;
                            }
                            for (a, b, c) in corners {
                                if full.get(a, b, c) > 0.0 {
                                    assert_eq!(
                                        full.material(a, b, c),
                                        public.material(a, b, c),
                                        "Material an der Oberfläche verändert (Eingang: {at_entrance})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(cells > 10_000, "zu wenige Oberflächen-Zellen: {cells}");
    assert!(entrance_cells > 100, "Eingänge nicht abgedeckt: {entrance_cells}");
    assert!(opened_cells > 50, "keine echten Höhlenöffnungen im Testgebiet: {opened_cells}");
}
