//! Welche Bäume in einer Spalte wirklich stehen, und welcher anvisiert ist (#7, #18).
//!
//! Gemeinsam für das Streaming (Bäume zeigen), den Server (Fällen prüfen) und den Godot-Client
//! ohne Server, damit alle dieselben Bäume an denselben Stellen sehen.

use std::sync::Arc;

use glam::{IVec2, IVec3, Vec2, Vec3};

use super::{TreeId, TreePlacement, TreeStyle, plan_trees, settle_tree};
use crate::edits::action::{REACH, ray_hit};
use crate::material::MaterialId;
use crate::movement::Trunk;
use crate::volume::{CHUNK_SIZE, ColumnData};
use crate::world::WorldGen;

/// Baumfreier Kreis um den Spawnpunkt (m), im Viewer, im Godot-Client und auf dem Server gleich.
pub const SPAWN_CLEARING: f32 = 17.0;
/// Höhe eines Baums bei Maßstab 1 (m), für das Anvisieren.
const TREE_HEIGHT: f32 = 5.0;
/// So nah muss der Strahl dem Stamm kommen (m).
const PICK_RADIUS: f32 = 1.2;

/// Die geplanten Bäume der Spalte, ohne die im freien Kreis `clearing` (Mittelpunkt x/z, Radius),
/// auf die echte Oberfläche gesetzt (`density` in Weltkoordinaten). Positionen lokal zur Spalte.
/// Gefällte Bäume sind noch enthalten.
pub fn column_trees(world: &WorldGen, column: &ColumnData, clearing: Option<(Vec2, f32)>, mut density: impl FnMut(IVec3) -> f32) -> Vec<TreePlacement> {
    let size = CHUNK_SIZE as f32;
    let (chunk_x, chunk_z) = (column.chunk_x, column.chunk_z);
    let mut trees = plan_trees(world.cfg.seed, world.cfg.sea_level, chunk_x, chunk_z, column.heights(), &column.biomes, &column.temperatures);
    if let Some((center, radius)) = clearing {
        trees.retain(|tree| (Vec2::new(chunk_x as f32 * size + tree.position.x, chunk_z as f32 * size + tree.position.z) - center).length() > radius);
    }
    let origin = IVec3::new(chunk_x * CHUNK_SIZE, 0, chunk_z * CHUNK_SIZE);
    trees.retain_mut(|tree| {
        // Lokale Koordinaten: Voxel x/z liegen im eigenen Chunk (0..=32); geplante Höhe an den
        // Ecken aus der Spalte.
        match settle_tree(tree.position, |x, z| column.height(x, z), |v| density(v + origin)) {
            Some(y) => {
                tree.position.y = y;
                true
            }
            None => false,
        }
    });
    trees
}

/// Der vorderste Baum, dessen Stamm der Strahl bis `limit` trifft (Abstand < 1,2 m). `trees`:
/// ID, Fußpunkt in Weltkoordinaten, Baum.
pub fn pick_tree<'a>(trees: impl IntoIterator<Item = (Vec3, &'a TreePlacement)>, origin: Vec3, direction: Vec3, limit: f32) -> Option<TreeId> {
    let mut best: Option<(f32, TreeId)> = None;
    for (base, tree) in trees {
        let height = TREE_HEIGHT * tree.scale * tree.scale_y;
        for i in 0..=10 {
            let point = base + Vec3::Y * height * i as f32 / 10.0;
            let along = (point - origin).dot(direction);
            if !(0.0..=limit).contains(&along) {
                continue;
            }
            if (origin + direction * along).distance(point) < PICK_RADIUS && best.is_none_or(|(d, _)| along < d) {
                best = Some((along, tree.id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// Der anvisierte, noch stehende Baum in den 3×3 Spalten um das Auge, vor dem Gelände: Strahl bis
/// `REACH` bzw. kurz hinter den ersten festen Voxel. `column` liefert die Spalten, `density` die
/// Dichte (Weltkoordinaten), `felled` die gefällten Bäume. Gibt den Baum mit Fußpunkt in
/// Weltkoordinaten zurück.
pub fn targeted_tree(
    world: &WorldGen,
    mut column: impl FnMut(IVec2) -> Arc<ColumnData>,
    clearing: Option<(Vec2, f32)>,
    mut density: impl FnMut(IVec3) -> f32,
    felled: impl Fn(TreeId) -> bool,
    eye: Vec3,
    direction: Vec3,
) -> Option<(Vec3, TreePlacement)> {
    let limit = ray_hit(eye, direction, REACH, |voxel| density(voxel) > 0.0).map_or(REACH, |(_, distance)| distance + 0.5);
    let size = CHUNK_SIZE as f32;
    let center = IVec2::new((eye.x / size).floor() as i32, (eye.z / size).floor() as i32);
    let mut trees = Vec::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let column = column(center + IVec2::new(dx, dz));
            let origin = Vec3::new(column.chunk_x as f32, 0.0, column.chunk_z as f32) * size;
            let standing = column_trees(world, &column, clearing, &mut density);
            trees.extend(standing.into_iter().filter(|tree| !felled(tree.id)).map(|tree| (origin + tree.position, tree)));
        }
    }
    let id = pick_tree(trees.iter().map(|(base, tree)| (*base, tree)), eye, direction, limit)?;
    trees.into_iter().find(|(_, tree)| tree.id == id)
}

/// Ausbeute beim Fällen: Material und Anzahl (≈ m³) nach Art und Größe.
pub fn felling_yield(tree: &TreePlacement) -> (MaterialId, u32) {
    let material = match tree.style {
        TreeStyle::Crystal => MaterialId::QUARTZ,
        _ => MaterialId::WOOD,
    };
    (material, (4.0 * tree.scale * tree.scale * tree.scale_y).round().max(1.0) as u32)
}

/// Stamm eines Baums als Kollisionsform (#58), Fuß in Weltkoordinaten (`origin`: Ursprung der
/// Spalte). Maße aus `geometry.rs` (Stamm bis zum Ansatz der Krone, ohne Äste), skaliert wie der
/// Baum: Radius mit `scale`, Höhe mit `scale · scale_y`.
pub fn trunk(tree: &TreePlacement, origin: Vec3) -> Trunk {
    let (radius, height) = match (tree.style, tree.variant) {
        (TreeStyle::Broadleaf, 0) => (0.24, 3.6),
        (TreeStyle::Broadleaf, 1) => (0.24, 4.3),
        (TreeStyle::Broadleaf, _) => (0.24, 5.0),
        (TreeStyle::Conifer, 0) => (0.25, 2.2),
        (TreeStyle::Conifer, 1) => (0.25, 2.8),
        (TreeStyle::Conifer, _) => (0.25, 2.0),
        // Die Palme neigt sich; bis Kopfhöhe ist sie fast senkrecht.
        (TreeStyle::Palm, _) => (0.24, 2.5),
        (TreeStyle::Cactus, 0) => (0.42, 4.8),
        (TreeStyle::Cactus, _) => (0.62, 2.9),
        (TreeStyle::Mushroom, 0) => (0.38, 4.8),
        (TreeStyle::Mushroom, 1) => (0.38, 3.4),
        (TreeStyle::Mushroom, _) => (0.38, 6.2),
        // Mehrere Spitzen, nach außen geneigt: eine dicke Kapsel um sie herum (Review-Fund: 0,6 m
        // ließ den Spieler 1-1,5 m tief in die Spitzen laufen).
        (TreeStyle::Crystal, _) => (1.6, 2.0),
        (TreeStyle::Dead, 0) => (0.28, 5.6),
        (TreeStyle::Dead, _) => (0.28, 4.2),
    };
    Trunk { base: origin + tree.position, radius: radius * tree.scale, height: height * tree.scale * tree.scale_y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vegetation::{TreeGeo, build_tree_geometry};

    /// Größter Abstand von der Achse, an dem das Mesh die Höhe `y` schneidet.
    fn extent_at(geo: &TreeGeo, y: f32) -> f32 {
        let mut extent = 0.0f32;
        for triangle in geo.indices.as_chunks::<3>().0 {
            let corners = triangle.map(|i| Vec3::from_array(geo.positions[i as usize]));
            for (a, b) in [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[0])] {
                if (a.y - y) * (b.y - y) <= 0.0 && a.y != b.y {
                    let point = a.lerp(b, (y - a.y) / (b.y - a.y));
                    extent = extent.max(Vec2::new(point.x, point.z).length());
                }
            }
        }
        extent
    }

    /// Der Stamm passt zum Mesh: In Hüfthöhe und Brusthöhe (soweit der Stamm reicht) zwischen 70 %
    /// und 130 % der Ausdehnung (Review-Fund: Kristalle waren 1-1,5 m dünner als ihre Spitzen).
    #[test]
    fn trunks_match_the_tree_meshes() {
        for style in TreeStyle::ALL {
            for variant in 0..3 {
                let geo = build_tree_geometry(style, variant);
                let tree = TreePlacement { id: TreeId { chunk_x: 0, chunk_z: 0, cell: 0 }, style, variant, position: Vec3::ZERO, rotation: 0.0, scale: 1.0, scale_y: 1.0 };
                let trunk = trunk(&tree, Vec3::ZERO);
                for y in [0.5, 1.3] {
                    if y > trunk.height {
                        continue;
                    }
                    let extent = extent_at(&geo, y);
                    assert!(
                        (0.7 * extent..=1.3 * extent).contains(&trunk.radius),
                        "{style:?} {variant}: Stamm {} bei Ausdehnung {extent} in {y} m",
                        trunk.radius
                    );
                }
            }
        }
    }
}
