use bevy::prelude::*;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub enum GameState {
    Unconnected,
    Joining,
    JoiningFailed,
    Lobby,
    // TODO:
    // Placement,
    // Game,
}

#[derive(Resource)]
pub struct PlayerId(pub u32);
