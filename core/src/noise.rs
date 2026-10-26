use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

pub struct Layer {
    noise: FastNoiseLite,
}

impl Layer {
    pub fn new(seed: i32, frequency: f32, octaves: i32) -> Self {
        let mut noise = FastNoiseLite::with_seed(seed);
        noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise.set_frequency(Some(frequency));
        noise.set_fractal_type(Some(FractalType::FBm));
        noise.set_fractal_octaves(Some(octaves));
        noise.set_fractal_lacunarity(Some(2.0));
        noise.set_fractal_gain(Some(0.5));
        Self { noise }
    }

    #[inline]
    pub fn get(&self, x: f32, z: f32) -> f32 {
        self.noise.get_noise_2d(x, z)
    }

    #[inline]
    pub fn get3(&self, x: f32, y: f32, z: f32) -> f32 {
        self.noise.get_noise_3d(x, y, z)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RawSample {
    pub continental: f32,
    pub erosion: f32,
    pub ridge: f32,
    pub detail: f32,
    pub temperature: f32,
    pub humidity: f32,
    pub weirdness: f32,
    pub realm: f32,
    pub volcanic: f32,
    pub crystal: f32,
    pub fungal: f32,
    pub river: f32,
}

pub struct NoiseBank {
    continental: Layer,
    erosion: Layer,
    ridges: Layer,
    detail: Layer,
    temperature: Layer,
    humidity: Layer,
    weirdness: Layer,
    realm: Layer,
    volcanic: Layer,
    crystal: Layer,
    fungal: Layer,
    river: Layer,
    warp_a: Layer,
    warp_b: Layer,
}

impl NoiseBank {
    pub fn new(seed: u32) -> Self {
        let seed = seed as i32;
        let layer = |offset: i32, frequency: f32, octaves: i32| {
            Layer::new(seed.wrapping_add(offset), frequency, octaves)
        };
        Self {
            continental: layer(11, 0.00055, 4),
            erosion: layer(23, 0.00110, 3),
            ridges: layer(37, 0.00160, 4),
            detail: layer(41, 0.01200, 3),
            temperature: layer(53, 0.00042, 3),
            humidity: layer(67, 0.00055, 3),
            weirdness: layer(71, 0.00070, 3),
            realm: layer(89, 0.00021, 2),
            volcanic: layer(97, 0.00090, 3),
            crystal: layer(101, 0.00110, 3),
            fungal: layer(103, 0.00130, 3),
            river: layer(113, 0.00190, 2),
            warp_a: layer(107, 0.00110, 3),
            warp_b: layer(109, 0.00110, 3),
        }
    }

    pub fn sample(&self, x: f32, z: f32) -> RawSample {
        let warp_x = self.warp_a.get(x + 31.7, z - 17.3) * 80.0;
        let warp_z = self.warp_b.get(x - 53.1, z + 91.7) * 80.0;
        let cx = x + warp_x;
        let cz = z + warp_z;

        let continental = (self.continental.get(cx, cz) * 1.15).clamp(-1.0, 1.0);
        let erosion = self.erosion.get(x, z);
        let ridge = 1.0 - self.ridges.get(cx * 0.85, cz * 0.85).abs();
        let detail = self.detail.get(x, z);

        RawSample {
            continental,
            erosion,
            ridge: ridge.clamp(0.0, 1.0),
            detail,
            temperature: self.temperature.get(x, z),
            humidity: self.humidity.get(x, z),
            weirdness: self.weirdness.get(x, z),
            realm: self.realm.get(x, z),
            volcanic: self.volcanic.get(x, z),
            crystal: self.crystal.get(x, z),
            fungal: self.fungal.get(x, z),
            river: self.river.get(cx, cz),
        }
    }
}
