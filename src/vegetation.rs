use bevy::math::{Quat, Vec3};

use crate::biome::FamilyKind;
use crate::world::BiomeRef;

const TAU: f32 = std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeStyle {
    Broadleaf,
    Conifer,
    Palm,
    Cactus,
    Mushroom,
    Crystal,
    Dead,
}

impl TreeStyle {
    pub const ALL: [TreeStyle; 7] = [
        TreeStyle::Broadleaf,
        TreeStyle::Conifer,
        TreeStyle::Palm,
        TreeStyle::Cactus,
        TreeStyle::Mushroom,
        TreeStyle::Crystal,
        TreeStyle::Dead,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TreePlacement {
    pub style: TreeStyle,
    pub variant: usize,
    pub position: Vec3,
    pub rotation: f32,
    pub scale: f32,
    pub scale_y: f32,
}

#[derive(Default)]
pub struct TreeGeo {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

pub fn plan_trees<const N: usize>(
    seed: u32,
    sea_level: f32,
    chunk_x: i32,
    chunk_z: i32,
    heights: &[[f32; N]; N],
    biomes: &[[BiomeRef; N]; N],
) -> Vec<TreePlacement> {
    let cells = N.saturating_sub(3);
    let mut trees = Vec::new();

    for z in 0..cells {
        for x in 0..cells {
            let kind = biomes[z + 1][x + 1].kind;
            let base = density(kind);
            if base <= 0.0 {
                continue;
            }

            let cluster = hash01(
                seed,
                chunk_x,
                chunk_z,
                (x / 4) as u32,
                (z / 4) as u32,
                0x51ED,
            );
            let chance = base * (0.18 + 1.7 * cluster * cluster);

            let mut rng = Rng::new(hash_cell(seed, chunk_x, chunk_z, x as u32, z as u32, 0xA53));
            if rng.f32() >= chance {
                continue;
            }

            let h00 = heights[z + 1][x + 1];
            let h10 = heights[z + 1][x + 2];
            let h01 = heights[z + 2][x + 1];
            let h11 = heights[z + 2][x + 2];

            let anchor_x = rng.range(0.18, 0.82);
            let anchor_z = rng.range(0.18, 0.82);
            let height = lerp(lerp(h00, h10, anchor_x), lerp(h01, h11, anchor_x), anchor_z);
            if height < sea_level + 1.1 || height > sea_level + 103.0 {
                continue;
            }

            let slope = (h10 - h00).abs() + (h01 - h00).abs();
            if slope > 18.0 {
                continue;
            }

            let (style, variant) = pick_style(kind, &mut rng);
            if slope > 13.0 && style != TreeStyle::Cactus {
                continue;
            }

            let (scale_min, scale_max) = match style {
                TreeStyle::Conifer => (0.72, 1.05),
                TreeStyle::Crystal => (0.62, 1.00),
                TreeStyle::Cactus => (0.60, 0.95),
                _ => (0.65, 0.95),
            };
            trees.push(TreePlacement {
                style,
                variant,
                position: Vec3::new(x as f32 + anchor_x, height - 0.25, z as f32 + anchor_z),
                rotation: rng.range(0.0, TAU),
                scale: rng.range(scale_min, scale_max),
                scale_y: rng.range(0.90, 1.12),
            });
        }
    }

    trees
}

fn density(kind: FamilyKind) -> f32 {
    match kind {
        FamilyKind::TemperateForest => 0.050,
        FamilyKind::TropicalForest => 0.055,
        FamilyKind::ColdForest => 0.045,
        FamilyKind::Grassland => 0.006,
        FamilyKind::Desert => 0.004,
        FamilyKind::Snow => 0.007,
        FamilyKind::Mountains => 0.007,
        FamilyKind::Volcanic => 0.005,
        FamilyKind::Swamp => 0.030,
        FamilyKind::Coast => 0.005,
        FamilyKind::Underwater => 0.0,
        FamilyKind::Freshwater => 0.0,
        FamilyKind::FungiFlora => 0.042,
        FamilyKind::Crystal => 0.022,
        FamilyKind::Caves => 0.0,
    }
}

const STYLE_TEMPERATE: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Broadleaf, 0, 0.50),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Conifer, 0, 0.18),
    (TreeStyle::Broadleaf, 2, 0.10),
];

const STYLE_TROPICAL: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Palm, 0, 0.36),
    (TreeStyle::Broadleaf, 0, 0.26),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Palm, 1, 0.16),
];

const STYLE_COLD: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 0, 0.46),
    (TreeStyle::Conifer, 1, 0.32),
    (TreeStyle::Broadleaf, 1, 0.12),
    (TreeStyle::Dead, 0, 0.10),
];

const STYLE_GRASSLAND: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Broadleaf, 0, 0.62),
    (TreeStyle::Broadleaf, 1, 0.22),
    (TreeStyle::Dead, 0, 0.16),
];

const STYLE_DESERT: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Cactus, 0, 0.58),
    (TreeStyle::Cactus, 1, 0.24),
    (TreeStyle::Dead, 0, 0.18),
];

const STYLE_SNOW: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 2, 0.52),
    (TreeStyle::Conifer, 1, 0.35),
    (TreeStyle::Dead, 1, 0.13),
];

const STYLE_MOUNTAINS: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Conifer, 0, 0.52),
    (TreeStyle::Conifer, 1, 0.30),
    (TreeStyle::Crystal, 1, 0.10),
    (TreeStyle::Dead, 1, 0.08),
];

const STYLE_VOLCANIC: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Dead, 1, 0.68),
    (TreeStyle::Dead, 0, 0.22),
    (TreeStyle::Crystal, 1, 0.10),
];

const STYLE_SWAMP: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Dead, 0, 0.44),
    (TreeStyle::Broadleaf, 1, 0.40),
    (TreeStyle::Broadleaf, 2, 0.16),
];

const STYLE_COAST: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Palm, 0, 0.62),
    (TreeStyle::Palm, 1, 0.24),
    (TreeStyle::Broadleaf, 0, 0.14),
];

const STYLE_FUNGI: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Mushroom, 0, 0.40),
    (TreeStyle::Mushroom, 1, 0.25),
    (TreeStyle::Mushroom, 2, 0.25),
    (TreeStyle::Broadleaf, 1, 0.10),
];

const STYLE_CRYSTAL: &[(TreeStyle, usize, f32)] = &[
    (TreeStyle::Crystal, 0, 0.45),
    (TreeStyle::Crystal, 1, 0.30),
    (TreeStyle::Crystal, 2, 0.15),
    (TreeStyle::Conifer, 1, 0.10),
];

const STYLE_NONE: &[(TreeStyle, usize, f32)] = &[];

fn pick_style(kind: FamilyKind, rng: &mut Rng) -> (TreeStyle, usize) {
    let table = match kind {
        FamilyKind::TemperateForest => STYLE_TEMPERATE,
        FamilyKind::TropicalForest => STYLE_TROPICAL,
        FamilyKind::ColdForest => STYLE_COLD,
        FamilyKind::Grassland => STYLE_GRASSLAND,
        FamilyKind::Desert => STYLE_DESERT,
        FamilyKind::Snow => STYLE_SNOW,
        FamilyKind::Mountains => STYLE_MOUNTAINS,
        FamilyKind::Volcanic => STYLE_VOLCANIC,
        FamilyKind::Swamp => STYLE_SWAMP,
        FamilyKind::Coast => STYLE_COAST,
        FamilyKind::Freshwater => STYLE_NONE,
        FamilyKind::FungiFlora => STYLE_FUNGI,
        FamilyKind::Crystal => STYLE_CRYSTAL,
        FamilyKind::Underwater | FamilyKind::Caves => STYLE_NONE,
    };
    let mut roll = rng.f32();
    for &(style, variant, weight) in table {
        if roll < weight {
            return (style, variant);
        }
        roll -= weight;
    }
    (TreeStyle::Dead, 0)
}

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

#[derive(Clone, Copy)]
struct Basis {
    right: Vec3,
    up: Vec3,
    forward: Vec3,
}

impl Basis {
    const UPRIGHT: Self = Self {
        right: Vec3::X,
        up: Vec3::Y,
        forward: Vec3::Z,
    };

    fn tilted(rotation: Quat) -> Self {
        Self {
            right: rotation * Vec3::X,
            up: rotation * Vec3::Y,
            forward: rotation * Vec3::Z,
        }
    }
}

#[derive(Clone, Copy)]
struct Prism {
    base: Vec3,
    r_bottom: f32,
    r_top: f32,
    height: f32,
    sides: usize,
    color: [f32; 3],
    rotation: f32,
}

#[derive(Clone, Copy)]
struct Cone {
    base: Vec3,
    radius: f32,
    height: f32,
    sides: usize,
    color: [f32; 3],
    basis: Basis,
    rotation: f32,
}

#[derive(Clone, Copy)]
struct Blob {
    center: Vec3,
    radii: Vec3,
    sides: usize,
    color: [f32; 3],
    basis: Basis,
    rotation: f32,
}

#[derive(Clone, Copy)]
struct Limb {
    a: Vec3,
    b: Vec3,
    radius: f32,
    sides: usize,
    color: [f32; 3],
}

#[derive(Default)]
struct Geo {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Geo {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3]) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let base = self.pos.len() as u32;
        for vertex in [a, b, c] {
            self.pos.push(vertex.to_array());
            self.nrm.push(normal.to_array());
            self.col.push([color[0], color[1], color[2], 1.0]);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 3]) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    fn prism(&mut self, p: Prism) {
        let bottom = ellipse(
            p.base,
            Basis::UPRIGHT,
            p.r_bottom,
            p.r_bottom,
            p.sides,
            p.rotation,
        );
        let top = ellipse(
            p.base + Vec3::Y * p.height,
            Basis::UPRIGHT,
            p.r_top,
            p.r_top,
            p.sides,
            p.rotation,
        );
        for i in 0..p.sides {
            let j = (i + 1) % p.sides;
            self.quad(bottom[i], top[i], top[j], bottom[j], p.color);
        }
    }

    fn cone(&mut self, c: Cone) {
        let apex = c.base + c.basis.up * c.height;
        let ring = ellipse(c.base, c.basis, c.radius, c.radius, c.sides, c.rotation);
        for i in 0..c.sides {
            let j = (i + 1) % c.sides;
            self.tri(ring[i], apex, ring[j], c.color);
        }
    }

    fn blob(&mut self, b: Blob) {
        let top = b.center + b.basis.up * b.radii.y;
        let bottom = b.center - b.basis.up * b.radii.y;
        let hi = ellipse(
            b.center + b.basis.up * (b.radii.y * 0.55),
            b.basis,
            b.radii.x * 0.72,
            b.radii.z * 0.72,
            b.sides,
            b.rotation,
        );
        let lo = ellipse(
            b.center - b.basis.up * (b.radii.y * 0.05),
            b.basis,
            b.radii.x,
            b.radii.z,
            b.sides,
            b.rotation,
        );
        for i in 0..b.sides {
            let j = (i + 1) % b.sides;
            self.tri(top, hi[j], hi[i], b.color);
            self.quad(hi[i], hi[j], lo[j], lo[i], b.color);
            self.tri(lo[i], lo[j], bottom, b.color);
        }
    }

    fn limb(&mut self, l: Limb) {
        let axis = (l.b - l.a).normalize_or_zero();
        if axis == Vec3::ZERO {
            return;
        }
        let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
        let u = axis.cross(helper).normalize_or_zero();
        let v = axis.cross(u).normalize_or_zero();
        let start = limb_ring(l.a, u, v, l.radius, l.sides);
        let end = limb_ring(l.b, u, v, l.radius, l.sides);
        for i in 0..l.sides {
            let j = (i + 1) % l.sides;
            self.quad(start[i], start[j], end[j], end[i], l.color);
        }
    }
}

fn ellipse(center: Vec3, basis: Basis, rx: f32, rz: f32, sides: usize, rotation: f32) -> Vec<Vec3> {
    (0..sides)
        .map(|index| {
            let angle = rotation + TAU * index as f32 / sides as f32;
            center + basis.right * (angle.cos() * rx) + basis.forward * (angle.sin() * rz)
        })
        .collect()
}

fn limb_ring(center: Vec3, u: Vec3, v: Vec3, radius: f32, sides: usize) -> Vec<Vec3> {
    (0..sides)
        .map(|index| {
            let angle = TAU * index as f32 / sides as f32;
            center + (u * angle.cos() + v * angle.sin()) * radius
        })
        .collect()
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(mix(seed))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.f32()
    }
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn hash_cell(seed: u32, chunk_x: i32, chunk_z: i32, x: u32, z: u32, salt: u64) -> u64 {
    let mut hash = seed as u64;
    hash = mix(hash ^ (chunk_x as u32 as u64).wrapping_mul(0xA24B_AED4_963E_E407));
    hash = mix(hash ^ (chunk_z as u32 as u64).wrapping_mul(0x9FB2_1C65_1E98_DF25));
    hash = mix(hash ^ ((x as u64) << 32 | z as u64));
    mix(hash ^ salt)
}

fn hash01(seed: u32, chunk_x: i32, chunk_z: i32, x: u32, z: u32, salt: u64) -> f32 {
    (hash_cell(seed, chunk_x, chunk_z, x, z, salt) >> 40) as f32 / (1u64 << 24) as f32
}

fn lin(rgb: [u8; 3]) -> [f32; 3] {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(rgb[0]), channel(rgb[1]), channel(rgb[2])]
}

fn scale_color(color: [f32; 3], factor: f32) -> [f32; 3] {
    [color[0] * factor, color[1] * factor, color[2] * factor]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
