//! Cave Culling (#8, D-006, D-018): Höhlen-Chunks nur zeichnen, wenn sie von der Kamera aus über
//! Luft erreichbar sind.
//!
//! Suche über (Chunk, Eintrittsseite); ein Chunk wird über eine Seite verlassen, wenn sie mit der
//! Eintrittsseite durch Luft verbunden ist. Verbindungen kennen wir für Höhlen- und
//! Oberflächen-Chunks; reine Luft und noch fehlende Daten gelten als offen (vorsichtig), massive
//! Chunks ohne Daten als geschlossen.

use std::collections::{HashSet, VecDeque};

use glam::IVec3;

use super::wanted::distance;
use crate::occlusion::{FACES, FaceLinks, face_offset, opposite};

/// Ob ein Höhlen-Chunk gezeichnet wird. Jenseits der Suchgrenze weiß die Suche nichts, solche
/// Chunks (z. B. tiefe gegrabene Schächte, die das Streaming in allen Spalten hält) bleiben
/// sichtbar (Review-Fund: sonst sah man durch sie hindurch).
pub(super) fn shown_by_culling(position: IVec3, start: IVec3, limit: i32, reached: &HashSet<IVec3>) -> bool {
    distance(position, start) > limit || reached.contains(&position)
}

/// Von `start` über Luft erreichbare Chunks (Chebyshev-Abstand ≤ `limit`).
///
/// Wie bei Minecraft darf die Suche eine einmal eingeschlagene Richtung nicht umkehren: Wer
/// einmal nach unten gegangen ist, darf nicht mehr nach oben (Sichtlinien laufen von der Kamera
/// weg). Ohne diese Regel fand die Suche über Seen, Täler und Tunnel fast jede Höhle.
pub(super) fn reachable_chunks(start: IVec3, limit: i32, links_at: impl Fn(IVec3) -> FaceLinks) -> HashSet<IVec3> {
    let mut reached = HashSet::from([start]);
    let mut visited = HashSet::new();
    // (Chunk, Austrittsseite, bisher genutzte Richtungen als Bitmaske)
    let mut queue: VecDeque<(IVec3, usize, u8)> = (0..FACES).map(|face| (start, face, 0)).collect();
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
