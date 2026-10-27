//! Bewegung und Kollision eines Körpers im Dichtefeld (#13, D-020, D-021).
//!
//! Im Core statt in Godot, damit der Server später dieselbe Bewegung rechnen kann (D-003). Der
//! Körper ist eine senkrechte Kapsel; Kollision gegen die Fläche `dichte == 0` des trilinear
//! interpolierten Dichtefelds, also dieselbe Fläche, die Surface Nets zeichnet (bis auf die
//! Glättung der Meshes).
//!
//! - `collide`: Dichte und Gradient an beliebigen Punkten, Herausschieben der Kapsel
//!
//! Konventionen wie in Godot und Bevy: y oben, Blickrichtung bei `yaw = 0` ist −z, `yaw` dreht
//! gegen den Uhrzeigersinn (von oben gesehen). Kein Klettern (D-020): Steiler als `max_slope` ist
//! eine Wand.

mod collide;

#[cfg(test)]
mod tests;

use glam::{IVec3, Vec2, Vec3};

use crate::streaming::Streamer;

/// Woher die Dichte kommt (> 0 fest, <= 0 Luft), an ganzzahligen Welt-Voxeln.
pub trait DensitySource {
    fn density(&self, voxel: IVec3) -> f32;
}

impl<M: Send + 'static> DensitySource for Streamer<M> {
    fn density(&self, voxel: IVec3) -> f32 {
        self.density_at(voxel)
    }
}

/// Maße und Bewegungswerte. Größe und Proportionen kommen später vom Charakter (#24).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub radius: f32,
    pub height: f32,
    pub eye_height: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub jump_speed: f32,
    pub gravity: f32,
    /// Wie schnell sich die Richtung in der Luft ändern lässt (Anteil pro Sekunde).
    pub air_control: f32,
    /// Normale mit `y` darunter ist kein Boden mehr, sondern Wand (0,7 ≈ 45°). Kanten bis etwa
    /// 0,6 m Höhe lassen sich hinauflaufen, höhere brauchen einen Sprung.
    pub max_slope: f32,
    /// So weit (m) folgt der Körper dem Boden nach unten (Hänge hinab laufen statt springen).
    pub snap: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radius: 0.35,
            height: 1.8,
            eye_height: 1.65,
            walk_speed: 4.5,
            run_speed: 8.0,
            jump_speed: 6.0,
            gravity: 22.0,
            air_control: 3.0,
            max_slope: 0.7,
            snap: 0.4,
        }
    }
}

/// Eingabe für einen Schritt. `movement`: x nach rechts, y nach vorn, Länge höchstens 1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub movement: Vec2,
    pub yaw: f32,
    pub jump: bool,
    pub run: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Fußpunkt (unten Mitte der Kapsel).
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    /// Zuletzt berührte Wände: waagerechte Normale (`ZERO` = keine) und Fußpunkt bei der Berührung.
    walls: [(Vec3, Vec3); 2],
}

/// Größte Strecke pro Teilschritt: kleiner als der Radius, damit nichts durch dünne Wände rutscht.
const SUBSTEP: f32 = 0.2;
/// Größter Zeitschritt und größte Fallgeschwindigkeit: begrenzt die Rechenzeit pro Schritt (der
/// Server bekommt später `dt` von außen).
const MAX_DT: f32 = 0.1;
const MAX_FALL_SPEED: f32 = 60.0;
/// Eine berührte Wand bleibt gemerkt, bis sich der Körper 15 cm von ihrer Ebene entfernt oder sie
/// neben ihm nicht mehr da ist (geprüft `WALL_PROBE` hinter der Körpermitte). Nur einen Schritt lang
/// reichte nicht: Die Kante zwischen Boden und Wand ist im interpolierten Feld eine Rampe, auf der der
/// Körper sonst immer wieder hinaufläuft und zurückgeschoben wird (Review-Fund: Zittern, auch beim
/// Entlanggleiten).
const WALL_MEMORY: f32 = 0.15;
const WALL_PROBE: f32 = 0.6;
/// Anteil der gewünschten Geschwindigkeit, der nach dem Abzug der Wände mindestens bleiben muss,
/// damit der Körper an ihnen entlanggleitet (45° zur Wand: ≈ 70 %).
const MIN_SLIDE: f32 = 0.3;

impl Body {
    pub fn new(position: Vec3) -> Self {
        Self { position, velocity: Vec3::ZERO, on_ground: false, walls: [(Vec3::ZERO, Vec3::ZERO); 2] }
    }

    /// Ein Schritt um `dt` Sekunden.
    pub fn step(&mut self, input: Input, dt: f32, settings: &Settings, field: &impl DensitySource) {
        let dt = dt.clamp(0.0, MAX_DT);
        let forward = Vec3::new(-input.yaw.sin(), 0.0, -input.yaw.cos());
        let right = Vec3::new(input.yaw.cos(), 0.0, -input.yaw.sin());
        let movement = input.movement.clamp_length_max(1.0);
        let speed = if input.run { settings.run_speed } else { settings.walk_speed };
        let wish = (right * movement.x + forward * movement.y) * speed;

        if self.on_ground {
            self.velocity.x = wish.x;
            self.velocity.z = wish.z;
        } else {
            let blend = (settings.air_control * dt).min(1.0);
            self.velocity.x += (wish.x - self.velocity.x) * blend;
            self.velocity.z += (wish.z - self.velocity.z) * blend;
        }
        // Nicht in gerade berührte Wände hineinlaufen, sondern daran entlang (zweimal für Ecken).
        let here = self.position;
        let middle = here + Vec3::Y * (settings.height * 0.5);
        for (normal, at) in self.walls.iter_mut() {
            let offset = Vec3::new(here.x - at.x, 0.0, here.z - at.z);
            let behind = middle - *normal * (settings.radius + WALL_PROBE);
            let still_there = collide::sample(field, behind).0 > 0.0;
            if offset.dot(*normal) > WALL_MEMORY || !still_there {
                *normal = Vec3::ZERO;
            }
        }
        let remembered: Vec<Vec3> = self.walls.iter().map(|(normal, _)| *normal).filter(|normal| *normal != Vec3::ZERO).collect();
        let wished = Vec3::new(self.velocity.x, 0.0, self.velocity.z).length();
        for _ in 0..2 {
            for &wall in &remembered {
                let into = self.velocity.x * wall.x + self.velocity.z * wall.z;
                if into < 0.0 {
                    self.velocity -= wall * into;
                }
            }
        }
        // Fast frontal in eine Wand oder Ecke: stehen bleiben statt an der Rundung entlangzukriechen
        // (die Normale einer Innenecke ändert sich beim Gleiten, der Körper pendelte sonst).
        if !remembered.is_empty() && Vec3::new(self.velocity.x, 0.0, self.velocity.z).length() < MIN_SLIDE * wished {
            self.velocity.x = 0.0;
            self.velocity.z = 0.0;
        }
        let jumped = self.on_ground && input.jump;
        if jumped {
            self.velocity.y = settings.jump_speed;
        } else if self.on_ground {
            self.velocity.y = 0.0;
        } else {
            self.velocity.y = (self.velocity.y - settings.gravity * dt).max(-MAX_FALL_SPEED);
        }

        let was_on_ground = self.on_ground;
        // Im Steigen (Sprung) gibt es keinen Boden: Sonst zählte an einer Wand die Kante zwischen
        // Boden und Wand als Boden, und der Sprung endete nach 10 cm (Review-Fund).
        let rising = self.velocity.y > 0.0;
        let travel = self.velocity * dt;
        let substeps = (travel.length() / SUBSTEP).ceil().max(1.0) as usize;
        self.on_ground = false;
        let mut walls = Vec::new();
        for _ in 0..substeps {
            let before = self.position;
            self.position += travel / substeps as f32;
            let mut contact = collide::resolve(&mut self.position, &mut self.velocity, settings, field);
            if contact.stuck {
                // Passt nicht hinein (z. B. Gang niedriger als der Körper): waagerecht zurück, sonst
                // steckte der Kopf im Fels (Review-Fund). Steckt er auch dort, weiter herausschieben.
                let mut back = Vec3::new(before.x, self.position.y, before.z);
                let mut velocity = Vec3::new(0.0, self.velocity.y, 0.0);
                let retry = collide::resolve(&mut back, &mut velocity, settings, field);
                if !retry.stuck {
                    let blocked = Vec3::new(before.x - self.position.x, 0.0, before.z - self.position.z).normalize_or_zero();
                    self.position = back;
                    self.velocity = velocity;
                    contact = retry;
                    contact.walls.push(blocked);
                }
            }
            self.on_ground |= contact.ground && !rising;
            walls.extend(contact.walls);
        }
        // Neue Wände einmischen: dieselbe Wand auffrischen, sonst einen freien oder den ältesten
        // Platz nehmen. Alte Wände bleiben, solange der Körper nah ist (eine Ecke hat zwei).
        for wall in walls.into_iter().rev() {
            let slot = match self.walls.iter().position(|(normal, _)| normal.dot(wall) > 0.99) {
                Some(same) => same,
                None => self.walls.iter().position(|(normal, _)| *normal == Vec3::ZERO).unwrap_or(1),
            };
            if slot == 1 && self.walls[1].0 != Vec3::ZERO && self.walls[0].0.dot(wall) <= 0.99 {
                self.walls[1] = self.walls[0];
                self.walls[0] = (wall, self.position);
            } else {
                self.walls[slot] = (wall, self.position);
            }
        }

        // Am Boden geblieben? Sonst ein Stück nach unten folgen (Hang hinab, kleine Kanten).
        if was_on_ground && !jumped && !self.on_ground {
            let mut probe = self.position - Vec3::Y * settings.snap;
            let mut velocity = self.velocity;
            let contact = collide::resolve(&mut probe, &mut velocity, settings, field);
            if contact.ground && !contact.stuck {
                self.position = probe;
                self.velocity.y = 0.0;
                self.on_ground = true;
            }
        }
    }
}
