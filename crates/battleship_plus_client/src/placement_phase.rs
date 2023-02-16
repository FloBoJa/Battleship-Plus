use bevy::prelude::*;

use crate::game_state::GameState;
use crate::networking;
use battleship_plus_common::messages::*;
use battleship_plus_common::types::*;
use battleship_plus_common::{messages, types};
use bevy_quinnet_client::Client;
use iyes_loopless::prelude::IntoConditionalSystem;

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

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(main.run_in_state(GameState::PlacementPhase));
    }
}

fn main(mut client: ResMut<Client>, mut commands: Commands) {
    //send debug placement to enter game state
    let con = client.get_connection().expect("");

    let mut assignment = vec![];

    //TODO: add ships to quadrant
    /*TODO: like this for ever ship (for testing: for loop over x or y value)
    assignment.push(ShipAssignment {
        ship_number: 0,
        coordinate: Some(Coordinate { x: 0, y: 0 }),
        direction: 0,
    });
     */

    let message = SetPlacementRequest {
        assignments: assignment,
    };

    if let Err(error) = con.send_message(message.into()) {
        error!("Could not send message <ShipActionRequest>: {error}");
    } else {
        println!("ship placement request send");
    }
}

//TODO: wait for ship placement response and enter GameState::Game
//commands.insert_resource(NextState(GameState::Game));
