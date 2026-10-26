//! Geometrie-Bausteine für Bäume und der Vertex-Puffer `Geo`.

use glam::{Quat, Vec3};

const TAU: f32 = std::f32::consts::TAU;

#[derive(Clone, Copy)]
pub(super) struct Basis {
    pub(super) right: Vec3,
    pub(super) up: Vec3,
    pub(super) forward: Vec3,
}

impl Basis {
    pub(super) const UPRIGHT: Self = Self {
        right: Vec3::X,
        up: Vec3::Y,
        forward: Vec3::Z,
    };

    pub(super) fn tilted(rotation: Quat) -> Self {
        Self {
            right: rotation * Vec3::X,
            up: rotation * Vec3::Y,
            forward: rotation * Vec3::Z,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Prism {
    pub(super) base: Vec3,
    pub(super) r_bottom: f32,
    pub(super) r_top: f32,
    pub(super) height: f32,
    pub(super) sides: usize,
    pub(super) color: [f32; 3],
    pub(super) rotation: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Cone {
    pub(super) base: Vec3,
    pub(super) radius: f32,
    pub(super) height: f32,
    pub(super) sides: usize,
    pub(super) color: [f32; 3],
    pub(super) basis: Basis,
    pub(super) rotation: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Blob {
    pub(super) center: Vec3,
    pub(super) radii: Vec3,
    pub(super) sides: usize,
    pub(super) color: [f32; 3],
    pub(super) basis: Basis,
    pub(super) rotation: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Limb {
    pub(super) a: Vec3,
    pub(super) b: Vec3,
    pub(super) radius: f32,
    pub(super) sides: usize,
    pub(super) color: [f32; 3],
}

#[derive(Default)]
pub(super) struct Geo {
    pub(super) pos: Vec<[f32; 3]>,
    pub(super) nrm: Vec<[f32; 3]>,
    pub(super) col: Vec<[f32; 4]>,
    pub(super) idx: Vec<u32>,
}

impl Geo {
    pub(super) fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3]) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let base = self.pos.len() as u32;
        for vertex in [a, b, c] {
            self.pos.push(vertex.to_array());
            self.nrm.push(normal.to_array());
            self.col.push([color[0], color[1], color[2], 1.0]);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    pub(super) fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 3]) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    pub(super) fn prism(&mut self, p: Prism) {
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

    pub(super) fn cone(&mut self, c: Cone) {
        let apex = c.base + c.basis.up * c.height;
        let ring = ellipse(c.base, c.basis, c.radius, c.radius, c.sides, c.rotation);
        for i in 0..c.sides {
            let j = (i + 1) % c.sides;
            self.tri(ring[i], apex, ring[j], c.color);
        }
    }

    pub(super) fn blob(&mut self, b: Blob) {
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

    pub(super) fn limb(&mut self, l: Limb) {
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

pub(super) fn ellipse(center: Vec3, basis: Basis, rx: f32, rz: f32, sides: usize, rotation: f32) -> Vec<Vec3> {
    (0..sides)
        .map(|index| {
            let angle = rotation + TAU * index as f32 / sides as f32;
            center + basis.right * (angle.cos() * rx) + basis.forward * (angle.sin() * rz)
        })
        .collect()
}

pub(super) fn limb_ring(center: Vec3, u: Vec3, v: Vec3, radius: f32, sides: usize) -> Vec<Vec3> {
    (0..sides)
        .map(|index| {
            let angle = TAU * index as f32 / sides as f32;
            center + (u * angle.cos() + v * angle.sin()) * radius
        })
        .collect()
}

pub(super) fn lin(rgb: [u8; 3]) -> [f32; 3] {
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

pub(super) fn scale_color(color: [f32; 3], factor: f32) -> [f32; 3] {
    [color[0] * factor, color[1] * factor, color[2] * factor]
}
