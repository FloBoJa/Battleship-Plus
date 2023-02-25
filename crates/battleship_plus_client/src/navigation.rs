use std::collections::HashSet;

use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    math::Vec3Swizzles,
    prelude::*,
};
use iyes_loopless::prelude::*;

use crate::game_state::GameState;

pub struct NavigationPlugin {
    pub enabled_in: HashSet<GameState>,
}

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        let enabled_in = self.enabled_in.clone();
        let is_in_enabled_state =
            move |state: Res<CurrentState<GameState>>| enabled_in.contains(&state.0);
        app.add_system(zoom.run_if(is_in_enabled_state.clone()));
        app.add_system(translate.run_if(is_in_enabled_state.clone()));
    }
}

fn zoom(
    mut camera: Query<(&mut Transform, With<Camera3d>)>,
    windows: Res<Windows>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
) {
    let window = windows
        .get_primary()
        .expect("This game always has a window");
    let target = match window.cursor_position() {
        Some(position) => {
            (position - Vec2::new(window.width() / 2.0, window.height() / 2.0)) / window.height()
        }
        None => Vec2::new(0.0, 0.0),
    };
    let (mut camera, _) = camera.single_mut();

    for &MouseWheel { y, .. } in mouse_wheel_events.iter() {
        // Empirical, hard-coded values are good enough for now.
        // TODO: Make this dependent on the scroll speed (y) without the zoom motion skipping.
        // Currently, the maximum zoom speed is limited to one click per frame.
        let zoom_factor = 1.05;
        let translation_factor = 34.0;

        if y < 0.0 {
            // Zoom in
            camera.scale *= Vec3::new(zoom_factor, zoom_factor, 1.0);
            let target = translation_factor * target * camera.scale.xy();
            camera.translation -= Vec3::new(target.x, target.y, 0.0);
        } else if y > 0.0 {
            // Zoom out
            let target = translation_factor * target * camera.scale.xy();
            camera.translation += Vec3::new(target.x, target.y, 0.0);
            camera.scale /= Vec3::new(zoom_factor, zoom_factor, 1.0);
        }
    }
}

fn translate(
    mut camera: Query<(&mut Transform, With<Camera3d>)>,
    mouse: Res<Input<MouseButton>>,
    mut mouse_motion_events: EventReader<MouseMotion>,
) {
    if !mouse.pressed(MouseButton::Left) {
        return;
    }

    let (mut camera, _) = camera.single_mut();
    for &MouseMotion { delta } in mouse_motion_events.iter() {
        let translation_factor = camera.scale * 0.5;
        camera.translation += translation_factor * Vec3::new(-delta.x, delta.y, 0.0);
    }
}
