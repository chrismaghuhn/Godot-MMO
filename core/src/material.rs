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
}

#[derive(Clone, Debug, Deserialize)]
pub struct Material {
    pub id: u16,
    /// Stabiler Schlüssel für Code und Daten, z. B. `"iron_ore"`.
    pub key: String,
    /// Anzeigename.
    pub name: String,
    pub category: MaterialCategory,
    /// sRGB.
    pub color: [u8; 3],
    /// Grob nach Mohs (1-10).
    pub hardness: f32,
    /// g/cm³.
    pub density: f32,
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
            assert!((1.0..=10.0).contains(&material.hardness), "{}: Härte", material.key);
            assert!(material.density > 0.0, "{}: Dichte", material.key);
        }
    }

    fn entry(id: u16, key: &str) -> String {
        format!(r#"(id: {id}, key: "{key}", name: "x", category: Rock, color: (1, 2, 3), hardness: 5.0, density: 2.0)"#)
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
