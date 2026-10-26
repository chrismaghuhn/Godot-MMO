//! Öffentliche Geologie (#44, D-013): Bodenschichten und Gestein, aus dem öffentlichen Seed.
//!
//! Aufbau von oben nach unten:
//! 1. **Boden** je Biom (Erde, Sand, Schnee, Kies, Ton). An Steilhängen fehlt er (nackter Fels).
//! 2. **Basaltdecke** in Vulkangebieten, dicker je vulkanischer.
//! 3. **Sedimente**: Sandstein, Kalkstein, Schiefer im Wechsel; gewellt, in Bergen stark gefaltet.
//! 4. **Grundgebirge** aus Granit in der Tiefe, in Bergen hochgedrückt; darüber ein dünnes
//!    Marmorband (Kontaktzone).
//!
//! Alles, was pro Spalte gleich ist (inkl. Noise), steht in `ColumnGeology` und wird einmal pro
//! Spalte berechnet. `material_at` braucht pro Sample nur noch einfache Arithmetik.

use crate::biome::FamilyKind;
use crate::material::MaterialId;
use crate::noise::Layer;
use crate::world::{BiomeRef, ColumnSample, smoothstep};

/// Sedimentfolge von unten nach oben: (Material, Dicke in m). Bewusst unregelmäßig, damit
/// keine gleichförmigen Streifen entstehen; wiederholt sich nach `STRATA_CYCLE` Metern.
const STRATA: [(MaterialId, f32); 9] = [
    (MaterialId::SANDSTONE, 9.0),
    (MaterialId::SLATE, 5.0),
    (MaterialId::LIMESTONE, 16.0),
    (MaterialId::SANDSTONE, 6.0),
    (MaterialId::SLATE, 11.0),
    (MaterialId::LIMESTONE, 8.0),
    (MaterialId::SANDSTONE, 14.0),
    (MaterialId::SLATE, 4.0),
    (MaterialId::LIMESTONE, 12.0),
];
const STRATA_CYCLE: f32 = 85.0;
/// Steigung (Höhenunterschied pro Meter), ab der der Boden abrutscht und Fels freiliegt.
const SOIL_SLOPE_START: f32 = 0.6;
const SOIL_SLOPE_END: f32 = 1.2;

/// Geologie einer Spalte (x/z). Pro Sample nur noch Tiefe und Höhe nötig.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnGeology {
    /// Bis zu zwei Bodenschichten von oben: (Material, Dicke in m). Dicke 0 = keine.
    pub soil: [(MaterialId, f32); 2],
    /// Dicke der Basaltdecke unter dem Boden (0 außerhalb von Vulkangebieten).
    pub basalt: f32,
    /// Verschiebung der Sedimentschichten (Wellung, Faltung).
    pub strata_shift: f32,
    /// Regionale Streckung der Schichtdicken (≈ 0,8-1,2): Schichten werden seitlich dünner/dicker.
    pub strata_scale: f32,
    /// Oberkante des Granit-Grundgebirges (Welt-y).
    pub basement: f32,
    /// Dicke des Marmorbands über dem Grundgebirge.
    pub marble: f32,
}

impl ColumnGeology {
    /// Für Tests und Platzhalter: nur Granit.
    pub const GRANITE_ONLY: ColumnGeology = ColumnGeology {
        soil: [(MaterialId::DIRT, 0.0), (MaterialId::DIRT, 0.0)],
        basalt: 0.0,
        strata_shift: 0.0,
        strata_scale: 1.0,
        basement: f32::INFINITY,
        marble: 0.0,
    };
}

/// Material eines festen Samples aus der Spalten-Geologie.
/// `depth` = Tiefe unter der Oberfläche der Spalte (> 0 im Festen), `world_y` = Höhe.
pub fn material_at(geology: &ColumnGeology, depth: f32, world_y: f32) -> MaterialId {
    let mut top = 0.0;
    for (material, thickness) in geology.soil {
        top += thickness;
        if depth < top {
            return material;
        }
    }
    if depth < top + geology.basalt {
        return MaterialId::BASALT;
    }
    if world_y < geology.basement {
        return MaterialId::GRANITE;
    }
    if world_y < geology.basement + geology.marble {
        return MaterialId::MARBLE;
    }
    let phase = ((world_y + geology.strata_shift) * geology.strata_scale).rem_euclid(STRATA_CYCLE);
    let mut top = 0.0;
    for (material, thickness) in STRATA {
        top += thickness;
        if phase < top {
            return material;
        }
    }
    STRATA[STRATA.len() - 1].0
}

/// Noise für die Geologie, aus dem öffentlichen Seed.
pub struct Geology {
    fold: Layer,
    phase: Layer,
    thickness: Layer,
    basement: Layer,
    soil: Layer,
    sea_level: f32,
}

impl Geology {
    pub fn new(seed: u32, sea_level: f32) -> Self {
        let seed = seed as i32;
        let layer = |offset: i32, frequency: f32, octaves: i32| Layer::new(seed.wrapping_add(offset), frequency, octaves);
        Self {
            fold: layer(211, 0.0045, 2),
            phase: layer(223, 0.0012, 2),
            thickness: layer(233, 0.0025, 2),
            basement: layer(227, 0.0020, 2),
            soil: layer(229, 0.0300, 2),
            sea_level,
        }
    }

    /// Geologie einer Spalte. `slope` = größter Höhenunterschied pro Meter zu den Nachbarn.
    pub fn column(&self, x: f32, z: f32, sample: &ColumnSample, biome: BiomeRef, slope: f32) -> ColumnGeology {
        let mountain = sample.mountain;
        // Wellung überall, kräftige Faltung in Bergen; Phase verschiebt die Folge regional.
        let strata_shift =
            self.fold.get(x, z) * (6.0 + 50.0 * mountain) + self.phase.get(x, z) * STRATA_CYCLE;
        let strata_scale = 1.0 + 0.2 * self.thickness.get(x, z);
        // Grundgebirge tief, in Bergen hochgedrückt (Granitkerne).
        let basement = -45.0 + 25.0 * self.basement.get(x, z) + 150.0 * smoothstep(0.35, 0.95, mountain);
        let marble = 6.0 * smoothstep(0.45, 0.8, mountain);
        let basalt = 50.0 * smoothstep(0.25, 0.75, sample.volcanic);

        let keep_soil = 1.0 - smoothstep(SOIL_SLOPE_START, SOIL_SLOPE_END, slope);
        let variation = 1.0 + 0.35 * self.soil.get(x, z);
        let soil = self
            .soil_layers(biome.kind, sample.height)
            .map(|(material, thickness)| (material, thickness * keep_soil * variation));

        ColumnGeology {
            soil,
            basalt,
            strata_shift,
            strata_scale,
            basement,
            marble,
        }
    }

    /// Bodenschichten je Biom-Familie (vor Hangneigung und Variation).
    fn soil_layers(&self, kind: FamilyKind, height: f32) -> [(MaterialId, f32); 2] {
        use MaterialId as M;
        let none = (M::DIRT, 0.0);
        match kind {
            FamilyKind::Desert => [(M::SAND, 6.0), none],
            FamilyKind::Coast => [(M::SAND, 3.5), (M::GRAVEL, 1.0)],
            FamilyKind::Snow => [(M::SNOW, 1.2), (M::GRAVEL, 1.0)],
            FamilyKind::Mountains if height > 140.0 => [(M::SNOW, 1.0), (M::GRAVEL, 0.8)],
            FamilyKind::Mountains => [(M::GRAVEL, 1.2), none],
            FamilyKind::Volcanic => [(M::GRAVEL, 0.6), none],
            FamilyKind::Crystal => [(M::GRAVEL, 1.0), none],
            FamilyKind::Swamp => [(M::CLAY, 2.5), (M::DIRT, 1.0)],
            FamilyKind::Freshwater => [(M::GRAVEL, 2.0), (M::SAND, 1.0)],
            FamilyKind::Underwater => {
                let depth = (self.sea_level - height).max(0.0);
                if depth < 12.0 {
                    [(M::SAND, 3.0), none]
                } else if depth < 40.0 {
                    [(M::GRAVEL, 2.0), (M::SAND, 1.0)]
                } else {
                    [(M::CLAY, 3.0), none]
                }
            }
            FamilyKind::ColdForest => [(M::DIRT, 2.0), (M::GRAVEL, 0.8)],
            FamilyKind::TemperateForest | FamilyKind::TropicalForest | FamilyKind::Grassland | FamilyKind::FungiFlora | FamilyKind::Caves => {
                [(M::DIRT, 2.5), none]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geology(soil: [(MaterialId, f32); 2], basalt: f32, basement: f32, marble: f32) -> ColumnGeology {
        ColumnGeology { soil, basalt, strata_shift: 0.0, strata_scale: 1.0, basement, marble }
    }

    #[test]
    fn layers_follow_the_documented_order() {
        let g = geology([(MaterialId::SAND, 2.0), (MaterialId::GRAVEL, 1.0)], 5.0, -50.0, 4.0);
        assert_eq!(material_at(&g, 0.5, 30.0), MaterialId::SAND);
        assert_eq!(material_at(&g, 2.5, 30.0), MaterialId::GRAVEL);
        assert_eq!(material_at(&g, 4.0, 30.0), MaterialId::BASALT);
        assert_eq!(material_at(&g, 9.0, -60.0), MaterialId::GRANITE);
        assert_eq!(material_at(&g, 9.0, -48.0), MaterialId::MARBLE);
        // Sedimentfolge über dem Grundgebirge (Grenzen 9, 14, 30, 36, 47, 55, 69, 73, 85)
        assert_eq!(material_at(&g, 9.0, 1.0), MaterialId::SANDSTONE);
        assert_eq!(material_at(&g, 9.0, 10.0), MaterialId::SLATE);
        assert_eq!(material_at(&g, 9.0, 20.0), MaterialId::LIMESTONE);
        assert_eq!(material_at(&g, 9.0, 84.0), MaterialId::LIMESTONE);
        assert_eq!(material_at(&g, 9.0, 86.0), MaterialId::SANDSTONE, "Folge wiederholt sich");
        let deep_basement = geology([(MaterialId::DIRT, 0.0), (MaterialId::DIRT, 0.0)], 0.0, -500.0, 0.0);
        assert_eq!(material_at(&deep_basement, 9.0, -84.0), MaterialId::SANDSTONE, "auch negativ (rem_euclid)");
    }

    #[test]
    fn strata_cycle_matches_the_table() {
        let total: f32 = STRATA.iter().map(|(_, thickness)| thickness).sum();
        assert_eq!(total, STRATA_CYCLE);
    }

    use crate::material::MaterialCategory;
    use crate::volume::{CHUNK_SIZE, ColumnData, DensityChunk, SAMPLE_MIN};
    use crate::world::WorldGen;

    /// Spalten über einen größeren Bereich, damit alle Biome und Gesteine vorkommen.
    fn sample_columns(world: &WorldGen) -> Vec<ColumnData> {
        let mut columns = Vec::new();
        for chunk_z in (-60..60).step_by(9) {
            for chunk_x in (-60..60).step_by(9) {
                columns.push(ColumnData::sample(world, chunk_x, chunk_z));
            }
        }
        columns
    }

    fn top_material(column: &ColumnData, x: i32, z: i32) -> MaterialId {
        let height = column.height(x, z);
        material_at(column.geology(x, z), 0.25, height - 0.25)
    }

    #[test]
    fn surface_materials_follow_the_biomes() {
        let world = WorldGen::new(42);
        // (Familie, erwartetes Material, Treffer, Gesamt) nur für flache Stellen
        let mut checks = [
            (FamilyKind::Desert, MaterialId::SAND, 0, 0),
            (FamilyKind::Coast, MaterialId::SAND, 0, 0),
            (FamilyKind::Snow, MaterialId::SNOW, 0, 0),
            (FamilyKind::Swamp, MaterialId::CLAY, 0, 0),
            (FamilyKind::TemperateForest, MaterialId::DIRT, 0, 0),
            (FamilyKind::Grassland, MaterialId::DIRT, 0, 0),
        ];
        for column in sample_columns(&world) {
            for z in (0..CHUNK_SIZE).step_by(4) {
                for x in (0..CHUNK_SIZE).step_by(4) {
                    let slope = (column.height(x + 1, z) - column.height(x, z))
                        .abs()
                        .max((column.height(x, z + 1) - column.height(x, z)).abs());
                    if slope > 0.3 {
                        continue;
                    }
                    let kind = column.biome(x, z).kind;
                    for check in checks.iter_mut().filter(|check| check.0 == kind) {
                        check.3 += 1;
                        if top_material(&column, x, z) == check.1 {
                            check.2 += 1;
                        }
                    }
                }
            }
        }
        for (kind, material, hits, total) in checks {
            assert!(total >= 20, "{kind:?}: zu wenige flache Proben ({total})");
            assert!(hits * 100 >= total * 95, "{kind:?}: nur {hits}/{total} mit {material:?}");
        }
    }

    #[test]
    fn steep_slopes_expose_rock() {
        let world = WorldGen::new(42);
        let (mut steep, mut rock) = (0, 0);
        for column in sample_columns(&world) {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let slope = (column.height(x + 1, z) - column.height(x, z))
                        .abs()
                        .max((column.height(x, z + 1) - column.height(x, z)).abs());
                    if slope < SOIL_SLOPE_END || column.height(x, z) < world.cfg.sea_level {
                        continue;
                    }
                    steep += 1;
                    let category = world.materials.get(top_material(&column, x, z)).unwrap().category;
                    // Basalt zählt als Fels; Boden darf an Steilhängen nicht liegen.
                    if category != MaterialCategory::Soil {
                        rock += 1;
                    }
                }
            }
        }
        assert!(steep > 100, "zu wenige Steilhänge: {steep}");
        assert_eq!(rock, steep, "Boden an Steilhängen");
    }

    #[test]
    fn basalt_only_in_volcanic_areas_and_every_rock_occurs() {
        let world = WorldGen::new(42);
        let mut seen = std::collections::HashSet::new();
        for column in sample_columns(&world) {
            for z in (0..CHUNK_SIZE).step_by(8) {
                for x in (0..CHUNK_SIZE).step_by(8) {
                    let origin = (column.chunk_x * CHUNK_SIZE + x, column.chunk_z * CHUNK_SIZE + z);
                    let volcanic = world.sample(origin.0 as f32, origin.1 as f32).volcanic;
                    let height = column.height(x, z);
                    for depth in [0.25, 2.0, 5.0, 15.0, 40.0, 80.0, 150.0, 250.0] {
                        let material = material_at(column.geology(x, z), depth, height - depth);
                        if material == MaterialId::BASALT {
                            assert!(volcanic > 0.25, "Basalt außerhalb eines Vulkangebiets ({volcanic})");
                        }
                        seen.insert(material);
                    }
                }
            }
        }
        for rock in [
            MaterialId::SANDSTONE,
            MaterialId::LIMESTONE,
            MaterialId::SLATE,
            MaterialId::GRANITE,
            MaterialId::MARBLE,
            MaterialId::BASALT,
        ] {
            assert!(seen.contains(&rock), "{rock:?} kommt nicht vor");
        }
        assert!(!seen.contains(&MaterialId::STONE), "Platzhalter-Gestein wird noch erzeugt");
    }

    #[test]
    fn geology_is_deterministic() {
        let (a, b) = (WorldGen::new(7), WorldGen::new(7));
        let (column_a, column_b) = (ColumnData::sample(&a, 3, -4), ColumnData::sample(&b, 3, -4));
        for z in SAMPLE_MIN..=CHUNK_SIZE + 1 {
            for x in SAMPLE_MIN..=CHUNK_SIZE + 1 {
                assert_eq!(column_a.geology(x, z), column_b.geology(x, z));
            }
        }
        let other = ColumnData::sample(&WorldGen::new(8), 3, -4);
        assert_ne!(column_a.geology(5, 5), other.geology(5, 5), "Seed hat keinen Einfluss");
    }

    /// Öffentliche Chunks: gemeinsame Samples haben auf allen drei Achsen dasselbe Material.
    #[test]
    fn neighbouring_public_chunks_agree_on_materials() {
        let world = WorldGen::new(42);
        let mut compared = 0;
        for (chunk_x, chunk_z) in [(0, 0), (40, -63), (-58, -39), (17, 22)] {
            let column = ColumnData::sample(&world, chunk_x, chunk_z);
            let east = ColumnData::sample(&world, chunk_x + 1, chunk_z);
            let south = ColumnData::sample(&world, chunk_x, chunk_z + 1);
            for chunk_y in column.surface_chunks() {
                let a = DensityChunk::from_column(&column, chunk_y);
                let pairs = [
                    (0, DensityChunk::from_column(&east, chunk_y)),
                    (1, DensityChunk::from_column(&column, chunk_y + 1)),
                    (2, DensityChunk::from_column(&south, chunk_y)),
                ];
                for (axis, b) in pairs {
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
                                assert_eq!(a.material(pa[0], pa[1], pa[2]), b.material(pb[0], pb[1], pb[2]), "Achse {axis}, {pa:?}");
                                compared += 1;
                            }
                        }
                    }
                }
            }
        }
        // 4 Spalten × 1-2 Oberflächen-Chunks × 3 Achsen × 2 Lagen × 34² ≈ 40.000
        assert!(compared > 20_000, "zu wenige Samples verglichen: {compared}");
    }

    #[test]
    fn missing_soil_exposes_rock() {
        let g = geology([(MaterialId::DIRT, 0.0), (MaterialId::DIRT, 0.0)], 0.0, -50.0, 0.0);
        assert_eq!(material_at(&g, 0.01, 1.0), MaterialId::SANDSTONE);
    }
}
