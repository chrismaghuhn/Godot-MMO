//! Items und Inventar (#18, D-022).
//!
//! Ein Item ist ein Stück Material mit Herkunft und Werten (`properties::MinedSample`). Gleiche
//! Items stapeln: gleiches Material, gleiche Herkunft (Provinz bei Erzen, sonst Region) und bei
//! Erzen gleiche Reinheitsstufe (5 %). Die Werte eines Stapels sind der nach Anzahl gewichtete
//! Mittelwert seiner Teile, wie bei einem Haufen Erz.
//!
//! Das Inventar führt der Server (D-022); der Godot-Client ohne Server nutzt dieselbe Logik lokal.

use std::collections::{BTreeMap, HashMap};

use glam::IVec3;

use crate::material::MaterialId;
use crate::properties::{MinedSample, OreSample, Properties};
use crate::regions::RegionId;

/// Plätze im Inventar.
pub const SLOTS: usize = 40;
/// Höchstens so viele Stück (≈ m³) pro Stapel.
pub const STACK_MAX: u32 = 999;
/// Reinheitsstufen, die nicht mehr stapeln (Anteil).
const PURITY_STEP: f32 = 0.05;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub material: MaterialId,
    /// Herkunft: undurchsichtige Provinz-Kennung bei Erzen, sonst aus der Region abgeleitet.
    pub origin: u64,
    pub properties: Properties,
    pub ore: Option<OreSample>,
}

impl Item {
    pub fn from_sample(sample: MinedSample) -> Self {
        let origin = match &sample.ore {
            Some(ore) => ore.province,
            None => region_origin(sample.region),
        };
        Self { material: sample.material, origin, properties: sample.properties, ore: sample.ore }
    }

    /// Was stapeln darf: Material, Herkunft, Reinheitsstufe.
    fn stack_key(&self) -> (MaterialId, u64, Option<i32>) {
        (self.material, self.origin, self.ore.as_ref().map(|ore| (ore.purity / PURITY_STEP).floor() as i32))
    }
}

fn region_origin(region: RegionId) -> u64 {
    (u64::from(region.0 as u32) << 32) | u64::from(region.1 as u32)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub item: Item,
    pub count: u32,
}

impl Stack {
    /// Nimmt `count` Stück von `item` auf und mittelt die Werte nach Anzahl.
    fn merge(&mut self, item: &Item, count: u32) {
        let total = (self.count + count) as f32;
        let share = count as f32 / total;
        let mix = |a: f32, b: f32| a + (b - a) * share;
        self.item.properties = self.item.properties.lerp(item.properties, share);
        if let (Some(mine), Some(theirs)) = (&mut self.item.ore, &item.ore) {
            mine.purity = mix(mine.purity, theirs.purity);
            for (element, amount) in &mut mine.traces {
                let other = theirs.traces.iter().find(|(e, _)| e == element).map_or(0.0, |t| t.1);
                *amount = mix(*amount, other);
            }
        }
        self.count += count;
    }
}

/// Inventar mit festen Plätzen: Ein leerer Platz bleibt leer, die anderen rücken nicht nach
/// (Review-Fund: sonst baute B nach dem letzten Sand unbemerkt mit dem Material vom nächsten Platz).
#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    slots: Vec<Option<Stack>>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: vec![None; SLOTS] }
    }
}

impl Inventory {
    /// Belegte Plätze mit ihrer Nummer.
    pub fn stacks(&self) -> impl Iterator<Item = (usize, &Stack)> {
        self.slots.iter().enumerate().filter_map(|(slot, stack)| stack.as_ref().map(|stack| (slot, stack)))
    }

    pub fn get(&self, slot: usize) -> Option<&Stack> {
        self.slots.get(slot)?.as_ref()
    }

    /// Wie viel von `item` noch Platz fände.
    pub fn room_for(&self, item: &Item) -> u32 {
        let key = item.stack_key();
        self.slots
            .iter()
            .map(|stack| match stack {
                Some(stack) if stack.item.stack_key() == key => STACK_MAX - stack.count,
                Some(_) => 0,
                None => STACK_MAX,
            })
            .sum()
    }

    /// Legt `count` Stück ab: zuerst auf passende Stapel, dann auf freie Plätze (vorderste zuerst).
    /// Gibt zurück, wie viel keinen Platz mehr fand.
    pub fn add(&mut self, item: &Item, mut count: u32) -> u32 {
        let key = item.stack_key();
        for stack in self.slots.iter_mut().flatten().filter(|stack| stack.item.stack_key() == key) {
            let fit = count.min(STACK_MAX - stack.count);
            if fit > 0 {
                stack.merge(item, fit);
                count -= fit;
            }
        }
        for slot in self.slots.iter_mut().filter(|slot| slot.is_none()) {
            if count == 0 {
                break;
            }
            let fit = count.min(STACK_MAX);
            *slot = Some(Stack { item: item.clone(), count: fit });
            count -= fit;
        }
        count
    }

    /// Nimmt bis zu `count` Stück vom Platz `slot`; ein leerer Platz bleibt leer.
    pub fn take(&mut self, slot: usize, count: u32) -> Option<(Item, u32)> {
        let entry = self.slots.get_mut(slot)?;
        let stack = entry.as_mut()?;
        let taken = count.min(stack.count);
        stack.count -= taken;
        let item = stack.item.clone();
        if stack.count == 0 {
            *entry = None;
        }
        Some((item, taken))
    }

    /// Legt einen gespeicherten Stapel auf seinen Platz (#19); außerhalb der Plätze wird er
    /// verworfen.
    pub fn restore(&mut self, slot: usize, stack: Stack) {
        if let Some(entry) = self.slots.get_mut(slot) {
            *entry = Some(stack);
        }
    }

    /// Anzahl aller Stück eines Materials.
    pub fn count(&self, material: MaterialId) -> u32 {
        self.stacks().filter(|(_, stack)| stack.item.material == material).map(|(_, stack)| stack.count).sum()
    }
}

/// Inventar plus gebaute Voxel mit dem Item, aus dem sie gebaut sind: Ausbeute beim Graben,
/// Verbrauch beim Bauen. Gebaute Voxel geben beim Graben ihr Item zurück, statt neu am Fundort
/// bewertet zu werden; sonst ließe sich Erz durch Ablegen in einer reichen Provinz veredeln (#15).
#[derive(Clone, Debug, Default)]
pub struct Holdings {
    pub inventory: Inventory,
    placed: HashMap<IVec3, Item>,
}

impl Holdings {
    /// Material und Anzahl auf einem Inventarplatz, falls dort etwas liegt.
    pub fn build_material(&self, slot: usize) -> Option<(MaterialId, u32)> {
        self.inventory.get(slot).map(|stack| (stack.item.material, stack.count))
    }

    /// Legt abgebaute Voxel ab. `sample` bewertet natürlich gewachsenes Material am Fundort; es wird
    /// einmal je Material aufgerufen, am Voxel dieses Materials, der `center` am nächsten liegt (in
    /// einer Grab-Kugel ändern sich die Werte kaum; die Mitte, damit an einer Provinzgrenze nicht
    /// eine Ecke der Kugel entscheidet). Gibt zurück, wie viel keinen Platz fand.
    pub fn store_yield(&mut self, center: IVec3, removed: &[(IVec3, MaterialId)], mut sample: impl FnMut(IVec3, MaterialId) -> MinedSample) -> u32 {
        let mut built = Vec::new();
        let mut natural: BTreeMap<MaterialId, (IVec3, u32)> = BTreeMap::new();
        for &(voxel, material) in removed {
            match self.placed.remove(&voxel) {
                Some(item) => built.push(item),
                None => {
                    let entry = natural.entry(material).or_insert((voxel, 0));
                    entry.1 += 1;
                    if voxel.distance_squared(center) < entry.0.distance_squared(center) {
                        entry.0 = voxel;
                    }
                }
            }
        }
        let mut lost = 0;
        for (material, (voxel, count)) in natural {
            lost += self.inventory.add(&Item::from_sample(sample(voxel, material)), count);
        }
        for item in built {
            lost += self.inventory.add(&item, 1);
        }
        lost
    }

    /// Gebaute Voxel mit ihrem Item (zum Speichern, #19).
    pub fn placed(&self) -> impl Iterator<Item = (IVec3, &Item)> + '_ {
        self.placed.iter().map(|(voxel, item)| (*voxel, item))
    }

    pub fn restore_placed(&mut self, voxel: IVec3, item: Item) {
        self.placed.insert(voxel, item);
    }

    /// Nimmt die verbauten Voxel vom Platz `slot` und merkt sich ihr Item.
    pub fn consume(&mut self, slot: usize, placed_at: &[IVec3]) {
        if let Some((item, _)) = self.inventory.take(slot, placed_at.len() as u32) {
            for &voxel in placed_at {
                self.placed.insert(voxel, item.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::TraceElement;

    fn counts(inventory: &Inventory) -> Vec<(usize, u32)> {
        inventory.stacks().map(|(slot, stack)| (slot, stack.count)).collect()
    }

    fn item(material: u16, origin: u64, hardness: f32, purity: Option<f32>) -> Item {
        Item {
            material: MaterialId(material),
            origin,
            properties: Properties { hardness, density: 2.0, toughness: 0.5, conductivity: 2.0 },
            ore: purity.map(|purity| OreSample { purity, province: origin, traces: vec![(TraceElement::Sulfur, purity)] }),
        }
    }

    #[test]
    fn equal_items_stack_and_average_their_values() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(&item(13, 1, 6.0, None), 10), 0);
        assert_eq!(inventory.add(&item(13, 1, 7.0, None), 30), 0);
        assert_eq!(counts(&inventory), vec![(0, 40)]);
        let stack = inventory.get(0).unwrap();
        assert!((stack.item.properties.hardness - 6.75).abs() < 1e-5);
        // Andere Herkunft, anderes Material: eigene Stapel.
        inventory.add(&item(13, 2, 6.0, None), 1);
        inventory.add(&item(11, 1, 3.0, None), 1);
        assert_eq!(counts(&inventory), vec![(0, 40), (1, 1), (2, 1)]);
        assert_eq!(inventory.count(MaterialId(13)), 41);
    }

    #[test]
    fn ores_stack_by_purity_step() {
        let mut inventory = Inventory::default();
        inventory.add(&item(33, 7, 5.0, Some(0.41)), 1);
        inventory.add(&item(33, 7, 5.0, Some(0.44)), 1);
        inventory.add(&item(33, 7, 5.0, Some(0.46)), 1);
        assert_eq!(counts(&inventory), vec![(0, 2), (1, 1)]);
        let ore = inventory.get(0).unwrap().item.ore.as_ref().unwrap();
        assert!((ore.purity - 0.425).abs() < 1e-5 && (ore.traces[0].1 - 0.425).abs() < 1e-5);
    }

    #[test]
    fn full_stacks_and_slots_overflow() {
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(&item(13, 1, 6.0, None), STACK_MAX + 5), 0);
        assert_eq!(counts(&inventory), vec![(0, STACK_MAX), (1, 5)]);
        for origin in 2..SLOTS as u64 {
            inventory.add(&item(13, origin, 6.0, None), 1);
        }
        assert_eq!(inventory.stacks().count(), SLOTS);
        assert_eq!(inventory.room_for(&item(14, 1, 6.0, None)), 0);
        assert_eq!(inventory.add(&item(14, 1, 6.0, None), 3), 3, "kein Platz mehr");
        assert_eq!(inventory.room_for(&item(13, 1, 6.0, None)), STACK_MAX - 5);
        // Auf den vorhandenen Stapel passt noch etwas.
        assert_eq!(inventory.add(&item(13, 1, 6.0, None), 3), 0);
    }

    /// Gebautes gibt beim Graben sein Item zurück, natürliches wird am Fundort bewertet.
    #[test]
    fn built_voxels_return_their_item() {
        let mut holdings = Holdings::default();
        holdings.inventory.add(&item(33, 7, 5.0, Some(0.2)), 3);
        let at = [IVec3::new(1, 2, 3), IVec3::new(2, 2, 3)];
        holdings.consume(0, &at);
        assert_eq!(holdings.inventory.count(MaterialId(33)), 1);
        let mut sampled = Vec::new();
        let removed = [(at[0], MaterialId(33)), (at[1], MaterialId(33)), (IVec3::new(9, 9, 9), MaterialId(33))];
        let lost = holdings.store_yield(IVec3::ZERO, &removed, |voxel, material| {
            sampled.push(voxel);
            MinedSample {
                material,
                region: RegionId(0, 0),
                properties: Properties { hardness: 5.0, density: 5.0, toughness: 0.5, conductivity: 5.0 },
                ore: Some(OreSample { purity: 0.9, province: 99, traces: Vec::new() }),
            }
        });
        assert_eq!(lost, 0);
        assert_eq!(sampled, vec![IVec3::new(9, 9, 9)], "nur der natürliche Voxel wird bewertet");
        // Die beiden gebauten kommen mit Reinheit 0,2 aus Provinz 7 zurück, nicht mit 0,9 aus 99.
        let from = |province: u64| holdings.inventory.stacks().filter(|(_, s)| s.item.origin == province).map(|(_, s)| s.count).sum::<u32>();
        assert_eq!((from(7), from(99)), (3, 1));
    }

    /// Ein leerer Platz bleibt leer, die anderen behalten ihre Nummer (Review-Fund); der nächste
    /// neue Stapel füllt die Lücke.
    #[test]
    fn emptied_slots_stay_in_place() {
        let mut inventory = Inventory::default();
        inventory.add(&item(2, 1, 1.5, None), 5);
        inventory.add(&item(13, 1, 6.0, None), 7);
        assert_eq!(inventory.take(0, 3).map(|(_, n)| n), Some(3));
        assert_eq!(inventory.take(0, 9).map(|(_, n)| n), Some(2));
        assert_eq!(counts(&inventory), vec![(1, 7)]);
        assert_eq!(inventory.take(0, 1), None);
        inventory.add(&item(11, 1, 3.0, None), 2);
        assert_eq!(counts(&inventory), vec![(0, 2), (1, 7)]);
    }

    /// Natürliches Material wird am Voxel nahe der Mitte bewertet.
    #[test]
    fn natural_yield_is_sampled_near_the_center() {
        let mut holdings = Holdings::default();
        let removed = [(IVec3::new(5, 0, 0), MaterialId(13)), (IVec3::new(1, 0, 0), MaterialId(13)), (IVec3::new(-4, 0, 0), MaterialId(13))];
        let mut sampled = Vec::new();
        holdings.store_yield(IVec3::ZERO, &removed, |voxel, material| {
            sampled.push(voxel);
            MinedSample { material, region: RegionId(0, 0), properties: Properties { hardness: 6.0, density: 2.7, toughness: 0.6, conductivity: 2.8 }, ore: None }
        });
        assert_eq!(sampled, vec![IVec3::new(1, 0, 0)]);
        assert_eq!(holdings.inventory.count(MaterialId(13)), 3);
    }
}
