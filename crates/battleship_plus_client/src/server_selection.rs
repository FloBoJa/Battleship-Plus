use bevy::prelude::*;
use bevy_egui::{EguiContext, EguiPlugin};
use egui_extras::{Column, TableBuilder};
use iyes_loopless::prelude::*;

use battleship_plus_common::messages;

use crate::game_state::{GameState, PlayerId};
use crate::networking;

pub struct ServerSelectionPlugin;

impl Plugin for ServerSelectionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugin(EguiPlugin);
        }
        app.add_system(draw_selection_screen.run_in_state(GameState::Unconnected))
            .add_system(process_join_response.run_in_state(GameState::Joining));
    }
}

fn draw_selection_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    servers: Query<(Entity, &networking::ServerInformation)>,
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
