use bevy::prelude::*;
use std::sync::Arc;

use battleship_plus_common::{
    game::{ship_manager::ShipManager, PlayerID as CommonPlayerID},
    messages, types,
};

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub enum GameState {
    Unconnected,
    Joining,
    JoiningFailed,
    Lobby,
    PlacementPhase,
    Game,
}

#[derive(Resource, Deref)]
pub struct PlayerId(pub CommonPlayerID);

#[derive(Resource, Deref, DerefMut, Default)]
pub struct Ships(pub ShipManager);

#[derive(Resource, Deref)]
pub struct PlayerTeam(pub types::Teams);

#[derive(Resource, Deref)]
pub struct Config(pub Arc<types::Config>);

#[derive(Resource, Deref)]
pub struct CachedEvents(pub Vec<messages::EventMessage>);
