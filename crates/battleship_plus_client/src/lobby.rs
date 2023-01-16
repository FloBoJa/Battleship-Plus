use bevy::prelude::*;
use bevy_egui::EguiContext;
use bevy_quinnet_client::Client;
use egui::Color32;
use egui_extras::{Column, TableBuilder};
use iyes_loopless::prelude::*;

use battleship_plus_common::{
    messages::{self, StatusCode},
    types,
};

use crate::networking::{self, CurrentServer};
use crate::placement_phase;
use crate::server_selection;
use crate::{
    game_state::{GameState, PlayerId},
    networking::ServerInformation,
};

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LobbyState>()
            .init_resource::<RequestState>()
            .add_system(draw_lobby_screen.run_in_state(GameState::Lobby))
            .add_system(process_lobby_events.run_in_state(GameState::Lobby))
            .add_system(process_responses.run_in_state(GameState::Lobby))
            // Catch events that happen immediately after joining.
            .add_enter_system(GameState::Lobby, repeat_cached_events)
            .add_enter_system(GameState::Lobby, reset_state);
    }
}

#[derive(Resource, Deref)]
pub struct UserName(pub String);

#[derive(Resource, Deref, Default)]
pub struct LobbyState(messages::LobbyChangeEvent);

impl LobbyState {
    fn total_player_count(&self) -> usize {
        self.0.team_state_a.len() + self.0.team_state_b.len()
    }

    fn is_in_team_a(&self, player_id: u32) -> bool {
        self.0
            .team_state_a
            .iter()
            .any(|player_state| player_state.player_id == player_id)
    }

    fn is_in_team_b(&self, player_id: u32) -> bool {
        self.0
            .team_state_b
            .iter()
            .any(|player_state| player_state.player_id == player_id)
    }

    fn to_table(
        &self,
        ui: &mut egui::Ui,
        request_state: &mut RequestState,
        player_id: u32,
        commands: &mut Commands,
        client: &mut ResMut<Client>,
    ) {
        ui.horizontal(|ui| {
            ui.push_id(0, |ui| {
                Self::team_to_table(
                    ui,
                    &self.team_state_a,
                    request_state,
                    commands,
                    client,
                    self.is_in_team_a(player_id),
                );
            });
            ui.separator();
            ui.push_id(1, |ui| {
                Self::team_to_table(
                    ui,
                    &self.team_state_b,
                    request_state,
                    commands,
                    client,
                    self.is_in_team_b(player_id),
                );
            });
        });
    }

    // Helper function for to_table
    fn team_to_table(
        ui: &mut egui::Ui,
        team: &Vec<types::PlayerLobbyState>,
        request_state: &mut RequestState,
        commands: &mut Commands,
        client: &mut ResMut<Client>,
        is_in_team: bool,
    ) {
        ui.vertical(|ui| {
            let enabled = !request_state.team_switch_requested && !is_in_team;
            let join_team_button = ui.add_enabled(enabled, egui::Button::new("Join team"));
            if join_team_button.clicked() {
                let connection = client
                    .get_connection()
                    .expect("There must be a connection in the Lobby state");
                if let Err(error) = connection.send_message(messages::TeamSwitchRequest {}.into()) {
                    error!("Could not send SetReadyStateRequest: {error}, disonnecting");
                    commands.insert_resource(NextState(GameState::Unconnected));
                } else {
                    request_state.team_switch_requested = true;
                }
            }
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::at_least(Column::auto(), 250.0))
                .column(Column::at_least(Column::auto(), 50.0))
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Name");
                    });
                    header.col(|ui| {
                        ui.centered_and_justified(|ui| {
                            ui.strong("Ready");
                        });
                    });
                })
                .body(|mut body| {
                    for player in team {
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                ui.label(player.name.clone());
                            });
                            row.col(|ui| {
                                ui.centered_and_justified(|ui| {
                                    if player.ready {
                                        ui.colored_label(Color32::GREEN, "\u{2713}");
                                    } else {
                                        ui.colored_label(Color32::RED, "\u{2717}");
                                    };
                                });
                            });
                        });
                    }
                });
        });
    }
}

#[derive(Resource, Default)]
struct RequestState {
    readiness_change_requested: bool,
    team_switch_requested: bool,
}

fn draw_lobby_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    mut request_state: ResMut<RequestState>,
    lobby_state: Res<LobbyState>,
    player_id: Res<PlayerId>,
    mut client: ResMut<Client>,
) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(600.0);
            ui.heading("Lobby");

            ui.add_space(20.0);

            ui.horizontal(|ui| {
                if ui.button("Leave").clicked() {
                    info!("User requested leave");
                    commands.insert_resource(NextState(GameState::Unconnected));
                }

                let enabled = !request_state.readiness_change_requested;
                let mut readiness_button_text = egui::text::LayoutJob::default();
                readiness_button_text.append("Ready: ", 0.0, egui::text::TextFormat::default());
                let current_readiness = get_readiness_from_event(&lobby_state, **player_id);
                if current_readiness {
                    let format = egui::text::TextFormat {
                        color: Color32::GREEN,
                        ..default()
                    };
                    readiness_button_text.append("\u{2713}", 0.0, format);
                } else {
                    let format = egui::text::TextFormat {
                        color: Color32::RED,
                        ..default()
                    };
                    readiness_button_text.append("\u{2717}", 0.0, format);
                };
                let readiness_button =
                    ui.add_enabled(enabled, egui::Button::new(readiness_button_text));
                if readiness_button.clicked() {
                    let connection = client
                        .get_connection()
                        .expect("There must be a connection in the Lobby state");
                    if let Err(error) = connection.send_message(
                        messages::SetReadyStateRequest {
                            ready_state: !current_readiness,
                        }
                        .into(),
                    ) {
                        error!("Could not send SetReadyStateRequest: {error}, disonnecting");
                        commands.insert_resource(NextState(GameState::Unconnected));
                    } else {
                        request_state.readiness_change_requested = true;
                    }
                }
            });

            ui.add_space(20.0);

            lobby_state.to_table(
                ui,
                &mut request_state,
                **player_id,
                &mut commands,
                &mut client,
            );
        });
    });
}

fn process_lobby_events(
    mut commands: Commands,
    mut events: EventReader<messages::EventMessage>,
    lobby_state: Res<LobbyState>,
    servers: Query<Entity, &ServerInformation>,
    current_server: Res<CurrentServer>,
) {
    for event in events.iter() {
        match event {
            messages::EventMessage::LobbyChangeEvent(lobby_state) => {
                commands.insert_resource(LobbyState(lobby_state.to_owned()));
            }
            messages::EventMessage::PlacementPhase(message) => {
                if let Some(corner) = &message.corner {
                    let server_information = servers
                        .get_component::<ServerInformation>(**current_server)
                        .expect("");
                    let config = server_information
                        .config
                        .as_ref()
                        .expect("Servers without config cannot be joined");
                    commands.insert_resource(placement_phase::Quadrant::new(
                        corner.to_owned(),
                        config.board_size,
                        lobby_state.total_player_count() as u32,
                    ));
                    commands.insert_resource(NextState(GameState::PlacementPhase));
                } else {
                    error!("Received PlacementPhase message without quadrant information, disconnecting");
                    commands.insert_resource(NextState(GameState::Unconnected));
                }
            }
            _other_events => {
                // ignore
            }
        }
    }
}

fn get_readiness_from_event(lobby_state: &messages::LobbyChangeEvent, player_id: u32) -> bool {
    let mut player_state = lobby_state
        .team_state_a
        .iter()
        .find(|player_state| player_state.player_id == player_id);
    if player_state.is_none() {
        player_state = lobby_state
            .team_state_b
            .iter()
            .find(|player_state| player_state.player_id == player_id);
    }
    player_state.map_or_else(|| false, |player_state| player_state.ready)
}

fn process_responses(
    mut commands: Commands,
    mut events: EventReader<networking::ResponseReceivedEvent>,
    mut request_state: ResMut<RequestState>,
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
                process_response_data(data, message, &mut request_state);
            }
            Some(StatusCode::OkWithWarning) => {
                if message.is_empty() {
                    warn!("Received OK response with warning but without message");
                } else {
                    warn!("Received OK response with warning: {message}");
                }
                process_response_data(data, message, &mut request_state);
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
    request_state: &mut ResMut<RequestState>,
) {
    match data {
        Some(messages::status_message::Data::SetReadyStateResponse(_)) => {
            if request_state.readiness_change_requested {
                debug!("Readiness change successful");
                request_state.readiness_change_requested = false;
            } else {
                warn!("Received unexpected SetReadyStateResponse");
            }
        }
        Some(messages::status_message::Data::TeamSwitchResponse(_)) => {
            if request_state.team_switch_requested {
                debug!("Team switch successful");
                request_state.team_switch_requested = false;
            } else {
                warn!("Received unexpected TeamSwitchResponse");
            }
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

fn repeat_cached_events(
    mut commands: Commands,
    cached_events: Option<Res<server_selection::CachedEvents>>,
    mut event_writer: EventWriter<messages::EventMessage>,
) {
    let cached_events = match cached_events {
        Some(events) => events.clone(),
        None => return,
    };
    event_writer.send_batch(cached_events.into_iter());
    commands.remove_resource::<server_selection::CachedEvents>();
}

fn reset_state(mut request_state: ResMut<RequestState>) {
    request_state.readiness_change_requested = false;
    request_state.team_switch_requested = false;
}
