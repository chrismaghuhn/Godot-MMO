//! Baumplanung: pro Zelle eines Chunks entscheiden, ob und welcher Baum dort steht.

use glam::Vec3;

use super::rng::{Rng, hash01, hash_cell};
use super::{TreeId, TreePlacement, TreeStyle};
use crate::biome::FamilyKind;
use crate::world::BiomeRef;

const TAU: f32 = std::f32::consts::TAU;

/// Unter dieser Temperatur wachsen an Küsten Nadelbäume wie im Kaltwald/Schnee.
const COAST_COLD: f32 = -0.20;
/// Ab dieser Temperatur stehen an Küsten Palmen (Tropenwald beginnt bei 0.22).
const COAST_WARM: f32 = 0.15;

pub fn plan_trees<const N: usize>(
    seed: u32,
    sea_level: f32,
    chunk_x: i32,
    chunk_z: i32,
    heights: &[[f32; N]; N],
    biomes: &[[BiomeRef; N]; N],
    temperatures: &[[f32; N]; N],
) -> Vec<TreePlacement> {
    let cells = N.saturating_sub(3);
    let mut trees = Vec::new();

    for z in 0..cells {
        for x in 0..cells {
            let kind = biomes[z + 1][x + 1].kind;
            let base = density(kind);
            if base <= 0.0 {
                continue;
            }

            let cluster = hash01(
                seed,
                chunk_x,
                chunk_z,
                (x / 4) as u32,
                (z / 4) as u32,
                0x51ED,
            );
            let chance = base * (0.18 + 1.7 * cluster * cluster);

            let mut rng = Rng::new(hash_cell(seed, chunk_x, chunk_z, x as u32, z as u32, 0xA53));
            if rng.f32() >= chance {
                continue;
            }

            let h00 = heights[z + 1][x + 1];
            let h10 = heights[z + 1][x + 2];
            let h01 = heights[z + 2][x + 1];
            let h11 = heights[z + 2][x + 2];

            let anchor_x = rng.range(0.18, 0.82);
            let anchor_z = rng.range(0.18, 0.82);
            let height = lerp(lerp(h00, h10, anchor_x), lerp(h01, h11, anchor_x), anchor_z);
            if height < sea_level + 1.1 || height > sea_level + 103.0 {
                continue;
            }

            let slope = (h10 - h00).abs() + (h01 - h00).abs();
            if slope > 18.0 {
                continue;
            }

            let (style, variant) = pick_style(kind, temperatures[z + 1][x + 1], &mut rng);
            if slope > 13.0 && style != TreeStyle::Cactus {
                continue;
            }

            let (scale_min, scale_max) = match style {
                TreeStyle::Conifer => (0.72, 1.05),
                TreeStyle::Crystal => (0.62, 1.00),
                TreeStyle::Cactus => (0.60, 0.95),
                _ => (0.65, 0.95),
            };
            trees.push(TreePlacement {
                id: TreeId { chunk_x, chunk_z, cell: (z * cells + x) as u16 },
                style,
                variant,
                position: Vec3::new(x as f32 + anchor_x, height - 0.25, z as f32 + anchor_z),
                rotation: rng.range(0.0, TAU),
                scale: rng.range(scale_min, scale_max),
                scale_y: rng.range(0.90, 1.12),
            });
        }
    }

    trees
}

fn density(kind: FamilyKind) -> f32 {
    match kind {
        FamilyKind::TemperateForest => 0.050,
        FamilyKind::TropicalForest => 0.055,
        FamilyKind::ColdForest => 0.045,
        FamilyKind::Grassland => 0.006,
        FamilyKind::Desert => 0.004,
        FamilyKind::Snow => 0.007,
        FamilyKind::Mountains => 0.007,
        FamilyKind::Volcanic => 0.005,
        FamilyKind::Swamp => 0.030,
        FamilyKind::Coast => 0.005,
        FamilyKind::Underwater => 0.0,
        FamilyKind::Freshwater => 0.0,
        FamilyKind::FungiFlora => 0.042,
        FamilyKind::Crystal => 0.022,
        FamilyKind::Caves => 0.0,
    }
}

const STYLE_TEMPERATE: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Broadleaf, 0, 0.50),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Conifer, 0, 0.18),
    (TreeStyle::Broadleaf, 2, 0.10),
];

const STYLE_TROPICAL: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Palm, 0, 0.36),
    (TreeStyle::Broadleaf, 0, 0.26),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Palm, 1, 0.16),
];

const STYLE_COLD: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 0, 0.46),
    (TreeStyle::Conifer, 1, 0.32),
    (TreeStyle::Broadleaf, 1, 0.12),
    (TreeStyle::Dead, 0, 0.10),
];

const STYLE_GRASSLAND: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Broadleaf, 0, 0.62),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Dead, 0, 0.16),
];

const STYLE_DESERT: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Cactus, 0, 0.58),
    (TreeStyle::Cactus, 1, 0.24),
    (TreeStyle::Dead, 0, 0.18),
];

const STYLE_SNOW: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 2, 0.52),
    (TreeStyle::Conifer, 1, 0.35),
    (TreeStyle::Dead, 1, 0.13),
];

const STYLE_MOUNTAINS: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 0, 0.52),
    (TreeStyle::Conifer, 1, 0.30),
    (TreeStyle::Crystal, 1, 0.10),
    (TreeStyle::Dead, 1, 0.08),
];

const STYLE_VOLCANIC: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Dead, 1, 0.68),
    (TreeStyle::Dead, 0, 0.22),
    (TreeStyle::Crystal, 1, 0.10),
];

const STYLE_SWAMP: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Dead, 0, 0.44),
    (TreeStyle::Broadleaf, 1, 0.40),
    (TreeStyle::Broadleaf, 2, 0.16),
];

const STYLE_COAST: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Palm, 0, 0.62),
    (TreeStyle::Palm, 1, 0.24),
    (TreeStyle::Broadleaf, 0, 0.14),
];

const STYLE_COAST_TEMPERATE: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Broadleaf, 0, 0.48),
    (TreeStyle::Conifer, 0, 0.30),
    (TreeStyle::Broadleaf, 1, 0.14),
    (TreeStyle::Dead, 0, 0.08),
];

const STYLE_COAST_COLD: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 2, 0.48),
    (TreeStyle::Conifer, 1, 0.34),
    (TreeStyle::Dead, 1, 0.18),
];

const STYLE_FUNGI: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Mushroom, 0, 0.40),
    (TreeStyle::Mushroom, 1, 0.25),
    (TreeStyle::Mushroom, 2, 0.25),
    (TreeStyle::Broadleaf, 1, 0.10),
];

const STYLE_CRYSTAL: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Crystal, 0, 0.45),
    (TreeStyle::Crystal, 1, 0.30),
    (TreeStyle::Crystal, 2, 0.15),
    (TreeStyle::Conifer, 1, 0.10),
];

const STYLE_NONE: &[(TreeStyle, usize, f32)] = &[];

fn pick_style(kind: FamilyKind, temperature: f32, rng: &mut Rng) -> (TreeStyle, usize) {
    let table = match kind {
        // Küsten gibt es in jedem Klima: Palmen nur, wo es warm ist (#40).
        FamilyKind::Coast if temperature < COAST_COLD => STYLE_COAST_COLD,
        FamilyKind::Coast if temperature < COAST_WARM => STYLE_COAST_TEMPERATE,
        FamilyKind::TemperateForest => STYLE_TEMPERATE,
        FamilyKind::TropicalForest => STYLE_TROPICAL,
        FamilyKind::ColdForest => STYLE_COLD,
        FamilyKind::Grassland => STYLE_GRASSLAND,
        FamilyKind::Desert => STYLE_DESERT,
        FamilyKind::Snow => STYLE_SNOW,
        FamilyKind::Mountains => STYLE_MOUNTAINS,
        FamilyKind::Volcanic => STYLE_VOLCANIC,
        FamilyKind::Swamp => STYLE_SWAMP,
        FamilyKind::Coast => STYLE_COAST,
        FamilyKind::Freshwater => STYLE_NONE,
        FamilyKind::FungiFlora => STYLE_FUNGI,
        FamilyKind::Crystal => STYLE_CRYSTAL,
        FamilyKind::Underwater | FamilyKind::Caves => STYLE_NONE,
    };
    let mut roll = rng.f32();
    for &(style, variant, weight) in table {
        if roll < weight {
            return (style, variant);
        }
        roll -= weight;
    }
    (TreeStyle::Dead, 0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 35;

    fn coast_trees(temperature: f32) -> Vec<TreePlacement> {
        let heights = [[40.0; N]; N];
        let biomes = [[BiomeRef { kind: FamilyKind::Coast, variant: 0 }; N]; N];
        let temperatures = [[temperature; N]; N];
        (0..40)
            .flat_map(|chunk| plan_trees(42, 32.0, chunk, -chunk, &heights, &biomes, &temperatures))
            .collect()
    }

    #[test]
    fn palms_only_grow_on_warm_coasts() {
        // #40: Vorher standen an verschneiten Küsten Palmen.
        let cold = coast_trees(-0.6);
        assert!(!cold.is_empty());
        assert!(cold.iter().all(|tree| tree.style != TreeStyle::Palm), "Palmen an kalter Küste");
        assert!(coast_trees(0.0).iter().all(|tree| tree.style != TreeStyle::Palm));
        assert!(coast_trees(0.5).iter().any(|tree| tree.style == TreeStyle::Palm));
    }
}
