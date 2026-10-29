//! Kampf im Godot-Client (#62, D-023).
//!
//! `TerraforgeCombat`: Waffendaten, Lage der Waffe im Schlag und Meshes für Waffen und Puppe, alles
//! aus dem Core (`terraforge_core::combat`), damit die Anzeige derselben Bahn folgt, mit der der
//! Server trifft. Am `TerraforgeTerrain`: `attack` schickt einen Schlag an den Server (ohne Server
//! rechnet ihn das Terrain selbst), `take_attack_results` liefert die Ergebnisse, `get_dummies` die
//! Trainingspuppen.

use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3;
use godot::classes::ArrayMesh;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::prelude::*;
use terraforge_core::combat::{self, Direction, Fighter, Pose, View, WEAPONS, WeaponKind, Zone};
use terraforge_protocol::{AttackOutcome, HitInfo, NetworkSource};

use crate::terrain::TerraforgeTerrain;

fn glam(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn godot(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// Lage als Transform: Ursprung in der Hand, +Y entlang der Klinge, +X nach rechts im Blick.
fn transform(pose: Pose, view: &View) -> Transform3D {
    let y = pose.blade;
    let x = (view.right - y * view.right.dot(y)).try_normalize().unwrap_or_else(|| y.any_orthonormal_vector());
    let z = x.cross(y);
    Transform3D::new(Basis::from_cols(godot(x), godot(y), godot(z)), godot(pose.hand))
}

/// Einfache Meshes aus Quadern mit Vertex-Farben (stilisiert, D-020), bis Schmieden und
/// Charaktere echte Formen liefern.
#[derive(Default)]
struct Boxes {
    vertices: Vec<Vector3>,
    normals: Vec<Vector3>,
    colors: Vec<Color>,
    indices: Vec<i32>,
}

impl Boxes {
    fn add(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        let color = Color::from_rgba8(color[0], color[1], color[2], 255);
        let faces = [
            (Vec3::X, [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
            (Vec3::NEG_X, [[0, 0, 1], [0, 1, 1], [0, 1, 0], [0, 0, 0]]),
            (Vec3::Y, [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]),
            (Vec3::NEG_Y, [[0, 0, 1], [0, 0, 0], [1, 0, 0], [1, 0, 1]]),
            (Vec3::Z, [[1, 0, 1], [1, 1, 1], [0, 1, 1], [0, 0, 1]]),
            (Vec3::NEG_Z, [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
        ];
        for (normal, corners) in faces {
            let base = self.vertices.len() as i32;
            for [x, y, z] in corners {
                let pick = |bit: i32, lo: f32, hi: f32| if bit == 1 { hi } else { lo };
                self.vertices.push(Vector3::new(pick(x, min.x, max.x), pick(y, min.y, max.y), pick(z, min.z, max.z)));
                self.normals.push(godot(normal));
                self.colors.push(color);
            }
            // Im Uhrzeigersinn von außen (Godot).
            self.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }

    fn mesh(self) -> Gd<ArrayMesh> {
        let mut arrays = VarArray::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(ArrayType::VERTEX.ord() as usize, &PackedVector3Array::from(self.vertices).to_variant());
        arrays.set(ArrayType::NORMAL.ord() as usize, &PackedVector3Array::from(self.normals).to_variant());
        arrays.set(ArrayType::COLOR.ord() as usize, &PackedColorArray::from(self.colors).to_variant());
        arrays.set(ArrayType::INDEX.ord() as usize, &PackedInt32Array::from(self.indices).to_variant());
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        mesh
    }
}

#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub(crate) struct TerraforgeCombat {}

#[godot_api]
impl TerraforgeCombat {
    #[func]
    fn weapon_count() -> i32 {
        WEAPONS.len() as i32
    }

    /// Name und Werte einer Waffe: `name`, `windup`, `strike`, `strike_thrust`, `recovery`, `cost`,
    /// `cost_thrust`, `reach`, `mass`.
    #[func]
    fn weapon_info(weapon: i32) -> VarDictionary {
        let mut info = VarDictionary::new();
        let Some(weapon) = usize::try_from(weapon).ok().and_then(|i| WEAPONS.get(i)) else {
            return info;
        };
        info.set("name", weapon.name);
        info.set("windup", weapon.windup());
        info.set("strike", weapon.strike(false));
        info.set("strike_thrust", weapon.strike(true));
        info.set("recovery", weapon.recovery());
        info.set("cost", weapon.stamina_cost(false));
        info.set("cost_thrust", weapon.stamina_cost(true));
        info.set("reach", weapon.reach);
        info.set("mass", weapon.mass);
        info
    }

    /// Richtung aus der Mausbewegung beim Ausholen (Pixel, y nach unten): 0 von oben, 1 von links,
    /// 2 von rechts, 3 Stich.
    #[func]
    fn direction_from_mouse(dx: f32, dy: f32) -> i32 {
        Direction::from_mouse(dx, dy).id().into()
    }

    #[func]
    fn direction_name(direction: i32) -> GString {
        let name = match u8::try_from(direction).ok().and_then(Direction::from_id) {
            Some(Direction::Overhead) => "von oben",
            Some(Direction::FromLeft) => "von links",
            Some(Direction::FromRight) => "von rechts",
            Some(Direction::Thrust) => "Stich",
            None => "",
        };
        name.into()
    }

    #[func]
    fn zone_name(zone: i32) -> GString {
        u8::try_from(zone).ok().and_then(Zone::from_id).map_or("", Zone::name).into()
    }

    /// Ausdauer: `max` und Erholung je Sekunde (`regen`), wie im Core.
    #[func]
    fn stamina_info() -> VarDictionary {
        let mut info = VarDictionary::new();
        info.set("max", combat::Stamina::default().max);
        info.set("regen", combat::STAMINA_REGEN);
        info
    }

    /// Lage bei der Erholung (`w` 0..1 von der Endlage des Schlags zurück in die Ruhe).
    #[func]
    fn recovery_pose(eye: Vector3, yaw: f32, pitch: f32, direction: i32, w: f32) -> Transform3D {
        let view = View::new(glam(eye), yaw, pitch);
        let pose = match u8::try_from(direction).ok().and_then(Direction::from_id) {
            Some(direction) => combat::recovery_pose(&view, direction, w),
            None => combat::rest(&view),
        };
        transform(pose, &view)
    }

    /// Lage der Waffe im Schlag (`s` 0..1), beim Ausholen (`windup` 0..1, `s` < 0 heißt: ausholen
    /// mit `-s`) bzw. in Ruhe (Richtung −1).
    #[func]
    fn pose(eye: Vector3, yaw: f32, pitch: f32, direction: i32, s: f32) -> Transform3D {
        let view = View::new(glam(eye), yaw, pitch);
        let pose = match u8::try_from(direction).ok().and_then(Direction::from_id) {
            None => combat::rest(&view),
            Some(direction) if s < 0.0 => combat::windup_pose(&view, direction, -s),
            Some(direction) => combat::pose(&view, direction, s),
        };
        transform(pose, &view)
    }

    /// Mesh einer Waffe: Ursprung in der Hand, Klinge entlang +Y.
    #[func]
    fn weapon_mesh(weapon: i32) -> Option<Gd<ArrayMesh>> {
        let weapon = usize::try_from(weapon).ok().and_then(|i| WEAPONS.get(i))?;
        let mut boxes = Boxes::default();
        let (wood, steel, leather) = ([110, 78, 48], [190, 196, 204], [70, 50, 36]);
        match weapon.kind {
            WeaponKind::Sword => {
                boxes.add(Vec3::new(-0.02, -weapon.butt, -0.02), Vec3::new(0.02, 0.0, 0.02), leather);
                boxes.add(Vec3::new(-0.11, 0.0, -0.025), Vec3::new(0.11, 0.03, 0.025), steel);
                boxes.add(Vec3::new(-0.025, 0.03, -0.006), Vec3::new(0.025, weapon.reach, 0.006), steel);
            }
            WeaponKind::Axe => {
                boxes.add(Vec3::new(-0.02, -weapon.butt, -0.02), Vec3::new(0.02, weapon.reach, 0.02), wood);
                boxes.add(Vec3::new(0.02, weapon.striking_from, -0.01), Vec3::new(0.16, weapon.reach - 0.02, 0.01), steel);
            }
            WeaponKind::Spear => {
                boxes.add(Vec3::new(-0.018, -weapon.butt, -0.018), Vec3::new(0.018, weapon.striking_from, 0.018), wood);
                boxes.add(Vec3::new(-0.03, weapon.striking_from, -0.008), Vec3::new(0.03, weapon.reach, 0.008), steel);
            }
        }
        Some(boxes.mesh())
    }

    /// Mesh der Trainingspuppe (Fuß im Ursprung, Blick nach −z): Pfahl und Zonen als Quader.
    #[func]
    fn dummy_mesh() -> Gd<ArrayMesh> {
        let mut boxes = Boxes::default();
        let dummy = combat::Target { feet: Vec3::ZERO, yaw: 0.0 };
        boxes.add(Vec3::new(-0.05, 0.0, -0.05), Vec3::new(0.05, 1.0, 0.05), [96, 70, 44]);
        for zone in Zone::ALL {
            let (a, b, radius) = dummy.zone(zone);
            let color = if zone == Zone::Head { [214, 196, 150] } else { [196, 172, 120] };
            boxes.add(a.min(b) - Vec3::splat(radius), a.max(b) + Vec3::splat(radius), color);
        }
        boxes.mesh()
    }
}

/// Kampfzustand im Terrain.
#[derive(Default)]
pub(crate) struct CombatState {
    next: i64,
    /// Beim Server laufende Schläge: Nummer beim Server → eigene Nummer.
    waiting: HashMap<u64, i64>,
    results: Vec<VarDictionary>,
    dummies: Vec<([f32; 3], f32)>,
    /// Ohne Server: Ausdauer und Tempo hier, mit eigener Uhr.
    local: Option<(Fighter, Instant)>,
}

fn result(id: i64, outcome: &AttackOutcome) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("request", id);
    match outcome {
        AttackOutcome::Refused { reason } => {
            result.set("ok", false);
            result.set("message", reason.as_str());
        }
        AttackOutcome::Swung { stamina, hit } => {
            result.set("ok", true);
            result.set("stamina", *stamina);
            if let Some(hit) = hit {
                let total: f32 = hit.damage.iter().sum();
                let kind = combat::damage_kind(&combat::Damage { cut: hit.damage[0], pierce: hit.damage[1], blunt: hit.damage[2] });
                let zone = Zone::from_id(hit.zone).map_or("?", Zone::name);
                let mut info = VarDictionary::new();
                info.set("dummy", i64::from(hit.dummy));
                info.set("zone", i64::from(hit.zone));
                info.set("zone_name", zone);
                info.set("damage", total);
                info.set("kind", kind);
                info.set("speed", hit.speed);
                info.set("impulse", hit.impulse);
                result.set("hit", &info);
                result.set("message", format!("{kind} am {zone}: {total:.0} ({:.0} m/s)", hit.speed).as_str());
            } else {
                result.set("message", "daneben");
            }
        }
    }
    result
}

impl CombatState {
    /// Antworten und Puppen vom Server abholen.
    pub(crate) fn poll(&mut self, network: Option<&NetworkSource>) {
        let Some(network) = network else { return };
        // Vorher prüfen: Was danach noch kommt, holt der nächste Aufruf.
        let failed = network.failed();
        if let Some(dummies) = network.take_dummies() {
            self.dummies = dummies;
        }
        for (request, outcome) in network.take_struck() {
            if let Some(id) = self.waiting.remove(&request) {
                self.results.push(result(id, &outcome));
            }
        }
        if failed {
            for (_, id) in self.waiting.drain() {
                self.results.push(result(id, &AttackOutcome::Refused { reason: "Server ausgefallen".into() }));
            }
        }
    }
}

#[godot_api(secondary)]
impl TerraforgeTerrain {
    /// Lässt einen Schlag los: Auge (Welt), Blick (Radiant), Waffe, Richtung (`TerraforgeCombat`).
    /// Ergebnis über `take_attack_results` (`ok`, `message`, `stamina`, `hit` mit `dummy`, `zone`,
    /// `zone_name`, `damage`, `kind`, `speed`).
    #[func]
    fn attack(&mut self, eye: Vector3, yaw: f32, pitch: f32, weapon: i32, direction: i32) -> i64 {
        self.combat.next += 1;
        let id = self.combat.next;
        let eye = glam(self.base().to_local(eye));
        let (weapon, direction) = (u8::try_from(weapon).unwrap_or(u8::MAX), u8::try_from(direction).unwrap_or(u8::MAX));
        if let Some(network) = self.network.clone() {
            match network.send_attack(eye.to_array(), yaw, pitch, weapon, direction) {
                Some(request) => {
                    self.combat.waiting.insert(request, id);
                }
                None => self.combat.results.push(result(id, &AttackOutcome::Refused { reason: "Server ausgefallen".into() })),
            }
            return id;
        }
        // Ohne Server: dieselben Regeln hier (wie beim Graben ohne Server).
        let Some(world) = self.world.clone() else { return -1 };
        if self.combat.dummies.is_empty() {
            self.combat.dummies = combat::training_dummies(&world).iter().map(|d| (d.feet.to_array(), d.yaw)).collect();
        }
        let (fighter, clock) = self.combat.local.get_or_insert_with(|| (Fighter::default(), Instant::now()));
        let outcome = match (WEAPONS.get(usize::from(weapon)), Direction::from_id(direction)) {
            (Some(weapon), Some(direction)) if eye.is_finite() => match fighter.release(clock.elapsed().as_secs_f64(), weapon, direction) {
                Ok(()) => {
                    let targets: Vec<combat::Target> = self.combat.dummies.iter().map(|&(feet, yaw)| combat::Target { feet: Vec3::from_array(feet), yaw }).collect();
                    let hit = combat::strike(&View::new(eye, yaw, pitch), weapon, direction, &targets);
                    AttackOutcome::Swung { stamina: fighter.stamina.value, hit: hit.as_ref().map(HitInfo::from) }
                }
                Err(reason) => AttackOutcome::Refused { reason: reason.into() },
            },
            _ => AttackOutcome::Refused { reason: "unbekannte Waffe oder Richtung".into() },
        };
        self.combat.results.push(result(id, &outcome));
        id
    }

    #[func]
    fn take_attack_results(&mut self) -> Array<VarDictionary> {
        std::mem::take(&mut self.combat.results).into_iter().collect()
    }

    /// Trainingspuppen (Welt): je `feet` und `yaw`. Mit Server, sobald er sie gemeldet hat.
    #[func]
    fn get_dummies(&mut self) -> Array<VarDictionary> {
        if self.network.is_none()
            && self.combat.dummies.is_empty()
            && let Some(world) = &self.world
        {
            self.combat.dummies = combat::training_dummies(world).iter().map(|d| (d.feet.to_array(), d.yaw)).collect();
        }
        let base = self.base().get_global_transform();
        self.combat
            .dummies
            .iter()
            .map(|&(feet, yaw)| {
                let mut entry = VarDictionary::new();
                entry.set("feet", base * godot(Vec3::from_array(feet)));
                entry.set("yaw", yaw);
                entry
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jedes Dreieck liegt von außen gesehen im Uhrzeigersinn (so zeichnet Godot Vorderseiten).
    #[test]
    fn boxes_face_outwards_in_godot() {
        let mut boxes = Boxes::default();
        boxes.add(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0), [1, 2, 3]);
        for triangle in boxes.indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| boxes.vertices[i as usize]);
            let normal = boxes.normals[triangle[0] as usize];
            assert!((b - a).cross(c - a).dot(normal) < 0.0, "Dreieck {triangle:?} gegen den Uhrzeigersinn");
            // Die Normale zeigt nach außen (vom Mittelpunkt weg).
            assert!(((a + b + c) / 3.0).dot(normal) > 0.0);
        }
    }

    /// Die Lage als Transform: +Y entlang der Klinge, rechtshändig.
    #[test]
    fn poses_become_orthonormal_transforms() {
        let view = View::new(Vec3::new(1.0, 2.0, 3.0), 0.7, 0.2);
        for direction in Direction::ALL {
            for s in [0.0, 0.5, 1.0] {
                let pose = combat::pose(&view, direction, s);
                let t = transform(pose, &view);
                assert!((t.basis.col_b() - godot(pose.blade)).length() < 1e-4);
                assert!((t.basis.determinant() - 1.0).abs() < 1e-3, "{direction:?} {s}");
            }
        }
    }
}
