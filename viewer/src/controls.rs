//! Steuerung: Laufmodus (F) und Autopilot für Messungen.

use bevy::{camera_controller::free_camera::FreeCameraState, prelude::*};

use crate::WorldRes;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (walk_controller, auto_pilot));
    }
}

#[derive(Resource, Default)]
pub struct WalkMode(pub bool);

#[derive(Resource, Default)]
pub struct WalkState(pub Vec3);

#[derive(Resource)]
pub struct AutoPilot(pub f32);

fn walk_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    world: Res<WorldRes>,
    mut mode: ResMut<WalkMode>,
    mut state: ResMut<WalkState>,
    mut camera: Query<(&mut Transform, &FreeCameraState), With<Camera3d>>,
) {
    let Ok((mut transform, free)) = camera.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::KeyF) {
        mode.0 = !mode.0;
        if mode.0 {
            state.0 = transform.translation;
        }
    }
    if !mode.0 {
        return;
    }

    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }

    if input != Vec2::ZERO {
        let yaw = free.yaw;
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let direction = (forward * input.y + right * input.x).normalize_or_zero();
        let sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let speed = if sprint { 9.0 } else { 4.5 };
        state.0.x += direction.x * speed * time.delta_secs();
        state.0.z += direction.z * speed * time.delta_secs();
    }

    let ground = world.0.column_height(state.0.x, state.0.z);
    let floor = ground.max(world.0.cfg.sea_level - 0.9);
    state.0.y = floor + 1.7;
    transform.translation = state.0;
}

fn auto_pilot(
    time: Res<Time>,
    pilot: Option<Res<AutoPilot>>,
    world: Res<WorldRes>,
    walk: Res<WalkMode>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    let Some(pilot) = pilot else {
        return;
    };
    if walk.0 {
        return;
    }
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    transform.translation.x += pilot.0 * time.delta_secs();
    let ground = world
        .0
        .column_height(transform.translation.x, transform.translation.z);
    transform.translation.y = ground + 32.0;
}
