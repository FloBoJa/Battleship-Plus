use bevy::prelude::*;
use bevy_egui::EguiContext;
use bevy_quinnet::client::Client;
use egui::Color32;
use egui_extras::{Column, TableBuilder};
use iyes_loopless::prelude::*;

use battleship_plus_common::{messages, types};

use crate::game_state::GameState;
use crate::placement_phase;
use crate::server_selection;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LobbyState>()
            .init_resource::<Readiness>()
            .add_system(draw_lobby_screen.run_in_state(GameState::Lobby))
            .add_system(process_lobby_events.run_in_state(GameState::Lobby))
            // Catch events that happen immediately after joining.
            .add_enter_system(GameState::Lobby, repeat_cached_events);
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

    fn to_table(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.push_id(0, |ui| {
                Self::team_to_table(ui, &self.team_state_a);
            });
            ui.separator();
            ui.push_id(1, |ui| {
                Self::team_to_table(ui, &self.team_state_b);
            });
        });
    }

    // Helper function for to_table
    fn team_to_table(ui: &mut egui::Ui, team: &Vec<types::PlayerLobbyState>) {
        ui.vertical(|ui| {
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

#[derive(Resource, Deref)]
#[derive(Default)]
pub struct Readiness(bool);



fn draw_lobby_screen(
    mut commands: Commands,
    mut egui_context: ResMut<EguiContext>,
    lobby_state: Res<LobbyState>,
    readiness: Res<Readiness>,
    client: ResMut<Client>,
) {
    egui::CentralPanel::default().show(egui_context.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(600.0);
            ui.heading("Lobby");
            if ui.button("Toggle readiness").clicked() {
                let ready_state = !readiness.0;
                let connection = client
                    .get_connection()
                    .expect("There must be a connection in the Lobby state");
                if let Err(error) =
                    connection.send_message(messages::SetReadyStateRequest { ready_state }.into())
                {
                    error!("Could not send SetReadyStateRequest: {error}, disonnecting");
                    commands.insert_resource(NextState(GameState::Unconnected));
                } else {
                    // TODO: consider waiting for SetReadyStateResponse
                    commands.insert_resource(Readiness(ready_state));
                }
            }
            lobby_state.to_table(ui);
        });
    });
}

fn process_lobby_events(
    mut commands: Commands,
    mut events: EventReader<messages::EventMessage>,
    lobby_state: Res<LobbyState>,
) {
    for event in events.iter() {
        match event {
            messages::EventMessage::LobbyChangeEvent(lobby_state) => {
                commands.insert_resource(LobbyState(lobby_state.to_owned()));
            }
            messages::EventMessage::PlacementPhase(message) => {
                if let Some(corner) = &message.corner {
                    commands.insert_resource(placement_phase::Quadrant::new(
                        corner.to_owned(),
                        lobby_state.total_player_count(),
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
