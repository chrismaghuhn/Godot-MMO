//! Öffentliche Regionen (#50): große Gebiete (≈ 4-8 km) mit eigener Identität.
//!
//! Jede Region hat
//! - eine feste **Variante pro Biom-Familie** (z. B. die ganze Region ist "Herbstwald"),
//! - eine **Klima-Tendenz** (wärmer/kälter, feuchter/trockener),
//! - ein **Thema**: normal, Vulkan, Kristall oder Pilz.
//!
//! Regionen sind Voronoi-Zellen auf verzerrten Koordinaten (organische Grenzen). An Grenzen
//! werden Themen, Klima und Farben über `BORDER_BLEND` Meter gemischt; genau auf der Grenze
//! zählen beide Regionen gleich, dadurch ist alles stetig.
//!
//! Alles aus dem öffentlichen Seed: Regionen sind sichtbar und dürfen vom Client erzeugt werden.

use crate::noise::Layer;

/// Abstand der Regions-Mittelpunkte (m). Mit Versatz 0,25-0,75 entstehen Regionen von ≈ 4-8 km.
pub const REGION_SIZE: f32 = 6000.0;
/// Breite der Übergangszone an Regionsgrenzen (m, je Seite).
pub const BORDER_BLEND: f32 = 700.0;
/// Verzerrung der Regionsgrenzen (m).
const WARP: f32 = 1200.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionId(pub i32, pub i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionTheme {
    Normal,
    Volcanic,
    Crystal,
    Fungal,
}

/// Region an einem Punkt und wie stark die Nachbarregion dort mitzählt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionSample {
    /// Region, zu der der Punkt gehört (nächster Mittelpunkt).
    pub id: RegionId,
    /// Nächste andere Region (für Übergänge).
    pub neighbour: RegionId,
    /// Gewicht der Nachbarregion: 0,5 genau auf der Grenze, 0 ab `BORDER_BLEND` im Inneren.
    pub neighbour_weight: f32,
}

impl RegionSample {
    /// Gewichteter Mittelwert einer Eigenschaft beider Regionen (stetig über die Grenze).
    pub fn blend(&self, mut value: impl FnMut(RegionId) -> f32) -> f32 {
        let w = self.neighbour_weight;
        value(self.id) * (1.0 - w) + value(self.neighbour) * w
    }
}

pub struct RegionMap {
    seed: u32,
    warp_x: Layer,
    warp_z: Layer,
}

/// Zweck-Konstanten für den Hash, damit Entscheidungen nicht korrelieren.
const SALT_CENTER_X: u64 = 1;
const SALT_CENTER_Z: u64 = 2;
const SALT_THEME: u64 = 3;
const SALT_TEMPERATURE: u64 = 4;
const SALT_HUMIDITY: u64 = 5;
const SALT_VARIANT: u64 = 100;

impl RegionMap {
    pub fn new(seed: u32) -> Self {
        let seed_i = seed as i32;
        Self {
            seed,
            warp_x: Layer::new(seed_i.wrapping_add(311), 0.00035, 2),
            warp_z: Layer::new(seed_i.wrapping_add(313), 0.00035, 2),
        }
    }

    fn hash(&self, id: RegionId, salt: u64) -> u64 {
        let mut h = mix(self.seed as u64 ^ 0x0005_EED0_F2E6_10A5);
        for part in [id.0 as i64 as u64, id.1 as i64 as u64, salt] {
            h = mix(h ^ part);
        }
        h
    }

    fn unit(&self, id: RegionId, salt: u64) -> f32 {
        (self.hash(id, salt) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Mittelpunkt, versetzt um 0,25..0,75 der Zelle (nächster Mittelpunkt liegt dann sicher in
    /// den 3×3 Nachbarzellen).
    pub fn center(&self, id: RegionId) -> (f32, f32) {
        let jitter_x = 0.25 + 0.5 * self.unit(id, SALT_CENTER_X);
        let jitter_z = 0.25 + 0.5 * self.unit(id, SALT_CENTER_Z);
        ((id.0 as f32 + jitter_x) * REGION_SIZE, (id.1 as f32 + jitter_z) * REGION_SIZE)
    }

    pub fn sample(&self, x: f32, z: f32) -> RegionSample {
        // Verzerrte Koordinaten für organische Grenzen.
        let wx = x + self.warp_x.get(x, z) * WARP;
        let wz = z + self.warp_z.get(x, z) * WARP;
        let base = ((wx / REGION_SIZE).floor() as i32, (wz / REGION_SIZE).floor() as i32);
        let (mut first, mut second) = ((f32::INFINITY, RegionId(0, 0)), (f32::INFINITY, RegionId(0, 0)));
        for dz in -1..=1 {
            for dx in -1..=1 {
                let id = RegionId(base.0 + dx, base.1 + dz);
                let (cx, cz) = self.center(id);
                let distance = ((cx - wx) * (cx - wx) + (cz - wz) * (cz - wz)).sqrt();
                if distance < first.0 {
                    second = first;
                    first = (distance, id);
                } else if distance < second.0 {
                    second = (distance, id);
                }
            }
        }
        // Halbe Differenz der Abstände ≈ Abstand zur Grenze.
        let to_border = (second.0 - first.0) * 0.5;
        let inside = smooth(to_border / BORDER_BLEND);
        RegionSample {
            id: first.1,
            neighbour: second.1,
            neighbour_weight: 0.5 * (1.0 - inside),
        }
    }

    pub fn theme(&self, id: RegionId) -> RegionTheme {
        match self.unit(id, SALT_THEME) {
            u if u < 0.07 => RegionTheme::Volcanic,
            u if u < 0.13 => RegionTheme::Crystal,
            u if u < 0.19 => RegionTheme::Fungal,
            _ => RegionTheme::Normal,
        }
    }

    /// Anteil eines Themas an einem Punkt (0..1, stetig über Grenzen).
    pub fn theme_weight(&self, sample: &RegionSample, theme: RegionTheme) -> f32 {
        sample.blend(|id| if self.theme(id) == theme { 1.0 } else { 0.0 })
    }

    /// Klima-Tendenz der Region: (Temperatur, Feuchte), je -0,3..0,3.
    pub fn climate_bias(&self, sample: &RegionSample) -> (f32, f32) {
        let temperature = sample.blend(|id| (self.unit(id, SALT_TEMPERATURE) * 2.0 - 1.0) * 0.3);
        let humidity = sample.blend(|id| (self.unit(id, SALT_HUMIDITY) * 2.0 - 1.0) * 0.3);
        (temperature, humidity)
    }

    /// Feste Wahl `0..count` je Region und Zweck (z. B. Variante einer Biom-Familie).
    pub fn pick(&self, id: RegionId, purpose: u64, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        (self.hash(id, SALT_VARIANT + purpose) % count as u64) as usize
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// SplitMix64-Finalizer.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_large_and_borders_blend_continuously() {
        let map = RegionMap::new(42);
        // Entlang einer Linie: Regionswechsel selten (große Regionen), Gewicht stetig.
        let (mut changes, mut last) = (0, map.sample(-20000.0, 1234.0));
        let mut x = -20000.0;
        while x < 20000.0 {
            x += 10.0;
            let sample = map.sample(x, 1234.0);
            assert!((0.0..=0.5).contains(&sample.neighbour_weight));
            if sample.id != last.id {
                changes += 1;
                // An der Grenze zählen beide Regionen etwa gleich.
                assert!(sample.neighbour_weight > 0.45 && last.neighbour_weight > 0.45, "Sprung an der Grenze");
            }
            last = sample;
        }
        // 40 km Strecke, Regionen ≈ 6 km: grob 4-12 Wechsel.
        assert!((3..=14).contains(&changes), "{changes} Regionswechsel auf 40 km");
    }

    #[test]
    fn themes_and_choices_are_deterministic_and_varied() {
        let (a, b) = (RegionMap::new(1), RegionMap::new(1));
        let mut themes = std::collections::HashMap::new();
        for cz in -15..15 {
            for cx in -15..15 {
                let id = RegionId(cx, cz);
                assert_eq!(a.theme(id), b.theme(id));
                assert_eq!(a.pick(id, 7, 20), b.pick(id, 7, 20));
                *themes.entry(a.theme(id)).or_insert(0) += 1;
            }
        }
        for theme in [RegionTheme::Normal, RegionTheme::Volcanic, RegionTheme::Crystal, RegionTheme::Fungal] {
            assert!(themes.get(&theme).copied().unwrap_or(0) > 20, "{theme:?} zu selten: {themes:?}");
        }
        assert_ne!(RegionMap::new(1).center(RegionId(3, 3)), RegionMap::new(2).center(RegionId(3, 3)));
    }
}
