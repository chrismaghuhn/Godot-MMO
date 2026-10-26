//! Fernterrain (#33, D-015): zwei Stufen grober Kacheln um die Nahzone.
//!
//! - Stufe 1: 256-m-Kacheln mit 4-m-Raster, in den 3×3 Stufe-2-Feldern um die Kamera.
//!   Zellen in der Nahzone werden ausgespart.
//! - Stufe 2: 1024-m-Kacheln mit 16-m-Raster, in den 7×7 Feldern ohne die inneren 3×3.
//!
//! **Aussparung (Nahzone):** Ihr Mittelpunkt rastet auf `HOLE_SNAP` Chunks ein und folgt der
//! Kamera mit Hysterese (erst ab `HOLE_SLACK` Chunks Abstand), damit Randkacheln selten neu gebaut
//! werden. Übernommen wird eine Aussparung erst, wenn **alle Spalten darin geladen sind**; sonst
//! bleibt die alte (falls noch vollständig geladen) oder es gibt keine. Ohne Aussparung liegt
//! das Fernterrain etwas tiefer unter der Nahzone, man sieht also nie durch den Boden – auch
//! nicht beim Start, beim Teleport oder wenn die Spalten beim schnellen Flug nachhängen. Eine
//! alte Aussparung wird aufgegeben, bevor die Spalten darunter entfernt werden.
//!
//! **Austausch:** Eine nicht mehr gewünschte Kachel verschwindet erst, wenn alle gewünschten
//! Kacheln über ihrer Fläche fertig sind (keine Lücken, kaum Überlappung).

use std::collections::HashMap;

use glam::{IVec2, Vec3, Vec3Swizzles};

use super::columns::Columns;
use super::jobs::{JobKind, Ticket};
use super::wanted::camera_chunk;
use super::{Built, Event, Frame};
use crate::lod::FarTile;
use crate::volume::CHUNK_SIZE;

const CHUNK: f32 = CHUNK_SIZE as f32;
const LEVEL1_TILE: f32 = 256.0;
const LEVEL1_STEP: f32 = 4.0;
const LEVEL2_TILE: f32 = 1024.0;
const LEVEL2_STEP: f32 = 16.0;
/// Ring der Stufe-2-Felder um die Kamera: innen Stufe 1, außen Stufe 2.
const LEVEL1_RING: i32 = 1;
pub(super) const LEVEL2_RING: i32 = 3;
/// Leicht tiefer, damit überlappendes Fernterrain nie durch feineres Terrain scheint.
const LEVEL1_DROP: f32 = 0.4;
const LEVEL2_DROP: f32 = 1.2;
/// Raster (in Chunks), auf das die Aussparung einrastet.
const HOLE_SNAP: i32 = 4;
/// So weit (Chunks, Chebyshev) darf sich die Kamera vom Mittelpunkt entfernen, bevor er wandert.
pub(super) const HOLE_SLACK: i32 = 3;

/// Sichtweite des Fernterrains von der Feldmitte (für Kamera-Sichtweite und Wasser).
pub const FAR_VIEW: f32 = LEVEL2_TILE * (LEVEL2_RING as f32 + 0.5);
/// Garantierte Reichweite von jeder Kameraposition aus (Kamera am Feldrand): Nebel endet davor.
pub const FAR_GUARANTEED: f32 = LEVEL2_TILE * LEVEL2_RING as f32;
/// Größter Sichtradius (Chunks), bei dem die Aussparung sicher in Stufe 1 liegt:
/// (radius + 1) · 32 m ≤ 1024 m Abstand von der Kamera zum Rand der Stufe 1.
pub const MAX_VIEW_RADIUS: i32 = 30;

/// Eine Fernkachel: Stufe (1 oder 2) und Kachel-Koordinaten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FarKey {
    pub level: u8,
    pub x: i32,
    pub z: i32,
}

impl FarKey {
    /// Ursprung der Kachel in Weltkoordinaten, leicht abgesenkt (Mesh-Positionen sind lokal dazu).
    pub fn origin(self) -> Vec3 {
        let size = tile_size(self.level);
        let drop = if self.level == 1 { LEVEL1_DROP } else { LEVEL2_DROP };
        Vec3::new(self.x as f32 * size, -drop, self.z as f32 * size)
    }
}

/// Ausgesparter Bereich (Weltkoordinaten, halboffen) oder keiner. Werte sind ganzzahlige
/// Vielfache von 32 und damit exakt vergleichbar.
pub(super) type Rect = (f32, f32, f32, f32);
pub(super) type Hole = Option<Rect>;

pub(super) struct FarBuild<M> {
    key: FarKey,
    hole: Hole,
    mesh: Option<M>,
}

pub(super) struct Far<M> {
    /// Eingerasteter Mittelpunkt (Chunks) und die gerade gültige Aussparung.
    snap: Option<IVec2>,
    hole: Option<(IVec2, Rect)>,
    /// Gezeichnete Kacheln (ob mit Mesh) mit der Aussparung, mit der sie gebaut wurden.
    tiles: HashMap<FarKey, (bool, Hole)>,
    pending: HashMap<FarKey, (Ticket, Hole)>,
    ready: Vec<FarBuild<M>>,
}

impl<M> Default for Far<M> {
    fn default() -> Self {
        Self { snap: None, hole: None, tiles: HashMap::new(), pending: HashMap::new(), ready: Vec::new() }
    }
}

impl<M: Send + 'static> Far<M> {
    pub(super) fn tile_count(&self) -> usize {
        self.tiles.values().filter(|(mesh, _)| *mesh).count()
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Vergisst einen Job, der nicht fertig wird (Panic).
    pub(super) fn forget(&mut self, id: u64) {
        self.pending.retain(|_, (ticket, _)| ticket.id != id);
    }

    pub(super) fn accept(&mut self, id: u64, build: FarBuild<M>) {
        if self.pending.get(&build.key).is_some_and(|(ticket, _)| ticket.id == id) {
            self.pending.remove(&build.key);
            self.ready.push(build);
        }
    }

    pub(super) fn update(&mut self, frame: &mut Frame<M>, columns: &Columns<M>) {
        let position = frame.camera;
        let limits = frame.ctx.config.far_limits;

        // Aussparung nur über vollständig geladenen Spalten.
        let center = camera_chunk(position).xz();
        let snap = snap_center(center, self.snap);
        self.snap = Some(snap);
        let loaded = |snap: IVec2, inner: i32| hole_chunks(snap, inner).all(|c| columns.is_loaded(c));
        self.hole = match hole_inner(frame.ctx.config.view_radius) {
            Some(inner) if loaded(snap, inner) => Some((snap, hole_rect(snap, inner))),
            // Alte Aussparung nur behalten, solange die Kamera nah genug ist: Spalten werden erst ab
            // `view_radius + 2` entfernt, die alte Aussparung reicht bei ≤ HOLE_SLACK + 1 Abstand
            // höchstens bis `view_radius`. So bleiben 2 Chunks Zeit für den Neubau der Randkacheln,
            // bevor dahinter Spalten verschwinden (Review-Fund: sonst kurz ein Loch hinter der Kamera).
            Some(inner) => self.hole.filter(|(old, _)| (center - *old).abs().max_element() <= HOLE_SLACK + 1 && loaded(*old, inner)),
            None => None,
        };
        let wanted = wanted_tiles(position, self.hole.map(|(_, rect)| rect));

        // Fertige Jobs übernehmen (veraltete verwerfen).
        let ready = std::mem::take(&mut self.ready);
        let mut inserted = 0;
        for build in ready {
            if wanted.get(&build.key) != Some(&build.hole) {
                continue;
            }
            if inserted >= limits.inserts {
                self.ready.push(build);
                continue;
            }
            inserted += 1;
            let mesh = build.mesh.is_some();
            // Alte Version derselben Kachel erst jetzt ersetzen (keine Lücke).
            let old = self.tiles.insert(build.key, (mesh, build.hole));
            if mesh || old.is_some_and(|(old, _)| old) {
                frame.events.push(Event::ShowFar { tile: build.key, mesh: build.mesh });
            }
        }

        // Neue Jobs: fehlende oder mit veralteter Aussparung, nächste zuerst.
        let mut missing: Vec<(FarKey, Hole, f32)> = wanted
            .iter()
            .filter(|(key, hole)| {
                self.tiles.get(key).map(|t| &t.1) != Some(hole)
                    && self.pending.get(key).map(|p| &p.1) != Some(hole)
                    && !self.ready.iter().any(|b| b.key == **key && b.hole == **hole)
            })
            .map(|(key, hole)| {
                let rect = tile_rect(*key);
                let (cx, cz) = ((rect.0 + rect.2) * 0.5, (rect.1 + rect.3) * 0.5);
                (*key, *hole, (cx - position.x).hypot(cz - position.z))
            })
            .collect();
        missing.sort_by(|a, b| a.2.total_cmp(&b.2));
        let slots = limits.jobs.saturating_sub(self.pending.len());
        for (key, hole, _) in missing.into_iter().take(slots) {
            let ctx = frame.ctx.clone();
            let ticket = frame.pool.spawn(JobKind::Far, move || {
                let (size, step) = if key.level == 1 { (LEVEL1_TILE, LEVEL1_STEP) } else { (LEVEL2_TILE, LEVEL2_STEP) };
                let tile = FarTile {
                    origin_x: key.x as f32 * size,
                    origin_z: key.z as f32 * size,
                    cells: (size / step) as usize,
                    step,
                    skirt: if key.level == 1 { 12.0 } else { 40.0 },
                };
                let mesh = tile.build(&ctx.world, |x, z| hole.is_some_and(|h| x >= h.0 && x < h.2 && z >= h.1 && z < h.3));
                Built::Far(FarBuild { key, hole, mesh: (!mesh.is_empty()).then(|| (ctx.convert)(mesh)) })
            });
            self.pending.insert(key, (ticket, hole));
        }

        // Nicht mehr gewünschte Kacheln entfernen, sobald ihre Fläche von gewünschten, fertigen
        // Kacheln abgedeckt ist.
        let present: Vec<Rect> = self.tiles.keys().filter(|key| wanted.contains_key(key)).map(|key| tile_rect(*key)).collect();
        let wanted_rects: Vec<Rect> = wanted.keys().map(|key| tile_rect(*key)).collect();
        let events = &mut *frame.events;
        self.tiles.retain(|key, (mesh, _)| {
            if wanted.contains_key(key) {
                return true;
            }
            let rect = tile_rect(*key);
            let needed = wanted_rects.iter().filter(|r| overlaps(**r, rect)).count();
            let covered = present.iter().filter(|r| overlaps(**r, rect)).count();
            if covered < needed {
                return true;
            }
            if *mesh {
                events.push(Event::RemoveFar { tile: *key });
            }
            false
        });
    }
}

pub(super) fn tile_size(level: u8) -> f32 {
    if level == 1 { LEVEL1_TILE } else { LEVEL2_TILE }
}

pub(super) fn tile_rect(key: FarKey) -> Rect {
    let size = tile_size(key.level);
    let (x0, z0) = (key.x as f32 * size, key.z as f32 * size);
    (x0, z0, x0 + size, z0 + size)
}

pub(super) fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1
}

/// Eingerasteter Mittelpunkt mit Hysterese: bleibt, solange die Kamera höchstens `HOLE_SLACK`
/// Chunks davon entfernt ist, sonst auf das nächste `HOLE_SNAP`-Raster.
pub(super) fn snap_center(center: IVec2, current: Option<IVec2>) -> IVec2 {
    if let Some(current) = current
        && (center - current).abs().max_element() <= HOLE_SLACK
    {
        return current;
    }
    IVec2::new(
        (center.x as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
        (center.y as f32 / HOLE_SNAP as f32).round() as i32 * HOLE_SNAP,
    )
}

/// Halbe Seitenlänge (Chunks) der Aussparung: Mit Abweichung ≤ `HOLE_SLACK` liegt sie eine Reihe
/// innerhalb der geladenen Spalten (Radius `view_radius`). `None`, wenn kein Platz ist.
pub(super) fn hole_inner(view_radius: i32) -> Option<i32> {
    let inner = view_radius - 1 - HOLE_SLACK;
    (inner >= 0).then_some(inner)
}

/// Chunks der Aussparung um `snap`.
pub(super) fn hole_chunks(snap: IVec2, inner: i32) -> impl Iterator<Item = IVec2> {
    (-inner..=inner).flat_map(move |dz| (-inner..=inner).map(move |dx| snap + IVec2::new(dx, dz)))
}

pub(super) fn hole_rect(snap: IVec2, inner: i32) -> Rect {
    (
        (snap.x - inner) as f32 * CHUNK,
        (snap.y - inner) as f32 * CHUNK,
        (snap.x + inner + 1) as f32 * CHUNK,
        (snap.y + inner + 1) as f32 * CHUNK,
    )
}

/// Gewünschte Kacheln für eine Kameraposition und Aussparung.
pub(super) fn wanted_tiles(position: Vec3, hole: Hole) -> HashMap<FarKey, Hole> {
    let field = ((position.x / LEVEL2_TILE).floor() as i32, (position.z / LEVEL2_TILE).floor() as i32);
    let per_field = (LEVEL2_TILE / LEVEL1_TILE) as i32;
    let mut wanted = HashMap::new();
    for dz in -LEVEL2_RING..=LEVEL2_RING {
        for dx in -LEVEL2_RING..=LEVEL2_RING {
            let (fx, fz) = (field.0 + dx, field.1 + dz);
            if dx.abs() <= LEVEL1_RING && dz.abs() <= LEVEL1_RING {
                for sz in 0..per_field {
                    for sx in 0..per_field {
                        let key = FarKey { level: 1, x: fx * per_field + sx, z: fz * per_field + sz };
                        let own_hole = hole.filter(|&h| overlaps(h, tile_rect(key)));
                        wanted.insert(key, own_hole);
                    }
                }
            } else {
                wanted.insert(FarKey { level: 2, x: fx, z: fz }, None);
            }
        }
    }
    wanted
}
