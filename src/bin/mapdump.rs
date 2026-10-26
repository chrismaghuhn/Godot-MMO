use std::collections::HashMap;
use std::error::Error;

use image::{Rgb, RgbImage};
use terraforge::{FamilyKind, WorldGen};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(42);
    let res: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let area: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4096.0);
    let center_x: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let center_z: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);

    let world = WorldGen::new(seed);
    let sea = world.cfg.sea_level;
    let step = area / res as f32;

    let mut samples = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            let x = center_x + ((i as f32 + 0.5) / res as f32 - 0.5) * area;
            let z = center_z + ((j as f32 + 0.5) / res as f32 - 0.5) * area;
            samples.push(world.sample(x, z));
        }
    }

    let mut map = RgbImage::new(res as u32, res as u32);
    let mut biome_map = RgbImage::new(res as u32, res as u32);
    let mut counts: HashMap<FamilyKind, u64> = HashMap::new();
    let mut land_counts: HashMap<FamilyKind, u64> = HashMap::new();
    let mut first_coords: HashMap<FamilyKind, (f32, f32)> = HashMap::new();
    let mut min_height = f32::MAX;
    let mut max_height = f32::MIN;
    let mut height_sum = 0.0f64;
    let mut river_max = 0.0f32;
    let mut river_wide = 0u64;

    for j in 0..res {
        for i in 0..res {
            let index = j * res + i;
            let sample = &samples[index];
            let biome = world.biome_at(sample);
            *counts.entry(biome.kind).or_insert(0) += 1;
            if sample.height >= sea - 0.5 {
                *land_counts.entry(biome.kind).or_insert(0) += 1;
            }
            let x = center_x + ((i as f32 + 0.5) / res as f32 - 0.5) * area;
            let z = center_z + ((j as f32 + 0.5) / res as f32 - 0.5) * area;
            first_coords.entry(biome.kind).or_insert((x, z));
            min_height = min_height.min(sample.height);
            max_height = max_height.max(sample.height);
            height_sum += sample.height as f64;
            river_max = river_max.max(sample.river);
            if sample.river > 0.1 {
                river_wide += 1;
            }

            let family_color = world.biome_color(terraforge::BiomeRef {
                kind: biome.kind,
                variant: 0,
            });

            if sample.height < sea - 0.5 {
                let depth = ((sea - sample.height) / 80.0).clamp(0.0, 1.0);
                let color = lerp_color([92, 156, 176], [16, 46, 76], depth);
                map.put_pixel(i as u32, j as u32, Rgb(color));
                biome_map.put_pixel(i as u32, j as u32, Rgb(multiply(color, 0.55)));
                continue;
            }

            let left = heights(&samples, res, i.saturating_sub(1), j);
            let right = heights(&samples, res, (i + 1).min(res - 1), j);
            let down = heights(&samples, res, i, j.saturating_sub(1));
            let up = heights(&samples, res, i, (j + 1).min(res - 1));
            let nx = (left - right) / (2.0 * step);
            let nz = (down - up) / (2.0 * step);
            let normal = [nx, 1.0, nz];
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            let normal = [normal[0] / length, normal[1] / length, normal[2] / length];
            let light = [-0.55f32, 0.78, -0.38];
            let light_length =
                (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
            let ndotl =
                (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]) / light_length;
            let shade = 0.55 + 0.6 * ndotl.clamp(0.0, 1.0);

            let mut color = world.biome_color(biome);
            color = world.apply_altitude(color, sample.height);
            let color = multiply(color, shade);
            map.put_pixel(i as u32, j as u32, Rgb(color));
            let family_shaded = multiply(family_color, shade);
            biome_map.put_pixel(i as u32, j as u32, Rgb(family_shaded));
        }
    }

    std::fs::create_dir_all("maps")?;
    let path = format!("maps/map_{seed}_{res}.png");
    let biome_path = format!("maps/biomes_{seed}_{res}.png");
    map.save(&path)?;
    biome_map.save(&biome_path)?;

    println!("seed {seed}, {res}x{res}, {area} x {area} Bloecke");
    println!(
        "Hoehe: {:.1} .. {:.1}, Mittel {:.1}",
        min_height,
        max_height,
        height_sum / (res * res) as f64
    );
    println!("geschrieben: {path}, {biome_path}");
    println!(
        "Flussmaske: max {:.3}, >0.1 auf {:.2}% der Flaeche",
        river_max,
        river_wide as f64 / (res * res) as f64 * 100.0
    );
    print_counts("gesamt", &counts, (res * res) as u64);
    print_counts("Land", &land_counts, land_counts.values().sum());
    let mut river_spot = None;
    let mut river_spot_distance = f32::INFINITY;
    for j in 2..res - 2 {
        for i in 2..res - 2 {
            let sample = &samples[j * res + i];
            if sample.river < 0.7 || sample.height > sea {
                continue;
            }
            let x = center_x + ((i as f32 + 0.5) / res as f32 - 0.5) * area;
            let z = center_z + ((j as f32 + 0.5) / res as f32 - 0.5) * area;
            let inland = [(40.0, 0.0), (-40.0, 0.0), (0.0, 40.0), (0.0, -40.0)]
                .into_iter()
                .all(|(dx, dz)| {
                    let neighbor = world.sample(x + dx, z + dz);
                    neighbor.height > sea + 3.0 && neighbor.river < 0.05
                });
            if inland {
                let distance = x * x + z * z;
                if distance < river_spot_distance {
                    river_spot_distance = distance;
                    river_spot = Some((x, z));
                }
            }
        }
    }
    if let Some((x, z)) = river_spot {
        println!("Fluss-Spot: x={x:.0} z={z:.0}");
    }
    println!("Beispielkoordinaten:");
    for kind in FamilyKind::ALL {
        if let Some((x, z)) = first_coords.get(&kind) {
            println!("  {:<16} x={x:.0} z={z:.0}", kind.key());
        }
    }
    Ok(())
}

fn heights(samples: &[terraforge::ColumnSample], res: usize, i: usize, j: usize) -> f32 {
    samples[j * res + i].height
}

fn multiply(color: [u8; 3], factor: f32) -> [u8; 3] {
    [
        (color[0] as f32 * factor).clamp(0.0, 255.0) as u8,
        (color[1] as f32 * factor).clamp(0.0, 255.0) as u8,
        (color[2] as f32 * factor).clamp(0.0, 255.0) as u8,
    ]
}

fn lerp_color(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8,
    ]
}

fn print_counts(label: &str, counts: &HashMap<FamilyKind, u64>, total: u64) {
    if total == 0 {
        return;
    }
    println!("{label}:");
    let mut rows: Vec<(FamilyKind, u64)> = counts.iter().map(|(k, v)| (*k, *v)).collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (kind, count) in rows {
        println!(
            "  {:<16} {:5.1}%  ({count})",
            kind.key(),
            count as f64 / total as f64 * 100.0
        );
    }
}
