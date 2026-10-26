//! Deterministischer Zufall: gleicher Seed und gleiche Zelle ergeben immer dasselbe Ergebnis.

pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(mix(seed))
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    pub(super) fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(super) fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.f32()
    }
}

pub(super) fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub(super) fn hash_cell(seed: u32, chunk_x: i32, chunk_z: i32, x: u32, z: u32, salt: u64) -> u64 {
    let mut hash = seed as u64;
    hash = mix(hash ^ (chunk_x as u32 as u64).wrapping_mul(0xA24B_AED4_963E_E407));
    hash = mix(hash ^ (chunk_z as u32 as u64).wrapping_mul(0x9FB2_1C65_1E98_DF25));
    hash = mix(hash ^ ((x as u64) << 32 | z as u64));
    mix(hash ^ salt)
}

pub(super) fn hash01(seed: u32, chunk_x: i32, chunk_z: i32, x: u32, z: u32, salt: u64) -> f32 {
    (hash_cell(seed, chunk_x, chunk_z, x, z, salt) >> 40) as f32 / (1u64 << 24) as f32
}
