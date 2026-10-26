//! Erz-Regeln (#45, D-013): welches Erz in welcher Form, in welchem Gestein und welcher Tiefe.
//!
//! Bewusst als typisierte Tabelle im Code (statt RON): leicht zu testen, und die Regeln sind
//! nicht geheim – geheim sind nur die Positionen (Provinzen, Noise aus dem geheimen Seed).

use crate::material::MaterialId;

/// Erzart. Die Reihenfolge in `ALL` ist zugleich die Priorität, wenn sich Vorkommen überlappen
/// (Seltenes zuerst).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OreKind {
    GoldSand,
    Gold,
    Silver,
    Cinnabar,
    Quartz,
    Obsidian,
    Sulfur,
    Magnetite,
    Tin,
    Copper,
    Iron,
    Coal,
}

impl OreKind {
    pub const ALL: [OreKind; 12] = [
        OreKind::GoldSand,
        OreKind::Gold,
        OreKind::Silver,
        OreKind::Cinnabar,
        OreKind::Quartz,
        OreKind::Obsidian,
        OreKind::Sulfur,
        OreKind::Magnetite,
        OreKind::Tin,
        OreKind::Copper,
        OreKind::Iron,
        OreKind::Coal,
    ];

    /// Grundmetalle: jede Provinz hat mindestens eines (D-013).
    pub const BASE_METALS: [OreKind; 3] = [OreKind::Iron, OreKind::Copper, OreKind::Tin];

    pub fn rule(self) -> &'static OreRule {
        &RULES[self as usize]
    }

    pub fn material(self) -> MaterialId {
        self.rule().material
    }

    pub fn from_material(material: MaterialId) -> Option<OreKind> {
        OreKind::ALL.into_iter().find(|kind| kind.material() == material)
    }
}

/// Form eines Vorkommens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Form {
    /// Klumpen: 3D-Noise über einer Schwelle. `y_stretch` > 1 macht flache Linsen.
    Nest { frequency: f32, y_stretch: f32, threshold: f32 },
    /// Ader: dünne Platte entlang der Null-Fläche einer Noise (`|a| < width`), begrenzt durch
    /// eine zweite Noise (`b > extent`).
    Vein { frequency: f32, width: f32, extent: f32 },
    /// Seife: vergrabene Goldsand-Linsen unter Flussbetten (eigene Logik in `ores.rs`).
    Placer,
}

#[derive(Clone, Copy, Debug)]
pub struct OreRule {
    pub kind: OreKind,
    pub material: MaterialId,
    pub form: Form,
    /// Gestein (öffentliches Material), in dem das Erz vorkommt.
    pub hosts: &'static [MaterialId],
    /// Mindesttiefe unter der eigenen Oberfläche.
    pub min_depth: f32,
    /// Erlaubter Höhenbereich (Welt-y).
    pub y_range: (f32, f32),
}

use MaterialId as M;

const SEDIMENTS: &[MaterialId] = &[M::SANDSTONE, M::SLATE, M::LIMESTONE];

/// Index = `OreKind as usize`; ein Test prüft die Zuordnung.
const RULES: [OreRule; 12] = [
    OreRule {
        kind: OreKind::GoldSand,
        material: MaterialId(36),
        form: Form::Placer,
        hosts: &[M::SANDSTONE, M::SLATE, M::LIMESTONE, M::GRANITE, M::BASALT, M::MARBLE, M::GRAVEL, M::SAND, M::CLAY],
        min_depth: 0.0,
        y_range: (-60.0, 120.0),
    },
    OreRule {
        kind: OreKind::Gold,
        material: MaterialId(35),
        form: Form::Vein { frequency: 0.016, width: 0.006, extent: 0.42 },
        hosts: &[M::GRANITE, M::SLATE, M::MARBLE],
        min_depth: 10.0,
        y_range: (-158.0, 60.0),
    },
    OreRule {
        kind: OreKind::Silver,
        material: MaterialId(34),
        form: Form::Vein { frequency: 0.018, width: 0.010, extent: 0.30 },
        hosts: &[M::SLATE, M::GRANITE, M::LIMESTONE],
        min_depth: 8.0,
        y_range: (-158.0, 120.0),
    },
    OreRule {
        kind: OreKind::Cinnabar,
        material: MaterialId(54),
        form: Form::Vein { frequency: 0.022, width: 0.020, extent: 0.25 },
        hosts: &[M::LIMESTONE, M::SLATE, M::BASALT],
        min_depth: 6.0,
        y_range: (-120.0, 160.0),
    },
    OreRule {
        kind: OreKind::Quartz,
        material: MaterialId(50),
        form: Form::Vein { frequency: 0.020, width: 0.017, extent: 0.18 },
        hosts: &[M::GRANITE, M::SANDSTONE, M::MARBLE],
        min_depth: 6.0,
        y_range: (-158.0, 220.0),
    },
    OreRule {
        kind: OreKind::Obsidian,
        material: MaterialId(51),
        form: Form::Nest { frequency: 0.032, y_stretch: 1.0, threshold: 0.51 },
        hosts: &[M::BASALT],
        min_depth: 4.0,
        y_range: (-158.0, 220.0),
    },
    OreRule {
        kind: OreKind::Sulfur,
        material: MaterialId(52),
        form: Form::Nest { frequency: 0.040, y_stretch: 1.0, threshold: 0.52 },
        hosts: &[M::BASALT, M::LIMESTONE],
        min_depth: 4.0,
        y_range: (-40.0, 220.0),
    },
    OreRule {
        kind: OreKind::Magnetite,
        material: MaterialId(53),
        form: Form::Nest { frequency: 0.032, y_stretch: 1.0, threshold: 0.575 },
        hosts: &[M::GRANITE, M::BASALT],
        min_depth: 8.0,
        y_range: (-158.0, 220.0),
    },
    OreRule {
        kind: OreKind::Tin,
        material: MaterialId(32),
        form: Form::Nest { frequency: 0.035, y_stretch: 1.0, threshold: 0.54 },
        hosts: &[M::GRANITE, M::MARBLE],
        min_depth: 8.0,
        y_range: (-158.0, 200.0),
    },
    OreRule {
        kind: OreKind::Copper,
        material: MaterialId(31),
        form: Form::Nest { frequency: 0.030, y_stretch: 1.2, threshold: 0.49 },
        hosts: &[M::LIMESTONE, M::SANDSTONE, M::GRANITE, M::BASALT],
        min_depth: 6.0,
        y_range: (-150.0, 200.0),
    },
    OreRule {
        kind: OreKind::Iron,
        material: MaterialId(33),
        form: Form::Nest { frequency: 0.025, y_stretch: 1.5, threshold: 0.48 },
        hosts: &[M::SANDSTONE, M::SLATE, M::BASALT, M::LIMESTONE],
        min_depth: 6.0,
        y_range: (-150.0, 200.0),
    },
    OreRule {
        kind: OreKind::Coal,
        material: MaterialId(30),
        form: Form::Nest { frequency: 0.028, y_stretch: 2.2, threshold: 0.46 },
        hosts: SEDIMENTS,
        min_depth: 6.0,
        y_range: (-130.0, 200.0),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{MaterialCategory, MaterialRegistry};

    #[test]
    fn rules_are_indexed_by_kind_and_use_ore_materials() {
        let registry = MaterialRegistry::load();
        for kind in OreKind::ALL {
            let rule = kind.rule();
            assert_eq!(rule.kind, kind, "RULES-Reihenfolge passt nicht zu OreKind");
            let material = registry.get(rule.material).unwrap_or_else(|| panic!("{kind:?}: Material fehlt"));
            assert!(
                matches!(material.category, MaterialCategory::Ore | MaterialCategory::Special),
                "{kind:?}: {} ist kein Erz",
                material.key
            );
            assert!(!rule.hosts.is_empty());
            for host in rule.hosts {
                let host = registry.get(*host).unwrap();
                assert!(
                    matches!(host.category, MaterialCategory::Rock | MaterialCategory::Soil),
                    "{kind:?}: Wirt {} ist kein Gestein/Boden",
                    host.key
                );
            }
            assert!(rule.y_range.0 < rule.y_range.1);
            assert_eq!(OreKind::from_material(rule.material), Some(kind));
        }
    }

    /// Chunks unterhalb von `CAVE_MIN_Y` werden ohne Untergrund erzeugt. Reicht ein Erz bis an die
    /// Grenze, sehen die beiden Chunks an der gemeinsamen Ebene verschiedene Materialien
    /// (Fund aus dem Review zu #45).
    #[test]
    fn ores_stay_above_the_cave_floor() {
        for kind in OreKind::ALL {
            assert!(kind.rule().y_range.0 > crate::underground::CAVE_MIN_Y, "{kind:?} reicht bis an CAVE_MIN_Y");
        }
    }

    #[test]
    fn every_ore_and_special_material_has_a_rule() {
        let registry = MaterialRegistry::load();
        for material in registry.iter() {
            if matches!(material.category, MaterialCategory::Ore | MaterialCategory::Special) {
                assert!(OreKind::from_material(MaterialId(material.id)).is_some(), "{} ohne Regel", material.key);
            }
        }
    }
}
