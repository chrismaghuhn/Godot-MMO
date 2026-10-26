use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FamilyKind {
    TemperateForest,
    TropicalForest,
    ColdForest,
    Grassland,
    Desert,
    Snow,
    Mountains,
    Volcanic,
    Swamp,
    Coast,
    Underwater,
    Freshwater,
    FungiFlora,
    Crystal,
    Caves,
}

impl FamilyKind {
    pub const ALL: [FamilyKind; 15] = [
        FamilyKind::TemperateForest,
        FamilyKind::TropicalForest,
        FamilyKind::ColdForest,
        FamilyKind::Grassland,
        FamilyKind::Desert,
        FamilyKind::Snow,
        FamilyKind::Mountains,
        FamilyKind::Volcanic,
        FamilyKind::Swamp,
        FamilyKind::Coast,
        FamilyKind::Underwater,
        FamilyKind::Freshwater,
        FamilyKind::FungiFlora,
        FamilyKind::Crystal,
        FamilyKind::Caves,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FamilyKind::TemperateForest => "temperate_forest",
            FamilyKind::TropicalForest => "tropical_forest",
            FamilyKind::ColdForest => "cold_forest",
            FamilyKind::Grassland => "grassland",
            FamilyKind::Desert => "desert",
            FamilyKind::Snow => "snow",
            FamilyKind::Mountains => "mountains",
            FamilyKind::Volcanic => "volcanic",
            FamilyKind::Swamp => "swamp",
            FamilyKind::Coast => "coast",
            FamilyKind::Underwater => "underwater",
            FamilyKind::Freshwater => "freshwater",
            FamilyKind::FungiFlora => "fungi_flora",
            FamilyKind::Crystal => "crystal",
            FamilyKind::Caves => "caves",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        FamilyKind::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantTag {
    Normal,
    Glow,
    Crystal,
    Ice,
    Ash,
    Toxic,
    Blood,
    Gold,
    Silver,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub color: [u8; 3],
    pub tag: VariantTag,
    pub underwater_group: u8,
}

#[derive(Clone, Debug)]
pub struct Family {
    pub kind: FamilyKind,
    pub label: String,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug)]
pub struct BiomeRegistry {
    families: Vec<Family>,
    family_indices: [usize; FamilyKind::ALL.len()],
}

#[derive(Deserialize)]
struct BiomeFile {
    families: Vec<FamilyDef>,
}

#[derive(Deserialize)]
struct FamilyDef {
    id: String,
    label: String,
    base_color: [u8; 3],
    variants: Vec<String>,
}

impl BiomeRegistry {
    pub fn load() -> Self {
        let file: BiomeFile = ron::from_str(include_str!("../assets/biomes.ron"))
            .expect("assets/biomes.ron muss gueltiges RON sein");
        let mut families = Vec::with_capacity(file.families.len());
        for def in file.families {
            let kind = FamilyKind::from_key(&def.id)
                .unwrap_or_else(|| panic!("unbekannte Biom-Familie: {}", def.id));
            let variants = def
                .variants
                .iter()
                .map(|name| make_variant(def.base_color, name))
                .collect();
            families.push(Family {
                kind,
                label: def.label,
                variants,
            });
        }
        let mut family_indices = [usize::MAX; FamilyKind::ALL.len()];
        for (index, family) in families.iter().enumerate() {
            family_indices[family.kind as usize] = index;
        }
        for kind in FamilyKind::ALL {
            assert!(
                family_indices[kind as usize] != usize::MAX,
                "Biom-Familie fehlt in biomes.ron: {}",
                kind.key()
            );
        }
        Self {
            families,
            family_indices,
        }
    }

    pub fn family(&self, kind: FamilyKind) -> &Family {
        &self.families[self.family_indices[kind as usize]]
    }

    pub fn families(&self) -> &[Family] {
        &self.families
    }
}

fn make_variant(base: [u8; 3], name: &str) -> Variant {
    let hash = hash_str(name);
    let (mut h, mut s, mut v) = rgb_to_hsv(base);
    h = (h + (rand01(hash, 1) - 0.5) * 0.05).rem_euclid(1.0);
    s = (s + (rand01(hash, 2) - 0.5) * 0.10).clamp(0.02, 1.0);
    v = (v + (rand01(hash, 3) - 0.5) * 0.14).clamp(0.04, 1.0);
    let mut rgb = hsv_to_rgb(h, s, v);
    let lower = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));

    let tag = if has(&["leucht", "bioluminesz", "glut", "lava", "magma", "feuer"]) {
        rgb = lerp_rgb(rgb, [255, 238, 150], 0.35);
        VariantTag::Glow
    } else if has(&[
        "kristall", "diamant", "amethyst", "quarz", "saphir", "rubin", "smaragd", "obsidian",
        "glas", "mineral", "geode",
    ]) {
        rgb = lerp_rgb(rgb, [150, 215, 230], 0.35);
        VariantTag::Crystal
    } else if has(&[
        "schnee",
        "frost",
        "gletscher",
        "polar",
        "raureif",
        "winter",
        "gefroren",
        "eiswüste",
        "eissteppe",
        "eisberg",
        "eisfeld",
        "eishöhle",
        "eisig",
        "ewiges eis",
        "schwarzeis",
        "kristalleis",
        "eisnadel",
        "eisküste",
        "eisunterwasser",
        "vereist",
    ]) {
        rgb = lerp_rgb(rgb, [228, 240, 250], 0.40);
        VariantTag::Ice
    } else if has(&["asche", "verbrannt", "schwarz", "schlacke", "verbrannte"]) {
        rgb = scale_rgb(rgb, 0.55);
        VariantTag::Ash
    } else if has(&["gift", "säure", "schwefel"]) {
        rgb = lerp_rgb(rgb, [130, 190, 70], 0.35);
        VariantTag::Toxic
    } else if has(&["blut", "rot"]) {
        rgb = lerp_rgb(rgb, [190, 55, 45], 0.35);
        VariantTag::Blood
    } else if has(&["gold", "gelb"]) {
        rgb = lerp_rgb(rgb, [235, 195, 75], 0.35);
        VariantTag::Gold
    } else if has(&["silber", "weiß", "weiss"]) {
        rgb = lerp_rgb(rgb, [215, 220, 230], 0.35);
        VariantTag::Silver
    } else {
        VariantTag::Normal
    };

    Variant {
        name: name.to_string(),
        color: rgb,
        tag,
        underwater_group: underwater_group(name),
    }
}

fn underwater_group(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if lower.contains("tiefsee")
        || lower.contains("graben")
        || lower.contains("raucher")
        || lower.contains("ozean")
        || lower.contains("ebene")
        || lower.contains("unterwasserwüste")
        || lower.contains("lavastrom")
        || lower.contains("unterwasser-lava")
    {
        2
    } else if lower.contains("riff")
        || lower.contains("kelp")
        || lower.contains("seegras")
        || lower.contains("korall")
        || lower.contains("quallen")
        || lower.contains("algen")
        || lower.contains("schwamm")
        || lower.contains("muschel")
        || lower.contains("lagune")
        || lower.contains("garten")
        || lower.contains("flach")
    {
        0
    } else {
        1
    }
}

fn hash_str(text: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn rand01(hash: u64, salt: u64) -> f32 {
    let mut x = hash ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
    ]
}

fn scale_rgb(rgb: [u8; 3], factor: f32) -> [u8; 3] {
    [
        (rgb[0] as f32 * factor).clamp(0.0, 255.0) as u8,
        (rgb[1] as f32 * factor).clamp(0.0, 255.0) as u8,
        (rgb[2] as f32 * factor).clamp(0.0, 255.0) as u8,
    ]
}

fn rgb_to_hsv(rgb: [u8; 3]) -> (f32, f32, f32) {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let sector = h.floor() as i32 % 6;
    let f = h - h.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match sector {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #37: Biom-Namen sind über alle Familien eindeutig (Anzeige, spätere Daten je Variante).
    #[test]
    fn variant_names_are_unique() {
        let registry = BiomeRegistry::load();
        let mut seen = std::collections::HashMap::new();
        let mut total = 0;
        for family in registry.families() {
            for variant in &family.variants {
                total += 1;
                if let Some(other) = seen.insert(variant.name.clone(), family.kind) {
                    panic!("{} doppelt: {:?} und {:?}", variant.name, other, family.kind);
                }
            }
        }
        assert_eq!(total, 352, "Anzahl der Biome hat sich geändert");
    }
}
