//! Spielstand (#19): Weltänderungen als Delta auf den Seed, dazu Inventar, gebaute Voxel mit ihrem
//! Item und gefällte Bäume. Nur der Server speichert; die Datei enthält auch Geheimes (gegrabene
//! Erze, Reinheit), sie gehört nie zum Client (D-005).
//!
//! Format: eine Datei je Welt, `postcard` mit Versionsnummer. Die Voxel-Änderungen stehen je Chunk
//! (`ChunkRecord`), damit sie sich später als Zeilen in eine Datenbank übernehmen lassen (#27).
//! Geschrieben wird in eine Hilfsdatei, die danach umbenannt wird: Ein Absturz beim Schreiben lässt
//! den alten Stand stehen.
//!
//! Ein Spielstand gehört zu genau einer Welt: öffentlicher Seed und Kennung des geheimen Seeds
//! müssen passen, sonst wird er nicht geladen (sonst lägen Gruben und Erze einer fremden Welt hier).

use std::path::{Path, PathBuf};
use std::io::Write;
use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use glam::IVec3;
use serde::{Deserialize, Serialize};
use terraforge_core::MaterialId;
use terraforge_core::edits::Voxel;
use terraforge_core::inventory::{Item, Stack};
use terraforge_core::material::TraceElement;
use terraforge_core::properties::{OreSample, Properties};
use terraforge_core::vegetation::TreeId;

use crate::ServerWorld;

/// Version des Formats; ältere Dateien werden (noch) nicht umgewandelt.
const VERSION: u32 = 1;
const FILE: &str = "world.tfsave";

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct SaveFile {
    version: u32,
    seed: u32,
    secret: u64,
    /// Materialnummern und ihre Schlüssel beim Speichern: Wird ein Material später umnummeriert,
    /// lädt der Spielstand nicht still mit falschen Materialien (neue dürfen dazukommen).
    materials: Vec<(u16, String)>,
    chunks: Vec<ChunkRecord>,
    felled: Vec<(i32, i32, u16)>,
    inventory: Vec<(u8, ItemRecord, u32)>,
    placed: Vec<([i32; 3], ItemRecord)>,
}

/// Geänderte Voxel eines Chunks: Position im Chunk (lokal 0..32 je Achse), Dichte, Material.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct ChunkRecord {
    chunk: [i32; 3],
    voxels: Vec<([u8; 3], f32, u16)>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct ItemRecord {
    material: u16,
    origin: u64,
    properties: [f32; 4],
    ore: Option<OreRecord>,
}

/// Reinheit, Provinz-Kennung, Spurenelemente (Nummer, Anteil in %).
type OreRecord = (f32, u64, Vec<(u8, f32)>);

impl From<&Item> for ItemRecord {
    fn from(item: &Item) -> Self {
        let p = &item.properties;
        Self {
            material: item.material.0,
            origin: item.origin,
            properties: [p.hardness, p.density, p.toughness, p.conductivity],
            ore: item.ore.as_ref().map(|ore| (ore.purity, ore.province, ore.traces.iter().map(|&(e, a)| (e as u8, a)).collect())),
        }
    }
}

impl From<ItemRecord> for Item {
    fn from(record: ItemRecord) -> Self {
        let [hardness, density, toughness, conductivity] = record.properties;
        Self {
            material: MaterialId(record.material),
            origin: record.origin,
            properties: Properties { hardness, density, toughness, conductivity },
            ore: record.ore.map(|(purity, province, traces)| OreSample {
                purity,
                province,
                traces: traces.into_iter().filter_map(|(id, amount)| TraceElement::from_id(id).map(|e| (e, amount))).collect(),
            }),
        }
    }
}

pub fn save_path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

impl ServerWorld {
    /// Alle Chunks mit Spieler-Änderungen für einen neu verbundenen Client (#19).
    pub fn touched_chunks(&self) -> Vec<[i32; 3]> {
        self.edits.read().unwrap_or_else(PoisonError::into_inner).touched_chunks().map(|chunk| chunk.to_array()).collect()
    }

    fn snapshot(&self) -> SaveFile {
        let edits = self.edits.read().unwrap_or_else(PoisonError::into_inner);
        let chunks = edits
            .chunks()
            .map(|(chunk, voxels)| ChunkRecord {
                chunk: chunk.to_array(),
                voxels: voxels
                    .iter()
                    .map(|(voxel, value)| {
                        let local = *voxel - chunk * terraforge_core::volume::CHUNK_SIZE;
                        ([local.x as u8, local.y as u8, local.z as u8], value.density, value.material.0)
                    })
                    .collect(),
            })
            .collect();
        let felled = edits.felled_trees().map(|id| (id.chunk_x, id.chunk_z, id.cell)).collect();
        drop(edits);
        let holdings = self.holdings.lock().unwrap_or_else(PoisonError::into_inner);
        let inventory = holdings.inventory.stacks().map(|(slot, stack)| (slot as u8, ItemRecord::from(&stack.item), stack.count)).collect();
        let placed = holdings.placed().map(|(voxel, item)| (voxel.to_array(), ItemRecord::from(item))).collect();
        let materials = self.world.materials.iter().map(|material| (material.id, material.key.clone())).collect();
        SaveFile { version: VERSION, seed: self.seed(), secret: self.underground.seed().fingerprint(), materials, chunks, felled, inventory, placed }
    }

    /// Speichert, falls sich seit dem letzten Mal etwas geändert hat. Geht es schief, bleibt die
    /// Änderung als ungespeichert markiert (nächster Versuch).
    ///
    /// Zuerst das Speicher-Lock, dann die Prüfung: Ein zweiter Aufrufer wartet, bis der erste fertig
    /// geschrieben hat. Sonst sah beim Beenden der zweite Thread „nichts zu tun“ und beendete den
    /// Prozess, während der erste noch schrieb (Review-Fund: bei 2 MB Spielstand 15 von 15 Mal der
    /// letzte Stand verloren).
    pub fn save_if_changed(&self, dir: &Path) -> std::io::Result<bool> {
        let _writing = self.save_lock.lock().unwrap_or_else(PoisonError::into_inner);
        if !self.unsaved.swap(false, Ordering::AcqRel) {
            return Ok(false);
        }
        self.write_save(dir).inspect_err(|_| self.unsaved.store(true, Ordering::Release))?;
        Ok(true)
    }

    /// Speichert in jedem Fall.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let _writing = self.save_lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.unsaved.store(false, Ordering::Release);
        self.write_save(dir).inspect_err(|_| self.unsaved.store(true, Ordering::Release))
    }

    /// Stand unter `edit_lock` erfassen (keine Aktion halb drin), dann ohne es schreiben: Graben
    /// wartet nicht auf die Platte. Erst in eine Hilfsdatei, auf die Platte gebracht (`sync_all`),
    /// dann umbenannt und den Ordner gesichert: Auch nach einem Absturz des Rechners bleibt der alte
    /// oder der neue Stand ganz.
    fn write_save(&self, dir: &Path) -> std::io::Result<()> {
        let data = {
            let _serial = self.edit_lock.lock().unwrap_or_else(PoisonError::into_inner);
            postcard::to_stdvec(&self.snapshot()).map_err(std::io::Error::other)?
        };
        std::fs::create_dir_all(dir)?;
        let path = save_path(dir);
        let temporary = dir.join(format!("{FILE}.tmp"));
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &path)?;
        // Den Ordner sichern (Umbenennen); unter Windows nicht möglich, dort reicht es nicht ganz.
        if let Ok(dir) = std::fs::File::open(dir) {
            dir.sync_all().ok();
        }
        Ok(())
    }

    /// Lädt einen Spielstand aus `dir`, falls es einen gibt. Fehler (fremde Welt, andere Version,
    /// kaputte Datei) melden, ohne etwas zu übernehmen.
    pub fn load(&self, dir: &Path) -> Result<bool, String> {
        let path = save_path(dir);
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let file: SaveFile = postcard::from_bytes(&data).map_err(|error| format!("{}: unlesbar ({error})", path.display()))?;
        if file.version != VERSION {
            return Err(format!("{}: Version {} statt {VERSION}", path.display(), file.version));
        }
        if file.seed != self.seed() || file.secret != self.underground.seed().fingerprint() {
            return Err(format!("{}: gehört zu einer anderen Welt (Seed {})", path.display(), file.seed));
        }
        for (id, key) in &file.materials {
            let current = self.world.materials.get(MaterialId(*id)).map(|material| material.key.as_str());
            if current != Some(key.as_str()) {
                return Err(format!("{}: Material {id} war „{key}“, ist jetzt {current:?}", path.display()));
            }
        }
        let _serial = self.edit_lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut edits = self.edits.write().unwrap_or_else(PoisonError::into_inner);
        for record in file.chunks {
            let chunk = IVec3::from_array(record.chunk);
            for ([x, y, z], density, material) in record.voxels {
                let voxel = chunk * terraforge_core::volume::CHUNK_SIZE + IVec3::new(x.into(), y.into(), z.into());
                edits.restore(voxel, Voxel { density, material: MaterialId(material) });
            }
        }
        for (chunk_x, chunk_z, cell) in file.felled {
            edits.fell_tree(TreeId { chunk_x, chunk_z, cell });
        }
        drop(edits);
        let mut holdings = self.holdings.lock().unwrap_or_else(PoisonError::into_inner);
        for (slot, item, count) in file.inventory {
            if usize::from(slot) >= terraforge_core::inventory::SLOTS {
                eprintln!("terraforge-server: Inventarplatz {slot} gibt es nicht mehr, {count} Stück verworfen");
            }
            holdings.inventory.restore(slot.into(), Stack { item: item.into(), count });
        }
        for (voxel, item) in file.placed {
            holdings.restore_placed(IVec3::from_array(voxel), item.into());
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terraforge_core::underground::SecretSeed;

    /// Ein umnummeriertes Material (Schlüssel passt nicht mehr zur Nummer) lädt nicht; ein neues
    /// Material, das es beim Speichern noch nicht gab, schon.
    #[test]
    fn renumbered_materials_are_not_loaded() {
        let dir = std::env::temp_dir().join(format!("terraforge-save-materials-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let world = ServerWorld::new(42, SecretSeed::insecure_dev_seed(42));
        let mut file = world.snapshot();
        file.materials.retain(|(id, _)| *id != 60);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(save_path(&dir), postcard::to_stdvec(&file).unwrap()).unwrap();
        assert_eq!(world.load(&dir), Ok(true), "Holz kam nach dem Speichern dazu");
        let (_, key) = file.materials.iter_mut().find(|(id, _)| *id == 13).unwrap();
        *key = "sandstone".into();
        std::fs::write(save_path(&dir), postcard::to_stdvec(&file).unwrap()).unwrap();
        let fresh = ServerWorld::new(42, SecretSeed::insecure_dev_seed(42));
        assert!(fresh.load(&dir).unwrap_err().contains("Material 13"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
