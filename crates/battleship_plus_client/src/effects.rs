use bevy::prelude::*;
use iyes_loopless::prelude::*;

use battleship_plus_common::{
    game::ship::Ship,
    types::{Coordinate, Direction},
};

use crate::game_state::{Config, GameState};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(load_assets)
            .add_system(initialize_shot_effects.run_in_state(GameState::Game))
            .add_system(initialize_scout_plane_effects.run_in_state(GameState::Game))
            .add_system(initialize_predator_missile_effects.run_in_state(GameState::Game))
            .add_system(initialize_multi_missile_effects.run_in_state(GameState::Game))
            .add_system(initialize_torpedo_effects.run_in_state(GameState::Game));
    }
}

#[derive(Resource)]
pub struct EffectAssets {
    shot_mesh: Handle<Mesh>,
    shot_material: Handle<StandardMaterial>,
    scout_plane_mesh: Handle<Mesh>,
    scout_plane_material: Handle<StandardMaterial>,
    predator_missile_travel_mesh: Handle<Mesh>,
    predator_missile_travel_material: Handle<StandardMaterial>,
    predator_missile_impact_mesh: Handle<Mesh>,
    predator_missile_impact_material: Handle<StandardMaterial>,
    multi_missile_travel_mesh: Handle<Mesh>,
    multi_missile_travel_material: Handle<StandardMaterial>,
    multi_missile_impact_mesh: Handle<Mesh>,
    multi_missile_impact_material: Handle<StandardMaterial>,
    torpedo_mesh: Handle<Mesh>,
    torpedo_material: Handle<StandardMaterial>,
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
        base_color: Color::rgba(1.0, 1.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let scout_plane_mesh = meshes.add(shape::Plane { size: 1.0 }.into());
    let scout_plane_material = materials.add(StandardMaterial {
        base_color: Color::rgba(0.0, 1.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let predator_missile_travel_mesh = shot_mesh.clone();
    let predator_missile_travel_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 0.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let predator_missile_impact_mesh = scout_plane_mesh.clone();
    let predator_missile_impact_material = predator_missile_travel_material.clone();

    let multi_missile_travel_mesh = predator_missile_travel_mesh.clone();
    let multi_missile_travel_material = materials.add(StandardMaterial {
        base_color: Color::rgba(0.0, 1.0, 1.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let multi_missile_impact_mesh = predator_missile_impact_mesh.clone();
    let multi_missile_impact_material = multi_missile_travel_material.clone();

    let torpedo_mesh = shot_mesh.clone();
    let torpedo_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands.insert_resource(EffectAssets {
        shot_mesh,
        shot_material,
        scout_plane_mesh,
        scout_plane_material,
        predator_missile_travel_mesh,
        predator_missile_travel_material,
        predator_missile_impact_mesh,
        predator_missile_impact_material,
        multi_missile_travel_mesh,
        multi_missile_travel_material,
        multi_missile_impact_mesh,
        multi_missile_impact_material,
        torpedo_mesh,
        torpedo_material,
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
    mut effects: Query<(Entity, &mut ShotEffect)>,
    assets: Res<EffectAssets>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let shot_vector = effect.target - effect.ship_position;
        let length = shot_vector.length();
        let angle = Vec2::X.angle_between(shot_vector);

        let height = 10.0;

        commands.entity(entity).insert(PbrBundle {
            mesh: assets.shot_mesh.clone(),
            transform: Transform::from_xyz(effect.ship_position.x, effect.ship_position.y, height)
                .with_scale(Vec3::new(length, 1.0, 1.0))
                .with_rotation(Quat::from_rotation_z(angle)),
            material: assets.shot_material.clone(),
            ..default()
        });

        effect.initialized = true;
    }
}

#[derive(Component)]
pub struct ScoutPlaneEffect {
    center: Vec2,
    initialized: bool,
}

impl ScoutPlaneEffect {
    pub fn new(_ship: &Ship, center: &Coordinate) -> Self {
        let center = Vec2::new(center.x as f32, center.y as f32);

        Self {
            center,
            initialized: false,
        }
    }
}

fn initialize_scout_plane_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut ScoutPlaneEffect)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let radius = config
            .carrier_balancing
            .as_ref()
            .expect("Carriers must have a balancing during a game")
            .scout_plane_radius as f32;
        let diameter = 1.0 + radius * 2.0;

        let height = 20.0;

        commands.entity(entity).insert(PbrBundle {
            mesh: assets.scout_plane_mesh.clone(),
            transform: Transform::from_xyz(effect.center.x, effect.center.y, height)
                .with_scale(Vec3::new(diameter, diameter, 1.0)),
            material: assets.scout_plane_material.clone(),
            ..default()
        });

        effect.initialized = true;
    }
}

#[derive(Component)]
pub struct PredatorMissileEffect {
    ship_position: Vec2,
    target: Vec2,
    initialized: bool,
}

impl PredatorMissileEffect {
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

fn initialize_predator_missile_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut PredatorMissileEffect)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let travel_vector = effect.target - effect.ship_position;
        let distance = travel_vector.length();
        let angle = Vec2::X.angle_between(travel_vector);

        let radius = config
            .battleship_balancing
            .as_ref()
            .expect("Battleships must have a balancing during a game")
            .predator_missile_radius as f32;
        let diameter = 1.0 + radius * 2.0;

        let height = 20.0;

        commands
            .spawn(PbrBundle {
                mesh: assets.predator_missile_travel_mesh.clone(),
                transform: Transform::from_xyz(
                    effect.ship_position.x,
                    effect.ship_position.y,
                    height,
                )
                .with_scale(Vec3::new(distance, 1.0, 1.0))
                .with_rotation(Quat::from_rotation_z(angle)),
                material: assets.predator_missile_travel_material.clone(),
                ..default()
            })
            .insert(Name::new("Travel"))
            .set_parent(entity);

        commands
            .spawn(PbrBundle {
                mesh: assets.predator_missile_impact_mesh.clone(),
                transform: Transform::from_xyz(effect.target.x, effect.target.y, height)
                    .with_scale(Vec3::new(diameter, diameter, 1.0)),
                material: assets.predator_missile_impact_material.clone(),
                ..default()
            })
            .insert(Name::new("Impact"))
            .set_parent(entity);

        effect.initialized = true;
    }
}

#[derive(Component)]
pub struct MultiMissileEffect {
    ship_position: Vec2,
    target: Vec2,
    initialized: bool,
}

impl MultiMissileEffect {
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

fn initialize_multi_missile_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut MultiMissileEffect)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let travel_vector = effect.target - effect.ship_position;
        let distance = travel_vector.length();
        let angle = Vec2::X.angle_between(travel_vector);

        let radius = config
            .destroyer_balancing
            .as_ref()
            .expect("Destroyers must have a balancing during a game")
            .multi_missile_radius as f32;
        let diameter = 1.0 + radius * 2.0;

        let height = 20.0;

        commands
            .spawn(PbrBundle {
                mesh: assets.multi_missile_travel_mesh.clone(),
                transform: Transform::from_xyz(
                    effect.ship_position.x,
                    effect.ship_position.y,
                    height,
                )
                .with_scale(Vec3::new(distance, 1.0, 1.0))
                .with_rotation(Quat::from_rotation_z(angle)),
                material: assets.multi_missile_travel_material.clone(),
                ..default()
            })
            .insert(Name::new("Travel"))
            .set_parent(entity);

        commands
            .spawn(PbrBundle {
                mesh: assets.multi_missile_impact_mesh.clone(),
                transform: Transform::from_xyz(effect.target.x, effect.target.y, height)
                    .with_scale(Vec3::new(diameter, diameter, 1.0)),
                material: assets.multi_missile_impact_material.clone(),
                ..default()
            })
            .insert(Name::new("Impact"))
            .set_parent(entity);

        effect.initialized = true;
    }
}

#[derive(Component)]
pub struct TorpedoEffect {
    ship_position: Vec2,
    ship_orientation: Vec2,
    direction: Vec2,
    initialized: bool,
}

impl TorpedoEffect {
    pub fn new(ship: &Ship, direction: Direction) -> Self {
        let ship_position = ship.position();
        let ship_position = Vec2::new(ship_position.0 as f32, ship_position.1 as f32);
        let ship_orientation = direction_to_vector(ship.orientation().into());
        let direction = direction_to_vector(direction);

        Self {
            ship_position,
            ship_orientation,
            direction,
            initialized: false,
        }
    }
}

fn initialize_torpedo_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut TorpedoEffect)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let distance = config
            .cruiser_balancing
            .as_ref()
            .expect("Cruisers must have a balancing during a game")
            .engine_boost_distance as f32;
        let offset = if effect.ship_orientation.abs_diff_eq(effect.direction, 0.01) {
            // Length of a cruiser:
            3.0
        } else {
            0.0
        };
        let torpedo_origin = effect.ship_position + offset * effect.direction;
        let angle = Vec2::X.angle_between(effect.direction);

        let height = 10.0;

        commands.entity(entity).insert(PbrBundle {
            mesh: assets.torpedo_mesh.clone(),
            transform: Transform::from_xyz(torpedo_origin.x, torpedo_origin.y, height)
                .with_scale(Vec3::new(distance, 1.0, 1.0))
                .with_rotation(Quat::from_rotation_z(angle)),
            material: assets.torpedo_material.clone(),
            ..default()
        });

        effect.initialized = true;
    }
}

fn direction_to_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}
