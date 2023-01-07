use bevy::prelude::*;

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

#[derive(Resource)]
pub struct PlayerId(pub u32);
