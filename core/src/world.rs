use crate::biome::{BiomeRegistry, FamilyKind};
use crate::geology::Geology;
use crate::material::MaterialRegistry;
use crate::noise::NoiseBank;

/// Wassertiefe in der Flussmitte.
const RIVER_DEPTH: f32 = 1.25;
/// Anstieg des Flussufers pro Einheit Fluss-Rauschen. Das Rauschen ändert sich im Mittel um
/// ~0.006 pro Meter, das Ufer steigt damit typisch ~0.2 m pro Meter (~11°).
const BANK_RISE: f32 = 35.0;
/// Weichheit des Übergangs vom Ufer ins ursprüngliche Gelände (Meter).
const BANK_BLEND: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct WorldConfig {
    pub seed: u32,
    pub sea_level: f32,
    pub view_radius: i32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            sea_level: 32.0,
            view_radius: 8,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnSample {
    pub height: f32,
    pub continentalness: f32,
    pub mountain: f32,
    pub temperature: f32,
    pub humidity: f32,
    pub weirdness: f32,
    pub realm: f32,
    pub volcanic: f32,
    pub crystal: f32,
    pub fungal: f32,
    pub river: f32,
    /// Wie viele Meter das Flussufer das Gelände hier abgesenkt hat (#35).
    pub river_cut: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BiomeRef {
    pub kind: FamilyKind,
    pub variant: usize,
}

pub struct WorldGen {
    pub cfg: WorldConfig,
    pub biomes: BiomeRegistry,
    pub materials: MaterialRegistry,
    pub geology: Geology,
    noise: NoiseBank,
}

impl WorldGen {
    pub fn new(seed: u32) -> Self {
        let cfg = WorldConfig {
            seed,
            ..WorldConfig::default()
        };
        Self {
            cfg,
            geology: Geology::new(seed, cfg.sea_level),
            biomes: BiomeRegistry::load(),
            materials: MaterialRegistry::load(),
            noise: NoiseBank::new(seed),
        }
    }

    pub fn sample(&self, x: f32, z: f32) -> ColumnSample {
        let raw = self.noise.sample(x, z);
        let sea = self.cfg.sea_level;

        let continental = smoothstep(-0.55, 0.35, raw.continental);
        let rugged = 1.0 - smoothstep(-0.75, 0.65, raw.erosion);
        let mountain = (continental * rugged).clamp(0.0, 1.0);

        // Ebenen: hohe Erosion abseits der Berge glättet das Detail fast komplett.
        let plains = smoothstep(-0.10, 0.55, raw.erosion) * (1.0 - mountain);
        // Bergkämme nur in echten Bergregionen, statt linear über ~70% des Landes auszulaufen.
        let ridge_mask = smoothstep(0.42, 0.90, mountain);
        // Hügel nur dort, wo das Gelände ohnehin rau ist, Berge bekommen am meisten.
        let hill_amp = (1.0 + 4.0 * rugged + 4.0 * ridge_mask) * (1.0 - 0.75 * plains);

        // Basishöhe als Kurve: Meeresboden -> Schelf -> flache Küstenebene -> sanftes Inland.
        // Die Meereslinie liegt wie vorher bei continental ~ -0.1, damit die Landmasse gleich bleibt.
        let c = raw.continental;
        let mut height = -46.0
            + 72.0 * smoothstep(-0.60, -0.15, c)
            + 14.0 * smoothstep(-0.17, 0.00, c)
            + 46.0 * smoothstep(0.00, 0.85, c);
        height += raw.ridge.powf(1.6) * 118.0 * ridge_mask;
        height += raw.detail * hill_amp;
        height += continental * 5.0;

        let lowland = smoothstep(sea + 55.0, sea + 3.0, height) * smoothstep(sea - 1.0, sea + 0.5, height);
        let river_band = 1.0 - smoothstep(0.0, 0.030, raw.river.abs());
        let river = river_band * lowland;
        let uncarved = height;
        if lowland > 0.0 {
            // Flussufer (#35): Das Gelände wird auf ein Ufer mit fester Steigung abgesenkt, das von
            // der Flussmitte aus ansteigt. Vorher wurde es über das schmale Flussband auf
            // Meereshöhe gezogen, dadurch fiel das Ufer bei hohem Gelände fast senkrecht ab.
            let bank = sea - RIVER_DEPTH + BANK_RISE * raw.river.abs();
            height += (smooth_min(height, bank, BANK_BLEND) - height) * lowland;
        }

        // Klima nach dem Gelände ohne Flussufer, damit Flüsse die Biome nicht verschieben.
        let altitude = (uncarved - sea).max(0.0);
        let temperature =
            (raw.temperature * 1.05 + raw.realm * 0.30 + 0.02 - altitude * 0.0035).clamp(-1.0, 1.0);
        let humidity = (raw.humidity * 1.35 - raw.realm * 0.25
            + 0.30 * smoothstep(70.0, 0.0, uncarved - sea))
        .clamp(-1.0, 1.0);

        let volcanic = smoothstep(
            0.40,
            0.72,
            raw.volcanic * 0.55 + mountain * 0.35 + raw.realm * 0.08,
        );
        let crystal = smoothstep(0.45, 0.80, raw.crystal + raw.realm * 0.08);
        let fungal = smoothstep(
            0.38,
            0.70,
            raw.fungal * 0.55 + raw.humidity * 0.45 + raw.realm * 0.08,
        );

        ColumnSample {
            height,
            continentalness: continental,
            mountain,
            temperature,
            humidity,
            weirdness: raw.weirdness * 0.5 + 0.5,
            realm: raw.realm * 0.5 + 0.5,
            volcanic,
            crystal,
            fungal,
            river,
            river_cut: uncarved - height,
        }
    }

    pub fn column_height(&self, x: f32, z: f32) -> f32 {
        self.sample(x, z).height
    }

    pub fn biome_at(&self, sample: &ColumnSample) -> BiomeRef {
        let kind = self.pick_family(sample);
        let variant = self.pick_variant(kind, sample);
        BiomeRef { kind, variant }
    }

    pub fn family(&self, kind: FamilyKind) -> &crate::biome::Family {
        self.biomes.family(kind)
    }

    pub fn biome_name(&self, biome: BiomeRef) -> &str {
        &self.biomes.family(biome.kind).variants[biome.variant].name
    }

    pub fn biome_color(&self, biome: BiomeRef) -> [u8; 3] {
        self.biomes.family(biome.kind).variants[biome.variant].color
    }

    pub fn apply_altitude(&self, color: [u8; 3], height: f32) -> [u8; 3] {
        let snow = smoothstep(118.0, 160.0, height) * 0.85;
        if snow <= 0.0 {
            return color;
        }
        let snow_color = [232.0, 240.0, 248.0];
        [
            (color[0] as f32 + (snow_color[0] - color[0] as f32) * snow) as u8,
            (color[1] as f32 + (snow_color[1] - color[1] as f32) * snow) as u8,
            (color[2] as f32 + (snow_color[2] - color[2] as f32) * snow) as u8,
        ]
    }

    fn pick_family(&self, sample: &ColumnSample) -> FamilyKind {
        let sea = self.cfg.sea_level;
        if sample.height < sea - 2.0 {
            return FamilyKind::Underwater;
        }
        if sample.river > 0.35 && sample.height < sea + 0.25 {
            return FamilyKind::Freshwater;
        }
        // Abgesenkte Flussufer zählen nicht als Küste, sonst säumen Strände jeden Fluss.
        if sample.height + sample.river_cut < sea + 6.0 {
            return FamilyKind::Coast;
        }
        if sample.volcanic > 0.58 && sample.height > sea + 5.0 {
            return FamilyKind::Volcanic;
        }
        if sample.crystal > 0.62 && sample.mountain > 0.15 {
            return FamilyKind::Crystal;
        }
        if sample.fungal > 0.58 && sample.humidity > 0.05 {
            return FamilyKind::FungiFlora;
        }
        if sample.mountain > 0.58 && sample.height > sea + 68.0 {
            return FamilyKind::Mountains;
        }
        if sample.temperature < -0.58 {
            return FamilyKind::Snow;
        }
        if sample.temperature > 0.22 && sample.humidity > 0.0 {
            return FamilyKind::TropicalForest;
        }
        if sample.humidity < -0.26 && sample.temperature > 0.02 {
            return FamilyKind::Desert;
        }
        if sample.humidity > 0.42 && sample.height + sample.river_cut < sea + 12.0 {
            return FamilyKind::Swamp;
        }
        if sample.temperature < -0.20 {
            return FamilyKind::ColdForest;
        }
        if sample.humidity > -0.10 {
            return FamilyKind::TemperateForest;
        }
        FamilyKind::Grassland
    }

    fn pick_variant(&self, kind: FamilyKind, sample: &ColumnSample) -> usize {
        let count = self.biomes.family(kind).variants.len();
        if count == 0 {
            return 0;
        }
        let wobble = (sample.weirdness + (sample.realm - 0.5) * 0.55).rem_euclid(1.0);

        if kind == FamilyKind::Underwater {
            let depth = (self.cfg.sea_level - sample.height).max(0.0);
            let group = if depth < 12.0 {
                0
            } else if depth < 40.0 {
                1
            } else {
                2
            };
            let variants = &self.biomes.family(kind).variants;
            let mut matching = 0usize;
            for variant in variants {
                if variant.underwater_group == group {
                    matching += 1;
                }
            }
            if matching > 0 {
                let target = slot(wobble, matching);
                let mut seen = 0usize;
                for (index, variant) in variants.iter().enumerate() {
                    if variant.underwater_group == group {
                        if seen == target {
                            return index;
                        }
                        seen += 1;
                    }
                }
            }
            return slot(wobble, count);
        }

        slot(wobble, count)
    }
}

fn slot(t: f32, count: usize) -> usize {
    ((t.clamp(0.0, 0.999_999) * count as f32) as usize).min(count - 1)
}

/// Minimum mit weichem Übergang der Breite `k` (polynomielles smooth-min).
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Landproben auf einem groben Raster (Seed 42).
    fn land_samples(world: &WorldGen, area: f32, steps: usize) -> Vec<(f32, f32, ColumnSample)> {
        let mut samples = Vec::new();
        for j in 0..steps {
            for i in 0..steps {
                let x = (i as f32 / steps as f32 - 0.5) * area;
                let z = (j as f32 / steps as f32 - 0.5) * area;
                let sample = world.sample(x, z);
                if sample.height >= world.cfg.sea_level {
                    samples.push((x, z, sample));
                }
            }
        }
        samples
    }

    #[test]
    fn crystal_biomes_exist_but_stay_rare() {
        // #49: Vorher lag der Kristallwert nie über 0.4, die Familie kam nie vor.
        let world = WorldGen::new(42);
        let land = land_samples(&world, 20_000.0, 160);
        let crystal = land
            .iter()
            .filter(|(_, _, sample)| world.biome_at(sample).kind == FamilyKind::Crystal)
            .count();
        let share = crystal as f64 / land.len() as f64;
        assert!((0.005..0.05).contains(&share), "Kristall-Anteil {share:.4}");
    }

    #[test]
    fn river_banks_are_mostly_gentle() {
        // #35: Vorher waren ~90 % der eingeschnittenen Ufer steiler als 30°.
        let world = WorldGen::new(42);
        let sea = world.cfg.sea_level;
        let (mut banks, mut steep) = (0usize, 0usize);
        for (x, z, sample) in land_samples(&world, 8_000.0, 400) {
            if sample.river_cut < 1.0 {
                continue;
            }
            let dx = world.column_height(x + 0.5, z) - world.column_height(x - 0.5, z);
            let dz = world.column_height(x, z + 0.5) - world.column_height(x, z - 0.5);
            banks += 1;
            if dx.hypot(dz).atan().to_degrees() > 30.0 {
                steep += 1;
            }
            if sample.height + sample.river_cut >= sea + 6.0 {
                let kind = world.biome_at(&sample).kind;
                assert_ne!(kind, FamilyKind::Coast, "Flussufer im Inland bei ({x},{z}) ist Küste");
            }
        }
        assert!(banks > 100, "zu wenige Uferproben: {banks}");
        let share = steep as f64 / banks as f64;
        assert!(share < 0.15, "steile Ufer: {share:.3} von {banks}");
    }
}
