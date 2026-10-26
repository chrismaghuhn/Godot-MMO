//! Bäume auf die tatsächliche Volumen-Oberfläche setzen (#7).
//!
//! Die Baumplanung arbeitet auf der Höhenkarte. Höhleneingänge, gegrabene Löcher oder gebautes
//! Material verändern aber die echte Oberfläche. `settle_tree` sucht an der Wurzel die
//! Oberfläche im Dichtefeld und entscheidet, ob der Baum dort stehen kann.

use glam::{IVec3, Vec3};

/// So weit (m) darf die echte Oberfläche von der geplanten Höhe abweichen, sonst entfällt der Baum
/// (gegrabene Grube, Höhleneingang, zugeschüttet).
pub const GROUND_SEARCH: f32 = 1.5;
/// Freiraum über der Wurzel (m): kein Überhang, keine Höhlendecke, nichts Gebautes.
const CLEARANCE: f32 = 3.0;
/// Wie tief die Wurzel im Boden steckt (wie in der Planung).
const ROOT_DEPTH: f32 = 0.25;

/// Neue Wurzelhöhe (Welt-y) oder `None`, wenn der Baum nicht stehen kann.
///
/// `root` ist die geplante Wurzel in Weltkoordinaten, `density(voxel)` die aktuelle Dichte
/// (> 0 fest) inklusive Höhlen und Spieler-Änderungen. Die Oberfläche wird an den vier
/// Ecken der Zelle gesucht und an der Wurzelposition interpoliert (wie das Terrain-Mesh); fehlt
/// sie an einer Ecke, steht der Baum nicht (Review zu #7: nur eine gerundete Ecke ließ Bäume an
/// Hängen schweben).
pub fn settle_tree(root: Vec3, mut density: impl FnMut(IVec3) -> f32) -> Option<f32> {
    let planned = root.y + ROOT_DEPTH;
    let (x0, z0) = (root.x.floor() as i32, root.z.floor() as i32);
    let (tx, tz) = (root.x - x0 as f32, root.z - z0 as f32);
    let mut corners = [0.0f32; 4];
    for (i, (dx, dz)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
        corners[i] = corner_surface(x0 + dx, z0 + dz, planned, &mut density)?;
    }
    let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
    let surface = lerp(lerp(corners[0], corners[1], tx), lerp(corners[2], corners[3], tx), tz);
    Some(surface - ROOT_DEPTH)
}

/// Oberfläche an einer Ecke: höchster Übergang fest -> Luft nahe `planned`, mit Freiraum darüber.
fn corner_surface(x: i32, z: i32, planned: f32, density: &mut impl FnMut(IVec3) -> f32) -> Option<f32> {
    let top = (planned + GROUND_SEARCH).ceil() as i32 + 1;
    let bottom = (planned - GROUND_SEARCH).floor() as i32 - 1;
    let mut surface = None;
    let mut y = top;
    while y > bottom {
        let (below, above) = (density(IVec3::new(x, y - 1, z)), density(IVec3::new(x, y, z)));
        if below > 0.0 && above <= 0.0 {
            surface = Some((y - 1) as f32 + below / (below - above));
            break;
        }
        y -= 1;
    }
    let surface = surface?;
    if (surface - planned).abs() > GROUND_SEARCH {
        return None;
    }
    let first = surface.floor() as i32 + 1;
    let last = (surface + CLEARANCE).ceil() as i32;
    if (first..=last).any(|y| density(IVec3::new(x, y, z)) > 0.0) {
        return None;
    }
    Some(surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vegetation::plan_trees;
    use crate::volume::{CHUNK_SIZE, ColumnData};
    use crate::world::WorldGen;

    fn trees(world: &WorldGen, chunk_x: i32, chunk_z: i32) -> Vec<crate::vegetation::TreePlacement> {
        let column = ColumnData::sample(world, chunk_x, chunk_z);
        plan_trees(world.cfg.seed, world.cfg.sea_level, chunk_x, chunk_z, column.heights(), &column.biomes, &column.temperatures)
    }

    #[test]
    fn tree_ids_are_unique_stable_and_match_their_cell() {
        let world = WorldGen::new(42);
        let mut total = 0;
        for (chunk_x, chunk_z) in [(0, 0), (-3, 5), (7, -2), (-11, -9)] {
            let first = trees(&world, chunk_x, chunk_z);
            let again = trees(&world, chunk_x, chunk_z);
            assert_eq!(first.iter().map(|t| t.id).collect::<Vec<_>>(), again.iter().map(|t| t.id).collect::<Vec<_>>());
            let mut ids = std::collections::HashSet::new();
            for tree in &first {
                assert!(ids.insert(tree.id), "doppelte ID {:?}", tree.id);
                assert_eq!((tree.id.chunk_x, tree.id.chunk_z), (chunk_x, chunk_z));
                let cell = (tree.position.z.floor() as i32 * CHUNK_SIZE + tree.position.x.floor() as i32) as u16;
                assert_eq!(tree.id.cell, cell, "ID passt nicht zur Zelle");
            }
            total += first.len();
        }
        assert!(total > 20, "zu wenige Bäume im Test: {total}");
    }

    /// Ebene auf 10,5 m: Baum steht; Grube darunter, Überhang oder zugeschüttet: kein Baum.
    /// Review zu #7: Auf einem Hang muss die Wurzel auf der interpolierten Höhe an der echten
    /// Position sitzen, nicht auf der Höhe einer gerundeten Ecke.
    #[test]
    fn trees_on_slopes_sit_at_the_interpolated_height() {
        for (slope_x, slope_z) in [(0.3f32, 0.0f32), (0.0, -0.4), (0.25, 0.2)] {
            let ground = |x: f32, z: f32| 20.0 + slope_x * x + slope_z * z;
            let tilted = move |v: IVec3| ground(v.x as f32, v.z as f32) - v.y as f32;
            for (x, z) in [(3.2f32, 7.8f32), (10.7, 1.3), (5.5, 5.5), (0.1, 0.9)] {
                let root = Vec3::new(x, ground(x, z) - ROOT_DEPTH, z);
                let y = settle_tree(root, tilted).expect("Baum auf Hang");
                assert!((y - root.y).abs() < 0.05, "Hang {slope_x}/{slope_z} bei {x},{z}: {y} statt {}", root.y);
            }
        }
    }

    #[test]
    fn settle_rules_on_synthetic_ground() {
        let flat = |v: IVec3| 10.5 - v.y as f32;
        let root = Vec3::new(3.2, 10.25, 7.8);
        let y = settle_tree(root, flat).expect("Baum auf ebenem Boden");
        assert!((y - 10.25).abs() < 1e-4, "Wurzel {y}");
        let pit = |v: IVec3| if v.x == 3 && v.z == 8 { 4.0 - v.y as f32 } else { flat(v) };
        assert_eq!(settle_tree(root, pit), None, "Baum über einer Grube");
        // Überhang über dem Suchfenster der Oberfläche (sonst würde er selbst als Boden gelten).
        let overhang = |v: IVec3| if v.y == 13 { 1.0 } else { flat(v) };
        assert_eq!(settle_tree(root, overhang), None, "Baum unter einem Überhang");
        let buried = |v: IVec3| 13.5 - v.y as f32;
        assert_eq!(settle_tree(root, buried), None, "zugeschütteter Baum");
        let slightly_higher = |v: IVec3| 11.2 - v.y as f32;
        let y = settle_tree(root, slightly_higher).expect("kleine Abweichung ist erlaubt");
        assert!((y - 10.95).abs() < 1e-4);
    }
}
