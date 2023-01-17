use bevy::prelude::*;

use battleship_plus_common::game::PlayerID as CommonPlayerID;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub enum GameState {
    Unconnected,
    Joining,
    JoiningFailed,
    Lobby,
    PlacementPhase,
    // TODO:
    // Game,
}

#[derive(Resource, Deref)]
pub struct PlayerId(pub CommonPlayerID);
