//! Tests für Graben und Bauen (#6).

use std::collections::HashMap;

use super::*;
use crate::mesh::surface_nets;
use crate::volume::ColumnData;
use crate::world::WorldGen;

/// Ungeänderte Welt aus dem öffentlichen Generator, mit Chunk-Cache.
struct Source<'a> {
    world: &'a WorldGen,
    columns: HashMap<(i32, i32), ColumnData>,
    chunks: HashMap<IVec3, DensityChunk>,
}

impl<'a> Source<'a> {
    fn new(world: &'a WorldGen) -> Self {
        Self { world, columns: HashMap::new(), chunks: HashMap::new() }
    }

    fn chunk(&mut self, position: IVec3) -> &DensityChunk {
        if !self.chunks.contains_key(&position) {
            let column = self
                .columns
                .entry((position.x, position.z))
                .or_insert_with(|| ColumnData::sample(self.world, position.x, position.z));
            let chunk = DensityChunk::from_column(column, position.y);
            self.chunks.insert(position, chunk);
        }
        &self.chunks[&position]
    }

    fn voxel(&mut self, voxel: IVec3) -> Voxel {
        let owner = owner_chunk(voxel);
        let local = voxel - owner * CHUNK_SIZE;
        let chunk = self.chunk(owner);
        Voxel { density: chunk.get(local.x, local.y, local.z), material: chunk.material(local.x, local.y, local.z) }
    }

    /// Chunk mit angewendeten Änderungen.
    fn edited(&mut self, edits: &WorldEdits, position: IVec3) -> DensityChunk {
        let column = self
            .columns
            .entry((position.x, position.z))
            .or_insert_with(|| ColumnData::sample(self.world, position.x, position.z));
        let mut chunk = DensityChunk::from_column(column, position.y);
        edits.apply(&mut chunk);
        chunk
    }
}

/// Ein Punkt 6 m unter der Oberfläche am Ursprung.
fn underground_point(world: &WorldGen, x: f32, z: f32) -> Vec3 {
    Vec3::new(x, world.column_height(x, z) - 6.0, z)
}

#[test]
fn digging_removes_rock_and_reports_the_yield() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    let center = underground_point(&world, 10.3, 20.7);
    let result = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 3.0, 10.0);
    // Kugel r = 3: ≈ 113 Voxel, fast alle fest.
    let total: u32 = result.removed.values().sum();
    assert!((90..=130).contains(&total), "Ausbeute {total} passt nicht zu r = 3");
    assert_eq!(result.too_hard, 0);
    // Jeder entfernte Voxel ist jetzt Luft, und die Ausbeute zählt genau diese.
    let mut counted: BTreeMap<MaterialId, u32> = BTreeMap::new();
    for voxel in sphere_voxels(center, 3.0) {
        let before = source.voxel(voxel);
        let after = edits.get(voxel).expect("Voxel in der Kugel nicht geändert");
        assert!(after.density <= 0.0 && after.material.is_air());
        if before.density > 0.0 {
            *counted.entry(before.material).or_insert(0) += 1;
        }
    }
    assert_eq!(counted, result.removed);
}

#[test]
fn hardness_limit_keeps_hard_rock() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    let center = underground_point(&world, -40.0, 35.0);
    // Schaufel: nur Boden (Härte ≤ 2,5).
    let result = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 3.0, 2.5);
    for material in result.removed.keys() {
        assert!(world.materials.get(*material).unwrap().hardness <= 2.5, "zu hartes Material abgebaut: {material:?}");
    }
    // 6 m unter der Oberfläche ist fast nur Gestein: das meiste bleibt stehen.
    assert!(result.too_hard > 50, "Härte-Grenze greift nicht: {}", result.too_hard);
    for voxel in sphere_voxels(center, 3.0) {
        let before = source.voxel(voxel);
        if before.density > 0.0 && world.materials.get(before.material).unwrap().hardness > 2.5 {
            assert!(edits.get(voxel).is_none(), "harter Voxel verändert");
        }
    }
}

/// Nach einer Änderung an einer Chunk-Ecke sehen alle beteiligten Chunks an gemeinsamen
/// Samples dieselben Werte (keine Nähte), und `affected` enthält jeden veränderten Chunk.
#[test]
fn edits_at_chunk_corners_stay_seamless() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    // Ecke von 4 Spalten bei x = z = 0, mitten durch die Oberfläche.
    let center = Vec3::new(0.0, world.column_height(0.0, 0.0), 0.0);
    let result = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 4.5, 10.0);
    assert!(result.affected.len() >= 4, "Ecke betrifft mehrere Chunks: {:?}", result.affected);
    let chunks: Vec<IVec3> = result.affected.iter().copied().collect();
    let edited: HashMap<IVec3, DensityChunk> = chunks.iter().map(|&c| (c, source.edited(&edits, c))).collect();
    let mut compared = 0;
    for (&a_pos, a) in &edited {
        for (&b_pos, b) in &edited {
            if a_pos == b_pos {
                continue;
            }
            // Gemeinsame Samples in Weltkoordinaten.
            let (oa, ob) = (a.origin(), b.origin());
            for voxel in sphere_voxels(center, 6.0) {
                let (la, lb) = (voxel - oa, voxel - ob);
                let in_range = |l: IVec3| (SAMPLE_MIN..=CHUNK_SIZE).contains(&l.x) && (SAMPLE_MIN..=CHUNK_SIZE).contains(&l.y) && (SAMPLE_MIN..=CHUNK_SIZE).contains(&l.z);
                if in_range(la) && in_range(lb) {
                    assert_eq!(a.get(la.x, la.y, la.z).to_bits(), b.get(lb.x, lb.y, lb.z).to_bits());
                    assert_eq!(a.material(la.x, la.y, la.z), b.material(lb.x, lb.y, lb.z));
                    compared += 1;
                }
            }
        }
    }
    assert!(compared > 100);
    // Jeder Chunk, dessen Samples sich geändert haben, steht in `affected`.
    for dz in -2..=1 {
        for dy in -3..=4 {
            for dx in -2..=1 {
                let position = IVec3::new(dx, dy, dz);
                let changed = {
                    let before = source.chunk(position);
                    let values: Vec<(u32, MaterialId)> = (SAMPLE_MIN..=CHUNK_SIZE)
                        .flat_map(|z| (SAMPLE_MIN..=CHUNK_SIZE).flat_map(move |y| (SAMPLE_MIN..=CHUNK_SIZE).map(move |x| (x, y, z))))
                        .map(|(x, y, z)| (before.get(x, y, z).to_bits(), before.material(x, y, z)))
                        .collect();
                    let after = source.edited(&edits, position);
                    let mut i = 0;
                    let mut differs = false;
                    for z in SAMPLE_MIN..=CHUNK_SIZE {
                        for y in SAMPLE_MIN..=CHUNK_SIZE {
                            for x in SAMPLE_MIN..=CHUNK_SIZE {
                                differs |= values[i] != (after.get(x, y, z).to_bits(), after.material(x, y, z));
                                i += 1;
                            }
                        }
                    }
                    differs
                };
                if changed {
                    assert!(result.affected.contains(&position), "{position} geändert, aber nicht in affected");
                    assert!(edits.touches(position));
                }
            }
        }
    }
}

#[test]
fn digging_inside_solid_rock_creates_a_mesh() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    // Tief im Fels, mitten in einem Chunk: vorher kein Mesh.
    let column = ColumnData::sample(&world, 3, 3);
    let deep = (column.surface_chunks().next().unwrap() - 3) * CHUNK_SIZE + 16;
    let center = Vec3::new(3.0 * 32.0 + 16.0, deep as f32, 3.0 * 32.0 + 16.0);
    let position = owner_chunk(center.as_ivec3());
    assert!(surface_nets(source.chunk(position)).is_empty(), "Testchunk sollte massiv sein");
    edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 5.0, 10.0);
    let chunk = source.edited(&edits, position);
    chunk.assert_material_invariant();
    let mesh = surface_nets(&chunk);
    assert!(!mesh.is_empty(), "Hohlraum ohne Mesh");
    // Innenwand einer Höhle: Normalen zeigen zur Kugelmitte (zur Luft).
    let origin = chunk.origin().as_vec3();
    let inward = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .filter(|(p, n)| Vec3::from(**n).dot(center - (origin + Vec3::from(**p))) > 0.0)
        .count();
    assert!(inward * 10 >= mesh.positions.len() * 9, "Normalen zeigen nicht in den Hohlraum");
}

#[test]
fn placing_fills_air_only_and_digging_it_again_returns_the_material() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    // In der Luft 10 m über dem Boden.
    let center = Vec3::new(50.0, world.column_height(50.0, 50.0) + 10.0, 50.0);
    let placed = edits.place_sphere(|v| source.voxel(v), center, 2.5, MaterialId::GRANITE);
    assert!(placed.placed > 40, "zu wenig gebaut: {}", placed.placed);
    for voxel in sphere_voxels(center, 2.5) {
        let value = edits.get(voxel).unwrap();
        assert!(value.density > 0.0 && value.material == MaterialId::GRANITE);
    }
    // Wieder abbauen: genau dieses Material kommt zurück, mit ausreichender Härte.
    let soft = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 4.0, 5.0);
    assert!(soft.removed.is_empty() && soft.too_hard > 0, "Granit mit Härte 5 abgebaut");
    let hard = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 4.0, 7.0);
    assert_eq!(hard.removed.get(&MaterialId::GRANITE).copied(), Some(placed.placed));
    assert_eq!(hard.removed.len(), 1);
}

#[test]
fn chunks_containing_matches_the_sample_ranges() {
    for voxel in [IVec3::new(0, 0, 0), IVec3::new(31, 5, -1), IVec3::new(-32, 64, 17), IVec3::new(15, -33, 31)] {
        let listed: std::collections::HashSet<IVec3> = chunks_containing(voxel).collect();
        let mut expected = std::collections::HashSet::new();
        for dz in -2..=2 {
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let chunk = owner_chunk(voxel) + IVec3::new(dx, dy, dz);
                    let local = voxel - chunk * CHUNK_SIZE;
                    if (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.x) && (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.y) && (SAMPLE_MIN..=CHUNK_SIZE).contains(&local.z) {
                        expected.insert(chunk);
                    }
                }
            }
        }
        assert_eq!(listed, expected, "Voxel {voxel}");
    }
}

/// Gespeicherte Voxel halten die Invariante Dichte <= 0 ⇔ Luft (Review-Fund: `place_sphere`
/// speicherte im Randband Luft mit Material).
#[test]
fn stored_voxels_keep_the_air_invariant() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    let air = Vec::from([Vec3::new(5.0, world.column_height(5.0, 5.0) + 8.0, 5.0)]);
    for center in air {
        edits.place_sphere(|v| source.voxel(v), center, 1.5, MaterialId::DIRT);
        edits.dig_sphere(&world.materials, |v| source.voxel(v), center + Vec3::X, 1.0, 10.0);
    }
    let surface = Vec3::new(-20.0, world.column_height(-20.0, 30.0), 30.0);
    edits.dig_sphere(&world.materials, |v| source.voxel(v), surface, 2.5, 10.0);
    edits.place_sphere(|v| source.voxel(v), surface, 1.8, MaterialId::SAND);
    let mut checked = 0;
    for owner in edits.by_owner.values() {
        for value in owner.values() {
            assert_eq!(value.density <= 0.0, value.material.is_air(), "{value:?}");
            checked += 1;
        }
    }
    assert!(checked > 100);
}

/// Wiederholtes Graben und Bauen bei negativen Koordinaten, quer über Chunk-Grenzen.
#[test]
fn repeated_edits_at_negative_coordinates() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    let start = Vec3::new(-32.0, world.column_height(-32.0, -64.0) - 1.0, -64.0);
    let mut total = 0;
    for step in 0..6 {
        let center = start + Vec3::new(-1.5 * step as f32, -(step as f32), 0.7 * step as f32);
        let result = edits.dig_sphere(&world.materials, |v| source.voxel(v), center, 2.0, 10.0);
        total += result.removed.values().sum::<u32>();
        // Nach jedem Schritt: Kugelinneres ist Luft, betroffene Chunks halten die Invariante.
        for voxel in sphere_voxels(center, 2.0) {
            let value = edits.get(voxel).unwrap_or_else(|| source.voxel(voxel));
            assert!(value.density <= 0.0, "Voxel {voxel} nicht ausgehöhlt");
        }
        for chunk in result.affected {
            source.edited(&edits, chunk).assert_material_invariant();
        }
    }
    // Überlappende Kugeln: Bereits Gegrabenes wird nicht doppelt gezählt.
    assert!(total > 100 && total < 6 * 34, "Ausbeute {total} passt nicht zu überlappenden Kugeln");
}

/// Eine Änderung, die dem Nachbar-Chunk gehört, aber in der Randlage (x = 32) dieses Chunks
/// liegt, kommt auch hier an.
#[test]
fn apply_reaches_edits_owned_by_a_neighbour() {
    let world = WorldGen::new(42);
    let mut source = Source::new(&world);
    let mut edits = WorldEdits::default();
    // Voxel x = 32 gehört Chunk 1, ist aber Sample x = 32 von Chunk 0 (dessen Randlage).
    let voxel = IVec3::new(32, (world.column_height(32.0, 5.0) - 3.0) as i32, 5);
    edits.dig_sphere(&world.materials, |v| source.voxel(v), voxel.as_vec3(), 0.4, 10.0);
    assert_eq!(owner_chunk(voxel).x, 1);
    let left = owner_chunk(voxel) - IVec3::X;
    assert!(edits.touches(left));
    let chunk = source.edited(&edits, left);
    let local = voxel - chunk.origin();
    assert_eq!(local.x, CHUNK_SIZE);
    assert!(chunk.get(local.x, local.y, local.z) <= 0.0, "Änderung des Nachbarn fehlt");
}

/// #7: Graben unter einem Baum oder Zuschütten nimmt ihm den Stand; daneben bleibt er.
#[test]
fn digging_under_or_burying_a_tree_removes_it() {
    use crate::vegetation::{plan_trees, settle_tree};
    let world = WorldGen::new(42);
    let mut found = None;
    'search: for chunk_z in -4..4 {
        for chunk_x in -4..4 {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            let trees = plan_trees(world.cfg.seed, world.cfg.sea_level, chunk_x, chunk_z, column.heights(), &column.biomes, &column.temperatures);
            if let Some(tree) = trees.into_iter().find(|t| (4.0..28.0).contains(&t.position.x) && (4.0..28.0).contains(&t.position.z)) {
                found = Some(Vec3::new((chunk_x * CHUNK_SIZE) as f32 + tree.position.x, tree.position.y, (chunk_z * CHUNK_SIZE) as f32 + tree.position.z));
                break 'search;
            }
        }
    }
    let root = found.expect("kein Baum gefunden");
    let density = |source: &mut Source, edits: &WorldEdits, v: IVec3| edits.get(v).map_or_else(|| source.voxel(v).density, |e| e.density);
    let plan = |x: i32, z: i32| world.column_height(x as f32, z as f32);

    let mut source = Source::new(&world);
    let untouched = WorldEdits::default();
    assert!(settle_tree(root, plan, |v| density(&mut source, &untouched, v)).is_some(), "Baum steht ohne Änderung nicht");

    let mut dug = WorldEdits::default();
    dug.dig_sphere(&world.materials, |v| source.voxel(v), root - Vec3::Y * 1.5, 2.5, 10.0);
    assert_eq!(settle_tree(root, plan, |v| density(&mut source, &dug, v)), None, "Baum über gegrabenem Loch");

    let mut buried = WorldEdits::default();
    buried.place_sphere(|v| source.voxel(v), root + Vec3::Y * 1.0, 2.5, MaterialId::DIRT);
    assert_eq!(settle_tree(root, plan, |v| density(&mut source, &buried, v)), None, "zugeschütteter Baum");

    let mut beside = WorldEdits::default();
    beside.dig_sphere(&world.materials, |v| source.voxel(v), root + Vec3::new(8.0, -1.0, 0.0), 2.0, 10.0);
    assert!(settle_tree(root, plan, |v| density(&mut source, &beside, v)).is_some(), "Grube daneben nimmt den Baum weg");

    let mut felled = WorldEdits::default();
    let id = crate::vegetation::TreeId { chunk_x: 0, chunk_z: 0, cell: 5 };
    assert!(felled.fell_tree(id));
    assert!(!felled.fell_tree(id), "zweimal gefällt");
    assert!(felled.is_felled(id) && !felled.is_empty());
}
