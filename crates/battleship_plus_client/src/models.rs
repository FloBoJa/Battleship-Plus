use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use bevy::prelude::*;

use battleship_plus_common::{
    game::ship::{GetShipID, Orientation, Ship as GameShip, ShipID},
    types::{Config, ShipType},
};

#[derive(Resource, Deref)]
pub struct ShipMeshes(pub HashMap<ShipType, Handle<Mesh>>);

impl ShipMeshes {
    pub fn new(meshes: &mut ResMut<Assets<Mesh>>) -> ShipMeshes {
        let ship_lengths: HashMap<ShipType, usize> = HashMap::from_iter(vec![
            (ShipType::Destroyer, 2),
            (ShipType::Submarine, 3),
            (ShipType::Cruiser, 3),
            (ShipType::Battleship, 4),
            (ShipType::Carrier, 5),
        ]);

        let ship_meshes = ship_lengths
            .iter()
            .map(|(ship_type, length)| {
                (
                    *ship_type,
                    meshes.add(
                        shape::Box {
                            min_x: -0.5,
                            max_x: -0.5 + *length as f32,
                            min_y: -0.5,
                            max_y: 0.5,
                            min_z: 0.0,
                            max_z: 5.0,
                        }
                        .into(),
                    ),
                )
            })
            .collect();

        ShipMeshes(ship_meshes)
    }
}

const OCEAN_SIZE: f32 = 320.0;
pub const CLICK_PLANE_OFFSET_Z: f32 = 4.9;

pub fn get_ship_model_transform(ship: &GameShip) -> Transform {
    let position = ship.position();
    let translation = Vec3::new(position.0 as f32 + 0.5, position.1 as f32 + 0.5, 0.0);
    let rotation = Quat::from_rotation_z(match ship.orientation() {
        Orientation::North => FRAC_PI_2,
        Orientation::East => 0.0,
        Orientation::South => -FRAC_PI_2,
        Orientation::West => PI,
    });

    Transform::from_translation(translation).with_rotation(rotation)
}

pub fn new_ship_model(ship: &GameShip, meshes: &Res<ShipMeshes>) -> PbrBundle {
    PbrBundle {
        mesh: meshes
            .get(&ship.ship_type())
            .expect("There are meshes for all configured ship types")
            .clone(),
        transform: get_ship_model_transform(ship),
        ..default()
    }
}

#[derive(Component)]
pub struct Ship {
    pub id: ShipID,
}

#[derive(Bundle)]
pub struct ShipBundle {
    pub model: PbrBundle,
    pub ship_id: Ship,
}

impl ShipBundle {
    pub fn new(ship: &GameShip, meshes: &Res<ShipMeshes>) -> Self {
        Self {
            model: new_ship_model(ship, meshes),
            ship_id: Ship { id: ship.id() },
        }
    }
}

#[derive(Resource)]
pub struct GameAssets {
    ocean_scene: Handle<Scene>,
}

pub fn load_assets(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GameAssets {
        ocean_scene: assets.load("models/ocean.glb#Scene0"),
    });
}

#[derive(Bundle)]
pub struct OceanBundle {
    scene: SceneBundle,
    name: Name,
}

impl OceanBundle {
    pub fn new(assets: &Res<GameAssets>, config: Arc<Config>) -> OceanBundle {
        let scale = config.board_size as f32 / OCEAN_SIZE;
        let transform = Transform::from_translation(Vec3::new(
            scale * OCEAN_SIZE / 2.0,
            scale * OCEAN_SIZE / 2.0,
            0.0,
        ))
        .with_scale(Vec3::new(scale, scale, 1.0));
        Self {
            scene: SceneBundle {
                scene: assets.ocean_scene.clone(),
                transform,
                ..default()
            },
            name: Name::new("Ocean"),
        }
    }
}
