use bevy::prelude::*;
use iyes_loopless::prelude::*;
use std::sync::Arc;

use battleship_plus_common::{
    game::ship::Ship,
    types::{Config, Coordinate, Direction},
};

use crate::game_state::GameState;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(load_assets)
            .add_system(initialize_shot_effects.run_in_state(GameState::Game));
    }
}

#[derive(Resource)]
pub struct EffectAssets {
    shot_mesh: Handle<Mesh>,
    shot_material: Handle<StandardMaterial>,
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let shot_mesh = meshes.add(
        shape::Box {
            min_x: 0.0,
            max_x: 1.0,
            min_y: -0.5,
            max_y: 0.5,
            min_z: -0.5,
            max_z: 0.5,
        }
        .into(),
    );
    let shot_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 0.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.insert_resource(EffectAssets {
        shot_mesh,
        shot_material,
    });
}

#[derive(Component)]
pub struct ShotEffect {
    ship_position: Vec2,
    target: Vec2,
    initialized: bool,
}

impl ShotEffect {
    pub fn new(ship: &Ship, target: &Coordinate) -> Self {
        let ship_position = ship.position();
        let ship_position = Vec2::new(ship_position.0 as f32, ship_position.1 as f32);
        let target = Vec2::new(target.x as f32, target.y as f32);
        Self {
            ship_position,
            target,
            initialized: false,
        }
    }
}

fn initialize_shot_effects(
    mut commands: Commands,
    mut shot_effects: Query<(Entity, &mut ShotEffect)>,
    assets: Res<EffectAssets>,
) {
    for (entity, mut shot_effect) in shot_effects.iter_mut() {
        if shot_effect.initialized {
            continue;
        }

        let shot_vector = shot_effect.target - shot_effect.ship_position;
        let length = shot_vector.length();
        let angle = Vec2::X.angle_between(shot_vector);

        let height = 10.0;

        commands.entity(entity).insert(PbrBundle {
            mesh: assets.shot_mesh.clone(),
            transform: Transform::from_xyz(
                shot_effect.ship_position.x,
                shot_effect.ship_position.y,
                height,
            )
            .with_scale(Vec3::new(length, 1.0, 1.0))
            .with_rotation(Quat::from_rotation_z(angle)),
            material: assets.shot_material.clone(),
            ..default()
        });

        shot_effect.initialized = true;
    }
}

#[derive(Component)]
pub struct ScoutPlaneEffect {}

impl ScoutPlaneEffect {
    pub fn new(ship: &Ship, target: &Coordinate) -> Self {
        Self {}
    }
}

#[derive(Component)]
pub struct PredatorMissileEffect {}

impl PredatorMissileEffect {
    pub fn new(ship: &Ship, target: &Coordinate) -> Self {
        Self {}
    }
}

#[derive(Component)]
pub struct MultiMissileEffect {}

impl MultiMissileEffect {
    pub fn new(ship: &Ship, target: &Coordinate) -> Self {
        Self {}
    }
}

#[derive(Component)]
pub struct TorpedoEffect {}

impl TorpedoEffect {
    pub fn new(ship: &Ship, direction: Direction) -> Self {
        Self {}
    }
}
