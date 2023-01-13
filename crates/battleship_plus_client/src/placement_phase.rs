use std::f32::consts::PI;

use bevy::prelude::*;
use iyes_loopless::prelude::AppLooplessStateExt;
use rstar::AABB;

use battleship_plus_common::{types, util};

use crate::game_state::GameState;

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(load_assets)
            .add_enter_system(GameState::PlacementPhase, spawn_components);
    }
}

#[derive(Resource, Deref)]
pub struct Quadrant(AABB<[i32; 2]>);

impl Quadrant {
    pub fn new(corner: types::Coordinate, board_size: u32, player_count: u32) -> Quadrant {
        let corner = (corner.x, corner.y);
        Quadrant(util::quadrant_from_corner(corner, board_size, player_count))
    }
}

#[derive(Resource)]
struct GameAssets {
    ocean_scene: Handle<Scene>,
}

fn load_assets(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        ocean_scene: assets.load("models/ocean.glb#Scene0"),
    });
}

fn spawn_components(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn(SceneBundle {
        scene: assets.ocean_scene.clone(),
        ..default()
    });
    commands.spawn(DirectionalLightBundle {
        transform: Transform::from_rotation(Quat::from_axis_angle(Vec3::new(1.0, -1.0, 0.0), 0.2)),
        directional_light: DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        ..default()
    });
}
