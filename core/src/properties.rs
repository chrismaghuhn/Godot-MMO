//! Materialeigenschaften beim Abbau (#15, D-022): Grundwerte aus `materials.ron`, abgewandelt nach
//! Ort. Derselbe Granit ist in einer anderen Region etwas härter oder leichter, tiefer liegendes
//! Gestein etwas dichter. Alles deterministisch aus dem Seed, damit Server und Client gleich rechnen.
//!
//! Hier steht nur der öffentliche Teil (Region, Tiefe). Erze bekommen dazu Reinheit, Provinz und
//! Spurenelemente aus dem geheimen Untergrund (`underground::Underground::mined_sample`, D-013);
//! für Erze und Spezialmaterialien sind die Werte dieses Moduls allein deshalb nicht maßgeblich.
//!
//! Die Werte gelten für *natürlich* gewachsenes Material am Fundort. Gebautes Material behält die
//! Werte des Items, aus dem es gebaut wurde (#18); sonst ließe sich Erz durch Ablegen in einer
//! reichen Provinz veredeln (Review-Fund).
//!
//! Ob ein Werkzeug ein Material schafft, entscheidet weiter die Grundhärte aus `materials.ron`
//! (`edits::action`), nicht die abgewandelte: Sonst hinge es vom Ort ab, ob die Schaufel Kalkstein
//! schafft, ohne dass der Spieler das sieht.

use glam::IVec3;

use crate::material::{Material, MaterialId, TraceElement};
use crate::noise::Layer;
use crate::regions::RegionId;
use crate::world::WorldGen;

/// Höchste Abweichung je Region (±), als Anteil des Grundwerts.
const REGIONAL: f32 = 0.10;
/// Höchste Abweichung innerhalb einer Region (±, Noise mit ≈ 200 m Wellenlänge).
const LOCAL: f32 = 0.04;
/// Zunahme von Härte und Dichte je 100 m Tiefe (Verdichtung), höchstens bis 200 m.
const DEPTH_HARDNESS: f32 = 0.03;
const DEPTH_DENSITY: f32 = 0.02;
/// Salz für die Werte je Region, Material und Eigenschaft (getrennt von den Salzen in `regions`).
const SALT: u64 = 1 << 32;

/// Noise für die Abweichung innerhalb einer Region (≈ 200 m Wellenlänge), Teil von `WorldGen`.
pub(crate) struct PropertyNoise(Layer);

impl PropertyNoise {
    pub(crate) fn new(seed: u32) -> Self {
        Self(Layer::new(seed as i32 ^ 0x005E_ED15, 1.0 / 200.0, 2))
    }
}

/// Physikalische Werte eines abgebauten Stücks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Properties {
    /// Grob nach Mohs (1-10).
    pub hardness: f32,
    /// g/cm³.
    pub density: f32,
    /// 0 spröde bis 1 zäh.
    pub toughness: f32,
    /// W/(m·K).
    pub conductivity: f32,
}

impl Properties {
    pub fn of(material: &Material) -> Self {
        Self { hardness: material.hardness, density: material.density, toughness: material.toughness, conductivity: material.conductivity }
    }

    /// Mischung `self` → `other` mit Anteil `t` (0..1).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            hardness: mix(self.hardness, other.hardness),
            density: mix(self.density, other.density),
            toughness: mix(self.toughness, other.toughness),
            conductivity: mix(self.conductivity, other.conductivity),
        }
    }
}

/// Ein abgebautes Stück Material mit Herkunft und Werten (Grundlage für Items, #18, und Schmieden,
/// D-019).
#[derive(Clone, Debug, PartialEq)]
pub struct MinedSample {
    pub material: MaterialId,
    /// Region der Fundstelle (öffentlich, D-014).
    pub region: RegionId,
    pub properties: Properties,
    /// Nur bei Erzen: Reinheit 0,05..1, Provinz und Spurenelemente in % (geheim, vom Server).
    pub ore: Option<OreSample>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OreSample {
    pub purity: f32,
    /// Undurchsichtige Kennung der Provinz (Hash mit geheimem Seed): Gleiche Herkunft ist
    /// erkennbar, Lage und Grenzen der Provinzen nicht (Review-Fund).
    pub province: u64,
    /// Spurenelemente in % des gewonnenen Metalls (bei Kohle: des Brennstoffs).
    pub traces: Vec<(TraceElement, f32)>,
}

impl WorldGen {
    /// Werte eines abgebauten Voxels aus öffentlichen Daten: Grundwerte, abgewandelt nach Region,
    /// Ort und Tiefe unter der geplanten Oberfläche. Ohne Erz-Daten (`ore` ist `None`).
    pub fn mined_sample(&self, voxel: IVec3, material: MaterialId) -> MinedSample {
        let base = self.materials.get(material).map(Properties::of).unwrap_or(Properties { hardness: 1.0, density: 1.0, toughness: 0.0, conductivity: 0.1 });
        self.vary(voxel, material, base)
    }

    /// Wandelt Werte nach Ort ab (öffentlich). Auch für Erze, deren Grundwerte der Server vorher aus
    /// der Reinheit mischt.
    pub fn vary(&self, voxel: IVec3, material: MaterialId, base: Properties) -> MinedSample {
        let (x, z) = (voxel.x as f32, voxel.z as f32);
        let region = self.regions.sample(x, z);
        // Je Region und Eigenschaft ein fester Faktor, an Grenzen stetig gemischt.
        let factor = |property: u64| {
            let salt = SALT + u64::from(material.0) * 8 + property;
            let regional = region.blend(|id| self.regions.unit(id, salt) * 2.0 - 1.0) * REGIONAL;
            let local = self.property_noise.0.get(x + property as f32 * 977.0, z - u64::from(material.0) as f32 * 613.0) * LOCAL;
            1.0 + regional + local
        };
        let depth = ((self.column_height(x, z) - voxel.y as f32) / 100.0).clamp(0.0, 2.0);
        let properties = Properties {
            hardness: (base.hardness * factor(0) * (1.0 + DEPTH_HARDNESS * depth)).clamp(1.0, 10.0),
            density: base.density * factor(1) * (1.0 + DEPTH_DENSITY * depth),
            toughness: (base.toughness * factor(2)).clamp(0.0, 1.0),
            conductivity: base.conductivity * factor(3),
        };
        MinedSample { material, region: region.id, properties, ore: None }
    }
}

impl MinedSample {
    /// Kurzer Text für Debug-Anzeigen, z. B. „Eisenerz: Härte 5,1 · 3,9 g/cm³ · Zähigkeit 0,52 ·
    /// 4,1 W/mK · Reinheit 48 % · Phosphor 0,21 %“.
    pub fn describe(&self, world: &WorldGen) -> String {
        let p = &self.properties;
        let name = world.materials.get(self.material).map_or("?", |m| m.name.as_str());
        let mut text = format!(
            "{name}: Härte {:.1} · {:.2} g/cm³ · Zähigkeit {:.2} · {:.1} W/mK",
            p.hardness, p.density, p.toughness, p.conductivity
        );
        if let Some(ore) = &self.ore {
            text += &format!(" · Reinheit {:.0} %", ore.purity * 100.0);
            for (element, percent) in &ore.traces {
                text += &format!(" · {} {percent:.2} %", element.name());
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface_voxel(world: &WorldGen, x: i32, z: i32, depth: f32) -> IVec3 {
        IVec3::new(x, (world.column_height(x as f32, z as f32) - depth).floor() as i32, z)
    }

    #[test]
    fn samples_are_deterministic_and_stay_near_the_base_values() {
        let (a, b) = (WorldGen::new(42), WorldGen::new(42));
        let granite = Properties::of(a.materials.get(MaterialId::GRANITE).unwrap());
        for i in 0..200 {
            let voxel = surface_voxel(&a, i * 173 - 17_000, i * 91 - 9_000, 5.0);
            let sample = a.mined_sample(voxel, MaterialId::GRANITE);
            assert_eq!(sample, b.mined_sample(voxel, MaterialId::GRANITE));
            let p = sample.properties;
            // Tiefe 5 m: höchstens +0,15 % (Härte) bzw. +0,1 % (Dichte) dazu.
            let (high, low) = (1.0 + REGIONAL + LOCAL + 0.002, 1.0 - REGIONAL - LOCAL);
            for (value, base) in [(p.hardness, granite.hardness), (p.density, granite.density), (p.toughness, granite.toughness), (p.conductivity, granite.conductivity)] {
                assert!((low..=high).contains(&(value / base)), "{p:?} bei {voxel}");
            }
        }
    }

    /// „Gleiches Material, andere Region → andere Werte“ (#15); dicht beieinander aber fast gleich.
    #[test]
    fn values_differ_between_regions_but_change_smoothly() {
        let world = WorldGen::new(42);
        let mut regions = std::collections::HashMap::new();
        for i in 0..400 {
            let voxel = surface_voxel(&world, i * 211 - 40_000, (i * 7919) % 30_000 - 15_000, 5.0);
            let sample = world.mined_sample(voxel, MaterialId::GRANITE);
            regions.entry(sample.region).or_insert(sample.properties.hardness);
        }
        let hardness: Vec<f32> = regions.values().copied().collect();
        let (min, max) = hardness.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        assert!(regions.len() >= 10 && max - min > 0.6, "{} Regionen, Härte {min}..{max}", regions.len());
        // An Regionsgrenzen (fremder Anteil ≈ 0,5) springt kein Wert von Meter zu Meter.
        let borders: Vec<(i32, i32)> = (0..20_000)
            .map(|i| ((i * 37) % 40_000 - 20_000, (i * 53) % 40_000 - 20_000))
            .filter(|&(x, z)| (world.regions.sample(x as f32, z as f32).foreign_weight() - 0.5).abs() < 0.02)
            .take(30)
            .collect();
        assert!(borders.len() >= 10, "nur {} Grenzpunkte gefunden", borders.len());
        for (x, z) in borders {
            let mut previous: Option<Properties> = None;
            for dx in -20..=20 {
                let p = world.mined_sample(IVec3::new(x + dx, 0, z), MaterialId::GRANITE).properties;
                if let Some(q) = previous {
                    assert!((p.hardness - q.hardness).abs() < 0.03 && (p.density - q.density).abs() < 0.01, "Sprung bei ({}, {z}): {q:?} → {p:?}", x + dx);
                }
                previous = Some(p);
            }
        }
    }

    #[test]
    fn deeper_rock_is_denser() {
        let world = WorldGen::new(42);
        let (mut shallow, mut deep) = (0.0, 0.0);
        for i in 0..50 {
            let (x, z) = (i * 37, i * 53);
            shallow += world.mined_sample(surface_voxel(&world, x, z, 2.0), MaterialId::BASALT).properties.density;
            deep += world.mined_sample(surface_voxel(&world, x, z, 150.0), MaterialId::BASALT).properties.density;
        }
        assert!(deep > shallow * 1.02, "flach {shallow}, tief {deep}");
    }
}
