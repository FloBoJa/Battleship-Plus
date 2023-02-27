use bevy::prelude::*;
use std::sync::Arc;

use battleship_plus_common::{
    game::ship::Ship,
    types::{Config, Coordinate, Direction},
};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {}
}

#[derive(Bundle)]
pub struct ShotEffect {}

impl ShotEffect {
    pub fn new(ship: &Ship, target: &Coordinate) -> Self {
        Self {}
    }
}

#[derive(Bundle)]
pub struct ScoutPlaneEffect {}

impl ScoutPlaneEffect {
    pub fn new(ship: &Ship, target: &Coordinate, config: Arc<Config>) -> Self {
        Self {}
    }
}

#[derive(Bundle)]
pub struct PredatorMissileEffect {}

impl PredatorMissileEffect {
    pub fn new(ship: &Ship, target: &Coordinate, config: Arc<Config>) -> Self {
        Self {}
    }
}

#[derive(Bundle)]
pub struct MultiMissileEffect {}

impl MultiMissileEffect {
    pub fn new(ship: &Ship, target: &Coordinate, config: Arc<Config>) -> Self {
        Self {}
    }
}

#[derive(Bundle)]
pub struct TorpedoEffect {}

impl TorpedoEffect {
    pub fn new(ship: &Ship, direction: Direction, config: Arc<Config>) -> Self {
        Self {}
    }
}
