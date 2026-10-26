//! Geologie-Bilder zum Prüfen von #44 (und später Erzen):
//! - Querschnitt: senkrechter Schnitt entlang x bei festem z (1 Pixel = 1 m), inkl. Höhlen
//! - Oberflächenkarte: Material direkt unter der Oberfläche (Boden bzw. freiliegender Fels)
//!
//! Aufruf: `cargo run --release --bin geodump -- [seed] [z] [x_start] [breite] [karte_radius]`
//! Höhlen kommen aus dem unsicheren Entwickler-Seed (nur Debug).

use std::collections::HashMap;
use std::error::Error;

use image::{Rgb, RgbImage};
use terraforge_core::{
    MaterialId, WorldGen,
    geology::material_at,
    underground::{SecretSeed, Underground},
    volume::{CHUNK_SIZE, ColumnData, DensityChunk},
};

const Y_TOP: i32 = 230;
const Y_BOTTOM: i32 = -170;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<i64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let seed = args.first().copied().unwrap_or(42) as u32;
    let slice_z = args.get(1).copied().unwrap_or(-80) as i32;
    let x_start = args.get(2).copied().unwrap_or(-200) as i32;
    let width = args.get(3).copied().unwrap_or(1200) as i32;
    let map_radius = args.get(4).copied().unwrap_or(1024) as i32;

    let world = WorldGen::new(seed);
    let underground = Underground::new(SecretSeed::insecure_dev_seed(seed), &world.cfg);
    std::fs::create_dir_all("maps")?;

    let section = cross_section(&world, &underground, slice_z, x_start, width);
    let section_path = format!("maps/geo_section_{seed}_z{slice_z}.png");
    section.save(&section_path)?;

    let (map, counts) = surface_map(&world, map_radius);
    let map_path = format!("maps/geo_surface_{seed}.png");
    map.save(&map_path)?;

    println!("geschrieben: {section_path}, {map_path}");
    let total: usize = counts.values().sum();
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1));
    println!("Oberflächenmaterial an Land (Anteil):");
    for (id, count) in rows {
        let name = world.materials.get(id).map_or("?", |m| m.name.as_str());
        println!("  {name:<12} {:5.1} %", count as f64 / total as f64 * 100.0);
    }
    Ok(())
}

/// Senkrechter Schnitt mit allen Materialien und Höhlen (Höhlenluft schwarz, Wasser blau).
fn cross_section(world: &WorldGen, underground: &Underground, slice_z: i32, x_start: i32, width: i32) -> RgbImage {
    let height = (Y_TOP - Y_BOTTOM) as u32;
    let mut image = RgbImage::new(width as u32, height);
    let mut columns: HashMap<(i32, i32), ColumnData> = HashMap::new();
    let mut chunks: HashMap<(i32, i32, i32), DensityChunk> = HashMap::new();
    let chunk_z = slice_z.div_euclid(CHUNK_SIZE);
    let local_z = slice_z.rem_euclid(CHUNK_SIZE);
    for px in 0..width {
        let x = x_start + px;
        let (chunk_x, local_x) = (x.div_euclid(CHUNK_SIZE), x.rem_euclid(CHUNK_SIZE));
        let column = columns
            .entry((chunk_x, chunk_z))
            .or_insert_with(|| ColumnData::sample(world, chunk_x, chunk_z));
        let surface = column.height(local_x, local_z);
        for py in 0..height as i32 {
            let y = Y_TOP - 1 - py;
            let (chunk_y, local_y) = (y.div_euclid(CHUNK_SIZE), y.rem_euclid(CHUNK_SIZE));
            let chunk = chunks.entry((chunk_x, chunk_y, chunk_z)).or_insert_with(|| {
                underground
                    .density_chunk(world, column, chunk_y)
                    .unwrap_or_else(|| DensityChunk::from_column(column, chunk_y))
            });
            let material = chunk.material(local_x, local_y, local_z);
            let color = if !material.is_air() {
                world.materials.color(material)
            } else if (y as f32) < surface {
                [10, 10, 14] // Höhle
            } else if (y as f32) < world.cfg.sea_level {
                [40, 90, 140] // Wasser
            } else {
                [150, 190, 225] // Himmel
            };
            image.put_pixel(px as u32, py as u32, Rgb(color));
        }
    }
    image
}

/// Draufsicht: Material knapp unter der Oberfläche (4 m pro Pixel), Wasser dunkel.
fn surface_map(world: &WorldGen, radius: i32) -> (RgbImage, HashMap<MaterialId, usize>) {
    let step = 4;
    let size = (2 * radius / step) as u32;
    let mut image = RgbImage::new(size, size);
    let mut counts = HashMap::new();
    let mut columns: HashMap<(i32, i32), ColumnData> = HashMap::new();
    for pz in 0..size as i32 {
        for px in 0..size as i32 {
            let (x, z) = (-radius + px * step, -radius + pz * step);
            let (chunk_x, chunk_z) = (x.div_euclid(CHUNK_SIZE), z.div_euclid(CHUNK_SIZE));
            let (local_x, local_z) = (x.rem_euclid(CHUNK_SIZE), z.rem_euclid(CHUNK_SIZE));
            let column = columns
                .entry((chunk_x, chunk_z))
                .or_insert_with(|| ColumnData::sample(world, chunk_x, chunk_z));
            let height = column.height(local_x, local_z);
            let material = material_at(column.geology(local_x, local_z), 0.25, height - 0.25);
            let mut color = world.materials.color(material);
            if height < world.cfg.sea_level {
                color = color.map(|c| (c as f32 * 0.45) as u8);
            } else {
                *counts.entry(material).or_insert(0) += 1;
            }
            image.put_pixel(px as u32, pz as u32, Rgb(color));
        }
    }
    (image, counts)
}
