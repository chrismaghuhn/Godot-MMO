//! Welche Spalten und Chunks um die Kamera gebraucht werden (reine Funktionen).

use glam::{IVec2, IVec3, Vec3};

use crate::volume::CHUNK_SIZE;

/// Chunk-Position eines Punkts.
pub fn camera_chunk(position: Vec3) -> IVec3 {
    (position / CHUNK_SIZE as f32).floor().as_ivec3()
}

/// Chebyshev-Abstand: größte Abweichung über alle Achsen.
pub(super) fn distance(a: IVec3, b: IVec3) -> i32 {
    (a - b).abs().max_element()
}

/// Ob eine Spalte im Quadrat mit Radius `radius` um `center` liegt.
pub(super) fn column_within(position: IVec2, center: IVec2, radius: i32) -> bool {
    (position.x - center.x).abs() <= radius && (position.y - center.y).abs() <= radius
}

/// Quadrierter Abstand zweier Spalten (Sortierung: nächste zuerst).
pub(super) fn column_distance(position: IVec2, center: IVec2) -> i32 {
    let d = position - center;
    d.x * d.x + d.y * d.y
}

/// Alle Spalten im Sichtradius mit ihrem quadrierten Abstand, Zeile für Zeile (z, dann x).
pub(super) fn columns_in_view(center: IVec2, radius: i32) -> impl Iterator<Item = (IVec2, i32)> {
    (-radius..=radius).flat_map(move |dz| (-radius..=radius).map(move |dx| (center + IVec2::new(dx, dz), dx * dx + dz * dz)))
}
