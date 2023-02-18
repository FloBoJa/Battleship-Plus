use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_mod_raycast::{Intersection, RaycastMesh};
use iyes_loopless::prelude::*;

use battleship_plus_common::{
    game::{
        ship::{GetShipID, Orientation, Ship},
        ship_manager::ShipManager,
    },
    messages::{self, ship_action_request::ActionProperties, EventMessage, StatusCode},
    types::{self, Teams},
};

use crate::{
    game_state::{Config, GameState, PlayerId, PlayerTeam, Ships},
    lobby,
    models::{GameAssets, OceanBundle, ShipBundle, ShipMeshes, CLICK_PLANE_OFFSET_Z},
    networking, placement_phase, RaycastSet,
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
        .add_enter_system(GameState::Game, spawn_components)
        .add_exit_system(GameState::Game, despawn_components)
        // raycast system has been added in PlacementPhasePlugin already
        .add_system(process_game_events.run_in_state(GameState::Game))
        .add_system(process_game_responses.run_in_state(GameState::Game))
        .add_system(select_ship.run_in_state(GameState::Game));
    }
}

#[derive(Resource, Deref)]
pub struct InitialGameState(pub types::ServerState);

#[derive(Resource, Deref, DerefMut)]
struct SelectedShip(u32);

enum State {
    WaitingForTurn,
    ChoosingAction,
    ChoseAction(ActionProperties),
    WaitingForResponse,
}

#[derive(Resource, Deref)]
struct TurnState(State);

#[derive(Component)]
struct DespawnOnExit;

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

fn spawn_components(
    mut commands: Commands,
    ships: Res<Ships>,
    ship_meshes: Res<ShipMeshes>,
    assets: Res<GameAssets>,
    config: Res<Config>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn(OceanBundle::new(&assets, config.clone()))
        .insert(DespawnOnExit);
    commands
        .spawn(DirectionalLightBundle {
            transform: Transform::from_rotation(Quat::from_axis_angle(
                Vec3::new(1.0, -1.0, 0.0),
                0.2,
            )),
            directional_light: DirectionalLight {
                illuminance: 10000.0,
                ..default()
            },
            ..default()
        })
        .insert(Name::new("Directional Light"))
        .insert(DespawnOnExit);

    for (_ship_id, ship) in ships.iter_ships() {
        commands
            .spawn(ShipBundle::new(ship, &ship_meshes))
            .insert(DespawnOnExit);
    }

    // TODO: Extract to models.rs
    let mesh = meshes.add(Mesh::from(shape::Plane {
        size: config.board_size as f32,
    }));
    let material = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        base_color: Color::NONE,
        ..default()
    });
    let click_plane_offset = config.board_size as f32 / 2.0;

    commands
        .spawn(PbrBundle {
            mesh,
            material,
            transform: Transform::from_xyz(
                click_plane_offset,
                click_plane_offset,
                CLICK_PLANE_OFFSET_Z,
            )
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ..default()
        })
        .insert(RaycastMesh::<RaycastSet>::default())
        .insert(Name::new("Grid"))
        .insert(DespawnOnExit);
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

fn select_ship(
    mut commands: Commands,
    intersections: Query<&Intersection<RaycastSet>>,
    selected: Option<ResMut<SelectedShip>>,
    ships: Res<Ships>,
    player_id: Res<PlayerId>,
    mouse_input: Res<Input<MouseButton>>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    let position = match board_position_from_intersection(intersections) {
        Some(position) => types::Coordinate {
            x: position[0] as u32,
            y: position[1] as u32,
        },
        None => return,
    };
    let (selected_player_id, ship_id) = match ships.get_by_position(position) {
        Some(ship) => ship.id(),
        None => return,
    };
    if selected_player_id != **player_id {
        return;
    }

    trace!("Selected ship {ship_id}");
    match selected {
        Some(mut selected) => **selected = ship_id,
        None => commands.insert_resource(SelectedShip(ship_id)),
    }
}

/*
 * TODO:
 * Action systems should be done similarly to placement_phase::send_placement.
 * The contents of the request are encoded in the
 * TurnState(ChoseAction(X)) and SelectedShip resources.
 */

fn despawn_components(
    mut commands: Commands,
    entities_to_despawn: Query<Entity, With<DespawnOnExit>>,
) {
    for entity in entities_to_despawn.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

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

fn board_position_from_intersection(
    intersections: Query<&Intersection<RaycastSet>>,
) -> Option<[i32; 2]> {
    let intersection = intersections.get_single().ok()?;
    intersection
        .position()
        .map(|Vec3 { x, y, .. }| [*x as i32, *y as i32])
}
