use bevy::prelude::*;
use iyes_loopless::prelude::*;
use std::collections::HashSet;

use battleship_plus_common::{
    game::{
        ship::{Orientation, Ship},
        ship_manager::ShipManager,
    },
    messages::{self, ship_action_request::ActionProperties, EventMessage, StatusCode},
    types::{self, Teams},
};

use crate::{
    game_state::{Config, GameState, PlayerTeam, Ships},
    lobby, networking, placement_phase,
};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(
            CoreStage::First,
            create_resources
                .run_in_state(GameState::PlacementPhase)
                .run_if(|next_state: Option<Res<NextState<GameState>>>| {
                    if let Some(next_state) = next_state {
                        matches!(*next_state, NextState(GameState::Game))
                    } else {
                        false
                    }
                }),
        )
        .add_enter_system(GameState::Game, repeat_cached_events)
        .add_system(process_game_events.run_in_state(GameState::Game))
        .add_system(process_game_responses.run_in_state(GameState::Game));
    }
}

#[derive(Resource, Deref)]
pub struct InitialGameState(pub types::ServerState);

#[derive(Resource, Deref)]
struct SelectedShip(u32);

enum State {
    WaitingForTurn,
    ChoosingAction,
    ChoseAction(ActionProperties),
    WaitingForResponse,
}

#[derive(Resource, Deref)]
struct TurnState(State);

fn create_resources(
    mut commands: Commands,
    initial_game_state: Res<InitialGameState>,
    lobby: Res<lobby::LobbyState>,
    config: Res<Config>,
    player_team: Res<PlayerTeam>,
) {
    commands.insert_resource(TurnState(State::WaitingForTurn));

    let team_state = match **player_team {
        Teams::TeamA => &lobby.team_state_a,
        Teams::TeamB => &lobby.team_state_b,
        Teams::None => unreachable!(),
    };
    let allied_players: HashSet<_> = team_state.iter().map(|player| player.player_id).collect();
    let mut ships = Vec::with_capacity(initial_game_state.team_ships.len());

    for allied_player in allied_players {
        let allied_ship_count = match **player_team {
            Teams::TeamA => config.ship_set_team_a.len(),
            Teams::TeamB => config.ship_set_team_b.len(),
            Teams::None => unreachable!(),
        };

        let ship_states: Vec<&types::ShipState> = initial_game_state
            .team_ships
            .iter()
            .filter(|ship| ship.owner_id == allied_player)
            .collect();

        if ship_states.len() != allied_ship_count {
            error!("Received wrong number of ships for player {allied_player}");
            commands.insert_resource(NextState(GameState::Unconnected));
        }

        for ship_index in 0..allied_ship_count {
            let ship_state = ship_states[ship_index];
            let ship_id = (allied_player, ship_index as u32);
            let position = ship_state
                .position
                .clone()
                .expect("All ships have positions in the initial state");
            let position = (position.x, position.y);
            let orientation = Orientation::from(ship_state.direction());

            ships.push(Ship::new_from_type(
                ship_state.ship_type(),
                ship_id,
                position,
                orientation,
                config.clone(),
            ));
        }
    }

    commands.insert_resource(Ships(ShipManager::new_with_ships(ships)));
}

fn process_game_events(mut events: EventReader<messages::EventMessage>) {
    for event in events.iter() {
        match event {
            EventMessage::NextTurn(_) => {}
            EventMessage::SplashEvent(_) => {}
            EventMessage::HitEvent(_) => {}
            EventMessage::DestructionEvent(_) => {}
            EventMessage::VisionEvent(_) => {}
            EventMessage::ShipActionEvent(_) => {}
            EventMessage::GameOverEvent(_) => {}
            _other_events => {
                // ignore
            }
        }
    }
}

fn process_game_responses(mut events: EventReader<networking::ResponseReceivedEvent>) {
    for networking::ResponseReceivedEvent(messages::StatusMessage {
        code,
        message,
        data,
    }) in events.iter()
    {
        match StatusCode::from_i32(*code) {
            Some(StatusCode::Ok) => {
                process_game_response_data(data, message);
            }
            None => {}
            Some(_) => {}
        }
    }
}

fn process_game_response_data(data: &Option<messages::status_message::Data>, message: &str) {
    match data {
        Some(messages::status_message::Data::ServerStateResponse(_)) => {
            println!("{}", message);
        }
        _ => {}
    }
}

/*
 * TODO:
 * Action systems should be done similarly to placement_phase::send_placement.
 * The contents of the request are encoded in the
 * TurnState(ChoseAction(X)) and SelectedShip resources.
 */

fn repeat_cached_events(
    mut commands: Commands,
    cached_events: Option<Res<placement_phase::CachedEvents>>,
    mut event_writer: EventWriter<messages::EventMessage>,
) {
    let cached_events = match cached_events {
        Some(events) => events.clone(),
        None => return,
    };
    event_writer.send_batch(cached_events.into_iter());
    commands.remove_resource::<placement_phase::CachedEvents>();
}
