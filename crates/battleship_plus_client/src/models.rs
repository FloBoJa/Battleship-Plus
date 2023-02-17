use std::f32::consts::{FRAC_PI_2, PI};
use std::collections::HashMap;

use bevy::prelude::*;

use battleship_plus_common::{types::ShipType, game::ship::{Ship, Orientation}};

#[derive(Resource, Deref)]
pub struct ShipMeshes(pub HashMap<ShipType, Handle<Mesh>>);

pub const OCEAN_SIZE: f32 = 320.0;
pub const CLICK_PLANE_OFFSET_Z: f32 = 4.9;

pub fn new_ship_model(ship: &Ship, meshes: &Res<ShipMeshes>) -> PbrBundle {
    let position = ship.position();
    let translation = Vec3::new(position.0 as f32 + 0.5, position.1 as f32 + 0.5, 0.0);
    let rotation = Quat::from_rotation_z(match ship.orientation() {
        Orientation::North => FRAC_PI_2,
        Orientation::East => 0.0,
        Orientation::South => -FRAC_PI_2,
        Orientation::West => PI,
    });
    PbrBundle {
        mesh: meshes
            .get(&ship.ship_type())
            .expect("There are meshes for all configured ship types")
            .clone(),
        transform: Transform::from_translation(translation).with_rotation(rotation),
        ..default()
    }
}

#[derive(Bundle)]
pub struct ShipBundle {
    pub model: PbrBundle,
}

impl ShipBundle {
    pub fn new(ship: &Ship, meshes: &Res<ShipMeshes>) -> Self {
        Self {
            model: new_ship_model(ship, meshes),
        }
    }
}
