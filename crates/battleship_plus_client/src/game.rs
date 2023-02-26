use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_egui::EguiContext;
use bevy_mod_raycast::{Intersection, RaycastMesh};
use iyes_loopless::prelude::*;

use battleship_plus_common::{
    game::{
        ship::{Cooldown, GetShipID, Orientation, Ship},
        ship_manager::ShipManager,
    },
    messages::{self, ship_action_request::ActionProperties, EventMessage, StatusCode},
    types::{self, CommonBalancing, Teams},
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
        .add_system(select_target.run_in_state(GameState::Game))
        .add_system(draw_menu.run_in_state(GameState::Game))
        .add_system(send_actions.run_in_state(GameState::Game));
    }
}

#[derive(Resource, Deref)]
pub struct InitialGameState(pub types::ServerState);

#[derive(Resource, Deref, DerefMut)]
struct SelectedShip(u32);

#[derive(Resource, Deref, DerefMut)]
struct SelectedTargets(Vec<types::Coordinate>);

type TargetCount = usize;
type PositionInQueue = Option<u32>;

enum State {
    WaitingForTurn(PositionInQueue),
    ChoosingAction,
    ChoosingTargets(TargetCount, ActionProperties),
    ChoseAction(Option<ActionProperties>),
    WaitingForResponse,
}

#[derive(Resource, Deref, DerefMut)]
struct TurnState(State);

#[derive(Component)]
struct DespawnOnExit;

#[derive(Resource, Deref, DerefMut)]
struct ActionPoints(u32);

fn create_resources(
    mut commands: Commands,
    initial_game_state: Res<InitialGameState>,
    lobby: Res<lobby::LobbyState>,
    config: Res<Config>,
    player_team: Res<PlayerTeam>,
) {
    commands.insert_resource(TurnState(State::WaitingForTurn(None)));
    commands.insert_resource(ActionPoints(0));
    commands.insert_resource(SelectedTargets(Vec::with_capacity(3)));

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
    (selected, mut selected_targets): (Option<ResMut<SelectedShip>>, ResMut<SelectedTargets>),
    ships: ResMut<Ships>,
    player_id: Res<PlayerId>,
    (action_points, mut turn_state): (Res<ActionPoints>, ResMut<TurnState>),
    config: Res<Config>,
) {
    let selected = match selected {
        Some(selected) => ships.get_by_id(&(**player_id, **selected)),
        None => None,
    };
    let may_execute_action = matches!(**turn_state, State::ChoosingAction);

    egui::TopBottomPanel::bottom(egui::Id::new("placement_menu")).show(
        egui_context.ctx_mut(),
        |ui| {
            ui.horizontal(|ui| {
                ui.set_height(50.0);

                ui.horizontal(|ui| {
                    ui.set_width(120.0);
                    match **turn_state {
                        State::WaitingForTurn(Some(remaining_turns)) => {
                            ui.label(format!("{remaining_turns} turns before you"))
                        }
                        State::WaitingForTurn(None) => ui.label("Waiting for turn..."),
                        _ => ui.label(format!(
                            "Action Points: {}/{}",
                            **action_points, config.action_point_gain
                        )),
                    }
                });

                ui.separator();

                let may_shoot = may_execute_action && may_shoot(&selected, &action_points, &config);
                let cooldown = get_shoot_cooldown(&selected);
                let button_text = match cooldown {
                    Some(cooldown) => format!("Shoot ({cooldown})"),
                    None => "Shoot".to_string(),
                };
                let shoot_button = ui.add_enabled(
                    may_shoot,
                    egui::Button::new(button_text).min_size(egui::Vec2::new(100.0, 0.0)),
                );

                let (required_action_points, cooldown, damage, range) = match selected {
                    Some(ship) => {
                        let balancing = get_common_balancing(ship, &config);
                        let types::Costs {
                            action_points,
                            cooldown,
                        } = balancing.shoot_costs.clone().unwrap_or_default();
                        let damage = balancing.shoot_damage;
                        let range = balancing.shoot_range;
                        (action_points, cooldown, damage, range)
                    }
                    None => default(),
                };

                let shoot_button = shoot_button.on_hover_text(format!(
                    "AP: {required_action_points}\nCD: {cooldown}\nDMG: {damage}\nRANGE: {range}"
                ));

                if shoot_button.clicked() {
                    trace!("Initiating shot, waiting for target selection...");
                    **turn_state =
                        State::ChoosingTargets(1, types::ShootProperties::default().into());

                    // let selected =
                    //     selected.expect("Button can only be clicked when a ship is selected");
                    // let position = selected.position();
                    // let target = Some(types::Coordinate {
                    //     x: position.0 as u32,
                    //     y: position.1 as u32,
                    // });
                    // let shoot_properties = types::ShootProperties { target };
                    // **turn_state = State::ChoseAction(Some(shoot_properties.into()));
                }

                let may_use_special =
                    may_execute_action && may_use_special(&selected, &action_points, &config);
                let cooldown = get_special_cooldown(&selected);
                // TODO: Display the actual ability name here.
                let button_text = match cooldown {
                    Some(cooldown) => format!("Special ({cooldown})"),
                    None => "Special".to_string(),
                };
                let special_button = ui.add_enabled(
                    may_use_special,
                    egui::Button::new(button_text).min_size(egui::Vec2::new(100.0, 0.0)),
                );

                let (required_action_points, cooldown, special_description) = match selected {
                    Some(ship) => {
                        let balancing = get_common_balancing(ship, &config);
                        let types::Costs {
                            action_points,
                            cooldown,
                        } = balancing.ability_costs.clone().unwrap_or_default();
                        let special_description =
                            format!("\n{}", get_special_description(ship, &config));
                        (action_points, cooldown, special_description)
                    }
                    None => default(),
                };
                let special_button = special_button.on_hover_text(format!(
                    "AP: {required_action_points}\nCD: {cooldown}{special_description}"
                ));

                if special_button.clicked() {
                    trace!("Initiating special ability...");
                    let ship =
                        selected.expect("Button can only be clicked when a ship is selected");
                    let action_properties = match ship.ship_type() {
                        types::ShipType::Carrier => {
                            trace!("Waiting for target selection...");
                            selected_targets.clear();
                            **turn_state = State::ChoosingTargets(
                                1,
                                types::ScoutPlaneProperties::default().into(),
                            );
                            None
                        }
                        // TODO: Orientation selection
                        types::ShipType::Submarine => Some(
                            types::TorpedoProperties {
                                direction: types::Direction::from(ship.orientation()).into(),
                            }
                            .into(),
                        ),
                        types::ShipType::Cruiser => Some(types::EngineBoostProperties {}.into()),
                        types::ShipType::Battleship => {
                            trace!("Waiting for target selection...");
                            selected_targets.clear();
                            **turn_state = State::ChoosingTargets(
                                1,
                                types::PredatorMissileProperties::default().into(),
                            );
                            None
                        }
                        types::ShipType::Destroyer => {
                            trace!("Waiting for three target selections...");
                            selected_targets.clear();
                            **turn_state = State::ChoosingTargets(
                                3,
                                types::MultiMissileProperties::default().into(),
                            );
                            None
                        }
                    };
                    if let Some(action_properties) = action_properties {
                        **turn_state = State::ChoseAction(Some(action_properties));
                    }
                }

                ui.separator();

                let cooldown = get_move_cooldown(&selected);
                let label_text = match cooldown {
                    Some(cooldown) => format!("Move ({cooldown}):"),
                    None => "Move:".to_string(),
                };
                ui.horizontal(|ui| {
                    ui.set_min_size(egui::Vec2::new(60.0, 0.0));
                    ui.label(label_text);
                });

                let may_move = may_execute_action && may_move(&selected, &action_points, &config);
                let forward_button = ui.add_enabled(may_move, egui::Button::new("\u{2b06}"));
                let backward_button = ui.add_enabled(may_move, egui::Button::new("\u{2b07}"));

                let (required_action_points, cooldown) = match selected {
                    Some(ship) => {
                        let balancing = get_common_balancing(ship, &config);
                        let types::Costs {
                            action_points,
                            cooldown,
                        } = balancing.movement_costs.clone().unwrap_or_default();
                        (action_points, cooldown)
                    }
                    None => default(),
                };

                let text = format!("AP: {required_action_points}\nCD: {cooldown}");
                let forward_button = forward_button.on_hover_text(text.clone());
                let backward_button = backward_button.on_hover_text(text);

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

                let cooldown = get_move_cooldown(&selected);
                let label_text = match cooldown {
                    Some(cooldown) => format!("Rotate ({cooldown}):"),
                    None => "Rotate:".to_string(),
                };
                ui.horizontal(|ui| {
                    ui.set_min_size(egui::Vec2::new(60.0, 0.0));
                    ui.label(label_text);
                });

                let may_rotate =
                    may_execute_action && may_rotate(&selected, &action_points, &config);
                let clockwise_button = ui.add_enabled(may_rotate, egui::Button::new("\u{21A9}"));
                let counter_clockwise_button =
                    ui.add_enabled(may_rotate, egui::Button::new("\u{21AA}"));

                let (required_action_points, cooldown) = match selected {
                    Some(ship) => {
                        let balancing = get_common_balancing(ship, &config);
                        let types::Costs {
                            action_points,
                            cooldown,
                        } = balancing.rotation_costs.clone().unwrap_or_default();
                        (action_points, cooldown)
                    }
                    None => default(),
                };

                let text = format!("AP: {required_action_points}\nCD: {cooldown}");
                let clockwise_button = clockwise_button.on_hover_text(text.clone());
                let counter_clockwise_button = counter_clockwise_button.on_hover_text(text);

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

                    let end_turn_button =
                        ui.add_enabled(may_execute_action, egui::Button::new("End Turn"));
                    if end_turn_button.clicked() {
                        trace!("Ending turn");
                        **turn_state = State::ChoseAction(None);
                    }
                });
            });
        },
    );
}

fn get_shoot_cooldown(selected: &Option<&Ship>) -> Option<u32> {
    (*selected)?.cool_downs().iter().find_map(|x| {
        if let &Cooldown::Cannon { remaining_rounds } = x {
            Some(remaining_rounds)
        } else {
            None
        }
    })
}

fn may_shoot(
    selected: &Option<&Ship>,
    action_points: &Res<ActionPoints>,
    config: &Res<Config>,
) -> bool {
    let ship = match selected {
        Some(selected) => *selected,
        None => return false,
    };
    let cooldown = get_shoot_cooldown(selected);
    let available_action_points = ***action_points;
    let required_action_points =
        if let Some(costs) = &get_common_balancing(ship, config).shoot_costs {
            costs.action_points
        } else {
            0
        };
    let enough_action_points = available_action_points > required_action_points;

    cooldown.is_none() && enough_action_points
}

fn get_move_cooldown(selected: &Option<&Ship>) -> Option<u32> {
    (*selected)?.cool_downs().iter().find_map(|x| {
        if let &Cooldown::Movement { remaining_rounds } = x {
            Some(remaining_rounds)
        } else {
            None
        }
    })
}

fn may_move(
    selected: &Option<&Ship>,
    action_points: &Res<ActionPoints>,
    config: &Res<Config>,
) -> bool {
    let ship = match selected {
        Some(selected) => *selected,
        None => return false,
    };
    let cooldown = get_move_cooldown(selected);
    let available_action_points = ***action_points;
    let required_action_points =
        if let Some(costs) = &get_common_balancing(ship, config).movement_costs {
            costs.action_points
        } else {
            0
        };
    let enough_action_points = available_action_points > required_action_points;

    cooldown.is_none() && enough_action_points
}

fn get_rotate_cooldown(selected: &Option<&Ship>) -> Option<u32> {
    (*selected)?.cool_downs().iter().find_map(|x| {
        if let &Cooldown::Rotate { remaining_rounds } = x {
            Some(remaining_rounds)
        } else {
            None
        }
    })
}

fn may_rotate(
    selected: &Option<&Ship>,
    action_points: &Res<ActionPoints>,
    config: &Res<Config>,
) -> bool {
    let ship = match selected {
        Some(selected) => *selected,
        None => return false,
    };
    let cooldown = get_rotate_cooldown(selected);
    let available_action_points = ***action_points;
    let required_action_points =
        if let Some(costs) = &get_common_balancing(ship, config).rotation_costs {
            costs.action_points
        } else {
            0
        };
    let enough_action_points = available_action_points > required_action_points;

    cooldown.is_none() && enough_action_points
}

fn get_special_cooldown(selected: &Option<&Ship>) -> Option<u32> {
    (*selected)?.cool_downs().iter().find_map(|x| {
        if let &Cooldown::Ability { remaining_rounds } = x {
            Some(remaining_rounds)
        } else {
            None
        }
    })
}

fn may_use_special(
    selected: &Option<&Ship>,
    action_points: &Res<ActionPoints>,
    config: &Res<Config>,
) -> bool {
    let ship = match selected {
        Some(selected) => *selected,
        None => return false,
    };
    let cooldown = get_special_cooldown(selected);
    let available_action_points = ***action_points;
    let required_action_points =
        if let Some(costs) = &get_common_balancing(ship, config).ability_costs {
            costs.action_points
        } else {
            0
        };
    let enough_action_points = available_action_points >= required_action_points;

    cooldown.is_none() && enough_action_points
}

fn get_special_description(ship: &Ship, config: &Res<Config>) -> String {
    match ship.ship_type() {
        types::ShipType::Carrier => {
            let balancing = config
                .carrier_balancing
                .as_ref()
                .expect("Carrier must have a balancing");
            format!(
                "RADIUS: {}\nRANGE: {}",
                balancing.scout_plane_radius, balancing.scout_plane_range
            )
        }
        types::ShipType::Battleship => {
            let balancing = config
                .battleship_balancing
                .as_ref()
                .expect("Battleship must have a balancing");
            format!(
                "DMG: {}\nRADIUS: {}\nRANGE: {}",
                balancing.predator_missile_damage,
                balancing.predator_missile_radius,
                balancing.predator_missile_range
            )
        }
        types::ShipType::Cruiser => {
            let balancing = config
                .cruiser_balancing
                .as_ref()
                .expect("Cruiser must have a balancing");
            format!("DIST: {}", balancing.engine_boost_distance)
        }
        types::ShipType::Submarine => {
            let balancing = config
                .submarine_balancing
                .as_ref()
                .expect("Submarine must have a balancing");
            format!(
                "DMG: {}\nRANGE: {}",
                balancing.torpedo_damage, balancing.torpedo_range
            )
        }
        types::ShipType::Destroyer => {
            let balancing = config
                .destroyer_balancing
                .as_ref()
                .expect("Destroyer must have a balancing");
            format!(
                "DMG: 3x{}\nRADIUS: {}",
                balancing.multi_missile_damage, balancing.multi_missile_radius
            )
        }
    }
}

fn get_common_balancing<'a>(ship: &Ship, config: &'a Res<Config>) -> &'a CommonBalancing {
    match ship.ship_type() {
        types::ShipType::Carrier => config
            .carrier_balancing
            .as_ref()
            .expect("Carrier must have a balancing")
            .common_balancing
            .as_ref(),
        types::ShipType::Battleship => config
            .battleship_balancing
            .as_ref()
            .expect("Battleship must have a balancing")
            .common_balancing
            .as_ref(),
        types::ShipType::Cruiser => config
            .cruiser_balancing
            .as_ref()
            .expect("Cruiser must have a balancing")
            .common_balancing
            .as_ref(),
        types::ShipType::Submarine => config
            .submarine_balancing
            .as_ref()
            .expect("Submarine must have a balancing")
            .common_balancing
            .as_ref(),
        types::ShipType::Destroyer => config
            .destroyer_balancing
            .as_ref()
            .expect("Destroyer must have a balancing")
            .common_balancing
            .as_ref(),
    }
    .expect("Ships must have a CommonBalancing")
}

fn process_game_events(
    mut commands: Commands,
    mut events: EventReader<messages::EventMessage>,
    (player_id, player_team): (Res<PlayerId>, Res<PlayerTeam>),
    mut turn_state: ResMut<TurnState>,
    mut action_points: ResMut<ActionPoints>,
    config: Res<Config>,
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
                    **action_points = config.action_point_gain;
                } else {
                    match **turn_state {
                        State::WaitingForTurn(_)
                        | State::ChoosingAction
                        | State::ChoosingTargets(_, _) => {}
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
                    };
                    **action_points = 0;
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
            Some(StatusCode::InsufficientResources) => {
                if message.is_empty() {
                    warn!("Server signaled insufficient resources, action was not executed");
                } else {
                    warn!("Server signaled insufficient resources, action was not executed: {message}");
                }
                **turn_state = State::ChoosingAction;
            }
            Some(StatusCode::InvalidMove) => {
                if message.is_empty() {
                    warn!("Server understood request, but the action was invalid. The action was not executed");
                } else {
                    warn!("Server understood request, but the action was invalid. The action was not executed: {message}");
                }
                **turn_state = State::ChoosingAction;
            }
            Some(StatusCode::BadRequest) => {
                if message.is_empty() {
                    warn!("Server did not understand or accept request");
                } else {
                    warn!("Server did not understand or accept request: {message}");
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
    turn_state: Res<TurnState>,
    ships: Res<Ships>,
    player_id: Res<PlayerId>,
    mouse_input: Res<Input<MouseButton>>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    // Only allow to change the selection while waiting for the player's turn or while choosing an action.
    // This excludes changing the selection during the target selection of the selected ship's
    // action, among other things.
    if !matches!(
        **turn_state,
        State::WaitingForTurn(_) | State::ChoosingAction
    ) {
        return;
    }
    let position = match board_position_from_intersection(intersections) {
        Some(position) => position,
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

fn select_target(
    intersections: Query<&Intersection<RaycastSet>>,
    mut turn_state: ResMut<TurnState>,
    mut selected_targets: ResMut<SelectedTargets>,
    mouse_input: Res<Input<MouseButton>>,
) {
    // TODO: Allow aborting selection mode.

    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    let (&target_count, action_properties) = match &**turn_state {
        State::ChoosingTargets(target_count, action_properties) => {
            (target_count, action_properties)
        }
        _ => return,
    };
    if selected_targets.len() >= target_count {
        return;
    }

    let position = match board_position_from_intersection(intersections) {
        Some(position) => position,
        None => return,
    };

    trace!("Selected target: ({}, {})", position.x, position.y);
    selected_targets.push(position);

    if selected_targets.len() >= target_count {
        // This position was the last one.
        let mut action_properties = action_properties.clone();
        match &mut action_properties {
            ActionProperties::ShootProperties(properties) => {
                properties.target = selected_targets.pop();
            }
            ActionProperties::ScoutPlaneProperties(properties) => {
                properties.center = selected_targets.pop();
            }
            ActionProperties::PredatorMissileProperties(properties) => {
                properties.center = selected_targets.pop();
            }
            ActionProperties::MultiMissileProperties(properties) => {
                properties.position_a = selected_targets.pop();
                properties.position_b = selected_targets.pop();
                properties.position_c = selected_targets.pop();
            }
            _ => unreachable!("Only actions with targets are allowed here"),
        }
        **turn_state = State::ChoseAction(Some(action_properties));
    }
}

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
) -> Option<types::Coordinate> {
    let intersection = intersections.get_single().ok()?;
    intersection
        .position()
        .map(|&Vec3 { x, y, .. }| types::Coordinate {
            x: x as u32,
            y: y as u32,
        })
}
