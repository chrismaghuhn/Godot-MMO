//! Spalten-Streaming: Spalten im Sichtradius, pro Spalte die Oberflächen-Chunks und Bäume.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{IVec2, IVec3, Vec3, Vec3Swizzles};

use super::jobs::{JobKind, Ticket};
use super::wanted::{camera_chunk, column_distance, column_within, columns_in_view};
use super::{Built, Context, Event, Frame};
use crate::mesh::ChunkMesh;
use crate::occlusion::{FaceLinks, face_links};
use crate::movement::Trunk;
use crate::vegetation::{TreeId, TreePlacement, column_trees, trunk};
use crate::volume::{self, ChunkKind, ColumnData, DensityChunk};

/// Ergebnis eines Spalten-Jobs: alle 3D-Chunks mit Oberfläche in dieser Spalte.
pub(super) struct ColumnBuild<M> {
    position: IVec2,
    column: Arc<ColumnData>,
    /// (Chunk-Höhe, Mesh) für jeden 3D-Chunk mit Oberfläche.
    meshes: Vec<(i32, M)>,
    /// Verbindungen der Seiten der gebauten Chunks (für Cave Culling, #8).
    links: Vec<(i32, FaceLinks)>,
    trees: Vec<TreePlacement>,
    /// Alle gebauten Dichtefelder (für Kollision nahe der Kamera, #13).
    fields: Vec<Arc<DensityChunk>>,
    /// Neubau nach Graben/Bauen: ersetzt die geladene Spalte.
    rebuild: bool,
    /// Stand der Änderungen beim Start des Jobs (`DensityField::epoch`).
    epoch: u64,
}

pub(super) struct Columns<M> {
    loaded: HashSet<IVec2>,
    pending: HashMap<IVec2, Ticket>,
    ready: Vec<ColumnBuild<M>>,
    /// Stämme der stehenden Bäume je geladener Spalte, für die Kollision (#58).
    trunks: HashMap<IVec2, Vec<(TreeId, Trunk)>>,
}

impl<M> Default for Columns<M> {
    fn default() -> Self {
        Self { loaded: HashSet::new(), pending: HashMap::new(), ready: Vec::new(), trunks: HashMap::new() }
    }
}

impl<M: Send + 'static> Columns<M> {
    pub(super) fn is_loaded(&self, position: IVec2) -> bool {
        self.loaded.contains(&position)
    }

    pub(super) fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn ready_count(&self) -> usize {
        self.ready.len()
    }

    /// Vergisst einen Job, der nicht fertig wird (Panic).
    pub(super) fn forget(&mut self, id: u64) {
        self.pending.retain(|_, ticket| ticket.id != id);
    }

    /// Übernimmt ein Job-Ergebnis, wenn der Job noch aktuell ist.
    pub(super) fn accept(&mut self, id: u64, build: ColumnBuild<M>) {
        if self.pending.get(&build.position).is_some_and(|ticket| ticket.id == id) {
            self.pending.remove(&build.position);
            self.ready.push(build);
        }
    }

    /// Stämme in den geladenen Spalten, die eine Kugel um `center` mit `radius` berühren können.
    pub(super) fn trunks_near(&self, center: Vec3, radius: f32) -> Vec<Trunk> {
        let size = volume::CHUNK_SIZE as f32;
        let (low, high) = ((center.xz() - radius - 1.0) / size, (center.xz() + radius + 1.0) / size);
        let mut found = Vec::new();
        for z in low.y.floor() as i32..=high.y.floor() as i32 {
            for x in low.x.floor() as i32..=high.x.floor() as i32 {
                let Some(trunks) = self.trunks.get(&IVec2::new(x, z)) else { continue };
                found.extend(trunks.iter().map(|(_, trunk)| *trunk).filter(|trunk| trunk.base.xz().distance(center.xz()) <= radius + trunk.radius));
            }
        }
        found
    }

    /// Ein gefällter Baum kollidiert sofort nicht mehr, nicht erst nach dem Neubau der Spalte.
    pub(super) fn forget_trunk(&mut self, id: TreeId) {
        if let Some(trunks) = self.trunks.get_mut(&IVec2::new(id.chunk_x, id.chunk_z)) {
            trunks.retain(|(tree, _)| *tree != id);
        }
    }

    pub(super) fn update(&mut self, frame: &mut Frame<M>) {
        let center = camera_chunk(frame.camera).xz();
        let radius = frame.ctx.config.view_radius;
        let limits = frame.ctx.config.columns;

        // 1. Spalten außerhalb des Radius entfernen (mit Budget).
        let mut removals = limits.removals;
        let (cache, surface_links, events, field) = (&mut *frame.cache, &mut *frame.surface_links, &mut *frame.events, &mut *frame.field);
        let trunks = &mut self.trunks;
        self.loaded.retain(|&position| {
            if column_within(position, center, radius + 1) || removals == 0 {
                return true;
            }
            removals -= 1;
            events.push(Event::RemoveColumn { column: position });
            trunks.remove(&position);
            field.remove_column(position);
            cache.remove(&position);
            surface_links.retain(|p, _| p.xz() != position);
            false
        });

        // 2. Veraltete Jobs verwerfen, fertige mit Budget einfügen (nächste zuerst).
        self.pending.retain(|&position, _| column_within(position, center, radius + 2));
        let mut ready = std::mem::take(&mut self.ready);
        ready.retain(|build| (build.rebuild || !self.loaded.contains(&build.position)) && column_within(build.position, center, radius + 2));
        ready.sort_by_key(|build| column_distance(build.position, center));
        let mut inserted = 0;
        for build in ready {
            if !column_within(build.position, center, radius + 1) || (!build.rebuild && self.loaded.contains(&build.position)) {
                continue;
            }
            if inserted >= limits.inserts {
                self.ready.push(build);
                continue;
            }
            self.insert(frame, build);
            inserted += 1;
        }

        // 3. Neubau veränderter Spalten zuerst. Läuft für die Spalte schon ein Job oder wartet ein
        // fertiger auf das Einfügen, bleibt die Markierung (der Job kennt die Änderung evtl. nicht)
        // und der Neubau folgt danach. Die Markierung verschwindet erst, wenn ein Job startet.
        let mut missing = Vec::new();
        let dirty: Vec<IVec2> = frame.dirty.columns.iter().copied().collect();
        for position in dirty {
            if self.in_flight(position) {
                continue;
            }
            if self.loaded.contains(&position) {
                missing.push((position, -1, true));
            } else {
                // Nicht geladen und kein Job: Beim Laden wird die Änderung ohnehin angewendet.
                frame.dirty.columns.remove(&position);
            }
        }
        for (position, distance) in columns_in_view(center, radius) {
            if !self.loaded.contains(&position) && !self.in_flight(position) {
                missing.push((position, distance, false));
            }
        }
        missing.sort_by_key(|(_, distance, _)| *distance);

        let slots = limits.jobs.saturating_sub(self.pending.len());
        for (position, _, rebuild) in missing.into_iter().take(slots) {
            if rebuild {
                frame.dirty.columns.remove(&position);
            }
            // Beim Neubau die vorhandenen Höhen wiederverwenden.
            let cached = if rebuild { frame.cache.get(&position).cloned() } else { None };
            let (ctx, epoch) = (frame.ctx.clone(), frame.field.epoch());
            let ticket = frame.pool.spawn(JobKind::Column, move || Built::Column(build_column(&ctx, position, cached, rebuild, epoch)));
            self.pending.insert(position, ticket);
        }
    }

    fn in_flight(&self, position: IVec2) -> bool {
        self.pending.contains_key(&position) || self.ready.iter().any(|build| build.position == position)
    }

    fn insert(&mut self, frame: &mut Frame<M>, mut build: ColumnBuild<M>) {
        let position = build.position;
        frame.cache.insert(position, Arc::clone(&build.column));
        frame.surface_links.retain(|p, _| p.xz() != position);
        for &(chunk_y, links) in &build.links {
            frame.surface_links.insert(IVec3::new(position.x, chunk_y, position.y), links);
        }
        // Erneut gegen gefällte Bäume filtern: Der Job kann vor dem Fällen gestartet sein.
        {
            let edits = frame.ctx.edits.read().expect("Edits-Lock vergiftet");
            build.trees.retain(|tree| !edits.is_felled(tree.id));
        }
        let camera = camera_chunk(frame.camera);
        for chunk in build.fields {
            frame.field.insert(chunk, build.epoch, camera);
        }
        let size = volume::CHUNK_SIZE as f32;
        let origin = Vec3::new(position.x as f32 * size, 0.0, position.y as f32 * size);
        self.trunks.insert(position, build.trees.iter().map(|tree| (tree.id, trunk(tree, origin))).collect());
        // Ein Neubau ersetzt die alte Spalte erst jetzt, wo die neue da ist (kein Flackern).
        self.loaded.insert(position);
        frame.events.push(Event::ShowColumn { column: position, chunks: build.meshes, trees: build.trees });
    }
}

/// Baut die Oberflächen-Chunks und Bäume einer Spalte (im Worker-Thread).
fn build_column<M>(ctx: &Context<M>, position: IVec2, cached: Option<Arc<ColumnData>>, rebuild: bool, epoch: u64) -> ColumnBuild<M> {
    let (world, source, edits) = (&*ctx.world, &*ctx.source, &*ctx.edits);
    let column = cached.unwrap_or_else(|| Arc::new(ColumnData::sample(world, position.x, position.y)));
    // Chunks mit Oberfläche (inkl. Höhleneingängen) und veränderte Chunks, die laut Generator
    // Luft sind (z. B. Gebautes). Massive Chunks – auch veränderte – lädt das Höhlen-Streaming.
    // Das Lock nur kurz halten, damit Graben nicht auf Jobs wartet.
    let touched = edits.read().expect("Edits-Lock vergiftet").touched_in_column(position.x, position.y);
    let mut heights: Vec<i32> = column.surface_chunks().collect();
    for &chunk_y in &touched {
        if column.classify(chunk_y) == ChunkKind::Air && !heights.contains(&chunk_y) {
            heights.push(chunk_y);
        }
    }
    // Dichtefelder bleiben für die Bäume erhalten (#7).
    let mut fields: HashMap<i32, DensityChunk> = HashMap::new();
    let mut meshes = Vec::new();
    let mut links = Vec::new();
    for chunk_y in heights {
        let density = if touched.contains(&chunk_y) {
            let mut chunk = source.materials_chunk(world, &column, chunk_y);
            edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
            chunk
        } else {
            match source.density_chunk(world, &column, chunk_y) {
                Some(chunk) => chunk,
                None => continue,
            }
        };
        let mesh = ChunkMesh::build(world, &column, &density);
        if !mesh.is_empty() {
            meshes.push((chunk_y, (ctx.convert)(mesh)));
        }
        links.push((chunk_y, face_links(&density)));
        fields.insert(chunk_y, density);
    }

    // Bäume auf die echte Oberfläche setzen (Höhleneingänge, Gegrabenes, Gebautes) und gefällte
    // weglassen. Fehlende Dichtefelder (meist massiv) werden nur bei Bedarf gebaut.
    let density = |voxel: IVec3| {
        let chunk_y = voxel.y.div_euclid(volume::CHUNK_SIZE);
        let chunk = fields.entry(chunk_y).or_insert_with(|| {
            let mut chunk = source.materials_chunk(world, &column, chunk_y);
            edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
            chunk
        });
        let o = chunk.origin();
        chunk.get(voxel.x - o.x, voxel.y - o.y, voxel.z - o.z)
    };
    let mut trees = column_trees(world, &column, ctx.config.clearing, density);
    // Gefällte Bäume einmal pro Spalte abfragen (ein konsistenter Stand).
    {
        let edits = edits.read().expect("Edits-Lock vergiftet");
        trees.retain(|tree| !edits.is_felled(tree.id));
    }
    let fields = fields.into_values().map(Arc::new).collect();
    ColumnBuild { position, column, meshes, links, trees, fields, rebuild, epoch }
}
