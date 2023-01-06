use bevy::prelude::*;
use bevy_egui::{EguiContext, EguiPlugin};
use bevy_quinnet::client::ConnectionErrorEvent;
use egui_extras::{Column, TableBuilder};
use iyes_loopless::prelude::*;
use std::str::FromStr;

use battleship_plus_common::messages::{self, StatusCode};

use crate::game_state::{GameState, PlayerId};
use crate::networking;

pub struct ServerSelectionPlugin;

impl Plugin for ServerSelectionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugin(EguiPlugin);
        }
        app.init_resource::<UiState>()
            .add_startup_system(setup_egui_font)
            .add_system(draw_selection_screen.run_in_state(GameState::Unconnected))
            .add_system(draw_joining_screen.run_in_state(GameState::Joining))
            .add_system(process_join_response.run_in_state(GameState::Joining))
            .add_system(process_connection_errors.run_in_state(GameState::Joining))
            .add_system(draw_joining_failed_screen.run_in_state(GameState::JoiningFailed));
    }
}

#[derive(Resource, Default)]
struct UiState {
    server_address: String,
    error_message: String,
    connection_errored: bool,
}

fn setup_egui_font(mut egui_context: ResMut<EguiContext>) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "emoji".to_string(),
        egui::FontData::from_static(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/NotoEmoji-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "symbols".to_string(),
        egui::FontData::from_static(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/u2400.ttf"
        ))),
    );
    let font_definitions = fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap();
    font_definitions.push("emoji".to_string());
    font_definitions.push("symbols".to_string());
    egui_context.ctx_mut().set_fonts(fonts);
}

fn draw_selection_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    servers: Query<(Entity, &networking::ServerInformation)>,
    mut ui_state: ResMut<UiState>,
    keyboard: Res<Input<KeyCode>>,
    state: Res<CurrentState<GameState>>,
    mut client: ResMut<bevy_quinnet::client::Client>,
) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(750.0);
            ui.heading("Server Selection");
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::at_least(Column::auto(), 250.0))
                .column(Column::at_least(Column::auto(), 300.0))
                .column(Column::at_least(Column::auto(), 100.0))
                .column(Column::at_least(Column::auto(), 100.0))
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Name");
                    });
                    header.col(|ui| {
                        ui.strong("Address");
                    });
                    header.col(|ui| {
                        ui.strong("Status");
                    });
                    header.col(|_| {});
                })
                .body(|mut body| {
                    for (server, server_information) in servers.iter() {
                        body.row(20.0, |mut row| {
                            row.col(|ui| {
                                ui.label(&server_information.name);
                            });
                            row.col(|ui| {
                                ui.label(format!("{}", server_information.address));
                            });
                            row.col(|ui| {
                                use egui::Color32;
                                use networking::{Empirical::*, SecurityLevel::*};
                                match server_information.security {
                                    Confirmed(AuthoritySigned) => {
                                        ui.colored_label(Color32::GREEN, "\u{2713} (CA-signed)")
                                    }
                                    Confirmed(SelfSigned) => {
                                        ui.colored_label(Color32::YELLOW, "\u{2713} (self-signed)")
                                    }
                                    Confirmed(NoVerification) => {
                                        ui.colored_label(Color32::RED, "\u{2713} (unsigned)")
                                    }
                                    Confirmed(ConnectionFailed) => {
                                        ui.colored_label(Color32::LIGHT_GRAY, "\u{2717} (failed)")
                                    }
                                    Unconfirmed(AuthoritySigned) => {
                                        ui.colored_label(Color32::GREEN, "...\u{1F4DE} (CA-signed)")
                                    }
                                    Unconfirmed(SelfSigned) => ui.colored_label(
                                        Color32::YELLOW,
                                        "...\u{1F4DE} (self-signed)",
                                    ),
                                    Unconfirmed(NoVerification) => {
                                        ui.colored_label(Color32::RED, "...\u{1F4DE} (unsigned)")
                                    }
                                    Unconfirmed(ConnectionFailed) => {
                                        ui.colored_label(Color32::LIGHT_GRAY, "\u{2717} (failed)")
                                    }
                                };
                            });
                            row.col(|ui| {
                                let mut enabled = true;
                                if let networking::Empirical::Unconfirmed(_) =
                                    server_information.security
                                {
                                    enabled = false;
                                } else if let networking::Empirical::Confirmed(
                                    networking::SecurityLevel::ConnectionFailed,
                                ) = server_information.security
                                {
                                    enabled = false;
                                }
                                let join_button =
                                    ui.add_enabled(enabled, egui::Button::new("Join"));
                                if join_button.clicked() {
                                    commands.insert_resource(networking::CurrentServer(server));
                                    commands.insert_resource(NextState(GameState::Joining));
                                }
                            });
                        });
                    }
                });
            ui.separator();
            ui.label("Add other server:");
            let socket_address = &mut ui_state.server_address;
            let address_text_edit = ui.text_edit_singleline(socket_address);
            if state.is_changed() {
                address_text_edit.request_focus();
            }
            let add_server_button = ui.button("Add");
            let popup_id = ui.make_persistent_id("add_button_popup");
            let confirmed_with_keyboard =
                address_text_edit.lost_focus() && keyboard.pressed(KeyCode::Return);
            if add_server_button.clicked() || confirmed_with_keyboard {
                match networking::ServerInformation::from_str(socket_address) {
                    Ok(server_information) => {
                        if !servers.iter().any(|(_, other_server_information)| {
                            server_information.address == other_server_information.address
                        }) {
                            // Only add server if it does not exist already.
                            let entity = commands.spawn(server_information.clone()).id();
                            server_information.connect(&mut commands, entity, &mut client);
                        }
                    }
                    Err(error) => {
                        ui_state.error_message = error;
                        ui.memory().toggle_popup(popup_id);
                    }
                }
            }

            let above = egui::AboveOrBelow::Above;
            egui::popup::popup_above_or_below_widget(
                ui,
                popup_id,
                &add_server_button,
                above,
                |ui| {
                    ui.set_min_width(200.0);
                    ui.label(ui_state.error_message.clone());
                },
            );
        });
    });
}

fn draw_joining_screen(mut egui_context: ResMut<EguiContext>) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.label("Joining...");
        });
    });
}

fn process_connection_errors(
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
    current_server: Option<Res<networking::CurrentServer>>,
    connections: Query<(Entity, &networking::Connection)>,
    mut connection_error_events: EventReader<ConnectionErrorEvent>,
) {
    if !ui_state.connection_errored {
        if let Some(current_server) = current_server {
            if let Ok(networking::Connection(current_connection_id)) =
                connections.get_component::<networking::Connection>(current_server.0)
            {
                if let Some(ConnectionErrorEvent(_, error)) =
                    connection_error_events
                        .iter()
                        .find(|ConnectionErrorEvent(connection_id, _)| {
                            connection_id == current_connection_id
                        })
                {
                    error!("{error}");
                    ui_state.error_message = error.clone();
                    ui_state.connection_errored = true;
                    commands.insert_resource(NextState(GameState::JoiningFailed));
                }
            }
        }
    }
}

fn draw_joining_failed_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.colored_label(egui::Color32::RED, ui_state.error_message.clone());
            let back_button = ui.button("Back to server selection");
            back_button.request_focus();
            if back_button.clicked() {
                ui_state.connection_errored = false;
                commands.insert_resource(NextState(GameState::Unconnected));
            }
        });
    });
}

fn process_join_response(
    mut events: EventReader<networking::ResponseReceivedEvent>,
    mut commands: Commands,
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
            Some(StatusCode::Ok) => process_join_response_data(&mut commands, message, data),
            Some(StatusCode::OkWithWarning) => {
                if message.is_empty() {
                    warn!("Received OK response to join request with warning but without message");
                } else {
                    warn!("Received OK response to join request with warning: {message}");
                }
                process_join_response_data(&mut commands, message, data)
            }
            Some(StatusCode::UsernameIsTaken) => {
                error!("User name is taken, disconnecting");
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            Some(StatusCode::LobbyIsFull) => {
                info!("The lobby is full, disconnecting");
                continue;
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

fn process_join_response_data(
    commands: &mut Commands,
    message: &str,
    data: &Option<messages::status_message::Data>,
) {
    match data {
        Some(messages::status_message::Data::JoinResponse(messages::JoinResponse {
            player_id,
        })) => {
            debug!("Join successful, got player ID {player_id}");
            commands.insert_resource(NextState(GameState::Lobby));
            commands.insert_resource(PlayerId(*player_id));
        }
        Some(_other_response) => {
            // ignore
        }
        None => {
            if message.is_empty() {
                warn!("No data in response after JoinRequest but status code 2XX");
            } else {
                warn!("No data in response after JoinRequest but status code 2XX with message: {message}");
            }
            // ignore
        }
    }
}
