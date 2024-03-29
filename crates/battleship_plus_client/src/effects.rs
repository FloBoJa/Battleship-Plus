use bevy::prelude::*;
use iyes_loopless::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;

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
            .add_system(initialize_torpedo_effects.run_in_state(GameState::Game))
            .add_system(initialize_hit_effects.run_in_state(GameState::Game))
            .add_system(animate_hit_material.run_in_state(GameState::Game))
            .add_system(initialize_splash_effects.run_in_state(GameState::Game))
            .add_system(animate_splash_material.run_in_state(GameState::Game))
            .add_system(check_lifetimes);
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
    hit_mesh: Handle<Mesh>,
    hit_material: Handle<StandardMaterial>,
    splash_mesh: Handle<Mesh>,
    splash_material: Handle<StandardMaterial>,
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
            min_y: -0.25,
            max_y: 0.25,
            min_z: -0.25,
            max_z: 0.25,
        }
        .into(),
    );
    let shot_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 1.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let scout_plane_mesh = meshes.add(shape::Cube { size: 1.0 }.into());
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

    let torpedo_mesh = meshes.add(
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
    let torpedo_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let hit_mesh = scout_plane_mesh.clone();
    let hit_material = materials.add(StandardMaterial {
        base_color: Color::rgba(1.0, 0.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let splash_mesh = hit_mesh.clone();
    let splash_material = materials.add(StandardMaterial {
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
        hit_mesh,
        hit_material,
        splash_mesh,
        splash_material,
    });
}

#[derive(Bundle)]
pub struct ShotEffect {
    data: ShotEffectData,
    name: Name,
}

#[derive(Component)]
pub struct ShotEffectData {
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
            data: ShotEffectData {
                ship_position,
                target,
                initialized: false,
            },
            name: Name::new("Shot Effect"),
        }
    }
}

fn initialize_shot_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut ShotEffectData)>,
    assets: Res<EffectAssets>,
    time: Res<Time>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let shot_vector = effect.target - effect.ship_position;
        let length = shot_vector.length();
        let angle = Vec2::X.angle_between(shot_vector);

        let height = 9.0;

        commands
            .entity(entity)
            .insert(PbrBundle {
                mesh: assets.shot_mesh.clone(),
                transform: Transform::from_xyz(
                    effect.ship_position.x,
                    effect.ship_position.y,
                    height,
                )
                .with_scale(Vec3::new(length, 1.0, 1.0))
                .with_rotation(Quat::from_rotation_z(angle)),
                material: assets.shot_material.clone(),
                ..default()
            })
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

        effect.initialized = true;
    }
}

#[derive(Bundle)]
pub struct ScoutPlaneEffect {
    data: ScoutPlaneEffectData,
    name: Name,
}

#[derive(Component)]
pub struct ScoutPlaneEffectData {
    center: Vec2,
    initialized: bool,
}

impl ScoutPlaneEffect {
    pub fn new(_ship: &Ship, center: &Coordinate) -> Self {
        let center = Vec2::new(center.x as f32, center.y as f32);

        Self {
            data: ScoutPlaneEffectData {
                center,
                initialized: false,
            },
            name: Name::new("Scout Plane Effect"),
        }
    }
}

fn initialize_scout_plane_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut ScoutPlaneEffectData)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
    time: Res<Time>,
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

        let height = 9.0;

        commands
            .entity(entity)
            .insert(PbrBundle {
                mesh: assets.scout_plane_mesh.clone(),
                transform: Transform::from_xyz(effect.center.x, effect.center.y, height)
                    .with_scale(Vec3::new(diameter, diameter, 1.0)),
                material: assets.scout_plane_material.clone(),
                ..default()
            })
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

        effect.initialized = true;
    }
}

#[derive(Bundle)]
pub struct PredatorMissileEffect {
    data: PredatorMissileEffectData,
    name: Name,
}

#[derive(Component)]
pub struct PredatorMissileEffectData {
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
            data: PredatorMissileEffectData {
                ship_position,
                target,
                initialized: false,
            },
            name: Name::new("Predator Missile Effect"),
        }
    }
}

fn initialize_predator_missile_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut PredatorMissileEffectData)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
    time: Res<Time>,
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

        let height = 9.0;

        commands
            .entity(entity)
            .insert(SpatialBundle::default())
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

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

#[derive(Bundle)]
pub struct MultiMissileEffect {
    data: MultiMissileEffectData,
    name: Name,
}

#[derive(Component)]
pub struct MultiMissileEffectData {
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
            data: MultiMissileEffectData {
                ship_position,
                target,
                initialized: false,
            },
            name: Name::new("Multi-Missile Attack Effect"),
        }
    }
}

fn initialize_multi_missile_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut MultiMissileEffectData)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
    time: Res<Time>,
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

        let height = 9.0;

        commands
            .entity(entity)
            .insert(SpatialBundle::default())
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

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

#[derive(Bundle)]
pub struct TorpedoEffect {
    data: TorpedoEffectData,
    name: Name,
}

#[derive(Component)]
pub struct TorpedoEffectData {
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
            data: TorpedoEffectData {
                ship_position,
                ship_orientation,
                direction,
                initialized: false,
            },
            name: Name::new("Torpedo Effect"),
        }
    }
}

fn initialize_torpedo_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut TorpedoEffectData)>,
    assets: Res<EffectAssets>,
    config: Res<Config>,
    time: Res<Time>,
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

        let height = 9.0;

        commands
            .entity(entity)
            .insert(PbrBundle {
                mesh: assets.torpedo_mesh.clone(),
                transform: Transform::from_xyz(torpedo_origin.x, torpedo_origin.y, height)
                    .with_scale(Vec3::new(distance, 1.0, 1.0))
                    .with_rotation(Quat::from_rotation_z(angle)),
                material: assets.torpedo_material.clone(),
                ..default()
            })
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
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

#[derive(Bundle)]
pub struct HitEffect {
    data: HitEffectData,
    name: Name,
}

#[derive(Component)]
pub struct HitEffectData {
    position: Vec2,
    initialized: bool,
}

impl HitEffect {
    pub fn new(position: &Coordinate) -> Self {
        let position = Vec2::new(position.x as f32, position.y as f32);

        Self {
            data: HitEffectData {
                position,
                initialized: false,
            },
            name: Name::new("Hit Effect"),
        }
    }
}

fn initialize_hit_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut HitEffectData)>,
    assets: Res<EffectAssets>,
    time: Res<Time>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let height = 9.0;

        commands
            .entity(entity)
            .insert(PbrBundle {
                mesh: assets.hit_mesh.clone(),
                transform: Transform::from_xyz(effect.position.x, effect.position.y, height),
                material: assets.hit_material.clone(),
                ..default()
            })
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

        effect.initialized = true;
    }
}

fn animate_hit_material(
    hit_effects: Query<&HitEffectData>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
) {
    if hit_effects.is_empty() {
        return;
    }

    let hit_material = materials
        .get_mut(&assets.hit_material)
        .expect("Hit material was created at startup");

    let pulse_frequency = 1.0;
    let alpha = pulse_frequency * 2.0 * PI * time.elapsed().as_secs_f32();

    hit_material.base_color.set_a(alpha);
}

#[derive(Bundle)]
pub struct SplashEffect {
    data: SplashEffectData,
    name: Name,
}

#[derive(Component)]
pub struct SplashEffectData {
    position: Vec2,
    initialized: bool,
}

impl SplashEffect {
    pub fn new(position: &Coordinate) -> Self {
        let position = Vec2::new(position.x as f32, position.y as f32);

        Self {
            data: SplashEffectData {
                position,
                initialized: false,
            },
            name: Name::new("Hit Effect"),
        }
    }
}

fn initialize_splash_effects(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut SplashEffectData)>,
    assets: Res<EffectAssets>,
    time: Res<Time>,
) {
    for (entity, mut effect) in effects.iter_mut() {
        if effect.initialized {
            continue;
        }

        let height = 9.0;

        commands
            .entity(entity)
            .insert(PbrBundle {
                mesh: assets.splash_mesh.clone(),
                transform: Transform::from_xyz(effect.position.x, effect.position.y, height),
                material: assets.splash_material.clone(),
                ..default()
            })
            .insert(Lifetime {
                ends_at: time.elapsed() + Duration::from_secs(5),
            });

        effect.initialized = true;
    }
}

fn animate_splash_material(
    splash_effects: Query<&SplashEffectData>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
) {
    if splash_effects.is_empty() {
        return;
    }

    let splash_material = materials
        .get_mut(&assets.splash_material)
        .expect("Splash material was created at startup");

    let pulse_frequency = 1.0;
    let alpha = pulse_frequency * 2.0 * PI * time.elapsed().as_secs_f32();

    splash_material.base_color.set_a(alpha);
}

#[derive(Component)]
struct Lifetime {
    ends_at: Duration,
}

fn check_lifetimes(mut commands: Commands, entities: Query<(Entity, &Lifetime)>, time: Res<Time>) {
    for (entity, lifetime) in entities.iter() {
        if time.elapsed() < lifetime.ends_at {
            continue;
        }
        commands.entity(entity).despawn_recursive();
    }
}
