//! Inventar-Anzeige am `TerraforgeTerrain` (#18): der letzte Stand vom Server bzw. aus dem
//! lokalen Inventar ohne Server (`dig.rs`).

use godot::prelude::*;
use terraforge_core::MaterialId;
use terraforge_core::material::TraceElement;

use crate::terrain::TerraforgeTerrain;

#[godot_api(secondary)]
impl TerraforgeTerrain {
    /// Die belegten Inventarplätze in Reihenfolge, je ein Dictionary: `slot` (feste Nummer, leere
    /// Plätze fehlen), `name`, `key`, `count`, `hardness`,
    /// `density`, `toughness`, `conductivity`, `purity` (−1 ohne Erz), `traces` (Name → %),
    /// `details` (fertiger Text für die Anzeige).
    #[func]
    fn get_inventory(&self) -> Array<VarDictionary> {
        let Some(world) = &self.world else {
            return Array::new();
        };
        self.edits
            .stacks
            .iter()
            .map(|stack| {
                let material = world.materials.get(MaterialId(stack.material));
                let [hardness, density, toughness, conductivity] = stack.properties;
                let mut entry = VarDictionary::new();
                entry.set("slot", i64::from(stack.slot));
                entry.set("name", material.map_or("?", |m| m.name.as_str()));
                entry.set("key", material.map_or("?", |m| m.key.as_str()));
                entry.set("count", i64::from(stack.count));
                entry.set("hardness", hardness);
                entry.set("density", density);
                entry.set("toughness", toughness);
                entry.set("conductivity", conductivity);
                let mut details = format!("Härte {hardness:.1} · {density:.2} g/cm³ · Zähigkeit {toughness:.2} · {conductivity:.1} W/mK");
                let mut traces = VarDictionary::new();
                let purity = match &stack.ore {
                    Some((purity, elements)) => {
                        details += &format!("\nReinheit {:.0} %", purity * 100.0);
                        for &(id, percent) in elements {
                            let name = TraceElement::from_id(id).map_or("?", TraceElement::name);
                            traces.set(name, percent);
                            details += &format!(" · {name} {percent:.2} %");
                        }
                        *purity
                    }
                    None => -1.0,
                };
                entry.set("purity", purity);
                entry.set("traces", &traces);
                entry.set("details", details.as_str());
                entry
            })
            .collect()
    }
}
