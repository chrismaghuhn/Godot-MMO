//! Umwandlung der engine-unabhängigen Core-Meshes in Bevy-Meshes.

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use terraforge_core::{mesh::ChunkMesh, vegetation::TreeGeo};

pub fn chunk_mesh_to_bevy(mesh: ChunkMesh) -> Mesh {
    let colors: Vec<[f32; 4]> = mesh
        .colors
        .iter()
        .map(|&rgb| {
            let linear = srgb_to_linear(rgb);
            [linear[0], linear[1], linear[2], 1.0]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mesh.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(mesh.indices))
}

pub fn tree_geo_to_mesh(geo: TreeGeo) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, geo.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, geo.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, geo.colors)
    .with_inserted_indices(Indices::U32(geo.indices))
}

fn srgb_to_linear(color: [u8; 3]) -> [f32; 3] {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(color[0]), channel(color[1]), channel(color[2])]
}
