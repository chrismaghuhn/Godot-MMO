use crate::biome::{BiomeRegistry, FamilyKind};
use crate::noise::NoiseBank;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BiomeRef {
    pub kind: FamilyKind,
    pub variant: usize,
}

pub struct WorldGen {
    pub cfg: WorldConfig,
    pub biomes: BiomeRegistry,
    noise: NoiseBank,
}

impl WorldGen {
    pub fn new(seed: u32) -> Self {
        Self {
            cfg: WorldConfig {
                seed,
                ..WorldConfig::default()
            },
            biomes: BiomeRegistry::load(),
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

        let river_band = 1.0 - smoothstep(0.0, 0.030, raw.river.abs());
        let river = river_band
            * smoothstep(sea + 55.0, sea + 3.0, height)
            * smoothstep(sea - 1.0, sea + 0.5, height);
        if river > 0.0 {
            height += (sea - 1.25 - height) * river;
        }

        let altitude = (height - sea).max(0.0);
        let temperature =
            (raw.temperature * 1.05 + raw.realm * 0.30 + 0.02 - altitude * 0.0035).clamp(-1.0, 1.0);
        let humidity = (raw.humidity * 1.35 - raw.realm * 0.25
            + 0.30 * smoothstep(70.0, 0.0, height - sea))
        .clamp(-1.0, 1.0);

        let volcanic = smoothstep(
            0.40,
            0.72,
            raw.volcanic * 0.55 + mountain * 0.35 + raw.realm * 0.08,
        );
        let crystal = smoothstep(0.45, 0.75, raw.crystal * 0.60 + raw.realm * 0.08);
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
        if sample.height < sea + 6.0 {
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
        if sample.humidity > 0.42 && sample.height < sea + 12.0 {
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

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
