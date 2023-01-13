use bevy::prelude::*;
use rstar::AABB;

use battleship_plus_common::{types, util};

pub struct PlacementPhasePlugin;

impl Plugin for PlacementPhasePlugin {
    fn build(&self, _app: &mut App) {}
}

#[derive(Resource, Deref)]
pub struct Quadrant(AABB<[i32; 2]>);

impl Quadrant {
    pub fn new(corner: types::Coordinate, board_size: u32, player_count: u32) -> Quadrant {
        let corner = (corner.x, corner.y);
        Quadrant(util::quadrant_from_corner(corner, board_size, player_count))
    }
}
