//! Tests der Bausteine des Streamings: Culling-Suche, Fernkacheln, Worker-Threads. Der ganze
//! Streamer: `streamer_tests`.

use std::time::{Duration, Instant};

use glam::{IVec2, IVec3, Vec3, Vec3Swizzles};

use super::culling::{reachable_chunks, shown_by_culling};
use super::far::{HOLE_SLACK, LEVEL2_RING, hole_chunks, hole_inner, hole_rect, overlaps, snap_center, tile_rect, tile_size, wanted_tiles};
use super::jobs::{JobKind, JobPool};
use super::{MAX_VIEW_RADIUS, camera_chunk};
use crate::occlusion::FaceLinks;

// --- Cave Culling ---

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

// --- Fernterrain ---

/// Die Aussparung liegt für jede Kameraposition innerhalb der geladenen Spalten
/// (Chebyshev-Radius `view_radius` um den Kamera-Chunk), auch mit Hysterese. Eine noch
/// behaltene alte Aussparung (Abstand ≤ HOLE_SLACK + 1) reicht höchstens bis `view_radius`,
/// also vor die Entfernungsgrenze der Spalten (`view_radius + 2`).
#[test]
fn hole_stays_inside_loaded_columns() {
    for view_radius in 0..=MAX_VIEW_RADIUS {
        let Some(inner) = hole_inner(view_radius) else {
            assert!(view_radius <= HOLE_SLACK, "Radius {view_radius} ohne Aussparung");
            continue;
        };
        let mut snap = None;
        // Kamera wandert in beiden Richtungen über viele Chunks.
        let path = (-40..40).chain((-40..40).rev()).map(|i| IVec2::new(i, -i / 2));
        for center in path {
            let s = snap_center(center, snap);
            snap = Some(s);
            for chunk in hole_chunks(s, inner) {
                assert!((chunk - center).abs().max_element() < view_radius, "Radius {view_radius}: {chunk} außerhalb um {center}");
            }
            // Behaltene alte Aussparung: Mittelpunkt höchstens HOLE_SLACK + 1 von der Kamera.
            let slack = HOLE_SLACK + 1;
            for old in [IVec2::new(slack, slack), IVec2::new(-slack, slack), IVec2::new(slack, -slack), IVec2::new(-slack, -slack)] {
                for chunk in hole_chunks(center + old, inner) {
                    // Spalten werden erst ab view_radius + 2 entfernt.
                    assert!((chunk - center).abs().max_element() <= view_radius, "alte Aussparung reicht zu weit");
                }
            }
        }
    }
}

#[test]
fn hysteresis_avoids_flipping_back_and_forth() {
    let mut snap = Some(snap_center(IVec2::new(2, 0), None));
    let first = snap;
    for x in [1, 2, 1, 2, 3, 2, 1] {
        snap = Some(snap_center(IVec2::new(x, 0), snap));
        assert_eq!(snap, first, "Mittelpunkt wandert bei kleinen Bewegungen");
    }
}

/// Stufe 1 und 2 decken lückenlos und ohne Überlappung ein Quadrat um die Kamera ab, und
/// die Aussparung (bei maximalem Radius) liegt vollständig in Stufe 1.
#[test]
fn tiles_cover_the_area_without_gaps_or_overlaps() {
    for position in [Vec3::new(10.0, 0.0, 10.0), Vec3::new(1023.0, 0.0, -1.0), Vec3::new(-5000.0, 0.0, 3071.0)] {
        let center = camera_chunk(position).xz();
        let inner = hole_inner(MAX_VIEW_RADIUS).unwrap();
        let snap = snap_center(center, None);
        let hole = hole_rect(snap, inner);
        let wanted = wanted_tiles(position, Some(hole));
        let area: f32 = wanted.keys().map(|k| tile_size(k.level).powi(2)).sum();
        let side = tile_size(2) * (2 * LEVEL2_RING + 1) as f32;
        assert_eq!(area, side * side, "Fläche passt nicht (Lücke oder Überlappung)");
        let keys: Vec<_> = wanted.keys().collect();
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert!(!overlaps(tile_rect(**a), tile_rect(**b)), "Überlappung {a:?} {b:?}");
            }
        }
        let level1_hole_area: f32 = wanted
            .iter()
            .filter_map(|(k, h)| h.map(|h| (k, h)))
            .map(|(k, h)| {
                let r = tile_rect(*k);
                (r.2.min(h.2) - r.0.max(h.0)).max(0.0) * (r.3.min(h.3) - r.1.max(h.1)).max(0.0)
            })
            .sum();
        let hole_area = (hole.2 - hole.0) * (hole.3 - hole.1);
        assert_eq!(level1_hole_area, hole_area, "Aussparung ragt aus Stufe 1 heraus");
    }
}

// --- Worker-Threads ---

fn wait_for<R: Send + 'static>(pool: &mut JobPool<R>, count: usize) -> Vec<super::jobs::Done<R>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut done = Vec::new();
    while done.len() < count && Instant::now() < deadline {
        done.extend(pool.poll());
        std::thread::sleep(Duration::from_millis(1));
    }
    done
}

#[test]
fn dropped_tickets_skip_jobs_that_have_not_started() {
    let mut pool = JobPool::new(1);
    let (release, wait) = std::sync::mpsc::channel::<()>();
    // Der erste Job blockiert den einzigen Thread, bis der zweite abgebrochen ist.
    let first = pool.spawn(JobKind::Column, move || {
        wait.recv().ok();
        1
    });
    let second = pool.spawn(JobKind::Cave, || 2);
    let third = pool.spawn(JobKind::Far, || 3);
    drop(second);
    release.send(()).unwrap();
    let done = wait_for(&mut pool, 2);
    let ids: Vec<u64> = done.iter().map(|d| d.id).collect();
    assert_eq!(ids, vec![first.id, third.id], "abgebrochener Job lief trotzdem");
    assert_eq!(done[1].kind, JobKind::Far);
    assert_eq!(done[1].result.as_ref().ok(), Some(&3));
}

#[test]
fn panics_in_jobs_come_back_as_errors() {
    let mut pool: JobPool<()> = JobPool::new(1);
    let ticket = pool.spawn(JobKind::Column, || panic!("Job-Panic"));
    let done = wait_for(&mut pool, 1);
    assert_eq!(done[0].id, ticket.id);
    let payload = done.into_iter().next().unwrap().result.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"Job-Panic"));
}
