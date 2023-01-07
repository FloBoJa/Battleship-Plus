use bevy::prelude::*;

use battleship_plus_common::types;

#[derive(Resource)]
pub struct Quadrant {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Quadrant {
    pub fn new(top_left_corner: types::Coordinate, player_count: usize) -> Quadrant {
        // TODO: quadrant calculation
        Quadrant {
            x: top_left_corner.x,
            y: top_left_corner.y,
            w: 1,
            h: 1,
        }
    }
}
