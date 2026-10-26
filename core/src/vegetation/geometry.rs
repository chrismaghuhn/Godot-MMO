//! Formen der Baumarten, aufgebaut aus den Bausteinen in `primitives`.

use glam::{Quat, Vec3};

use super::primitives::{Basis, Blob, Cone, Geo, Limb, Prism, lin, scale_color};
use super::rng::Rng;
use super::{TreeGeo, TreeStyle};

const TAU: f32 = std::f32::consts::TAU;

pub fn build_tree_geometry(style: TreeStyle, variant: usize) -> TreeGeo {
    let mut geo = Geo::default();
    let mut rng =
        Rng::new(0xBEEF_5EED ^ ((style.index() as u64) << 12) ^ (variant as u64 * 0x9E37));
    let rotation = rng.range(0.0, TAU);

    match style {
        TreeStyle::Broadleaf => broadleaf(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Conifer => conifer(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Palm => palm(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Cactus => cactus(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Mushroom => mushroom(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Crystal => crystal(&mut geo, variant, rotation, &mut rng),
        TreeStyle::Dead => dead(&mut geo, variant, rotation, &mut rng),
    }

    TreeGeo {
        positions: geo.pos,
        normals: geo.nrm,
        colors: geo.col,
        indices: geo.idx,
    }
}

fn broadleaf(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (trunk_h, canopy, tint) = match variant {
        0 => (3.6, Vec3::new(3.1, 2.5, 2.9), [96, 140, 72]),
        1 => (4.3, Vec3::new(3.7, 2.3, 3.5), [62, 106, 58]),
        _ => (5.0, Vec3::new(2.5, 2.9, 2.5), [154, 128, 56]),
    };
    let crown_y = trunk_h + canopy.y * 0.7;
    trunk(geo, crown_y, 0.27, 0.15, lin([104, 80, 56]), rotation);
    let canopy_color = lin(tint);

    geo.blob(Blob {
        center: Vec3::Y * crown_y,
        radii: canopy,
        sides: 6,
        color: canopy_color,
        basis: Basis::UPRIGHT,
        rotation,
    });

    for _ in 0..2 {
        let angle = rng.range(0.0, TAU);
        let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * canopy.x * 0.55;
        geo.blob(Blob {
            center: Vec3::Y * crown_y + offset - Vec3::Y * canopy.y * 0.15,
            radii: canopy * 0.55,
            sides: 5,
            color: scale_color(canopy_color, rng.range(0.82, 1.02)),
            basis: Basis::UPRIGHT,
            rotation: angle,
        });
    }
}

fn conifer(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (trunk_h, radii, layer_h, tint, snowy) = match variant {
        0 => (2.2, [1.95, 1.55, 1.1], 2.6, [46, 88, 62], false),
        1 => (2.8, [1.65, 1.3, 0.9], 3.1, [62, 100, 84], false),
        _ => (2.0, [1.95, 1.55, 1.05], 2.4, [74, 106, 90], true),
    };
    trunk(
        geo,
        trunk_h + layer_h * 1.9,
        0.28,
        0.16,
        lin([76, 62, 48]),
        rotation,
    );

    let mut color = lin(tint);
    let mut y = trunk_h;
    for (index, radius) in radii.iter().enumerate() {
        if index > 0 {
            color = scale_color(color, 1.06);
        }
        geo.cone(Cone {
            base: Vec3::Y * y,
            radius: *radius,
            height: layer_h,
            sides: 7,
            color,
            basis: Basis::UPRIGHT,
            rotation: rotation + index as f32 * 0.23,
        });
        y += layer_h * 0.55;
    }

    if snowy {
        geo.cone(Cone {
            base: Vec3::Y * (y - layer_h * 0.35),
            radius: radii[2] * 0.55,
            height: layer_h * 0.55,
            sides: 6,
            color: lin([232, 242, 248]),
            basis: Basis::UPRIGHT,
            rotation: rng.range(0.0, TAU),
        });
    }
}

fn palm(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (trunk_h, tint, lean) = match variant {
        0 => (7.6, [78, 136, 62], 0.9),
        _ => (5.4, [112, 146, 66], 0.5),
    };
    let trunk_color = lin([122, 96, 62]);
    let leaf_color = lin(tint);

    let mut previous = Vec3::ZERO;
    for index in 0..3 {
        let t = (index + 1) as f32 / 3.0;
        let next = Vec3::new(
            lean * t * t,
            trunk_h * t,
            (rotation + index as f32 * 0.5).cos() * 0.15 * t,
        );
        geo.limb(Limb {
            a: previous,
            b: next,
            radius: 0.24 - 0.03 * index as f32,
            sides: 5,
            color: trunk_color,
        });
        previous = next;
    }

    let fronds = 7;
    for index in 0..fronds {
        let angle = rotation + index as f32 / fronds as f32 * TAU + rng.range(-0.12, 0.12);
        let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
        let right = Vec3::Y.cross(direction).normalize_or_zero();
        let droop = if index % 2 == 0 { -0.15 } else { 0.05 };
        geo.blob(Blob {
            center: previous + direction * 1.35 + Vec3::Y * droop,
            radii: Vec3::new(0.42, 0.17, 1.7),
            sides: 5,
            color: scale_color(leaf_color, rng.range(0.86, 1.05)),
            basis: Basis {
                right,
                up: Vec3::Y,
                forward: direction,
            },
            rotation: 0.0,
        });
    }

    geo.blob(Blob {
        center: previous + Vec3::Y * 0.15,
        radii: Vec3::splat(0.5),
        sides: 5,
        color: scale_color(leaf_color, 0.9),
        basis: Basis::UPRIGHT,
        rotation,
    });
}

fn cactus(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (height, radius, tint, arms) = match variant {
        0 => (4.8, 0.42, [76, 122, 70], true),
        _ => (2.9, 0.62, [62, 112, 86], false),
    };
    let color = lin(tint);

    geo.prism(Prism {
        base: Vec3::ZERO,
        r_bottom: radius,
        r_top: radius * 0.82,
        height,
        sides: 6,
        color,
        rotation,
    });
    geo.blob(Blob {
        center: Vec3::Y * height,
        radii: Vec3::new(radius * 0.85, radius * 0.55, radius * 0.85),
        sides: 5,
        color: scale_color(color, 1.04),
        basis: Basis::UPRIGHT,
        rotation,
    });

    if arms {
        for side in [-1.0f32, 1.0] {
            let y = height * rng.range(0.48, 0.68);
            let elbow = Vec3::new(side * (radius + 0.75), y, 0.0);
            geo.limb(Limb {
                a: Vec3::new(side * radius * 0.4, y, 0.0),
                b: elbow,
                radius: 0.26,
                sides: 5,
                color,
            });
            let tip = elbow + Vec3::Y * rng.range(1.0, 1.7);
            geo.limb(Limb {
                a: elbow,
                b: tip,
                radius: 0.25,
                sides: 5,
                color,
            });
            geo.blob(Blob {
                center: tip,
                radii: Vec3::splat(0.26),
                sides: 4,
                color: scale_color(color, 1.05),
                basis: Basis::UPRIGHT,
                rotation: 0.0,
            });
        }
    }
}

fn mushroom(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (stem_h, cap, tint) = match variant {
        0 => (4.8, Vec3::new(2.7, 1.25, 2.7), [138, 96, 158]),
        1 => (3.4, Vec3::new(3.6, 1.35, 3.6), [170, 74, 88]),
        _ => (6.2, Vec3::new(2.2, 1.05, 2.2), [88, 184, 194]),
    };
    let stem_color = lin([226, 214, 190]);
    let cap_color = lin(tint);

    geo.prism(Prism {
        base: Vec3::ZERO,
        r_bottom: 0.42,
        r_top: 0.30,
        height: stem_h,
        sides: 6,
        color: stem_color,
        rotation,
    });
    geo.blob(Blob {
        center: Vec3::Y * stem_h,
        radii: cap,
        sides: 8,
        color: cap_color,
        basis: Basis::UPRIGHT,
        rotation,
    });

    for _ in 0..4 {
        let angle = rng.range(0.0, TAU);
        let distance = rng.range(0.5, cap.x * 0.62);
        let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * distance;
        geo.blob(Blob {
            center: Vec3::Y * (stem_h + cap.y * 0.55) + offset,
            radii: Vec3::new(0.34, 0.14, 0.34),
            sides: 4,
            color: lin([238, 236, 214]),
            basis: Basis::UPRIGHT,
            rotation: angle,
        });
    }
}

fn crystal(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (spires, tint) = match variant {
        0 => (4usize, [126, 196, 208]),
        1 => (3, [166, 124, 214]),
        _ => (5, [226, 186, 92]),
    };
    let color = lin(tint);

    geo.blob(Blob {
        center: Vec3::new(0.0, 0.15, 0.0),
        radii: Vec3::new(1.15, 0.45, 1.15),
        sides: 6,
        color: lin([64, 68, 78]),
        basis: Basis::UPRIGHT,
        rotation,
    });

    for index in 0..spires {
        let angle = rotation + index as f32 / spires as f32 * TAU + rng.range(-0.2, 0.2);
        let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
        let distance = if index == 0 {
            0.0
        } else {
            rng.range(0.45, 0.95)
        };
        let tilt_angle = if index == 0 {
            rng.range(0.0, 0.15)
        } else {
            rng.range(0.18, 0.55)
        };
        let tilt = Quat::from_axis_angle(Vec3::Y.cross(outward), tilt_angle);
        let height = rng.range(2.6, 7.4) * if index == 0 { 1.15 } else { 1.0 };
        geo.cone(Cone {
            base: outward * distance - Vec3::Y * 0.1,
            radius: rng.range(0.45, 0.85),
            height,
            sides: 5,
            color: scale_color(color, rng.range(0.84, 1.08)),
            basis: Basis::tilted(tilt),
            rotation: angle,
        });
    }
}

fn dead(geo: &mut Geo, variant: usize, rotation: f32, rng: &mut Rng) {
    let (height, tint) = match variant {
        0 => (5.6, [88, 76, 64]),
        _ => (4.2, [54, 48, 44]),
    };
    let color = lin(tint);

    geo.prism(Prism {
        base: Vec3::ZERO,
        r_bottom: 0.34,
        r_top: 0.16,
        height,
        sides: 5,
        color,
        rotation,
    });

    for index in 0..3 {
        let angle = rotation + index as f32 * 2.1 + rng.range(-0.25, 0.25);
        let y = height * (0.42 + index as f32 * 0.17);
        let end = Vec3::new(
            angle.cos() * rng.range(1.1, 1.9),
            y + rng.range(1.0, 1.8),
            angle.sin() * rng.range(1.1, 1.9),
        );
        geo.limb(Limb {
            a: Vec3::Y * y,
            b: end,
            radius: 0.10,
            sides: 4,
            color,
        });
        geo.limb(Limb {
            a: end,
            b: end + Vec3::new(angle.cos() * 0.7, 0.7, angle.sin() * 0.7),
            radius: 0.06,
            sides: 4,
            color,
        });
    }
}

fn trunk(geo: &mut Geo, height: f32, r_bottom: f32, r_top: f32, color: [f32; 3], rotation: f32) {
    geo.prism(Prism {
        base: Vec3::ZERO,
        r_bottom,
        r_top,
        height,
        sides: 5,
        color,
        rotation,
    });
}
