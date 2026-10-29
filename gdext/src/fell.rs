//! Bäume fällen im Godot-Client (#18): mit Server prüft und fällt der Server (`dig.rs` nimmt die
//! Antwort entgegen), ohne Server fällt das Terrain selbst, mit denselben Regeln
//! (`vegetation::targeted_tree`).

use std::sync::Arc;

use glam::{IVec2, Vec3};
use godot::prelude::*;
use terraforge_core::inventory::Item;
use terraforge_core::vegetation::{felling_yield, targeted_tree};
use terraforge_core::volume::ColumnData;
use terraforge_protocol::EditAction;
use terraforge_protocol::inventory::view;

use crate::dig::{felled, refused};
use crate::terrain::TerraforgeTerrain;

#[godot_api(secondary)]
impl TerraforgeTerrain {
    /// Fällt den Baum in Blickrichtung (höchstens 8 m, nicht hinter Gelände). Ergebnis über
    /// `take_edit_results` (`felled`: Anzahl Holz), Nummer der Anfrage, −1 vor `_ready`.
    #[func]
    fn fell(&mut self, eye: Vector3, direction: Vector3) -> i64 {
        let Some(world) = self.world.clone() else {
            return -1;
        };
        let id = self.edits.next_id();
        let local = self.base().to_local(eye);
        let (eye, direction) = (Vec3::new(local.x, local.y, local.z), Vec3::new(direction.x, direction.y, direction.z));
        let Some(direction) = direction.try_normalize().filter(|_| eye.is_finite()) else {
            self.edits.results.push(refused(id, "ungültige Position oder Richtung"));
            return id;
        };
        if let Some(network) = self.network.clone() {
            match network.send_edit(eye.to_array(), direction.to_array(), EditAction::Fell) {
                Some(request) => {
                    self.edits.waiting.insert(request, (id, None));
                }
                None => self.edits.results.push(refused(id, "Server ausgefallen")),
            }
            return id;
        }
        let Some(streamer) = self.streamer.as_ref() else {
            return -1;
        };
        let target = {
            let edits = self.edits.world.read().expect("Edits-Lock vergiftet");
            let column = |position: IVec2| streamer.column(position).cloned().unwrap_or_else(|| Arc::new(ColumnData::sample(&world, position.x, position.y)));
            targeted_tree(&world, column, streamer.config().clearing, |voxel| streamer.density_at(voxel), |tree| edits.is_felled(tree), eye, direction)
        };
        let Some((base, tree)) = target else {
            self.edits.results.push(refused(id, "kein Baum im Visier"));
            return id;
        };
        let (material, count) = felling_yield(&tree);
        let item = Item::from_sample(world.mined_sample(base.round().as_ivec3(), material));
        if self.edits.local.inventory.room_for(&item) == 0 {
            self.edits.results.push(refused(id, "Inventar voll"));
            return id;
        }
        let lost = self.edits.local.inventory.add(&item, count);
        self.edits.world.write().expect("Edits-Lock vergiftet").fell_tree(tree.id);
        if let Some(streamer) = self.streamer.as_mut() {
            streamer.fell_tree(tree.id);
        }
        self.edits.stacks = view(&self.edits.local.inventory);
        self.edits.results.push(felled(id, &world.materials, material, count, lost));
        id
    }
}
