//! Höhlen-Streaming (#4): reine Höhlen-Chunks unter der Oberfläche, nur nahe der Kamera.
//!
//! Oberflächen-Chunks (auch mit Höhleneingängen) gehören zu den Spalten. Hier geht es nur um
//! Chunks, die laut Oberfläche massiv sind, aber Höhlen enthalten können, und um veränderte
//! massive Chunks. Sichtbar werden die Höhlen-Chunks über das Cave Culling (`culling`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{IVec2, IVec3, Vec3Swizzles};

use super::culling::{reachable_chunks, shown_by_culling};
use super::jobs::{JobKind, Ticket};
use super::wanted::{camera_chunk, distance};
use super::{Built, Context, CullStats, Event, Frame};
use crate::mesh::ChunkMesh;
use crate::occlusion::{FaceLinks, face_links};
use crate::volume::{ChunkKind, ColumnData, DensityChunk};

/// Ein bearbeiteter Höhlen-Chunk: ob er ein Mesh hat, ob es gezeichnet wird, Seiten-Verbindungen.
struct CaveEntry {
    mesh: bool,
    visible: bool,
    links: FaceLinks,
}

pub(super) struct CaveBuild<M> {
    position: IVec3,
    mesh: Option<M>,
    links: FaceLinks,
    /// Dichtefeld für die Kollision (#13); `None` bei massivem Fels ohne Hohlraum.
    density: Option<Arc<DensityChunk>>,
}

pub(super) struct Caves<M> {
    /// Bearbeitete Positionen (mit oder ohne Mesh).
    map: HashMap<IVec3, CaveEntry>,
    pending: HashMap<IVec3, Ticket>,
    ready: Vec<CaveBuild<M>>,
}

impl<M> Default for Caves<M> {
    fn default() -> Self {
        Self { map: HashMap::new(), pending: HashMap::new(), ready: Vec::new() }
    }
}

impl<M: Send + 'static> Caves<M> {
    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn ready_count(&self) -> usize {
        self.ready.len()
    }

    pub(super) fn mesh_count(&self) -> usize {
        self.map.values().filter(|entry| entry.mesh).count()
    }

    /// Vergisst einen Job, der nicht fertig wird (Panic).
    pub(super) fn forget(&mut self, id: u64) {
        self.pending.retain(|_, ticket| ticket.id != id);
    }

    pub(super) fn accept(&mut self, id: u64, build: CaveBuild<M>) {
        if self.pending.get(&build.position).is_some_and(|ticket| ticket.id == id) {
            self.pending.remove(&build.position);
            self.ready.push(build);
        }
    }

    pub(super) fn update(&mut self, frame: &mut Frame<M>) {
        let center = camera_chunk(frame.camera);
        let radius = frame.ctx.config.cave_radius;
        let limits = frame.ctx.config.caves;

        // Veränderte massive Chunks gelten in allen geladenen Spalten, nicht nur im Höhlen-Radius
        // und auch unter der Höhlengrenze (sonst stünde ein gegrabener Schacht offen über dem Nichts).
        let touched: HashSet<IVec3> = frame
            .ctx
            .edits
            .read()
            .expect("Edits-Lock vergiftet")
            .touched_chunks()
            .filter(|c| frame.cache.get(&c.xz()).is_some_and(|column| column.classify(c.y) == ChunkKind::Solid))
            .collect();
        let wanted = |position: IVec3, margin: i32| distance(position, center) <= radius + margin || touched.contains(&position);

        // 1. Weit entfernte Chunks entfernen (mit Budget), laufende Jobs dort verwerfen.
        let mut removals = limits.removals;
        let (events, field) = (&mut *frame.events, &mut *frame.field);
        self.map.retain(|&position, entry| {
            if wanted(position, 1) {
                return true;
            }
            if !entry.mesh {
                field.remove(position);
                return false;
            }
            if removals == 0 {
                return true;
            }
            removals -= 1;
            events.push(Event::RemoveCave { chunk: position });
            field.remove(position);
            false
        });
        self.pending.retain(|&position, _| wanted(position, 2));

        // 2. Fertige Jobs mit Budget einfügen (nächste zuerst).
        let mut ready = std::mem::take(&mut self.ready);
        ready.retain(|build| wanted(build.position, 1));
        ready.sort_by_key(|build| distance(build.position, center));
        let mut inserted = 0;
        for build in ready {
            if inserted >= limits.inserts {
                self.ready.push(build);
                continue;
            }
            let mesh = build.mesh.is_some();
            inserted += usize::from(mesh);
            match build.density {
                Some(density) => frame.field.insert(density, center),
                None => frame.field.remove(build.position),
            }
            let old = self.map.insert(build.position, CaveEntry { mesh, visible: true, links: build.links });
            // Neubau nach Graben: das alte Mesh wird erst jetzt ersetzt.
            if mesh || old.is_some_and(|old| old.mesh) {
                frame.events.push(Event::ShowCave { chunk: build.position, mesh: build.mesh });
            }
        }

        // 3. Neue Jobs für unbearbeitete Positionen unter der Oberfläche.
        let mut missing = Vec::new();
        if let Some(lowest) = frame.ctx.source.lowest_cave_chunk() {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    let column_position = center.xz() + IVec2::new(dx, dz);
                    // Ohne geladene Spalte keine Höhen: später erneut versuchen.
                    let Some(column) = frame.cache.get(&column_position) else {
                        continue;
                    };
                    for dy in -radius..=radius {
                        let chunk_y = center.y + dy;
                        let position = IVec3::new(column_position.x, chunk_y, column_position.y);
                        if chunk_y < lowest
                            || self.known(position)
                            // Oberflächen-Chunks lädt die Spalte, Luft braucht nichts.
                            || column.classify(chunk_y) != ChunkKind::Solid
                        {
                            continue;
                        }
                        missing.push((position, Arc::clone(column)));
                    }
                }
            }
        }
        for &position in &touched {
            if !self.known(position)
                && !missing.iter().any(|(p, _)| *p == position)
                && let Some(column) = frame.cache.get(&position.xz())
            {
                missing.push((position, Arc::clone(column)));
            }
        }
        missing.sort_by_key(|(position, _)| distance(*position, center));

        // Veränderte massive Chunks zuerst neu bauen (nur bereits bearbeitete Positionen; der Rest
        // bekommt die Änderung beim ersten Laden). Die Markierung bleibt, solange schon ein Job
        // für die Position läuft oder fertig wartet, und verschwindet erst, wenn der Neubau startet.
        let dirty: Vec<IVec3> = frame.dirty.caves.iter().copied().collect();
        let mut rebuilds = Vec::new();
        for position in dirty {
            if self.in_flight(position) {
                continue;
            }
            match frame.cache.get(&position.xz()) {
                Some(column) if self.map.contains_key(&position) => rebuilds.push((position, Arc::clone(column))),
                // Nicht geladen und kein Job: Beim Laden wird die Änderung ohnehin angewendet.
                _ => {
                    frame.dirty.caves.remove(&position);
                }
            }
        }
        missing.retain(|(p, _)| !rebuilds.iter().any(|(r, _)| r == p));
        rebuilds.extend(missing);

        let slots = limits.jobs.saturating_sub(self.pending.len());
        for (position, column) in rebuilds.into_iter().take(slots) {
            frame.dirty.caves.remove(&position);
            let ctx = frame.ctx.clone();
            let ticket = frame.pool.spawn(JobKind::Cave, move || Built::Cave(build_cave(&ctx, position, &column)));
            self.pending.insert(position, ticket);
        }
    }

    /// Setzt die Sichtbarkeit der Höhlen-Chunks nach dem Cave Culling. Nur Änderungen werden
    /// gemeldet; ohne Culling (`enabled = false`) ist alles sichtbar.
    pub(super) fn cull(&mut self, frame: &mut Frame<M>, enabled: bool) -> CullStats {
        let start = camera_chunk(frame.camera);
        let limit = frame.ctx.config.cave_radius + 2;
        let links_at = |position: IVec3| -> FaceLinks {
            if let Some(entry) = self.map.get(&position) {
                return entry.links;
            }
            if let Some(links) = frame.surface_links.get(&position) {
                return *links;
            }
            match frame.cache.get(&position.xz()).map(|column| column.classify(position.y)) {
                Some(ChunkKind::Solid) => FaceLinks::NONE,
                // Luft, noch nicht gebaute Oberfläche oder unbekannte Spalte: vorsichtig offen.
                _ => FaceLinks::ALL,
            }
        };
        let reached = if enabled { reachable_chunks(start, limit, links_at) } else { HashSet::new() };
        let mut stats = CullStats::default();
        for (&position, entry) in self.map.iter_mut().filter(|(_, entry)| entry.mesh) {
            let visible = !enabled || shown_by_culling(position, start, limit, &reached);
            if visible {
                stats.shown += 1;
            } else {
                stats.hidden += 1;
            }
            if entry.visible != visible {
                entry.visible = visible;
                frame.events.push(Event::CaveVisibility { chunk: position, visible });
            }
        }
        stats
    }

    fn in_flight(&self, position: IVec3) -> bool {
        self.pending.contains_key(&position) || self.ready.iter().any(|build| build.position == position)
    }

    fn known(&self, position: IVec3) -> bool {
        self.map.contains_key(&position) || self.in_flight(position)
    }
}

/// Baut einen Höhlen-Chunk (im Worker-Thread).
fn build_cave<M>(ctx: &Context<M>, position: IVec3, column: &ColumnData) -> CaveBuild<M> {
    let (world, source, edits) = (&*ctx.world, &*ctx.source, &*ctx.edits);
    // Lock nur kurz halten, damit Graben nicht auf Jobs wartet.
    let touched = edits.read().expect("Edits-Lock vergiftet").touches(position);
    let density = if touched {
        let mut chunk = source.materials_chunk(world, column, position.y);
        edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
        Some(chunk)
    } else {
        source.density_chunk(world, column, position.y)
    };
    let links = density.as_ref().map_or(FaceLinks::NONE, face_links);
    let mesh = density
        .as_ref()
        .map(|density| ChunkMesh::build(world, column, density))
        .filter(|mesh| !mesh.is_empty())
        .map(|mesh| (ctx.convert)(mesh));
    CaveBuild { position, mesh, links, density: density.map(Arc::new) }
}
