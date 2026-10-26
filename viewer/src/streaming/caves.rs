//! Höhlen-Streaming (#4): reine Höhlen-Chunks unter der Oberfläche, nur nahe der Kamera.
//!
//! Oberflächen-Chunks (auch mit Höhleneingängen) gehören zu den Spalten. Hier geht es nur
//! um Chunks, die laut Oberfläche massiv sind, aber Höhlen enthalten können. Ohne
//! Begrenzung müsste der Viewer überall die Höhlensysteme mitrendern (bis Cave Culling, #8).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use terraforge_core::{
    mesh::ChunkMesh,
    occlusion::{FACES, FaceLinks, face_links, face_offset, opposite},
    underground::CAVE_MIN_Y,
    volume::ChunkKind,
};

use super::{CHUNK_SIZE, ChunkAssets, ColumnCache, DirtyChunks, SurfaceLinks, TerrainChunk, camera_chunk};
use crate::meshes::chunk_mesh_to_bevy;
use crate::{EditsRes, UndergroundRes, WorldRes, env_parse};

const CAVE_JOBS_IN_FLIGHT: usize = 6;
const CAVES_READY_PER_FRAME: usize = 4;
const CAVE_DESPAWNS_PER_FRAME: usize = 8;

/// Markiert Terrain-Chunks, die nur wegen Höhlen existieren.
#[derive(Component)]
pub struct CaveChunk;

/// Radius (in Chunks, Chebyshev-Abstand in 3D) um die Kamera, in dem Höhlen geladen werden.
/// Umgebungsvariable `TERRAFORGE_CAVE_RADIUS`, Standard 3 (≈ 100 m).
#[derive(Resource)]
pub(super) struct CaveSettings {
    radius: i32,
    cull: bool,
}

impl CaveSettings {
    pub(super) fn from_env() -> Self {
        Self {
            radius: env_parse("TERRAFORGE_CAVE_RADIUS").unwrap_or(3),
            cull: env_parse::<u32>("TERRAFORGE_CULL").unwrap_or(1) != 0,
        }
    }
}

/// Bearbeitete Positionen: `Some` = Chunk mit Mesh, `None` = geprüft, keine Höhle.
#[derive(Resource, Default)]
pub(super) struct CaveMap(HashMap<IVec3, CaveEntry>);

/// Ein bearbeiteter Höhlen-Chunk: Mesh-Entity (falls Hohlraum) und Verbindungen der Seiten (#8).
#[derive(Clone, Copy)]
pub(super) struct CaveEntry {
    entity: Option<Entity>,
    links: FaceLinks,
}

struct CaveBuild {
    position: IVec3,
    mesh: Option<Mesh>,
    links: FaceLinks,
}

#[derive(Resource, Default)]
pub struct CaveJobs {
    pending: HashMap<IVec3, Task<CaveBuild>>,
    ready: Vec<CaveBuild>,
}

impl CaveJobs {
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[derive(SystemParam)]
pub(super) struct CaveContext<'w> {
    world: Res<'w, WorldRes>,
    underground: Res<'w, UndergroundRes>,
    assets: Res<'w, ChunkAssets>,
    settings: Res<'w, CaveSettings>,
    cache: Res<'w, ColumnCache>,
    map: ResMut<'w, CaveMap>,
    jobs: ResMut<'w, CaveJobs>,
    edits: Res<'w, EditsRes>,
    dirty: ResMut<'w, DirtyChunks>,
}

/// Chebyshev-Abstand: größte Abweichung über alle Achsen.
fn distance(a: IVec3, b: IVec3) -> i32 {
    (a - b).abs().max_element()
}

pub(super) fn stream_caves(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, With<Camera3d>>,
    mut ctx: CaveContext,
) {
    let Ok(transform) = camera.single() else {
        return;
    };
    let center = camera_chunk(transform.translation);
    let radius = ctx.settings.radius;

    // Veränderte massive Chunks gelten in allen geladenen Spalten, nicht nur im Höhlen-Radius
    // und auch unter CAVE_MIN_Y (sonst stünde ein gegrabener Schacht offen über dem Nichts).
    let touched: std::collections::HashSet<IVec3> = ctx
        .edits
        .0
        .read()
        .expect("Edits-Lock vergiftet")
        .touched_chunks()
        .filter(|c| ctx.cache.0.get(&c.xz()).is_some_and(|column| column.classify(c.y) == ChunkKind::Solid))
        .collect();
    let wanted = |position: IVec3, margin: i32| distance(position, center) <= radius + margin || touched.contains(&position);

    // 1. Weit entfernte Chunks entfernen (mit Budget), laufende Jobs dort verwerfen.
    let mut despawn_budget = CAVE_DESPAWNS_PER_FRAME;
    ctx.map.0.retain(|&position, entry| {
        if wanted(position, 1) {
            return true;
        }
        match &entry.entity {
            Some(entity) if despawn_budget > 0 => {
                despawn_budget -= 1;
                commands.entity(*entity).despawn();
                false
            }
            Some(_) => true,
            None => false,
        }
    });
    ctx.jobs.pending.retain(|&position, _| wanted(position, 2));

    // 2. Fertige Jobs einsammeln und mit Budget einfügen (nächste zuerst).
    let mut finished = Vec::new();
    ctx.jobs.pending.retain(|_, task| match block_on(poll_once(task)) {
        Some(build) => {
            finished.push(build);
            false
        }
        None => true,
    });
    let mut ready = std::mem::take(&mut ctx.jobs.ready);
    ready.extend(finished);
    ready.retain(|build| wanted(build.position, 1));
    ready.sort_by_key(|build| distance(build.position, center));
    let mut inserted = 0;
    for build in ready {
        if inserted >= CAVES_READY_PER_FRAME {
            ctx.jobs.ready.push(build);
            continue;
        }
        let entity = build.mesh.map(|mesh| {
            inserted += 1;
            commands
                .spawn((
                    TerrainChunk,
                    CaveChunk,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(ctx.assets.terrain_material.clone()),
                    Transform::from_translation(build.position.as_vec3() * CHUNK_SIZE),
                ))
                .id()
        });
        // Neubau nach Graben: altes Mesh erst jetzt entfernen.
        if let Some(CaveEntry { entity: Some(old), .. }) = ctx.map.0.insert(build.position, CaveEntry { entity, links: build.links }) {
            commands.entity(old).despawn();
        }
    }

    // 3. Neue Jobs für unbearbeitete Positionen unter der Oberfläche.
    let lowest = (CAVE_MIN_Y / CHUNK_SIZE).floor() as i32;
    let mut missing = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let column_position = center.xz() + IVec2::new(dx, dz);
            // Ohne geladene Spalte keine Höhen: später erneut versuchen.
            let Some(column) = ctx.cache.0.get(&column_position) else {
                continue;
            };
            for dy in -radius..=radius {
                let chunk_y = center.y + dy;
                let position = IVec3::new(column_position.x, chunk_y, column_position.y);
                if chunk_y < lowest
                    || ctx.map.0.contains_key(&position)
                    || ctx.jobs.pending.contains_key(&position)
                    || ctx.jobs.ready.iter().any(|build| build.position == position)
                    // Oberflächen-Chunks lädt die Spalte, Luft braucht nichts.
                    || column.classify(chunk_y) != ChunkKind::Solid
                {
                    continue;
                }
                missing.push((position, Arc::clone(column)));
            }
        }
    }
    for &position in &touched {
        let known = ctx.map.0.contains_key(&position)
            || ctx.jobs.pending.contains_key(&position)
            || ctx.jobs.ready.iter().any(|build| build.position == position)
            || missing.iter().any(|(p, _)| *p == position);
        if !known && let Some(column) = ctx.cache.0.get(&position.xz()) {
            missing.push((position, Arc::clone(column)));
        }
    }
    missing.sort_by_key(|(position, _)| distance(*position, center));

    // Veränderte massive Chunks zuerst neu bauen (nur bereits bearbeitete Positionen; der Rest
    // bekommt die Änderung beim ersten Laden).
    // Die Markierung bleibt, solange schon ein Job für die Position läuft oder fertig wartet,
    // und verschwindet erst, wenn der Neubau-Job startet.
    let dirty: Vec<IVec3> = ctx.dirty.caves.iter().copied().collect();
    let mut rebuilds = Vec::new();
    for position in dirty {
        let in_flight = ctx.jobs.pending.contains_key(&position) || ctx.jobs.ready.iter().any(|b| b.position == position);
        if in_flight {
            continue;
        }
        match ctx.cache.0.get(&position.xz()) {
            Some(column) if ctx.map.0.contains_key(&position) => rebuilds.push((position, Arc::clone(column))),
            _ => {
                // Nicht geladen und kein Job: Beim Laden wird die Änderung ohnehin angewendet.
                ctx.dirty.caves.remove(&position);
            }
        }
    }
    missing.retain(|(p, _)| !rebuilds.iter().any(|(r, _)| r == p));
    rebuilds.extend(missing);
    let missing = rebuilds;

    let slots = CAVE_JOBS_IN_FLIGHT.saturating_sub(ctx.jobs.pending.len());
    for (position, column) in missing.into_iter().take(slots) {
        ctx.dirty.caves.remove(&position);
        let world = Arc::clone(&ctx.world.0);
        let underground = Arc::clone(&ctx.underground.0);
        let edits = Arc::clone(&ctx.edits.0);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let job_start = std::time::Instant::now();
            // Lock nur kurz halten, damit Graben nicht auf Jobs wartet.
            let touched = edits.read().expect("Edits-Lock vergiftet").touches(position);
            let density = if touched {
                let mut chunk = underground.materials_chunk(&world, &column, position.y);
                edits.read().expect("Edits-Lock vergiftet").apply(&mut chunk);
                Some(chunk)
            } else {
                underground.density_chunk(&world, &column, position.y)
            };
            let links = density.as_ref().map_or(FaceLinks::NONE, face_links);
            let mesh = density
                .map(|density| ChunkMesh::build(&world, &column, &density))
                .filter(|mesh| !mesh.is_empty())
                .map(chunk_mesh_to_bevy);
            crate::bench::record_job(crate::bench::JobKind::Cave, job_start);
            CaveBuild { position, mesh, links }
        });
        ctx.jobs.pending.insert(position, task);
    }
}

/// Cave Culling (#8, D-006): Höhlen-Chunks nur zeichnen, wenn sie von der Kamera aus über Luft
/// erreichbar sind. Suche über (Chunk, Eintrittsseite); ein Chunk wird über eine Seite verlassen,
/// wenn sie mit der Eintrittsseite durch Luft verbunden ist. Verbindungen kennen wir für Höhlen-
/// und Oberflächen-Chunks; reine Luft und noch fehlende Daten gelten als offen (vorsichtig),
/// massive Chunks ohne Daten als geschlossen.
#[allow(clippy::too_many_arguments)]
pub(super) fn cull_caves(
    camera: Query<&Transform, With<Camera3d>>,
    settings: Res<CaveSettings>,
    cache: Res<ColumnCache>,
    map: Res<CaveMap>,
    surface: Res<SurfaceLinks>,
    mut visibility: Query<&mut Visibility, With<CaveChunk>>,
    mut stats: ResMut<CullStats>,
    xray: Res<crate::xray::Xray>,
) {
    let Ok(transform) = camera.single() else {
        return;
    };
    let start = camera_chunk(transform.translation);
    let limit = settings.radius + 2;
    // `TERRAFORGE_CULL=0` zeichnet alles (zum Vergleichen); im Röntgenmodus sollen alle Höhlen
    // durch den Fels sichtbar sein.
    let cull = settings.cull && !xray.enabled;
    let links_at = |position: IVec3| -> FaceLinks {
        if let Some(entry) = map.0.get(&position) {
            return entry.links;
        }
        if let Some(links) = surface.0.get(&position) {
            return *links;
        }
        match cache.0.get(&position.xz()).map(|column| column.classify(position.y)) {
            Some(ChunkKind::Solid) => FaceLinks::NONE,
            // Luft, noch nicht gebaute Oberfläche oder unbekannte Spalte: vorsichtig offen.
            _ => FaceLinks::ALL,
        }
    };
    let reached = if cull { reachable_chunks(start, limit, links_at) } else { Default::default() };
    let (mut shown, mut hidden) = (0, 0);
    for (position, entry) in &map.0 {
        let Some(entity) = entry.entity else { continue };
        let Ok(mut current) = visibility.get_mut(entity) else { continue };
        let target = if !cull || shown_by_culling(*position, start, limit, &reached) {
            shown += 1;
            Visibility::Inherited
        } else {
            hidden += 1;
            Visibility::Hidden
        };
        if *current != target {
            *current = target;
        }
    }
    stats.shown = shown;
    stats.hidden = hidden;
}

/// Ob ein Höhlen-Chunk gezeichnet wird. Jenseits der Suchgrenze weiß die Suche nichts, solche
/// Chunks (z. B. tiefe gegrabene Schächte, die das Streaming in allen Spalten hält) bleiben
/// sichtbar (Review-Fund: sonst sah man durch sie hindurch).
fn shown_by_culling(position: IVec3, start: IVec3, limit: i32, reached: &std::collections::HashSet<IVec3>) -> bool {
    distance(position, start) > limit || reached.contains(&position)
}

/// Zahl der sichtbaren und ausgeblendeten Höhlen-Chunks (für das Overlay).
#[derive(Resource, Default)]
pub struct CullStats {
    pub shown: usize,
    pub hidden: usize,
}

/// Von `start` über Luft erreichbare Chunks (Chebyshev-Abstand ≤ `limit`).
///
/// Wie bei Minecraft darf die Suche eine einmal eingeschlagene Richtung nicht umkehren: Wer
/// einmal nach unten gegangen ist, darf nicht mehr nach oben (Sichtlinien laufen von der Kamera
/// weg). Ohne diese Regel fand die Suche über Seen, Täler und Tunnel fast jede Höhle.
fn reachable_chunks(start: IVec3, limit: i32, links_at: impl Fn(IVec3) -> FaceLinks) -> std::collections::HashSet<IVec3> {
    let mut reached = std::collections::HashSet::from([start]);
    let mut visited = std::collections::HashSet::new();
    // (Chunk, Austrittsseite, bisher genutzte Richtungen als Bitmaske)
    let mut queue: std::collections::VecDeque<(IVec3, usize, u8)> = (0..FACES).map(|face| (start, face, 0)).collect();
    while let Some((position, face, used)) = queue.pop_front() {
        if used & (1 << opposite(face)) != 0 {
            continue;
        }
        let next = position + IVec3::from(face_offset(face));
        if distance(next, start) > limit {
            continue;
        }
        let entry = opposite(face);
        let used = used | (1 << face);
        if !visited.insert((next, entry, used)) {
            continue;
        }
        let links = links_at(next);
        if !links.open(entry) {
            continue;
        }
        reached.insert(next);
        for exit in 0..FACES {
            if exit != entry && links.connects(entry, exit) {
                queue.push_back((next, exit, used));
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seiten: 0 -x, 1 +x, 2 -y, 3 +y, 4 -z, 5 +z.
    fn link(pairs: &[(usize, usize)]) -> FaceLinks {
        let mut links = FaceLinks::NONE;
        for &(a, b) in pairs {
            links.0[a] |= (1 << a) | (1 << b);
            links.0[b] |= (1 << a) | (1 << b);
        }
        links
    }

    #[test]
    fn chunks_beyond_the_search_limit_stay_visible() {
        let reached = reachable_chunks(IVec3::ZERO, 2, |_| FaceLinks::NONE);
        assert!(!shown_by_culling(IVec3::new(0, -2, 0), IVec3::ZERO, 2, &reached), "abgeschlossener Chunk in Reichweite sichtbar");
        assert!(shown_by_culling(IVec3::new(0, -3, 0), IVec3::ZERO, 2, &reached), "Chunk jenseits der Suchgrenze ausgeblendet");
        assert!(shown_by_culling(IVec3::ZERO, IVec3::ZERO, 2, &reached), "Kamera-Chunk ausgeblendet");
    }

    #[test]
    fn sealed_caves_below_the_camera_are_hidden() {
        // Kamera in einem offenen Chunk bei y = 0; darunter (y = -1) Fels; bei y = -2 eine
        // geschlossene Höhle, die nur seitlich (±x) verbunden ist.
        let links_at = |p: IVec3| match p.y {
            y if y >= 0 => FaceLinks::ALL,
            -1 => FaceLinks::NONE,
            -2 => link(&[(0, 1)]),
            _ => FaceLinks::NONE,
        };
        let reached = reachable_chunks(IVec3::ZERO, 3, links_at);
        assert!(!reached.iter().any(|p| p.y < 0), "geschlossene Höhle erreicht: {reached:?}");
    }

    #[test]
    fn caves_behind_an_entrance_are_visible() {
        // Eingang: Chunk (0,-1,0) verbindet oben (+y) mit unten (-y); darunter ein Tunnel nach +x.
        let links_at = |p: IVec3| {
            if p.y >= 0 {
                FaceLinks::ALL
            } else if p == IVec3::new(0, -1, 0) {
                link(&[(3, 2)])
            } else if p == IVec3::new(0, -2, 0) {
                link(&[(3, 1)])
            } else if p.y == -2 && p.x > 0 && p.z == 0 {
                link(&[(0, 1)])
            } else {
                FaceLinks::NONE
            }
        };
        let reached = reachable_chunks(IVec3::new(0, 1, 0), 4, links_at);
        for x in 0..=3 {
            assert!(reached.contains(&IVec3::new(x, -2, 0)), "Tunnelchunk {x} nicht erreicht");
        }
        assert!(!reached.contains(&IVec3::new(1, -1, 0)), "Fels neben dem Eingang erreicht");
    }

    /// Eine Höhle, die nur über einen Umweg zurück Richtung Kamera erreichbar wäre (erst hinunter,
    /// dann seitlich, dann wieder hinauf), ist von der Kamera aus nicht zu sehen.
    #[test]
    fn no_turning_back_towards_the_camera() {
        // Kamera bei (0,0,0) in offener Luft (y >= 0). Unter (2,-1,0) ein Schacht nach unten,
        // bei y = -2 ein Gang zurück zu x = -2, dort ein Schacht hinauf in eine Höhle (-2,-1,0),
        // die nach oben geschlossen ist.
        let links_at = |p: IVec3| {
            let (x, y, z) = (p.x, p.y, p.z);
            if z != 0 && y < 0 {
                return FaceLinks::NONE;
            }
            match (x, y) {
                (_, y) if y >= 0 => FaceLinks::ALL,
                (2, -1) => link(&[(3, 2)]),
                (2, -2) => link(&[(3, 0)]),
                (x, -2) if (-1..=1).contains(&x) => link(&[(0, 1)]),
                (-2, -2) => link(&[(1, 3)]),
                (-2, -1) => link(&[(2, 2)]),
                _ => FaceLinks::NONE,
            }
        };
        let reached = reachable_chunks(IVec3::ZERO, 4, links_at);
        assert!(reached.contains(&IVec3::new(2, -2, 0)), "Schacht unten nicht erreicht");
        // Der Gang führt zurück Richtung Kamera (-x nach +x) und liegt hinter Fels: auch unsichtbar.
        assert!(!reached.contains(&IVec3::new(-2, -2, 0)), "Gang hinter Fels sichtbar");
        assert!(!reached.contains(&IVec3::new(-2, -1, 0)), "Höhle nur über Umkehr erreichbar, trotzdem sichtbar");
    }

    #[test]
    fn entering_needs_an_open_face_on_the_entry_side() {
        // Höhle unter der Kamera, deren Luft die Oberseite nicht berührt (nur ±z verbunden).
        let links_at = |p: IVec3| if p.y == -1 { link(&[(4, 5)]) } else if p.y >= 0 { FaceLinks::ALL } else { FaceLinks::NONE };
        let reached = reachable_chunks(IVec3::ZERO, 2, links_at);
        assert!(!reached.contains(&IVec3::new(0, -1, 0)));
    }
}
