//! Tests der Bewegung in künstlichen Dichtefeldern (Boden, Hang, Wand, Decke) und in der echten
//! Welt über den Streamer.

use glam::{IVec3, Vec2, Vec3, Vec3Swizzles};

use super::{Body, DensitySource, Input, Settings};

const DT: f32 = 1.0 / 60.0;

/// Dichtefeld aus einer Funktion der Position (ausgewertet an den ganzzahligen Voxeln).
struct Field<F: Fn(Vec3) -> f32>(F);

impl<F: Fn(Vec3) -> f32> DensitySource for Field<F> {
    fn density(&self, voxel: IVec3) -> f32 {
        (self.0)(voxel.as_vec3())
    }
}

fn flat() -> Field<impl Fn(Vec3) -> f32> {
    Field(|p: Vec3| 10.0 - p.y)
}

fn run(body: &mut Body, input: Input, seconds: f32, field: &impl DensitySource) {
    let settings = Settings::default();
    for _ in 0..(seconds / DT).round() as usize {
        body.step(input, DT, &settings, field);
    }
}

fn walk(forward: f32) -> Input {
    Input { movement: Vec2::new(0.0, forward), ..Input::default() }
}

#[test]
fn falls_and_lands_on_the_ground() {
    let mut body = Body::new(Vec3::new(0.5, 30.0, 0.5));
    run(&mut body, Input::default(), 3.0, &flat());
    assert!(body.on_ground, "nicht gelandet: {body:?}");
    assert!((body.position.y - 10.0).abs() < 0.1, "Füße bei {}", body.position.y);
    // Stehen bleibt stabil.
    let before = body.position;
    run(&mut body, Input::default(), 2.0, &flat());
    assert!(body.position.distance(before) < 1e-3, "rutscht von {before} nach {}", body.position);
}

#[test]
fn walks_with_the_expected_speed_and_direction() {
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, Input::default(), 0.5, &flat());
    let start = body.position;
    // yaw = 0: vorn ist −z.
    run(&mut body, walk(1.0), 2.0, &flat());
    let moved = body.position - start;
    assert!((moved.z + 9.0).abs() < 0.2 && moved.x.abs() < 1e-3, "{moved}");
    // yaw = 90°: vorn ist −x; Rennen ist schneller.
    let start = body.position;
    run(&mut body, Input { yaw: std::f32::consts::FRAC_PI_2, run: true, ..walk(1.0) }, 1.0, &flat());
    let moved = body.position - start;
    assert!((moved.x + 8.0).abs() < 0.2 && moved.z.abs() < 1e-3, "{moved}");
}

#[test]
fn jumps_and_comes_back_down() {
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, Input::default(), 0.5, &flat());
    let settings = Settings::default();
    body.step(Input { jump: true, ..Input::default() }, DT, &settings, &flat());
    let mut highest: f32 = 0.0;
    for _ in 0..120 {
        body.step(Input::default(), DT, &settings, &flat());
        highest = highest.max(body.position.y - 10.0);
    }
    // v²/2g = 36/44 ≈ 0,82 m.
    assert!((highest - 0.82).abs() < 0.1, "Sprunghöhe {highest}");
    assert!(body.on_ground && (body.position.y - 10.0).abs() < 0.1);
}

#[test]
fn walks_up_and_down_gentle_slopes() {
    // Anstieg 0,5 m pro m (≈ 27°) in −z-Richtung.
    let slope = Field(|p: Vec3| 10.0 - 0.5 * p.z - p.y);
    let mut body = Body::new(Vec3::new(0.5, 12.0, 0.5));
    run(&mut body, Input::default(), 1.0, &slope);
    run(&mut body, walk(1.0), 2.0, &slope);
    let ground = 10.0 - 0.5 * body.position.z;
    assert!(body.on_ground && (body.position.y - ground).abs() < 0.4, "{body:?}, Boden {ground}");
    // Bergauf etwas langsamer (das Herausschieben aus dem Hang nimmt einen Teil der Bewegung).
    assert!(body.position.z < -6.0, "kam nicht voran: {}", body.position.z);
    // Hinab bleibt er am Boden (kein Hüpfen).
    let mut airborne = 0;
    let settings = Settings::default();
    for _ in 0..120 {
        body.step(walk(-1.0), DT, &settings, &slope);
        airborne += usize::from(!body.on_ground);
    }
    assert_eq!(airborne, 0, "beim Hinablaufen {airborne} Schritte in der Luft");
}

#[test]
fn steep_slopes_and_walls_block_the_way() {
    // Anstieg 2 m pro m (≈ 63°): steiler als 45°, kein Klettern (D-020).
    let steep = Field(|p: Vec3| (10.0 - p.y).max(10.0 - 2.0 * (p.z + 3.0) - p.y));
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, Input::default(), 0.5, &steep);
    run(&mut body, walk(1.0), 3.0, &steep);
    assert!(body.position.y < 11.0, "Steilhang hinaufgelaufen: {}", body.position);

    // Senkrechte Wand bei z = -3: bleibt davor stehen. Die interpolierte Ecke zwischen Boden und
    // Wand ist abgerundet, die untere Kugel stößt dort etwas früher an.
    let wall = Field(|p: Vec3| (10.0 - p.y).max(-3.0 - p.z));
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, walk(1.0), 3.0, &wall);
    assert!(body.position.z > -3.0 + 0.3 && body.position.z < -3.0 + 0.7, "Wand durchlaufen oder zu früh gestoppt: {}", body.position.z);
}

#[test]
fn ceilings_stop_jumps() {
    // Boden bei 10, Decke bei 12.5: Kopf (1,8 m) stößt beim Sprung an.
    let cave = Field(|p: Vec3| (10.0 - p.y).max(p.y - 12.5));
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, Input::default(), 0.5, &cave);
    let settings = Settings::default();
    body.step(Input { jump: true, ..Input::default() }, DT, &settings, &cave);
    for _ in 0..60 {
        body.step(Input::default(), DT, &settings, &cave);
        assert!(body.position.y + settings.height <= 12.5 + 0.05, "Kopf in der Decke: {}", body.position.y);
    }
}

/// Fallen aus großer Höhe mit großem Zeitschritt: nicht durch den Boden.
#[test]
fn fast_falls_do_not_tunnel() {
    let thin = Field(|p: Vec3| (10.0 - p.y).min(p.y - 8.0).max(-20.0));
    let mut body = Body::new(Vec3::new(0.5, 200.0, 0.5));
    let settings = Settings::default();
    for _ in 0..200 {
        body.step(Input::default(), 0.05, &settings, &thin);
    }
    assert!(body.on_ground && (body.position.y - 10.0).abs() < 0.2, "{body:?}");
}

#[test]
fn same_inputs_give_the_same_path() {
    let slope = Field(|p: Vec3| 10.0 - 0.3 * p.x - 0.2 * p.z - p.y);
    let path = || {
        let mut body = Body::new(Vec3::new(0.3, 15.0, 0.7));
        let settings = Settings::default();
        for i in 0..300 {
            let input = Input { movement: Vec2::new((i as f32 * 0.1).sin(), 1.0), yaw: i as f32 * 0.01, jump: i % 70 == 0, run: i % 2 == 0 };
            body.step(input, DT, &settings, &slope);
        }
        body
    };
    assert_eq!(path(), path());
}

/// Gegen Wand, Steilhang und Ecke laufen: Der Körper bleibt ruhig stehen (Review-Fund: vorher
/// 7–15 cm Zittern pro Frame, mit der Kamera daran).
#[test]
fn pushing_against_walls_does_not_jitter() {
    let wall = Field(|p: Vec3| (10.0 - p.y).max(-3.0 - p.z));
    let steep = Field(|p: Vec3| (10.0 - p.y).max(10.0 - 2.0 * (p.z + 3.0) - p.y));
    let corner = Field(|p: Vec3| (10.0 - p.y).max(-3.0 - p.z).max(-3.0 - p.x));
    let settings = Settings::default();
    // Wand und Steilhang frontal, die Ecke schräg (beide Seiten blockieren): Gleiten an einer Wand
    // entlang ist erlaubt und wird hier nicht gemessen.
    let check = |name: &str, movement: Vec2, field: &dyn Fn(&mut Body, Input)| {
        let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
        let input = Input { movement, ..Input::default() };
        for _ in 0..180 {
            field(&mut body, input);
        }
        let mut worst: f32 = 0.0;
        for _ in 0..60 {
            let before = body.position;
            field(&mut body, input);
            worst = worst.max(body.position.distance(before));
        }
        assert!(worst < 0.01, "{name}: zittert {worst} m pro Frame");
    };
    check("Wand", Vec2::Y, &|b, i| b.step(i, DT, &settings, &wall));
    check("Steilhang", Vec2::Y, &|b, i| b.step(i, DT, &settings, &steep));
    check("Ecke", Vec2::new(-0.6, 0.8), &|b, i| b.step(i, DT, &settings, &corner));

    // Schräg an der Wand entlang: entlang (x) darf er sich bewegen, quer dazu (y, z) nicht wippen
    // (Review-Fund: die Wand wurde nach 15 cm Weg vergessen).
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    let input = Input { movement: Vec2::new(-0.7, 0.7), ..Input::default() };
    for _ in 0..180 {
        body.step(input, DT, &settings, &wall);
    }
    let mut worst: f32 = 0.0;
    for _ in 0..120 {
        let before = body.position;
        body.step(input, DT, &settings, &wall);
        let moved = body.position - before;
        worst = worst.max(moved.y.abs()).max(moved.z.abs());
        assert!(moved.x < -0.02, "gleitet nicht: {moved}");
    }
    assert!(worst < 0.01, "wippt beim Entlanggleiten um {worst} m pro Frame");
}

/// Ein Gang, niedriger als der Körper: nicht hinein, Kopf nie im Fels (Review-Fund).
#[test]
fn low_passages_stop_the_body() {
    let settings = Settings::default();
    // Boden bei 10, ab z < -3 eine Decke bei 11,5 (1,5 m Höhe bei 1,8 m Körper).
    let passage = Field(|p: Vec3| (10.0 - p.y).max(if p.z < -3.0 { p.y - 11.5 } else { -100.0 }));
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    for _ in 0..240 {
        body.step(walk(1.0), DT, &settings, &passage);
        let head = body.position + Vec3::Y * (settings.height - settings.radius);
        let (distance, _) = super::collide::surface(&passage, head).expect("Normale");
        assert!(distance >= settings.radius - 0.05, "Kopf {distance} m tief im Fels bei {}", body.position);
    }
    // Die Decke beginnt im interpolierten Feld erst zwischen z = -3 und -4.
    assert!(body.position.z > -4.0, "in den Gang gekrochen: {}", body.position);
}

#[test]
fn walks_on_the_real_terrain() {
    use crate::WorldGen;
    use crate::edits::WorldEdits;
    use crate::streaming::{Config, PublicSurface, Streamer};
    use std::sync::{Arc, RwLock};
    let world = Arc::new(WorldGen::new(42));
    let spawn = crate::spawn::find_spawn(&world);
    let mut config = Config::new(2);
    config.far = false;
    let mut streamer = Streamer::new(Arc::clone(&world), Arc::new(PublicSurface), Arc::new(RwLock::new(WorldEdits::default())), config, |_| ());
    let mut body = Body::new(spawn + Vec3::Y * 3.0);
    let settings = Settings::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    for steps in 0..60 * 30 {
        assert!(std::time::Instant::now() < deadline, "zu langsam");
        streamer.update(body.position);
        streamer.drain_events().for_each(drop);
        let input = if steps < 60 { Input::default() } else { walk(1.0) };
        body.step(input, DT, &settings, &streamer);
        let ground = world.column_height(body.position.x, body.position.z);
        if steps > 60 {
            assert!(body.position.y > ground - 0.5, "unter die Oberfläche gefallen: {} (Boden {ground})", body.position);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // Laufen geht oder ein Hindernis (Steilhang, Wasser) hält auf, aber gelaufen ist er.
    assert!(body.position.xz().distance(spawn.xz()) > 20.0, "kam nicht voran: {}", body.position);
}

/// Mit Untergrund: Auch nach 300 m Weg hält der Streamer das Dichtefeld um den Körper, und es ist
/// genau der Untergrund (Review-Fund: vorher war es ab ~180 m leer, Höhlen wurden zu Wänden).
#[cfg(feature = "underground")]
#[test]
fn the_density_field_follows_the_body() {
    use crate::WorldGen;
    use crate::edits::WorldEdits;
    use crate::streaming::{Config, Streamer, UndergroundSource};
    use crate::underground::{SecretSeed, Underground};
    use crate::volume::{CHUNK_SIZE, ColumnData};
    use std::sync::{Arc, RwLock};
    let world = Arc::new(WorldGen::new(42));
    let underground = Arc::new(Underground::new(SecretSeed::insecure_dev_seed(42), &world.cfg));
    let mut config = Config::new(4);
    config.far = false;
    let source: Arc<dyn UndergroundSource> = underground.clone();
    let mut streamer = Streamer::new(Arc::clone(&world), source, Arc::new(RwLock::new(WorldEdits::default())), config, |_| ());
    let spawn = crate::spawn::find_spawn(&world);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    for step in 0..=10 {
        // Kamera 30 m pro Schritt weiter, dann warten, bis das Feld voll ist.
        let camera = spawn + Vec3::new(step as f32 * 30.0, 2.0, 0.0);
        loop {
            assert!(std::time::Instant::now() < deadline, "zu langsam");
            streamer.update(camera);
            streamer.drain_events().for_each(drop);
            let stats = streamer.stats();
            if stats.field_jobs == 0 && stats.column_jobs == 0 && stats.columns_ready == 0 && stats.cave_jobs == 0 && stats.caves_ready == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(streamer.stats().field_chunks >= 25, "nach {} m nur {} Chunks im Feld", step * 30, streamer.stats().field_chunks);
        // Um die Kamera stimmt jedes Voxel mit dem Untergrund überein.
        let center = (camera / CHUNK_SIZE as f32).floor().as_ivec3();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let chunk = center + IVec3::new(dx, dy, dz);
                    let column = ColumnData::sample(&world, chunk.x, chunk.z);
                    let expected = underground.materials_chunk(&world, &column, chunk.y);
                    for i in 0..CHUNK_SIZE {
                        let voxel = chunk * CHUNK_SIZE + IVec3::new(i, (i * 7) % CHUNK_SIZE, (i * 13) % CHUNK_SIZE);
                        let local = voxel - chunk * CHUNK_SIZE;
                        assert_eq!(streamer.density_at(voxel).to_bits(), expected.get(local.x, local.y, local.z).to_bits(), "Voxel {voxel} nach {} m", step * 30);
                    }
                }
            }
        }
    }
}


/// Springen direkt an einer Wand: volle Höhe, danach wieder am Boden.
#[test]
fn jumping_next_to_a_wall_reaches_full_height() {
    let wall = Field(|p: Vec3| (10.0 - p.y).max(-3.0 - p.z));
    let settings = Settings::default();
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, walk(1.0), 2.0, &wall);
    let start = body.position.y;
    body.step(Input { jump: true, ..walk(1.0) }, DT, &settings, &wall);
    let mut highest: f32 = 0.0;
    for _ in 0..90 {
        body.step(walk(1.0), DT, &settings, &wall);
        highest = highest.max(body.position.y - start);
    }
    assert!((highest - 0.82).abs() < 0.15, "Sprunghöhe an der Wand {highest}");
    assert!(body.on_ground && (body.position.y - start).abs() < 0.1, "nicht zurück am Boden: {body:?}");
}

/// Eine gemerkte Wand hält nicht fest: umdrehen und weglaufen geht sofort.
#[test]
fn walking_away_from_a_wall_is_not_blocked() {
    let wall = Field(|p: Vec3| (10.0 - p.y).max(-3.0 - p.z));
    let mut body = Body::new(Vec3::new(0.5, 10.0, 0.5));
    run(&mut body, walk(1.0), 2.0, &wall);
    let start = body.position;
    run(&mut body, walk(-1.0), 1.0, &wall);
    assert!(body.position.z - start.z > 4.0, "kommt nicht weg: {} → {}", start, body.position);
    // Seitlich an der Wand entlang geht auch.
    run(&mut body, walk(1.0), 2.0, &wall);
    let start = body.position;
    run(&mut body, Input { movement: Vec2::new(1.0, 0.3), ..Input::default() }, 1.0, &wall);
    assert!(body.position.x - start.x > 3.5, "gleitet nicht an der Wand entlang: {} → {}", start, body.position);
}



/// Bergab gleichmäßig, ohne Sprünge von Frame zu Frame (Review-Fund #58: Das Nachführen am Boden
/// schob die Probe entlang der Hangnormalen, also auch waagerecht; jeder zweite Schritt war 2-3×
/// so lang).
#[test]
fn walking_downhill_is_smooth() {
    let settings = Settings::default();
    for steepness in [0.3, 0.5, 0.9] {
        let slope = Field(move |p: Vec3| 10.0 - steepness * p.z - p.y);
        let mut body = Body::new(Vec3::new(0.5, 12.0, -10.0));
        run(&mut body, Input::default(), 1.0, &slope);
        // Bergab ist +z (Blick nach +z: yaw = 180°).
        let input = Input { yaw: std::f32::consts::PI, ..walk(1.0) };
        run(&mut body, input, 0.5, &slope);
        let longest = settings.walk_speed * DT * (1.0 + steepness * steepness).sqrt();
        for _ in 0..120 {
            let before = body.position;
            body.step(input, DT, &settings, &slope);
            let step = body.position.distance(before);
            assert!(step < longest * 1.5, "Hang {steepness}: Schritt {step:.3} m statt ≈ {longest:.3} bei {}", body.position);
        }
    }
}

#[path = "trunk_tests.rs"]
mod trunks;
