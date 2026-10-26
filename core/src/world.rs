use crate::biome::{BiomeRegistry, FamilyKind};
use crate::geology::Geology;
use crate::material::MaterialRegistry;
use crate::noise::NoiseBank;
use crate::regions::{RegionId, RegionMap, RegionSample, RegionTheme};

/// Wassertiefe in der Flussmitte.
const RIVER_DEPTH: f32 = 1.25;
/// Anstieg des Flussufers pro Einheit Fluss-Rauschen. Seit #50 ändert sich das Rauschen im
/// Mittel um ~0.003 pro Meter, das Ufer steigt damit typisch ~0.2 m pro Meter (~11°).
const BANK_RISE: f32 = 70.0;
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
    /// Region (#50) und Übergang zur Nachbarregion.
    pub region: RegionSample,
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
    pub regions: RegionMap,
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
            regions: RegionMap::new(seed),
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
        // Breiterer, weich einsetzender Übergang zu den Kämmen plus Vorgebirge (#36), damit Massive
        // nicht abrupt aus der Ebene aufsteigen.
        let ridge_mask = smoothstep(0.25, 1.00, mountain).powf(1.5);
        let foothills = smoothstep(0.15, 0.70, mountain);
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
        height += 30.0 * foothills;
        height += raw.detail * hill_amp;
        height += continental * 5.0;

        // Flussband an die halb so steile Fluss-Noise seit #50 angepasst (gleiche Breite wie vorher).
        let lowland = smoothstep(sea + 55.0, sea + 3.0, height) * smoothstep(sea - 1.0, sea + 0.5, height);
        let river_band = 1.0 - smoothstep(0.0, 0.0144, raw.river.abs());
        let river = river_band * lowland;
        let uncarved = height;
        if lowland > 0.0 {
            // Flussufer (#35): Das Gelände wird auf ein Ufer mit fester Steigung abgesenkt, das von
            // der Flussmitte aus ansteigt. Vorher wurde es über das schmale Flussband auf
            // Meereshöhe gezogen, dadurch fiel das Ufer bei hohem Gelände fast senkrecht ab.
            let bank = sea - RIVER_DEPTH + BANK_RISE * raw.river.abs();
            height += (smooth_min(height, bank, BANK_BLEND) - height) * lowland;
        }

        // Regionen (#50): Klima-Tendenz und Thema, an Grenzen gemischt.
        let region = self.regions.sample(x, z);
        let (temperature_bias, humidity_bias) = self.regions.climate_bias(&region);

        // Klima nach dem Gelände ohne Flussufer, damit Flüsse die Biome nicht verschieben.
        let altitude = (uncarved - sea).max(0.0);
        let temperature = (raw.temperature * 1.05 + raw.realm * 0.30 + 0.02 + temperature_bias
            - altitude * 0.0035)
            .clamp(-1.0, 1.0);
        let humidity = (raw.humidity * 1.35 - raw.realm * 0.25
            + humidity_bias
            + 0.30 * smoothstep(70.0, 0.0, uncarved - sea))
        .clamp(-1.0, 1.0);

        // Spezial-Biome kommen aus dem Thema der Region (#49, #50). Im Inneren (Gewicht ≈ 1) liegt
        // der Wert immer über der Biom-Schwelle (0,6), die lokale Noise formt nur den Rand der
        // Zone; an Grenzen zählt das Thema höchstens halb, dort entstehen keine Spezial-Biome.
        let shape = |value: f32| smoothstep(-0.4, 0.5, value);
        let volcanic = self.regions.theme_weight(&region, RegionTheme::Volcanic) * (0.62 + 0.38 * shape(raw.volcanic));
        let crystal = self.regions.theme_weight(&region, RegionTheme::Crystal) * (0.62 + 0.38 * shape(raw.crystal));
        let fungal = self.regions.theme_weight(&region, RegionTheme::Fungal) * (0.62 + 0.38 * shape(raw.fungal));

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
            region,
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

    /// Oberflächenfarbe mit weichem Übergang an Regionsgrenzen: Mittel der Varianten, die jede
    /// mitzählende Region für diese Familie hätte, gewichtet wie die Regionen.
    pub fn surface_color(&self, sample: &ColumnSample, biome: BiomeRef) -> [u8; 3] {
        if sample.region.foreign_weight() <= 0.0 {
            return self.biome_color(biome);
        }
        let mut sum = [0.0f32; 3];
        for &(region, weight) in sample.region.regions() {
            let color = self.biome_color(BiomeRef {
                kind: biome.kind,
                variant: self.variant_in_region(biome.kind, sample, region),
            });
            for i in 0..3 {
                sum[i] += color[i] as f32 * weight;
            }
        }
        sum.map(|c| c.round().clamp(0.0, 255.0) as u8)
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
        // Kristall- und Pilz-Regionen auf jedem Gelände (früher nur Berge bzw. feucht; #49).
        if sample.crystal > 0.6 {
            return FamilyKind::Crystal;
        }
        if sample.fungal > 0.6 {
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
        self.variant_in_region(kind, sample, sample.region.id)
    }

    /// Variante einer Familie in einer Region: fest pro Region (#50). Unter Wasser zusätzlich
    /// nach Tiefe gruppiert (flach, mittel, tief).
    fn variant_in_region(&self, kind: FamilyKind, sample: &ColumnSample, region: RegionId) -> usize {
        let variants = &self.biomes.family(kind).variants;
        if variants.is_empty() {
            return 0;
        }
        let purpose = kind as u64;
        if kind == FamilyKind::Underwater {
            let depth = (self.cfg.sea_level - sample.height).max(0.0);
            let group = if depth < 12.0 {
                0
            } else if depth < 40.0 {
                1
            } else {
                2
            };
            let matching: Vec<usize> = (0..variants.len()).filter(|&i| variants[i].underwater_group == group).collect();
            if !matching.is_empty() {
                return matching[self.regions.pick(region, purpose * 4 + group as u64, matching.len())];
            }
        }
        self.regions.pick(region, purpose, variants.len())
    }
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
    use std::collections::{HashMap, VecDeque};

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
        // #49: Vorher lag der Kristallwert nie über 0.4, die Familie kam nie vor. Seit #50 gibt es
        // Kristall-Regionen (≈ 6 % der Regionen, mehrere km groß); der Anteil schwankt deshalb mit
        // dem Ausschnitt und wird über 40×40 km gemessen.
        let world = WorldGen::new(42);
        let land = land_samples(&world, 40_000.0, 200);
        let crystal = land
            .iter()
            .filter(|(_, _, sample)| world.biome_at(sample).kind == FamilyKind::Crystal)
            .count();
        let share = crystal as f64 / land.len() as f64;
        assert!((0.01..0.12).contains(&share), "Kristall-Anteil {share:.4}");
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

    /// #49: Spezial-Biome kommen vor, aber nur in Regionen mit passendem Thema.
    #[test]
    fn special_biomes_exist_only_in_themed_regions() {
        let world = WorldGen::new(42);
        let mut counts: HashMap<FamilyKind, usize> = HashMap::new();
        for z in (-20000..20000).step_by(97) {
            for x in (-20000..20000).step_by(97) {
                let sample = world.sample(x as f32, z as f32);
                let kind = world.biome_at(&sample).kind;
                let theme = match kind {
                    FamilyKind::Volcanic => RegionTheme::Volcanic,
                    FamilyKind::Crystal => RegionTheme::Crystal,
                    FamilyKind::FungiFlora => RegionTheme::Fungal,
                    _ => continue,
                };
                *counts.entry(kind).or_default() += 1;
                assert!(
                    world.regions.theme_weight(&sample.region, theme) > 0.5,
                    "{kind:?} außerhalb einer passenden Region bei {x},{z}"
                );
            }
        }
        for kind in [FamilyKind::Volcanic, FamilyKind::Crystal, FamilyKind::FungiFlora] {
            assert!(counts.get(&kind).copied().unwrap_or(0) > 200, "{kind:?} zu selten: {counts:?}");
        }
    }

    /// #50: Im Inneren einer Region hat jede Familie genau eine Variante.
    #[test]
    fn one_variant_per_family_inside_a_region() {
        let world = WorldGen::new(42);
        let mut seen: HashMap<(RegionId, FamilyKind), usize> = HashMap::new();
        let mut checked = 0;
        for z in (-12000..12000).step_by(131) {
            for x in (-12000..12000).step_by(131) {
                let sample = world.sample(x as f32, z as f32);
                if sample.region.foreign_weight() > 0.0 {
                    continue;
                }
                let biome = world.biome_at(&sample);
                if biome.kind == FamilyKind::Underwater {
                    continue; // unter Wasser zusätzlich nach Tiefe gruppiert
                }
                let variant = *seen.entry((sample.region.id, biome.kind)).or_insert(biome.variant);
                assert_eq!(variant, biome.variant, "zwei Varianten von {:?} in Region {:?}", biome.kind, sample.region.id);
                checked += 1;
            }
        }
        assert!(checked > 5000);
    }

    /// #50: Varianten-Flächen sind groß. "Typische Größe" = flächengewichteter Mittelwert von
    /// sqrt(Fläche). Vor #50 waren es ≈ 0,1 km (Flickenteppich), danach ≈ 1,5-2 km.
    #[test]
    fn variant_patches_are_large() {
        let world = WorldGen::new(42);
        let (half, step) = (6000, 32);
        let n = (2 * half / step) as usize;
        let mut label = vec![None; n * n];
        for j in 0..n {
            for i in 0..n {
                let sample = world.sample((-half + i as i32 * step) as f32, (-half + j as i32 * step) as f32);
                if sample.height >= world.cfg.sea_level {
                    label[j * n + i] = Some(world.biome_at(&sample));
                }
            }
        }
        let mut seen = vec![false; n * n];
        let (mut weighted, mut total) = (0.0f64, 0.0f64);
        for start in 0..n * n {
            let Some(own) = label[start] else { continue };
            if seen[start] {
                continue;
            }
            let mut queue = VecDeque::from([start]);
            seen[start] = true;
            let mut cells = 0usize;
            while let Some(p) = queue.pop_front() {
                cells += 1;
                let (x, y) = (p % n, p / n);
                let neighbours = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
                for (nx, ny) in neighbours {
                    if nx < n && ny < n && !seen[ny * n + nx] && label[ny * n + nx] == Some(own) {
                        seen[ny * n + nx] = true;
                        queue.push_back(ny * n + nx);
                    }
                }
            }
            let area = cells as f64 * (step * step) as f64;
            weighted += area * area.sqrt();
            total += area;
        }
        let typical_km = weighted / total / 1000.0;
        assert!(typical_km > 1.0, "Varianten-Flächen nur {typical_km:.2} km groß");
    }

    /// Weicher Übergang: Auf einer Grenze zweier Regionen ist die Farbe das Mittel beider
    /// Varianten; im Inneren gibt es keine Mischung.
    #[test]
    fn surface_colour_blends_at_region_borders() {
        let world = WorldGen::new(42);
        let mut blended = 0;
        let mut x = -15000.0;
        while x < 15000.0 {
            x += 7.0;
            let sample = world.sample(x, 777.0);
            let biome = world.biome_at(&sample);
            let color = world.surface_color(&sample, biome);
            let regions = sample.region.regions();
            if regions.len() == 1 {
                assert_eq!(color, world.biome_color(biome), "im Inneren keine Mischung");
            } else if regions.len() == 2 && sample.region.foreign_weight() > 0.49 {
                let other = world.biome_color(BiomeRef {
                    kind: biome.kind,
                    variant: world.variant_in_region(biome.kind, &sample, regions[1].0),
                });
                let own = world.biome_color(biome);
                for i in 0..3 {
                    let mean = (own[i] as f32 + other[i] as f32) / 2.0;
                    assert!((color[i] as f32 - mean).abs() <= 2.0, "keine Mischung an der Grenze");
                }
                blended += 1;
            }
        }
        assert!(blended > 0, "keine Grenze auf der Teststrecke");
    }

    /// Review-Fund zu #50: An Dreiländerecken sprangen Thema, Klima und Farbe, weil nur zwei
    /// Regionen gemischt wurden. 2D-Abtastung im 2-m-Raster um echte Dreiländerecken: Benachbarte
    /// Punkte dürfen sich nur wenig unterscheiden.
    #[test]
    fn region_blend_is_continuous_around_triple_points() {
        let world = WorldGen::new(42);
        // Dreiländerecken suchen: Punkte, an denen drei Regionen mitzählen.
        let mut triples = Vec::new();
        'search: for z in (-20000..20000).step_by(150) {
            for x in (-20000..20000).step_by(150) {
                let sample = world.regions.sample(x as f32, z as f32);
                if sample.regions().len() >= 3 && sample.regions()[2].1 > 0.2 {
                    triples.push((x as f32, z as f32));
                    if triples.len() >= 6 {
                        break 'search;
                    }
                }
            }
        }
        assert!(triples.len() >= 3, "keine Dreiländerecken gefunden");
        let themes = [RegionTheme::Volcanic, RegionTheme::Crystal, RegionTheme::Fungal, RegionTheme::Normal];
        let values = |x: f32, z: f32| {
            let region = world.regions.sample(x, z);
            let (t, h) = world.regions.climate_bias(&region);
            let mut v = vec![t, h];
            v.extend(themes.iter().map(|&theme| world.regions.theme_weight(&region, theme)));
            v
        };
        let mut max_step = 0.0f32;
        for (cx, cz) in triples {
            for j in -100..100 {
                for i in -100..100 {
                    let (x, z) = (cx + i as f32 * 2.0, cz + j as f32 * 2.0);
                    let here = values(x, z);
                    for (dx, dz) in [(2.0, 0.0), (0.0, 2.0)] {
                        let there = values(x + dx, z + dz);
                        for (a, b) in here.iter().zip(&there) {
                            max_step = max_step.max((a - b).abs());
                        }
                    }
                }
            }
        }
        // Breite der Übergangszone 1400 m: auf 2 m ändert sich ein Gewicht um deutlich < 0,02.
        // Vorher sprang es um bis zu 0,5.
        assert!(max_step < 0.02, "Sprung um {max_step} auf 2 m");
    }

    /// #36: Gebirgsränder nicht blockartig. Profile von Gipfeln nach außen; Median der steilsten
    /// 15-m-Stufe. Vorher (harte Kamm-Maske) ≈ 1,19 m/m, mit Vorgebirge ≈ 1,06 m/m.
    #[test]
    fn mountain_edges_are_not_blocky() {
        let world = WorldGen::new(42);
        let sea = world.cfg.sea_level;
        let mut steps = Vec::new();
        for z in (-12000..12000).step_by(800) {
            for x in (-12000..12000).step_by(800) {
                let sample = world.sample(x as f32, z as f32);
                if sample.mountain < 0.8 || sample.height < sea + 80.0 {
                    continue;
                }
                for a in 0..8 {
                    let angle = a as f32 / 8.0 * std::f32::consts::TAU;
                    let (dx, dz) = (angle.cos() * 5.0, angle.sin() * 5.0);
                    let profile: Vec<f32> =
                        (0..300).map(|i| world.column_height(x as f32 + dx * i as f32, z as f32 + dz * i as f32)).collect();
                    steps.push(profile.windows(4).map(|w| (w[0] - w[3]).abs()).fold(0.0, f32::max) / 15.0);
                }
            }
        }
        assert!(steps.len() > 200, "zu wenige Profile: {}", steps.len());
        steps.sort_by(f32::total_cmp);
        let median = steps[steps.len() / 2];
        assert!(median < 1.13, "steilste Stufe an Gebirgsrändern im Median {median:.2} m/m");
    }
}
