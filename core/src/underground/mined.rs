//! Abgebaute Erze (#15, D-022): Reinheit, Provinz und Spurenelemente aus dem geheimen Untergrund.
//!
//! Ein Erz-Voxel ist eine Mischung aus Erz und dem Gestein, in dem es sitzt (taubes Gestein): Bei
//! Reinheit 1 gelten die Werte des Erzes aus `materials.ron`, bei geringer Reinheit fast die des
//! Gesteins, das ohne Erz an dieser Stelle läge. Danach wandelt der öffentliche Teil sie wie bei
//! jedem Material nach Ort ab (`WorldGen::vary`). Die Spurenelemente sind je Provinz und Erz fest;
//! reinere Erze enthalten weniger davon.

use glam::IVec3;

use super::{OreKind, Underground};
use crate::material::MaterialId;
use crate::properties::{MinedSample, OreSample, Properties};
use crate::volume::{CHUNK_SIZE, ColumnData};
use crate::world::WorldGen;

/// Salze (getrennt von den anderen Hashes des Untergrunds).
const SALT_TRACES: i64 = 0x0712_4CE5;
const SALT_PROVINCE_ID: i64 = 0x0D15_C0DE;
/// Reinheit eines Erzes außerhalb einer Provinz, die es hat (z. B. später gebaut).
const NO_DEPOSIT_PURITY: f32 = 0.05;

/// Anteil der Spurenelemente in Abhängigkeit von der Reinheit: 1,0 bei 0,2 und darunter, 0,2 bei
/// reinem Erz.
pub(super) fn trace_factor(purity: f32) -> f32 {
    (1.2 - purity).clamp(0.2, 1.0)
}

impl Underground {
    /// Werte eines natürlich gewachsenen, abgebauten Voxels mit Erz-Daten. `column` ist die Spalte
    /// des Voxels (für das umgebende Gestein). Für Nicht-Erze dasselbe wie `WorldGen::mined_sample`.
    pub fn mined_sample(&self, world: &WorldGen, column: &ColumnData, voxel: IVec3, material: MaterialId) -> MinedSample {
        let (Some(kind), Some(mineral)) = (OreKind::from_material(material), world.materials.get(material)) else {
            return world.mined_sample(voxel, material);
        };
        let (x, z) = (voxel.x as f32, voxel.z as f32);
        let province = self.province_at(world, x, z);
        let purity = province.deposit(kind).map_or(NO_DEPOSIT_PURITY, |deposit| self.provinces.purity(deposit, x, z));
        let host = host_rock(column, voxel, kind);
        let gangue = world.materials.get(host).map_or(Properties::of(mineral), Properties::of);
        let mut sample = world.vary(voxel, material, gangue.lerp(Properties::of(mineral), purity));
        let (cell_x, cell_z) = (i64::from(province.id.0), i64::from(province.id.1));
        let traces = mineral
            .traces
            .iter()
            .map(|&(element, max)| {
                // Material-ID und feste Element-Nummer: Umsortieren der Enums ändert nichts.
                let share = self.seed.unit(&[SALT_TRACES, cell_x, cell_z, i64::from(material.0), element as i64]);
                (element, max * share * trace_factor(purity))
            })
            .collect();
        let province = self.seed.hash(&[SALT_PROVINCE_ID, cell_x, cell_z]);
        sample.ore = Some(OreSample { purity, province, traces });
        sample
    }
}

/// Gestein, das ohne Erz an dieser Stelle läge; über der Oberfläche oder selbst kein Wirtsgestein
/// des Erzes (Höhlenwand, Rand) das erste Wirtsgestein laut Regel.
fn host_rock(column: &ColumnData, voxel: IVec3, kind: OreKind) -> MaterialId {
    let (local_x, local_z) = (voxel.x - column.chunk_x * CHUNK_SIZE, voxel.z - column.chunk_z * CHUNK_SIZE);
    let inside = (0..CHUNK_SIZE).contains(&local_x) && (0..CHUNK_SIZE).contains(&local_z);
    let public = if inside { column.public_material(local_x, voxel.y, local_z) } else { MaterialId::AIR };
    let hosts = kind.rule().hosts;
    if hosts.contains(&public) { public } else { hosts[0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::owner_chunk;
    use crate::underground::SecretSeed;

    fn setup() -> (WorldGen, Underground) {
        let world = WorldGen::new(42);
        let underground = Underground::new(SecretSeed::insecure_dev_seed(42), &world.cfg);
        (world, underground)
    }

    fn column(world: &WorldGen, voxel: IVec3) -> ColumnData {
        let chunk = owner_chunk(voxel);
        ColumnData::sample(world, chunk.x, chunk.z)
    }

    /// Stellen 40 m unter der Oberfläche, deren Provinz das Erz hat bzw. nicht hat.
    fn spots(world: &WorldGen, underground: &Underground, material: MaterialId, has: bool) -> Vec<IVec3> {
        (0..600)
            .map(|i| {
                let (x, z) = (i * 457 - 140_000, (i * 311) % 60_000 - 30_000);
                IVec3::new(x, (world.column_height(x as f32, z as f32) - 40.0) as i32, z)
            })
            .filter(|v| underground.purity(world, v.x as f32, v.z as f32, material).is_some() == has)
            .collect()
    }

    /// Werte exakt nach dem Modell: Gestein → Erz nach Reinheit gemischt, dann nach Ort abgewandelt.
    #[test]
    fn ore_values_mix_host_rock_and_mineral_by_purity() {
        let (world, underground) = setup();
        for kind in OreKind::ALL {
            let material = kind.material();
            let mineral = Properties::of(world.materials.get(material).unwrap());
            let spots = spots(&world, &underground, material, true);
            assert!(!spots.is_empty(), "{kind:?}: keine Provinz damit");
            for &voxel in spots.iter().take(15) {
                let column = column(&world, voxel);
                let sample = underground.mined_sample(&world, &column, voxel, material);
                let ore = sample.ore.as_ref().expect("Erz ohne Erz-Daten");
                assert_eq!(Some(ore.purity), underground.purity(&world, voxel.x as f32, voxel.z as f32, material));
                let host = host_rock(&column, voxel, kind);
                assert!(kind.rule().hosts.contains(&host));
                let gangue = Properties::of(world.materials.get(host).unwrap());
                let expected = world.vary(voxel, material, gangue.lerp(mineral, ore.purity));
                assert_eq!(sample.properties, expected.properties, "{kind:?} bei {voxel}");
            }
        }
    }

    /// Goldsand liegt in Sand und Kies, nicht im Platzhalter „Gestein“ (Review-Fund: unreiner
    /// Goldsand war fast so hart wie Granit).
    #[test]
    fn gold_sand_stays_soft() {
        let (world, underground) = setup();
        let gold_sand = MaterialId(36);
        let mut checked = 0;
        for voxel in spots(&world, &underground, gold_sand, true) {
            // Goldsand liegt in Flussablagerungen: Stellen knapp unter der Oberfläche mit Sand, Kies, Ton.
            let column = column(&world, voxel);
            let top = world.column_height(voxel.x as f32, voxel.z as f32) as i32;
            let (lx, lz) = (voxel.x - column.chunk_x * CHUNK_SIZE, voxel.z - column.chunk_z * CHUNK_SIZE);
            for y in top - 6..top {
                if [MaterialId::SAND, MaterialId::GRAVEL, MaterialId::CLAY].contains(&column.public_material(lx, y, lz)) {
                    let voxel = IVec3::new(voxel.x, y, voxel.z);
                    let hardness = underground.mined_sample(&world, &column, voxel, gold_sand).properties.hardness;
                    assert!(hardness < 3.0, "Goldsand mit Härte {hardness} bei {voxel}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "keine Stelle mit Sand, Kies oder Ton gefunden");
    }

    /// Je Provinz und Erz ein fester Anteil der Spurenelemente (unabhängig von der Reinheit), in
    /// verschiedenen Provinzen verschieden; reinere Erze enthalten weniger.
    #[test]
    fn traces_belong_to_the_province() {
        let (world, underground) = setup();
        let iron = MaterialId(33);
        let max: Vec<f32> = world.materials.get(iron).unwrap().traces.iter().map(|t| t.1).collect();
        let mut shares: std::collections::HashMap<u64, Vec<f32>> = std::collections::HashMap::new();
        for voxel in spots(&world, &underground, iron, true) {
            let ore = underground.mined_sample(&world, &column(&world, voxel), voxel, iron).ore.unwrap();
            let share: Vec<f32> = ore.traces.iter().zip(&max).map(|(&(_, amount), &max)| amount / (max * trace_factor(ore.purity))).collect();
            assert!(share.iter().all(|s| (0.0..=1.0).contains(s)));
            if let Some(known) = shares.get(&ore.province) {
                assert!(known.iter().zip(&share).all(|(a, b)| (a - b).abs() < 1e-5), "Anteile innerhalb einer Provinz verschieden");
            }
            shares.insert(ore.province, share);
        }
        assert!(shares.len() >= 5, "nur {} Provinzen mit Eisen", shares.len());
        let distinct: std::collections::HashSet<String> = shares.values().map(|s| format!("{s:?}")).collect();
        assert_eq!(distinct.len(), shares.len(), "gleiche Anteile in verschiedenen Provinzen");
        assert!(trace_factor(0.9) < trace_factor(0.5) && trace_factor(0.5) < trace_factor(0.2));
    }

    #[test]
    fn ore_outside_its_province_has_the_lowest_purity() {
        let (world, underground) = setup();
        let iron = MaterialId(33);
        let voxel = spots(&world, &underground, iron, false)[0];
        let ore = underground.mined_sample(&world, &column(&world, voxel), voxel, iron).ore.unwrap();
        assert_eq!(ore.purity, NO_DEPOSIT_PURITY);
    }

    #[test]
    fn other_materials_get_the_public_values() {
        let (world, underground) = setup();
        let voxel = IVec3::new(100, -20, -300);
        let sample = underground.mined_sample(&world, &column(&world, voxel), voxel, MaterialId::GRANITE);
        assert_eq!(sample, world.mined_sample(voxel, MaterialId::GRANITE));
        assert!(sample.ore.is_none());
    }
}
