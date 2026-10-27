//! Inventar-Stapel ↔ `StackInfo` (#18).

use terraforge_core::inventory::{Inventory, Stack};

use crate::messages::StackInfo;

impl StackInfo {
    pub fn new(slot: usize, stack: &Stack) -> Self {
        let p = &stack.item.properties;
        let round = |value: f32, step: f32| (value / step).round() * step;
        Self {
            slot: slot as u8,
            material: stack.item.material.0,
            count: stack.count,
            origin: stack.item.origin,
            properties: [p.hardness, p.density, p.toughness, p.conductivity],
            ore: stack.item.ore.as_ref().map(|ore| (round(ore.purity, 0.01), ore.traces.iter().map(|&(element, amount)| (element as u8, round(amount, 0.01))).collect())),
        }
    }
}

/// Alle belegten Plätze eines Inventars.
pub fn view(inventory: &Inventory) -> Vec<StackInfo> {
    inventory.stacks().map(|(slot, stack)| StackInfo::new(slot, stack)).collect()
}
