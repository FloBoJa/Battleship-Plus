use battleship_plus_common::messages;
use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};
use bevy_inspector_egui::WorldInspectorPlugin;
use iyes_loopless::prelude::*;

mod game_state;
mod lobby;
mod networking;

use game_state::GameState;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            window: WindowDescriptor {
                title: "Battleship plus".to_string(),
                width: 1280.,
                height: 720.,
                mode: WindowMode::Windowed,
                resizable: false,
                decorations: true,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            },
            ..default()
        }))
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(WorldInspectorPlugin::default())
        .add_loopless_state(GameState::Unconnected)
        .add_plugin(networking::NetworkingPlugin)
        .add_startup_system(fps_counter)
        .add_startup_system(camera_setup)
        .insert_resource(lobby::UserName("Userus Namus XXVII.".to_string()))
        .add_system(text_update_system)
        .add_system(join_any_server.run_in_state(GameState::Unconnected))
        .add_system(process_join_response.run_in_state(GameState::Joining))
        .add_system(debug_state_change)
        .run();
}

#[derive(Component)]
struct FpsText;

fn camera_setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn fps_counter(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        TextBundle::from_sections([
            TextSection::new(
                "FPS: ",
                TextStyle {
                    font: asset_server.load("fonts/LEMONMILK-Regular.otf"),
                    font_size: 20.0,
                    color: Color::WHITE,
                },
            ),
            TextSection::from_style(TextStyle {
                font: asset_server.load("fonts/LEMONMILK-Regular.otf"),
                font_size: 20.0,
                color: Color::GOLD,
            }),
        ]),
        FpsText,
    ));
}

fn text_update_system(diagnostics: Res<Diagnostics>, mut query: Query<&mut Text, With<FpsText>>) {
    for mut text in &mut query {
        if let Some(fps) = diagnostics.get(FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                // Update the value of the second section
                text.sections[1].value = format!("{value:.2}");
            }
        }
    }
}

fn join_any_server(
    mut commands: Commands,
    servers: Query<(Entity, &networking::ServerInformation)>,
) {
    if let Some((entity, _)) = servers.iter().next() {
        commands.insert_resource(networking::CurrentServer(entity));
        commands.insert_resource(NextState(GameState::Joining));
    }
}

#[derive(Resource)]
struct PlayerId(u32);

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

fn debug_state_change(state: Res<CurrentState<GameState>>) {
    if state.is_changed() {
        debug!("State changed to {state:?}");
    }
}
