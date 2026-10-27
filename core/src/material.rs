//! Materialien: was ein Voxel ist (Erde, Granit, Eisenerz, …), datengetrieben aus
//! `assets/materials.ron` (D-013).
//!
//! IDs sind dauerhaft (Speicherung, Netzwerk) und stehen deshalb fest in der Datei.
//! ID 0 ist Luft und gehört zu keinem Eintrag.

use std::collections::HashSet;

use serde::Deserialize;

/// Material eines Voxels. `u16`, damit später auch von Spielern gebaute Materialien Platz haben.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(pub u16);

impl MaterialId {
    /// Luft: jedes Sample mit Dichte <= 0.
    pub const AIR: MaterialId = MaterialId(0);
    /// Allgemeines Gestein (Platzhalter aus #43, vom Generator nicht mehr verwendet).
    pub const STONE: MaterialId = MaterialId(1);

    // Vom öffentlichen Generator verwendete Materialien (#44). Ein Test prüft, dass jede
    // Konstante zum Schlüssel in `materials.ron` passt.
    pub const DIRT: MaterialId = MaterialId(2);
    pub const SAND: MaterialId = MaterialId(3);
    pub const GRAVEL: MaterialId = MaterialId(4);
    pub const CLAY: MaterialId = MaterialId(5);
    pub const SNOW: MaterialId = MaterialId(6);
    pub const SANDSTONE: MaterialId = MaterialId(10);
    pub const LIMESTONE: MaterialId = MaterialId(11);
    pub const SLATE: MaterialId = MaterialId(12);
    pub const GRANITE: MaterialId = MaterialId(13);
    pub const BASALT: MaterialId = MaterialId(14);
    pub const MARBLE: MaterialId = MaterialId(15);
    pub const QUARTZ: MaterialId = MaterialId(50);
    /// Aus gefällten Bäumen (#18).
    pub const WOOD: MaterialId = MaterialId(60);

    pub fn is_air(self) -> bool {
        self == Self::AIR
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum MaterialCategory {
    Soil,
    Rock,
    Ore,
    Special,
    /// Pflanzlich, aus Bäumen (#18).
    Wood,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Material {
    pub id: u16,
    /// Stabiler Schlüssel für Code und Daten, z. B. `"iron_ore"`.
    pub key: String,
    /// Anzeigename.
    pub name: String,
    pub category: MaterialCategory,
    /// Natürliche Grundfarbe (sRGB).
    pub color: [u8; 3],
    /// Akzentfarbe für Sprenkel/Glanz (sRGB), vor allem für Erze, damit sie sich vom
    /// Gestein abheben. Im Viewer eingemischt, später als Shader-Muster.
    #[serde(default)]
    pub accent: Option<[u8; 3]>,
    /// Grob nach Mohs (1-10).
    pub hardness: f32,
    /// g/cm³.
    pub density: f32,
    /// Zähigkeit: 0 spröde (zerbricht) bis 1 zäh (verformt sich).
    pub toughness: f32,
    /// Wärmeleitfähigkeit in W/(m·K).
    pub conductivity: f32,
    /// Mögliche Spurenelemente mit höchstem Anteil in % des gewonnenen Metalls (bei Kohle: des
    /// Brennstoffs). Wie viel eine Provinz davon hat, ist geheim (D-013, `underground`).
    #[serde(default)]
    pub traces: Vec<(TraceElement, f32)>,
}

/// Spurenelemente in Erzen (#15). Wichtig fürs Verhütten und Schmieden (D-019): Phosphor macht
/// Eisen kaltbrüchig, Schwefel rotbrüchig, Nickel und Mangan zäher, Arsen härtet Kupfer, Silber
/// macht Gold blasser.
///
/// Die Zahlen sind dauerhaft (gehen in Hashes und später in Speicherstände ein): nie ändern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
pub enum TraceElement {
    Phosphorus = 1,
    Sulfur = 2,
    Manganese = 3,
    Nickel = 4,
    Arsenic = 5,
    Silver = 6,
    Copper = 7,
    Antimony = 8,
    Lead = 9,
}

impl TraceElement {
    pub const ALL: [TraceElement; 9] = [
        TraceElement::Phosphorus,
        TraceElement::Sulfur,
        TraceElement::Manganese,
        TraceElement::Nickel,
        TraceElement::Arsenic,
        TraceElement::Silver,
        TraceElement::Copper,
        TraceElement::Antimony,
        TraceElement::Lead,
    ];

    /// Zu einer festen Nummer (`element as u8`), z. B. aus dem Netz.
    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|element| *element as u8 == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            TraceElement::Phosphorus => "Phosphor",
            TraceElement::Sulfur => "Schwefel",
            TraceElement::Manganese => "Mangan",
            TraceElement::Nickel => "Nickel",
            TraceElement::Arsenic => "Arsen",
            TraceElement::Silver => "Silber",
            TraceElement::Copper => "Kupfer",
            TraceElement::Antimony => "Antimon",
            TraceElement::Lead => "Blei",
        }
    }
}

#[derive(Deserialize)]
struct MaterialFile {
    materials: Vec<Material>,
}

#[derive(Clone, Debug)]
pub struct MaterialRegistry {
    /// Index = ID; `None` für Lücken und für Luft.
    by_id: Vec<Option<Material>>,
}

impl MaterialRegistry {
    /// Die eingebaute Materialliste.
    pub fn load() -> Self {
        Self::parse(include_str!("../assets/materials.ron"))
            .unwrap_or_else(|error| panic!("assets/materials.ron ist ungültig: {error}"))
    }

    /// Parst und prüft eine Materialliste.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: MaterialFile = ron::from_str(text).map_err(|error| error.to_string())?;
        let mut keys = HashSet::new();
        let mut by_id: Vec<Option<Material>> = Vec::new();
        for material in file.materials {
            if material.id == MaterialId::AIR.0 {
                return Err(format!("ID 0 ist für Luft reserviert ({})", material.key));
            }
            if !keys.insert(material.key.clone()) {
                return Err(format!("Schlüssel doppelt: {}", material.key));
            }
            let index = material.id as usize;
            if by_id.len() <= index {
                by_id.resize(index + 1, None);
            }
            if let Some(existing) = &by_id[index] {
                return Err(format!("ID {} doppelt: {} und {}", material.id, existing.key, material.key));
            }
            by_id[index] = Some(material);
        }
        let registry = Self { by_id };
        if registry.get(MaterialId::STONE).map(|m| m.key.as_str()) != Some("stone") {
            return Err("ID 1 muss \"stone\" sein (MaterialId::STONE)".into());
        }
        Ok(registry)
    }

    /// Material zu einer ID; `None` für Luft und unbekannte IDs.
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.by_id.get(id.0 as usize)?.as_ref()
    }

    pub fn by_key(&self, key: &str) -> Option<MaterialId> {
        self.iter().find(|material| material.key == key).map(|material| MaterialId(material.id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.by_id.iter().flatten()
    }

    /// Farbe zu einer ID (sRGB). Unbekannte IDs werden auffällig magenta dargestellt.
    pub fn color(&self, id: MaterialId) -> [u8; 3] {
        self.get(id).map_or([255, 0, 255], |material| material.color)
    }

    /// Farbe für die Darstellung mit Vertex-Farben: Grundfarbe, bei Materialien mit
    /// Akzent zur Hälfte mit der Akzentfarbe gemischt.
    pub fn display_color(&self, id: MaterialId) -> [u8; 3] {
        let Some(material) = self.get(id) else {
            return [255, 0, 255];
        };
        match material.accent {
            Some(accent) => std::array::from_fn(|i| ((material.color[i] as u16 + accent[i] as u16) / 2) as u8),
            None => material.color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_list_loads_and_contains_the_planned_materials() {
        let registry = MaterialRegistry::load();
        // D-013: Startliste.
        for key in [
            "stone", "dirt", "sand", "gravel", "snow", "sandstone", "limestone", "slate", "granite", "basalt",
            "coal", "copper_ore", "tin_ore", "iron_ore", "silver_ore", "gold_ore", "gold_sand", "quartz",
            "obsidian", "sulfur", "magnetite", "cinnabar",
        ] {
            assert!(registry.by_key(key).is_some(), "Material fehlt: {key}");
        }
        assert_eq!(registry.by_key("stone"), Some(MaterialId::STONE));
        assert!(registry.get(MaterialId::AIR).is_none());
        for category in [MaterialCategory::Soil, MaterialCategory::Rock, MaterialCategory::Ore, MaterialCategory::Special] {
            assert!(registry.iter().any(|material| material.category == category), "{category:?} fehlt");
        }
        for material in registry.iter() {
            if matches!(material.category, MaterialCategory::Ore | MaterialCategory::Special) {
                assert!(material.accent.is_some(), "{}: Erz ohne Akzentfarbe", material.key);
            }
            assert!((1.0..=10.0).contains(&material.hardness), "{}: Härte", material.key);
            assert!(material.density > 0.0, "{}: Dichte", material.key);
            assert!((0.0..=1.0).contains(&material.toughness), "{}: Zähigkeit", material.key);
            assert!(material.conductivity > 0.0, "{}: Wärmeleitfähigkeit", material.key);
            for &(element, percent) in &material.traces {
                assert!(percent > 0.0 && percent < 50.0, "{}: {element:?}", material.key);
            }
        }
    }

    fn entry(id: u16, key: &str) -> String {
        format!(r#"(id: {id}, key: "{key}", name: "x", category: Rock, color: (1, 2, 3), hardness: 5.0, density: 2.0, toughness: 0.5, conductivity: 2.0)"#)
    }

    fn list(entries: &[String]) -> String {
        format!("(materials: [{}])", entries.join(","))
    }

    #[test]
    fn validation_rejects_broken_lists() {
        let stone = entry(1, "stone");
        assert!(MaterialRegistry::parse(&list(&[stone.clone(), entry(2, "a")])).is_ok());
        assert!(MaterialRegistry::parse(&list(&[stone.clone(), entry(0, "luft")])).is_err(), "ID 0");
        assert!(MaterialRegistry::parse(&list(&[stone.clone(), entry(2, "a"), entry(2, "b")])).is_err(), "ID doppelt");
        assert!(MaterialRegistry::parse(&list(&[stone.clone(), entry(2, "a"), entry(3, "a")])).is_err(), "Schlüssel doppelt");
        assert!(MaterialRegistry::parse(&list(&[entry(1, "granite")])).is_err(), "ID 1 ist nicht stone");
        assert!(MaterialRegistry::parse("kein ron").is_err());
    }

    #[test]
    fn constants_match_the_material_list() {
        let registry = MaterialRegistry::load();
        for (constant, key) in [
            (MaterialId::STONE, "stone"),
            (MaterialId::DIRT, "dirt"),
            (MaterialId::SAND, "sand"),
            (MaterialId::GRAVEL, "gravel"),
            (MaterialId::CLAY, "clay"),
            (MaterialId::SNOW, "snow"),
            (MaterialId::SANDSTONE, "sandstone"),
            (MaterialId::LIMESTONE, "limestone"),
            (MaterialId::SLATE, "slate"),
            (MaterialId::GRANITE, "granite"),
            (MaterialId::BASALT, "basalt"),
            (MaterialId::MARBLE, "marble"),
            (MaterialId::QUARTZ, "quartz"),
            (MaterialId::WOOD, "wood"),
        ] {
            assert_eq!(registry.by_key(key), Some(constant), "Konstante für {key} passt nicht");
        }
    }

    #[test]
    fn unknown_ids_are_visible_not_silent() {
        let registry = MaterialRegistry::load();
        assert!(registry.get(MaterialId(999)).is_none());
        assert_eq!(registry.color(MaterialId(999)), [255, 0, 255]);
    }
}
