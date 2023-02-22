use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_egui::EguiContext;
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
use bevy_quinnet_client::Client;

use crate::{
    game_state::{CachedEvents, Config, GameState, PlayerId, PlayerTeam, Ships},
    lobby,
    models::{GameAssets, OceanBundle, ShipBundle, ShipMeshes, CLICK_PLANE_OFFSET_Z},
    networking, RaycastSet,
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
        .add_system(process_responses.run_in_state(GameState::Game))
        .add_system_to_stage(
            CoreStage::PostUpdate,
            process_game_events.run_in_state(GameState::Game),
        )
        .add_system(select_ship.run_in_state(GameState::Game))
        .add_system(draw_menu.run_in_state(GameState::Game))
        .add_system(send_actions.run_in_state(GameState::Game));
    }
}

#[derive(Resource, Deref)]
pub struct InitialGameState(pub types::ServerState);

#[derive(Resource, Deref, DerefMut)]
struct SelectedShip(u32);

type PositionInQueue = Option<u32>;

enum State {
    WaitingForTurn(PositionInQueue),
    ChoosingAction,
    ChoseAction(Option<ActionProperties>),
    WaitingForResponse,
}

#[derive(Resource, Deref, DerefMut)]
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
    commands.insert_resource(TurnState(State::WaitingForTurn(None)));

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

        for (ship_index, ship_state) in ship_states.iter().enumerate().take(allied_ship_count) {
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

fn draw_menu(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    selected: Option<ResMut<SelectedShip>>,
    ships: ResMut<Ships>,
    player_id: Res<PlayerId>,
    mut turn_state: ResMut<TurnState>,
) {
    let selected = match selected {
        Some(selected) => ships.get_by_id(&(**player_id, **selected)),
        None => None,
    };

    egui::TopBottomPanel::bottom(egui::Id::new("placement_menu")).show(
        egui_context.ctx_mut(),
        |ui| {
            ui.horizontal(|ui| {
                ui.horizontal_centered(|ui| {
                    ui.set_height(50.0);

                    // TODO: respect cooldowns and resources.
                    let may_shoot = selected.is_some();
                    let shoot_button = ui.add_enabled(may_shoot, egui::Button::new("Shoot"));
                    if shoot_button.clicked() {
                        debug!("Initiating shot...");
                        debug!("Selecting target...");
                        debug!("Selected self");
                        let selected =
                            selected.expect("Button can only be clicked when a ship is selected");
                        let position = selected.position();
                        let target = Some(types::Coordinate {
                            x: position.0 as u32,
                            y: position.1 as u32,
                        });
                        let shoot_properties = types::ShootProperties { target };
                        **turn_state = State::ChoseAction(Some(ActionProperties::ShootProperties(
                            shoot_properties,
                        )));
                    }

                    let may_use_special = selected.is_some();
                    let special_button =
                        ui.add_enabled(may_use_special, egui::Button::new("Special"));
                    if special_button.clicked() {
                        debug!("Initiating special ability...");
                        let _selected =
                            selected.expect("Button can only be clicked when a ship is selected");
                        warn!("Special abilities are not yet implemented!");
                    }
                });

                ui.separator();

                ui.label("Move:");
                let may_move = selected.is_some();
                let forward_button = ui.add_enabled(may_move, egui::Button::new("\u{2b06}"));
                let backward_button = ui.add_enabled(may_move, egui::Button::new("\u{2b07}"));
                let mut direction = None;
                if forward_button.clicked() {
                    trace!("Moving forward");
                    direction = Some(types::MoveDirection::Forward);
                } else if backward_button.clicked() {
                    trace!("Moving backward");
                    direction = Some(types::MoveDirection::Backward);
                }
                if let Some(direction) = direction {
                    **turn_state = State::ChoseAction(Some(ActionProperties::MoveProperties(
                        types::MoveProperties {
                            direction: direction.into(),
                        },
                    )));
                }

                ui.separator();

                ui.label("Rotate:");
                let may_rotate = selected.is_some();
                let clockwise_button = ui.add_enabled(may_rotate, egui::Button::new("\u{21A9}"));
                let counter_clockwise_button =
                    ui.add_enabled(may_rotate, egui::Button::new("\u{21AA}"));
                let mut direction = None;
                if clockwise_button.clicked() {
                    trace!("Rotating clockwise");
                    direction = Some(types::RotateDirection::Clockwise);
                } else if counter_clockwise_button.clicked() {
                    trace!("Rotating counter-clockwise");
                    direction = Some(types::RotateDirection::CounterClockwise);
                }
                if let Some(direction) = direction {
                    **turn_state = State::ChoseAction(Some(ActionProperties::RotateProperties(
                        types::RotateProperties {
                            direction: direction.into(),
                        },
                    )));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_height(50.0);

                    let format = egui::text::TextFormat {
                        color: egui::Color32::RED,
                        ..default()
                    };
                    let mut text = egui::text::LayoutJob::default();
                    text.append("Leave Game", 0.0, format);

                    if ui.button(text).clicked() {
                        info!("Disconnecting from the server on user request");
                        commands.insert_resource(NextState(GameState::Unconnected));
                    }

                    let end_turn_button = ui.add_enabled(
                        matches!(**turn_state, State::ChoosingAction),
                        egui::Button::new("End Turn"),
                    );
                    if end_turn_button.clicked() {
                        trace!("Ending turn");
                        **turn_state = State::ChoseAction(None);
                    }
                });
            });
        },
    );
}

fn process_game_events(
    mut commands: Commands,
    mut events: EventReader<messages::EventMessage>,
    (player_id, player_team): (Res<PlayerId>, Res<PlayerTeam>),
    mut turn_state: ResMut<TurnState>,
) {
    let mut transition_happened = false;
    for event in events.iter() {
        match event {
            EventMessage::NextTurn(messages::NextTurn {
                next_player_id,
                position_in_queue,
            }) => {
                if **player_id == *next_player_id {
                    info!("Turn started");
                    **turn_state = State::ChoosingAction;
                } else {
                    match **turn_state {
                        State::WaitingForTurn(_) | State::ChoosingAction => {}
                        State::ChoseAction(_) => {
                            debug!("Action is aborted, the turn ended");
                        }
                        State::WaitingForResponse => {
                            warn!("Was waiting for response when turn ended, assuming that action did not execute.");
                            // TODO: Robustness: request server state.
                        }
                    };
                    **turn_state = if *position_in_queue == 0 {
                        info!("It is {next_player_id}'s turn now");
                        State::WaitingForTurn(None)
                    } else {
                        info!("It is {next_player_id}'s turn now. {position_in_queue} turns remaining");
                        State::WaitingForTurn(Some(*position_in_queue))
                    }
                }
            }
            EventMessage::SplashEvent(splash) => {
                let splashes: Vec<_> = splash.coordinate.iter().map(|x| (x.x, x.y)).collect();
                if splashes.len() == 1 {
                    info!("Splash at {:?}", splashes[0]);
                } else {
                    info!("Splashes at {:?}", splashes);
                }
            }
            EventMessage::HitEvent(hit) => {
                if let Some(types::Coordinate { x, y }) = hit.coordinate {
                    info!("Hit at ({x}, {y}) for {} damage", hit.damage);
                }
            }
            EventMessage::DestructionEvent(destruction) => {
                if let Some(types::Coordinate { x, y }) = destruction.coordinate {
                    info!(
                        "Player {} lost ship {} at ({x}, {y}), facing {:?}",
                        destruction.owner,
                        destruction.ship_number,
                        destruction.direction()
                    );
                }
            }
            EventMessage::VisionEvent(vision) => {
                for types::Coordinate { x, y } in &vision.vanished_ship_fields {
                    info!("Lost sight of ship at ({x}, {y})");
                }
                for types::Coordinate { x, y } in &vision.discovered_ship_fields {
                    info!("Sighted ship at ({x}, {y})");
                }
            }
            EventMessage::ShipActionEvent(action) => {
                info!(
                    "Ship {} executed {:?}",
                    action.ship_number, action.action_properties
                );
            }
            EventMessage::GameOverEvent(messages::GameOverEvent { reason, winner }) => {
                let reason = types::GameEndReason::from_i32(*reason);
                let winner = types::Teams::from_i32(*winner);
                if Some(types::GameEndReason::Disconnect) == reason {
                    info!("Someone left the game, forcing it to be aborted");
                }
                match winner {
                    Some(team) => {
                        if **player_team == team {
                            info!("Victory!");
                        } else if types::Teams::None == team {
                            info!("Draw!");
                        } else {
                            info!("Defeat!");
                        }
                    }
                    None => todo!(),
                }
                info!("Returning to lobby");
                commands.insert_resource(NextState(GameState::Lobby));
                transition_happened = true;
            }
            _other_events => {
                // ignore
            }
        }
    }

    if transition_happened {
        trace!("Repeating events that happened during state transition");
        let events = Vec::from_iter(events.iter().map(|event| (*event).clone()));
        commands.insert_resource(CachedEvents(events));
    }
}

fn process_responses(
    mut commands: Commands,
    mut events: EventReader<networking::ResponseReceivedEvent>,
    mut turn_state: ResMut<TurnState>,
) {
    for networking::ResponseReceivedEvent(messages::StatusMessage {
        code,
        message,
        data,
    }) in events.iter()
    {
        let original_code = code;
        let code = StatusCode::from_i32(*code);
        match code {
            Some(StatusCode::Ok) => {
                process_response_data(data, message, &mut turn_state);
            }
            Some(StatusCode::OkWithWarning) => {
                if message.is_empty() {
                    warn!("Received OK response with warning but without message");
                } else {
                    warn!("Received OK response with warning: {message}");
                }
                process_response_data(data, message, &mut turn_state);
            }
            Some(StatusCode::BadRequest) => {
                if message.is_empty() {
                    warn!("Illegal ship placement");
                } else {
                    warn!("Illegal ship placement: {message}");
                }
                **turn_state = State::ChoosingAction;
            }
            Some(StatusCode::ServerError) => {
                if message.is_empty() {
                    error!("Server error, disconnecting");
                } else {
                    error!("Server error with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            Some(StatusCode::UnsupportedVersion) => {
                if message.is_empty() {
                    error!("Unsupported protocol version, disconnecting");
                } else {
                    error!("Unsupported protocol version, disconnecting. Attached message: \"{message}\"");
                }
            }
            Some(other_code) => {
                if message.is_empty() {
                    error!("Received inappropriate status code {other_code:?}, disconnecting");
                } else {
                    error!("Received inappropriate status code {other_code:?} with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            None => {
                if message.is_empty() {
                    error!("Received unknown status code {original_code}, disconnecting");
                } else {
                    error!("Received unknown status code {original_code} with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
        }
    }
}

fn process_response_data(
    data: &Option<messages::status_message::Data>,
    message: &str,
    turn_state: &mut ResMut<TurnState>,
) {
    match data {
        Some(messages::status_message::Data::ShipActionResponse(_)) => {
            ***turn_state = State::ChoosingAction;
        }
        Some(_other_response) => {
            // ignore
        }
        None => {
            if message.is_empty() {
                warn!("No data in OK response");
            } else {
                warn!("No data in OK response with message: {message}");
            }
            // ignore
        }
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

fn send_actions(
    mut commands: Commands,
    mut turn_state: ResMut<TurnState>,
    selected: Option<ResMut<SelectedShip>>,
    client: Res<Client>,
) {
    let ship_number = match selected {
        Some(selected) => **selected,
        None => return,
    };
    let action_properties = match &**turn_state {
        State::ChoseAction(action) => action.clone(),
        _ => return,
    };
    let message = messages::ShipActionRequest {
        ship_number,
        action_properties,
    };
    if let Err(error) = client.connection().send_message(message.into()) {
        error!("Could not send ShipActionRequest: {error}, disonnecting");
        commands.insert_resource(NextState(GameState::Unconnected));
    } else {
        **turn_state = State::WaitingForResponse;
    }
}

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
    cached_events: Option<Res<CachedEvents>>,
    mut event_writer: EventWriter<messages::EventMessage>,
) {
    let cached_events = match cached_events {
        Some(events) => events.clone(),
        None => return,
    };
    event_writer.send_batch(cached_events.into_iter());
    commands.remove_resource::<CachedEvents>();
}

fn board_position_from_intersection(
    intersections: Query<&Intersection<RaycastSet>>,
) -> Option<[i32; 2]> {
    let intersection = intersections.get_single().ok()?;
    intersection
        .position()
        .map(|Vec3 { x, y, .. }| [*x as i32, *y as i32])
}
