use bevy::prelude::*;
use bevy_egui::{EguiContext, EguiPlugin};
use bevy_quinnet::client::ConnectionErrorEvent;
use egui_extras::{Column, TableBuilder};
use iyes_loopless::prelude::*;
use std::str::FromStr;

use battleship_plus_common::messages;

use crate::game_state::{GameState, PlayerId};
use crate::networking;

pub struct ServerSelectionPlugin;

impl Plugin for ServerSelectionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugin(EguiPlugin);
        }
        app.init_resource::<UiState>()
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

fn draw_selection_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    servers: Query<(Entity, &networking::ServerInformation)>,
    mut ui_state: ResMut<UiState>,
    keyboard: Res<Input<KeyCode>>,
    state: Res<CurrentState<GameState>>,
) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(600.0);
            ui.heading("Server Selection");
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::at_least(Column::auto(), 250.0))
                .column(Column::at_least(Column::auto(), 300.0))
                .column(Column::at_least(Column::auto(), 100.0))
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Name");
                    });
                    header.col(|ui| {
                        ui.strong("Address");
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
                                if ui.button("Join").clicked() {
                                    commands.insert_resource(networking::CurrentServer(server));
                                    commands.insert_resource(NextState(GameState::Joining));
                                }
                            });
                        });
                    }
                });
            ui.separator();
            ui.label("Join other server:");
            let socket_address = &mut ui_state.server_address;
            let address_text_edit = ui.text_edit_singleline(socket_address);
            if state.is_changed() {
                address_text_edit.request_focus();
            }
            let join_button = ui.button("Join");
            let popup_id = ui.make_persistent_id("join_button_popup");
            let confirmed_with_keyboard =
                address_text_edit.lost_focus() && keyboard.pressed(KeyCode::Return);
            if join_button.clicked() || confirmed_with_keyboard {
                match networking::ServerInformation::from_str(&socket_address) {
                    Ok(server_information) => {
                        let entity = match servers
                            .iter()
                            .find(|(_, other_server_information)| {
                                server_information.address == other_server_information.address
                            })
                            .map(|(entity, _)| entity)
                        {
                            Some(entity) => entity,
                            // Only add server if it does not exist already.
                            None => commands.spawn(server_information).id(),
                        };
                        commands.insert_resource(networking::CurrentServer(entity));
                        commands.insert_resource(NextState(GameState::Joining));
                    }
                    Err(error) => {
                        ui_state.error_message = error;
                        ui.memory().toggle_popup(popup_id);
                    }
                }
            }

            let above = egui::AboveOrBelow::Above;
            egui::popup::popup_above_or_below_widget(ui, popup_id, &join_button, above, |ui| {
                ui.set_min_width(200.0);
                ui.label(ui_state.error_message.clone());
            });
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
    mut connection_error_events: EventReader<bevy_quinnet::client::ConnectionErrorEvent>,
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
        match code {
            code if code / 100 == 2 => match data {
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
            },
            441 => {
                if message.is_empty() {
                    warn!(
                        "User name was taken, this should not happen \
                           and might indicate an error in the server"
                    );
                } else {
                    warn!(
                        "User name was taken, this should not happen \
                           and might indicate an error in the server. \
                           The following message was included: {message}"
                    );
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            442 => info!("The lobby is full, disconnecting"),
            code if code / 10 == 44 => {
                if message.is_empty() {
                    warn!(
                        "Unsuccessful, but received unknown status code {code} with data {data:?}"
                    );
                } else {
                    warn!(
                        "Unsuccessful, but received unknown status code {code} \
                           with message \"{message}\" and data {data:?}"
                    );
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            code if code / 100 == 5 => {
                if message.is_empty() {
                    error!("Server error {code}, disconnecting");
                } else {
                    error!("Server error {code} with message \"{message}\", disconnecting");
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            code => {
                if message.is_empty() {
                    error!("Received unknown or illegal error code {code}, disconnecting");
                } else {
                    error!(
                        "Received unknown or illegal error code {code} with message \"{message}\", disconnecting"
                    );
                }
                commands.insert_resource(NextState(GameState::Unconnected));
            }
        }
    }
}
