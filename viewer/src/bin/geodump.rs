//! Geologie-Bilder zum Prüfen von #44 und #45:
//! - Querschnitt: senkrechter Schnitt entlang x bei festem z (1 Pixel = 1 m), inkl. Höhlen
//!   und Erzen (vollständiges Material, auch im massiven Fels)
//! - Oberflächenkarte: Material direkt unter der Oberfläche (Boden bzw. freiliegender Fels)
//! - Provinzkarte: geologische Provinzen, eingefärbt nach ihrem seltensten Erz
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

    let provinces = province_map(&world, &underground, map_radius);
    let provinces_path = format!("maps/geo_provinces_{seed}.png");
    provinces.save(&provinces_path)?;

    println!("geschrieben: {section_path}, {map_path}, {provinces_path}");
    print_provinces(&world, &underground, map_radius);
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
            let chunk = chunks
                .entry((chunk_x, chunk_y, chunk_z))
                .or_insert_with(|| underground.materials_chunk(world, column, chunk_y));
            let material = chunk.material(local_x, local_y, local_z);
            let color = if !material.is_air() {
                world.materials.display_color(material)
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

/// Provinzen in Draufsicht (8 m pro Pixel): Farbe des seltensten Erzes, Grenzen dunkel.
fn province_map(world: &WorldGen, underground: &Underground, radius: i32) -> RgbImage {
    let step = 8;
    let size = (2 * radius / step) as u32;
    let mut image = RgbImage::new(size, size);
    let mut cache: HashMap<(i32, i32), [u8; 3]> = HashMap::new();
    let mut ids = vec![(0, 0); (size * size) as usize];
    for pz in 0..size as i32 {
        for px in 0..size as i32 {
            let (x, z) = ((-radius + px * step) as f32, (-radius + pz * step) as f32);
            let province = underground.province_at(world, x, z);
            ids[(pz as u32 * size + px as u32) as usize] = province.id;
            let color = *cache.entry(province.id).or_insert_with(|| {
                let rarest = province.deposits.first().map(|d| d.kind.material());
                let base = rarest.map_or([128, 128, 128], |m| world.materials.display_color(m));
                base.map(|c| ((c as u16 + 96) / 2) as u8)
            });
            image.put_pixel(px as u32, pz as u32, Rgb(color));
        }
    }
    for pz in 1..size {
        for px in 1..size {
            let id = ids[(pz * size + px) as usize];
            if id != ids[(pz * size + px - 1) as usize] || id != ids[((pz - 1) * size + px) as usize] {
                image.put_pixel(px, pz, Rgb([20, 20, 24]));
            }
        }
    }
    image
}

/// Liste der Provinzen im Kartenbereich mit ihren Erzen.
fn print_provinces(world: &WorldGen, underground: &Underground, radius: i32) {
    let mut seen = std::collections::BTreeMap::new();
    for z in (-radius..radius).step_by(64) {
        for x in (-radius..radius).step_by(64) {
            let province = underground.province_at(world, x as f32, z as f32);
            seen.entry(province.id).or_insert(province);
        }
    }
    println!("Provinzen im Kartenbereich ({}):", seen.len());
    for province in seen.values() {
        let ores: Vec<String> = province
            .deposits
            .iter()
            .map(|d| {
                let name = world.materials.get(d.kind.material()).map_or("?", |m| m.name.as_str());
                format!("{name} (Menge {:.0} %, Reinheit {:.0} %)", d.abundance * 100.0, d.base_purity * 100.0)
            })
            .collect();
        println!("  {:?}: {}", province.id, ores.join(", "));
    }
}
