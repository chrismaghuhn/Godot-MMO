//! Bäume (#12): pro Spalte und Baum-Mesh ein `MultiMeshInstance3D`.
//!
//! Im Viewer war jeder Baum eine eigene Entity (bis ~11.000). Hier teilen sich alle Bäume einer
//! Spalte mit derselben Art und Variante ein MultiMesh; die 21 Baum-Meshes (7 Arten × 3 Varianten)
//! werden einmal beim Start gebaut.

use godot::classes::base_material_3d::{CullMode, Flags};
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{ArrayMesh, MultiMesh, MultiMeshInstance3D, Node3D, StandardMaterial3D};
use godot::prelude::*;
use terraforge_core::vegetation::{TreePlacement, TreeStyle, build_tree_geometry};

use crate::mesh::MeshData;

/// Varianten pro Baumart (wie im Viewer).
const VARIANTS: usize = 3;

pub(crate) struct TreeAssets {
    meshes: Vec<Gd<ArrayMesh>>,
    material: Gd<StandardMaterial3D>,
}

fn mesh_index(tree: &TreePlacement) -> usize {
    tree.style.index() * VARIANTS + tree.variant
}

/// Lage eines Baums in seiner Spalte: erst skaliert (Höhe extra), dann um die Hochachse gedreht,
/// wie im Viewer.
fn instance_transform(tree: &TreePlacement) -> Transform3D {
    let rotation = Basis::from_axis_angle(Vector3::UP, tree.rotation);
    let scale = Basis::from_scale(Vector3::new(tree.scale, tree.scale * tree.scale_y, tree.scale));
    Transform3D::new(rotation * scale, Vector3::new(tree.position.x, tree.position.y, tree.position.z))
}

/// Die Bäume einer Spalte nach Mesh gruppiert: (Mesh-Index, Lage jedes Baums). Ohne Godot-Aufrufe,
/// damit es testbar ist (headless speichert Godot die Instanz-Transforms nicht).
fn instances(trees: &[TreePlacement]) -> Vec<(usize, Vec<Transform3D>)> {
    let mut groups: Vec<Vec<Transform3D>> = vec![Vec::new(); TreeStyle::ALL.len() * VARIANTS];
    for tree in trees {
        groups[mesh_index(tree)].push(instance_transform(tree));
    }
    groups.into_iter().enumerate().filter(|(_, group)| !group.is_empty()).collect()
}

impl TreeAssets {
    pub(crate) fn new() -> Self {
        let meshes = TreeStyle::ALL
            .iter()
            .flat_map(|&style| (0..VARIANTS).map(move |variant| MeshData::from_tree(build_tree_geometry(style, variant)).into_array_mesh()))
            .collect();
        // Wie im Viewer: matt und beidseitig (Blätter sind einfache Flächen).
        let mut material = StandardMaterial3D::new_gd();
        material.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        material.set_cull_mode(CullMode::DISABLED);
        material.set_roughness(0.88);
        material.set_metallic(0.0);
        material.set_specular(0.0);
        Self { meshes, material }
    }

    /// Hängt die Bäume einer Spalte an `column` (Positionen lokal zur Spalte). Gibt die Zahl der
    /// Bäume zurück.
    pub(crate) fn add_to(&self, column: &mut Gd<Node3D>, trees: &[TreePlacement]) -> usize {
        for (index, transforms) in instances(trees) {
            // Reihenfolge wichtig: Das Format lässt sich nur bei 0 Instanzen setzen.
            let mut multimesh = MultiMesh::new_gd();
            multimesh.set_transform_format(TransformFormat::TRANSFORM_3D);
            multimesh.set_mesh(&self.meshes[index]);
            multimesh.set_instance_count(transforms.len() as i32);
            for (i, transform) in transforms.into_iter().enumerate() {
                multimesh.set_instance_transform(i as i32, transform);
            }
            let mut instance = MultiMeshInstance3D::new_alloc();
            instance.set_multimesh(&multimesh);
            instance.set_material_override(&self.material);
            column.add_child(&instance);
        }
        trees.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terraforge_core::vegetation::TreeId;

    fn tree(rotation: f32, scale: f32, scale_y: f32) -> TreePlacement {
        TreePlacement {
            id: TreeId { chunk_x: 0, chunk_z: 0, cell: 0 },
            style: TreeStyle::Conifer,
            variant: 2,
            position: glam::Vec3::new(3.0, 40.0, 7.5),
            rotation,
            scale,
            scale_y,
        }
    }

    /// Wie `Transform::from_translation(p).with_rotation(Quat::from_rotation_y(r)).with_scale(..)` im
    /// Viewer: Ein Punkt des Baum-Meshes wird erst skaliert, dann gedreht, dann verschoben.
    #[test]
    fn transform_matches_the_viewer() {
        let placement = tree(0.7, 1.3, 1.1);
        let transform = instance_transform(&placement);
        let point = Vector3::new(1.0, 2.0, 0.5);
        let expected = glam::Vec3::new(3.0, 40.0, 7.5) + glam::Quat::from_rotation_y(0.7) * (glam::Vec3::new(1.0, 2.0, 0.5) * glam::Vec3::new(1.3, 1.3 * 1.1, 1.3));
        let got = transform * point;
        assert!((got - Vector3::new(expected.x, expected.y, expected.z)).length() < 1e-4, "{got:?} statt {expected:?}");
    }

    /// Jeder Baum landet genau einmal in der Gruppe seines Meshes, mit seiner eigenen Lage.
    #[test]
    fn every_tree_gets_its_own_instance_in_its_group() {
        let mut trees = Vec::new();
        for (i, style) in [TreeStyle::Conifer, TreeStyle::Palm, TreeStyle::Conifer, TreeStyle::Dead, TreeStyle::Conifer].into_iter().enumerate() {
            let mut placement = tree(i as f32 * 0.5, 1.0 + i as f32 * 0.1, 1.0);
            placement.style = style;
            placement.variant = i % 2;
            placement.position.x = i as f32 * 4.0;
            trees.push(placement);
        }
        let groups = instances(&trees);
        assert_eq!(groups.iter().map(|(_, transforms)| transforms.len()).sum::<usize>(), trees.len());
        for placement in &trees {
            let (_, transforms) = groups.iter().find(|(index, _)| *index == mesh_index(placement)).expect("Gruppe fehlt");
            let expected = instance_transform(placement);
            assert_eq!(transforms.iter().filter(|t| **t == expected).count(), 1, "Baum bei x = {}", placement.position.x);
        }
    }

    /// Die Mesh-Liste wird in der Reihenfolge von `TreeStyle::ALL` gebaut, `mesh_index` nutzt
    /// `TreeStyle::index`: Beides muss übereinstimmen.
    #[test]
    fn style_order_matches_the_index() {
        assert!(TreeStyle::ALL.iter().enumerate().all(|(i, style)| style.index() == i));
    }

    #[test]
    fn every_style_and_variant_has_its_own_mesh() {
        let indices: std::collections::HashSet<usize> = TreeStyle::ALL
            .iter()
            .flat_map(|&style| (0..VARIANTS).map(move |variant| mesh_index(&TreePlacement { style, variant, ..tree(0.0, 1.0, 1.0) })))
            .collect();
        assert_eq!(indices.len(), TreeStyle::ALL.len() * VARIANTS);
        assert!(indices.iter().all(|&i| i < TreeStyle::ALL.len() * VARIANTS));
    }
}
