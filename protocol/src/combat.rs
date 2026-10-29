//! Treffer des Kampfs (#62) ↔ `HitInfo`.

use terraforge_core::combat::Hit;

use crate::messages::HitInfo;

impl From<&Hit> for HitInfo {
    fn from(hit: &Hit) -> Self {
        Self {
            dummy: hit.target as u32,
            zone: hit.zone.id(),
            at: hit.at,
            speed: hit.speed,
            impulse: hit.impulse,
            damage: [hit.damage.cut, hit.damage.pierce, hit.damage.blunt],
        }
    }
}
