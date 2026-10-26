//! Erz-Platzierung (#45, D-013): Nester, Adern und Seifen im geheimen Untergrund.
//!
//! Pro Sample: Provinz an x/z bestimmen → deren Vorkommen nach Priorität prüfen → das erste,
//! dessen Wirtsgestein, Tiefe und Form passen, ersetzt das Material. Die Formen nutzen
//! 3D-Noise auf dem gemeinsamen 4-m-Gitter (`grid.rs`).
//!
//! **Schutzregel (D-013):** Erze nur in Samples mit mindestens `SURFACE_SHELL` Tiefe unter der
//! niedrigsten Oberfläche der 3×3-Nachbarspalten – ohne Ausnahme für Eingänge. Damit bleibt das
//! Material jeder Zelle, die öffentliche Luft berührt, unverändert (gleiches Argument wie bei
//! der Höhlen-Schale).

use std::collections::HashMap;

use crate::noise::Layer;
use crate::volume::{CHUNK_SIZE, ColumnData, DensityChunk, SAMPLE_MIN};
use crate::world::WorldGen;

use super::SecretSeed;
use super::caves::{SURFACE_SHELL, ShellHeights};
use super::grid::CoarseGrid;
use super::ore_rules::{Form, OreKind};
use super::provinces::{Deposit, Province, ProvinceId, ProvinceMap};

/// Seifen: Fluss-Faktor, ab dem eine Spalte als Flussbett zählt.
const PLACER_RIVER: f32 = 0.25;
/// Seifen liegen in diesem Tiefenband unter der Schale (alte, überdeckte Flussablagerungen).
const PLACER_BAND: (f32, f32) = (SURFACE_SHELL + 0.5, SURFACE_SHELL + 4.5);
const PLACER_THRESHOLD: f32 = -0.15;
/// Wie stark eine geringe Häufigkeit die Schwelle eines Nests anhebt.
const ABUNDANCE_SHIFT: f32 = 0.15;

pub(super) struct OreNoise {
    /// Pro Erzart: Haupt-Noise (Nest bzw. Aderfläche) und bei Adern die Ausdehnungs-Noise.
    fields: Vec<(Layer, Option<Layer>)>,
    placer: Layer,
}

impl OreNoise {
    pub(super) fn new(seed: &SecretSeed) -> Self {
        let fields = OreKind::ALL
            .iter()
            .map(|&kind| {
                let salt = 0x0E_0000 + kind as u64 * 16;
                match kind.rule().form {
                    Form::Nest { frequency, .. } => (Layer::new(seed.layer(salt), frequency, 2), None),
                    Form::Vein { frequency, .. } => (
                        Layer::new(seed.layer(salt), frequency, 2),
                        Some(Layer::new(seed.layer(salt + 1), frequency * 0.6, 1)),
                    ),
                    // Seifen nutzen `placer`; dieser Eintrag wird nie gelesen.
                    Form::Placer => (Layer::new(seed.layer(salt), 0.01, 1), None),
                }
            })
            .collect();
        Self {
            fields,
            placer: Layer::new(seed.layer(0x5E_1F), 0.05, 2),
        }
    }
}

/// Gitter einer Erzart für einen Chunk.
struct KindGrids {
    primary: CoarseGrid,
    secondary: Option<CoarseGrid>,
}

impl KindGrids {
    fn build(noise: &OreNoise, kind: OreKind, chunk: (i32, i32, i32)) -> Self {
        let (layer, extent) = &noise.fields[kind as usize];
        let y_stretch = match kind.rule().form {
            Form::Nest { y_stretch, .. } => y_stretch,
            _ => 1.0,
        };
        let grid = |layer: &Layer, stretch: f32| {
            CoarseGrid::build(chunk.0, chunk.1, chunk.2, |_, _| (), |_, x, y, z| layer.get3(x, y * stretch, z))
        };
        Self {
            primary: grid(layer, y_stretch),
            secondary: extent.as_ref().map(|layer| grid(layer, 1.0)),
        }
    }

    /// Liegt das Sample in einem Vorkommen dieser Form?
    fn matches(&self, form: Form, deposit: &Deposit, x: i32, y: i32, z: i32) -> bool {
        match form {
            Form::Nest { threshold, .. } => {
                self.primary.sample(x, y, z) > threshold + (1.0 - deposit.abundance) * ABUNDANCE_SHIFT
            }
            Form::Vein { width, extent, .. } => {
                let secondary = self.secondary.as_ref().expect("Ader ohne zweite Noise");
                self.primary.sample(x, y, z).abs() < width * (0.6 + 0.4 * deposit.abundance)
                    && secondary.sample(x, y, z) > extent
            }
            Form::Placer => false,
        }
    }
}

/// Setzt Erze in ein Dichtefeld (nach dem Höhlen-Carving). Gibt die Zahl der Erz-Samples zurück.
pub(super) fn apply_ores(
    noise: &OreNoise,
    provinces: &ProvinceMap,
    world: &WorldGen,
    column: &ColumnData,
    shell: &ShellHeights,
    chunk: &mut DensityChunk,
) -> usize {
    let origin = chunk.origin();
    let span = (CHUNK_SIZE - SAMPLE_MIN + 1) as usize;
    let lowest_y = (origin.y + SAMPLE_MIN) as f32;
    let highest_y = (origin.y + CHUNK_SIZE) as f32;

    // 1. Provinz je Sample-Spalte. Ohne ein einziges Sample unterhalb der Schale: nichts zu tun.
    let mut column_province: Vec<ProvinceId> = Vec::with_capacity(span * span);
    let mut defs: HashMap<ProvinceId, Province> = HashMap::new();
    let mut any_deep = false;
    for z in SAMPLE_MIN..=CHUNK_SIZE {
        for x in SAMPLE_MIN..=CHUNK_SIZE {
            let id = provinces.province_at((origin.x + x) as f32, (origin.z + z) as f32);
            defs.entry(id).or_insert_with(|| provinces.province(world, id));
            column_province.push(id);
            any_deep |= shell.lowest(x, z) - lowest_y >= SURFACE_SHELL;
        }
    }
    if !any_deep {
        return 0;
    }

    // 2. Gitter nur für Erzarten, die hier vorkommen und deren Höhenbereich den Chunk schneidet.
    let mut grids: HashMap<OreKind, KindGrids> = HashMap::new();
    for province in defs.values() {
        for deposit in &province.deposits {
            let rule = deposit.kind.rule();
            let in_range = rule.y_range.0 <= highest_y && rule.y_range.1 >= lowest_y;
            if rule.form != Form::Placer && in_range && !grids.contains_key(&deposit.kind) {
                let position = (chunk.position.x, chunk.position.y, chunk.position.z);
                grids.insert(deposit.kind, KindGrids::build(noise, deposit.kind, position));
            }
        }
    }

    // 3. Pro Sample das erste passende Vorkommen.
    let mut placed = 0;
    let mut index = 0;
    for z in SAMPLE_MIN..=CHUNK_SIZE {
        for x in SAMPLE_MIN..=CHUNK_SIZE {
            let province = &defs[&column_province[index]];
            index += 1;
            let lowest_surface = shell.lowest(x, z);
            let surface = column.height(x, z);
            let placer_column = province.deposit(OreKind::GoldSand).is_some()
                && column.river(x, z) > PLACER_RIVER
                && noise.placer.get((origin.x + x) as f32, (origin.z + z) as f32) > PLACER_THRESHOLD;
            for y in SAMPLE_MIN..=CHUNK_SIZE {
                let host = chunk.material(x, y, z);
                if host.is_air() {
                    continue;
                }
                let world_y = (origin.y + y) as f32;
                let shell_depth = lowest_surface - world_y;
                // Schutzregel: innerhalb der Schale nie ein Erz.
                if shell_depth < SURFACE_SHELL {
                    continue;
                }
                for deposit in &province.deposits {
                    let rule = deposit.kind.rule();
                    if !rule.hosts.contains(&host) || world_y < rule.y_range.0 || world_y > rule.y_range.1 {
                        continue;
                    }
                    let hit = match rule.form {
                        Form::Placer => placer_column && (PLACER_BAND.0..PLACER_BAND.1).contains(&shell_depth),
                        form => {
                            surface - world_y >= rule.min_depth
                                && grids.get(&deposit.kind).is_some_and(|g| g.matches(form, deposit, x, y, z))
                        }
                    };
                    if hit {
                        chunk.set_material(x, y, z, rule.material);
                        placed += 1;
                        break;
                    }
                }
            }
        }
    }
    placed
}
