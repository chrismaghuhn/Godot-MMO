//! Geologische Provinzen (#45, D-013): große Gebiete mit wenigen, festen Erzarten.
//!
//! Voronoi-Zellen mit ≈ `PROVINCE_SIZE` Abstand; Mittelpunkte und Erz-Auswahl kommen aus dem
//! geheimen Seed. Die Auswahl hängt von der (öffentlichen) Umgebung am Mittelpunkt ab, z. B.
//! Schwefel nur in Vulkangebieten. Jede Provinz hat mindestens ein Grundmetall.

use crate::biome::FamilyKind;
use crate::noise::Layer;
use crate::world::WorldGen;

use super::SecretSeed;
use super::ore_rules::OreKind;

/// Abstand der Provinz-Mittelpunkte (m).
pub const PROVINCE_SIZE: f32 = 900.0;
/// Höchstens so viele Erzarten pro Provinz ("stark regional").
pub const MAX_ORES: usize = 5;

/// Zelle des Provinz-Rasters; identifiziert eine Provinz.
pub type ProvinceId = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deposit {
    pub kind: OreKind,
    /// 0..1: wie viel Erz (verschiebt die Schwelle der Form).
    pub abundance: f32,
    /// Grund-Reinheit in dieser Provinz.
    pub base_purity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Province {
    pub id: ProvinceId,
    pub center: (f32, f32),
    /// Nach Priorität sortiert (`OreKind`-Reihenfolge: Seltenes zuerst).
    pub deposits: Vec<Deposit>,
}

impl Province {
    pub fn deposit(&self, kind: OreKind) -> Option<&Deposit> {
        self.deposits.iter().find(|deposit| deposit.kind == kind)
    }
}

pub(super) struct ProvinceMap {
    seed: SecretSeed,
    purity: Layer,
}

/// Zweck-Konstanten für `SecretSeed::hash`, damit Entscheidungen nicht korrelieren.
const SALT_CENTER_X: i64 = 1;
const SALT_CENTER_Z: i64 = 2;
const SALT_PICK: i64 = 100;
const SALT_ABUNDANCE: i64 = 200;
const SALT_PURITY: i64 = 300;

impl ProvinceMap {
    pub(super) fn new(seed: &SecretSeed) -> Self {
        Self {
            seed: *seed,
            purity: Layer::new(seed.layer(0x9_0417), 0.004, 2),
        }
    }

    /// Mittelpunkt einer Zelle, versetzt um 0,25..0,75 der Zellgröße. Mit diesem Bereich liegt
    /// der nächste Mittelpunkt garantiert in den 3×3 Nachbarzellen (max. Abstand ≈ 1,06 Zellen,
    /// jeder Mittelpunkt außerhalb ist ≥ 1,25 Zellen entfernt), was `province_at` voraussetzt.
    fn center(&self, (cell_x, cell_z): ProvinceId) -> (f32, f32) {
        let jitter_x = 0.25 + 0.5 * self.seed.unit(&[cell_x as i64, cell_z as i64, SALT_CENTER_X]);
        let jitter_z = 0.25 + 0.5 * self.seed.unit(&[cell_x as i64, cell_z as i64, SALT_CENTER_Z]);
        ((cell_x as f32 + jitter_x) * PROVINCE_SIZE, (cell_z as f32 + jitter_z) * PROVINCE_SIZE)
    }

    /// Provinz, zu der ein Punkt gehört (nächster Mittelpunkt). Reine Funktion von x/z.
    pub(super) fn province_at(&self, x: f32, z: f32) -> ProvinceId {
        let (base_x, base_z) = ((x / PROVINCE_SIZE).floor() as i32, (z / PROVINCE_SIZE).floor() as i32);
        let mut best = (base_x, base_z);
        let mut best_distance = f32::INFINITY;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let cell = (base_x + dx, base_z + dz);
                let (cx, cz) = self.center(cell);
                let distance = (cx - x) * (cx - x) + (cz - z) * (cz - z);
                // Gleichstand deterministisch: kleinere Zelle gewinnt (Reihenfolge der Schleife).
                if distance < best_distance {
                    best_distance = distance;
                    best = cell;
                }
            }
        }
        best
    }

    /// Erz-Auswahl einer Provinz, abhängig von der Umgebung am Mittelpunkt.
    pub(super) fn province(&self, world: &WorldGen, id: ProvinceId) -> Province {
        let center = self.center(id);
        let sample = world.sample(center.0, center.1);
        let crystal = sample.crystal > 0.4 || world.biome_at(&sample).kind == FamilyKind::Crystal;
        let roll = |purpose: i64| self.seed.unit(&[id.0 as i64, id.1 as i64, SALT_PICK + purpose]);

        let mut kinds: Vec<OreKind> = Vec::new();
        // Grundmetall: immer eines, manchmal ein zweites.
        let pick_base = |r: f32| match r {
            r if r < 0.40 => OreKind::Iron,
            r if r < 0.75 => OreKind::Copper,
            _ => OreKind::Tin,
        };
        let first = pick_base(roll(0));
        kinds.push(first);
        if roll(1) < 0.30 {
            let second = pick_base(roll(2));
            if second != first {
                kinds.push(second);
            }
        }
        // Weitere Erze nach Umgebung, seltenes zuerst geprüft.
        let mut extras: Vec<OreKind> = Vec::new();
        let gold = roll(10) < 0.12 + 0.18 * sample.mountain;
        if gold {
            extras.push(OreKind::Gold);
            if roll(11) < 0.8 {
                extras.push(OreKind::GoldSand);
            }
        }
        if roll(12) < 0.20 {
            extras.push(OreKind::Silver);
        }
        if sample.volcanic > 0.35 {
            if roll(13) < 0.7 {
                extras.push(OreKind::Sulfur);
            }
            if roll(14) < 0.6 {
                extras.push(OreKind::Obsidian);
            }
            if roll(15) < 0.35 {
                extras.push(OreKind::Cinnabar);
            }
        }
        // Quarz: sehr häufig in Kristallgebieten, sonst als Quarzadern in Bergprovinzen (Granit).
        if (crystal && roll(16) < 0.8) || (sample.mountain > 0.5 && roll(16) < 0.25) {
            extras.push(OreKind::Quartz);
        }
        if sample.mountain > 0.55 && roll(17) < 0.35 {
            extras.push(OreKind::Magnetite);
        }
        if sample.volcanic < 0.4 && roll(18) < 0.45 {
            extras.push(OreKind::Coal);
        }
        for kind in extras {
            if kinds.len() >= MAX_ORES {
                break;
            }
            kinds.push(kind);
        }
        kinds.sort();

        let deposits = kinds
            .into_iter()
            .map(|kind| {
                let k = kind as i64;
                Deposit {
                    kind,
                    abundance: 0.3 + 0.7 * self.seed.unit(&[id.0 as i64, id.1 as i64, SALT_ABUNDANCE + k]),
                    base_purity: 0.25 + 0.7 * self.seed.unit(&[id.0 as i64, id.1 as i64, SALT_PURITY + k]),
                }
            })
            .collect();
        Province { id, center, deposits }
    }

    /// Reinheit 0,05..1 eines Vorkommens an x/z: Grund-Reinheit der Provinz plus langsame
    /// Schwankung ("Eisen aus diesem Tal ist besser").
    pub(super) fn purity(&self, deposit: &Deposit, x: f32, z: f32) -> f32 {
        let offset = deposit.kind as usize as f32 * 1013.0;
        (deposit.base_purity + 0.15 * self.purity.get(x + offset, z - offset)).clamp(0.05, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn provinces(secret: u64, world: &WorldGen) -> Vec<Province> {
        let map = ProvinceMap::new(&SecretSeed::from_u64(secret));
        let mut result = Vec::new();
        for cell_z in -12..12 {
            for cell_x in -12..12 {
                result.push(map.province(world, (cell_x, cell_z)));
            }
        }
        result
    }

    #[test]
    fn every_province_is_small_and_has_a_base_metal() {
        let world = WorldGen::new(42);
        for province in provinces(1, &world) {
            let n = province.deposits.len();
            assert!((1..=MAX_ORES).contains(&n), "{:?}: {n} Erze", province.id);
            assert!(
                province.deposits.iter().any(|d| OreKind::BASE_METALS.contains(&d.kind)),
                "{:?} ohne Grundmetall",
                province.id
            );
            let unique: HashSet<_> = province.deposits.iter().map(|d| d.kind).collect();
            assert_eq!(unique.len(), n, "doppelte Erzart");
            for deposit in &province.deposits {
                assert!((0.3..=1.0).contains(&deposit.abundance));
                assert!((0.25..=0.95).contains(&deposit.base_purity));
            }
        }
    }

    /// "Stark regional": kein Grundmetall ist fast überall, jede Erzart kommt irgendwo vor,
    /// Spezialmaterialien nur in passender Umgebung.
    #[test]
    fn ores_are_regional_and_follow_the_environment() {
        let world = WorldGen::new(42);
        let all = provinces(2, &world);
        let mut counts: HashMap<OreKind, usize> = HashMap::new();
        for province in &all {
            let sample = world.sample(province.center.0, province.center.1);
            for deposit in &province.deposits {
                *counts.entry(deposit.kind).or_default() += 1;
                match deposit.kind {
                    OreKind::Sulfur | OreKind::Obsidian | OreKind::Cinnabar => assert!(sample.volcanic > 0.35),
                    OreKind::Magnetite => assert!(sample.mountain > 0.55),
                    OreKind::Coal => assert!(sample.volcanic < 0.4),
                    OreKind::GoldSand => assert!(province.deposit(OreKind::Gold).is_some(), "Seife ohne Gold"),
                    _ => {}
                }
            }
        }
        let n = all.len() as f64;
        for kind in OreKind::BASE_METALS {
            let share = *counts.get(&kind).unwrap_or(&0) as f64 / n;
            assert!(share < 0.70, "{kind:?} in {:.0} % der Provinzen", share * 100.0);
        }
        for kind in OreKind::ALL {
            assert!(counts.contains_key(&kind), "{kind:?} kommt in {n} Provinzen nie vor");
        }
        let average: f64 = all.iter().map(|p| p.deposits.len()).sum::<usize>() as f64 / n;
        assert!(average <= 3.5, "im Schnitt {average:.1} Erzarten pro Provinz");
    }

    #[test]
    fn province_assignment_is_a_voronoi_partition() {
        let map = ProvinceMap::new(&SecretSeed::from_u64(3));
        for (x, z) in [(0.0, 0.0), (1234.5, -987.0), (-4000.0, 2500.0), (899.9, 900.1)] {
            let id = map.province_at(x, z);
            let own = map.center(id);
            let own_distance = (own.0 - x).powi(2) + (own.1 - z).powi(2);
            // Kein Mittelpunkt in einem größeren Umkreis ist näher.
            for dz in -3..=3 {
                for dx in -3..=3 {
                    let other = map.center((id.0 + dx, id.1 + dz));
                    assert!((other.0 - x).powi(2) + (other.1 - z).powi(2) >= own_distance);
                }
            }
            assert_eq!(map.province_at(x, z), id, "nicht deterministisch");
        }
    }

    #[test]
    fn different_secrets_give_different_provinces_and_purity_is_bounded() {
        let world = WorldGen::new(42);
        let (a, b) = (provinces(1, &world), provinces(2, &world));
        let differing = a.iter().zip(&b).filter(|(p, q)| p.deposits != q.deposits).count();
        assert!(differing > a.len() / 2, "Provinzen hängen kaum vom Seed ab");
        let map = ProvinceMap::new(&SecretSeed::from_u64(1));
        for province in &a {
            for deposit in &province.deposits {
                for (x, z) in [(0.0, 0.0), (500.0, -300.0), (-2000.0, 1700.0)] {
                    let purity = map.purity(deposit, x, z);
                    assert!((0.05..=1.0).contains(&purity));
                }
            }
        }
    }
}
