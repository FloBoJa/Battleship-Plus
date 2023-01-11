use bevy::prelude::*;

use battleship_plus_common::types;

#[derive(Resource)]
pub struct Quadrant {
    pub x: u32,
    pub y: u32,
    pub size: u32,
}

impl Quadrant {
    pub fn new(top_left_corner: types::Coordinate, _player_count: usize) -> Quadrant {
        // TODO: quadrant calculation
        Quadrant {
            x: top_left_corner.x,
            y: top_left_corner.y,
            size: 1,
        }
    }
}
