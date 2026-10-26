//! Öffentliche Regionen (#50): große Gebiete (≈ 4-8 km) mit eigener Identität.
//!
//! Jede Region hat
//! - eine feste **Variante pro Biom-Familie** (z. B. die ganze Region ist "Herbstwald"),
//! - eine **Klima-Tendenz** (wärmer/kälter, feuchter/trockener),
//! - ein **Thema**: normal, Vulkan, Kristall oder Pilz.
//!
//! Regionen sind Voronoi-Zellen auf verzerrten Koordinaten (organische Grenzen). An Grenzen
//! werden Themen, Klima und Farben über `BORDER_BLEND` Meter gemischt. Jede nahe Region bekommt
//! ein stetiges Gewicht, das mit dem Abstand zur nächsten Region abnimmt; die Gewichte werden
//! normiert. Auf einer Grenze zweier Regionen ergibt das 50/50, an Dreiländerecken mischen sich
//! alle drei – ohne Sprung (Review-Fund zu #50: mit nur zwei Regionen sprang der Wert dort).
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

/// Kandidaten der 3×3-Suche; so viele Regionen kann ein Punkt höchstens mischen.
const CANDIDATES: usize = 9;

/// Region an einem Punkt und alle Regionen, die dort mitzählen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionSample {
    /// Region, zu der der Punkt gehört (nächster Mittelpunkt).
    pub id: RegionId,
    /// Mitzählende Regionen mit normierten Gewichten (Summe 1), die eigene zuerst.
    weights: [(RegionId, f32); CANDIDATES],
    count: u8,
}

impl RegionSample {
    /// Alle mitzählenden Regionen mit Gewicht; im Inneren nur die eigene mit 1,0.
    pub fn regions(&self) -> &[(RegionId, f32)] {
        &self.weights[..self.count as usize]
    }

    /// Anteil, den fremde Regionen an diesem Punkt haben (0 im Inneren, 0,5 auf einer Grenze).
    pub fn foreign_weight(&self) -> f32 {
        1.0 - self.weights[0].1
    }

    /// Gewichteter Mittelwert einer Eigenschaft über alle mitzählenden Regionen (stetig).
    pub fn blend(&self, mut value: impl FnMut(RegionId) -> f32) -> f32 {
        self.regions().iter().map(|&(id, weight)| value(id) * weight).sum()
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
        let mut candidates = [(RegionId(0, 0), 0.0f32); CANDIDATES];
        let mut index = 0;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let id = RegionId(base.0 + dx, base.1 + dz);
                let (cx, cz) = self.center(id);
                candidates[index] = (id, ((cx - wx) * (cx - wx) + (cz - wz) * (cz - wz)).sqrt());
                index += 1;
            }
        }
        // Grenze der 3×3-Suche: In seltenen Ecklagen könnte ein Mittelpunkt zwei Zellen entfernt
        // noch knapp Gewicht bekommen (≤ 0,5 % nach Normierung). Gemessen über 9 Mio. Punkte nie
        // aufgetreten (Review zu #50); bewusst in Kauf genommen statt einer 5×5-Suche.
        // Nächste zuerst; bei gleichem Abstand entscheidet die Region (deterministisch).
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let nearest = candidates[0].1;
        // Halbe Abstandsdifferenz ≈ Abstand zur Grenze mit der nächsten Region. Gewicht 1 auf der
        // Grenze, 0 ab `BORDER_BLEND` dahinter; stetig in der Position.
        let mut weights = [(RegionId(0, 0), 0.0f32); CANDIDATES];
        let (mut count, mut total) = (0usize, 0.0f32);
        for (id, distance) in candidates {
            let weight = smooth(1.0 - (distance - nearest) * 0.5 / BORDER_BLEND);
            if weight > 0.0 {
                weights[count] = (id, weight);
                count += 1;
                total += weight;
            }
        }
        for entry in &mut weights[..count] {
            entry.1 /= total;
        }
        RegionSample {
            id: candidates[0].0,
            weights,
            count: count as u8,
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
            assert!((0.0..=0.67).contains(&sample.foreign_weight()));
            let total: f32 = sample.regions().iter().map(|r| r.1).sum();
            assert!((total - 1.0).abs() < 1e-4, "Gewichte nicht normiert");
            if sample.id != last.id {
                changes += 1;
                // An der Grenze zählt die eigene Region höchstens etwa halb.
                assert!(sample.foreign_weight() > 0.45 && last.foreign_weight() > 0.45, "Sprung an der Grenze");
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
