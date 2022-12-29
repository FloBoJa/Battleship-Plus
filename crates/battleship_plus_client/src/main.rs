use battleship_plus_common::messages::{self, ProtocolMessage};
use bevy::{
    diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};
use bevy_quinnet::client::Client;
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
        .add_plugin(networking::NetworkingPlugin)
        .init_resource::<CurrentServer>()
        .add_loopless_state(GameState::Unconnected)
        .add_startup_system(fps_counter)
        .add_startup_system(camera_setup)
        .add_system(text_update_system)
        .add_system(join_any_server.run_in_state(GameState::Unconnected))
        .add_system(process_join_response.run_in_state(GameState::Joining))
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

#[derive(Resource, Default)]
struct CurrentServer(Option<networking::ConnectionRecord>);

fn join_any_server(
    mut commands: Commands,
    servers: Query<&networking::ServerInformation>,
    connection_records: Query<&networking::ConnectionRecord>,
    client: Res<Client>,
) {
    let connection_record = match servers
        .iter()
        .filter(|server| server.config.is_some())
        .find_map(|server| {
            connection_records
                .iter()
                .find(|record| record.server_address == server.address)
        }) {
        Some(value) => value,
        None => return,
    };
    client
        .get_connection_by_id(connection_record.connection_id)
        .expect("ConnectionRecords correspond to open connections")
        .send_message(ProtocolMessage::JoinRequest(messages::JoinRequest {
            username: "Player Name".to_string(),
        }))
        .unwrap_or_else(|error| warn!("Could not send join request: {error}"));
    commands.insert_resource(NextState(GameState::Joining));
    commands.insert_resource(CurrentServer(Some(connection_record.clone())));
}

#[derive(Resource)]
struct PlayerId(u32);

fn process_join_response(
    mut events: EventReader<networking::MessageReceivedEvent>,
    current_server: Res<CurrentServer>,
    mut commands: Commands,
) {
    let current_server = current_server
        .0
        .as_ref()
        .expect("Joining state requires CurrentServer to have a value");
    for networking::MessageReceivedEvent(messages::StatusMessage { code, data }, sender) in
        events.iter()
    {
        if *sender != current_server.server_address {
            // ignore
            continue;
        }

        // TODO: Include response.message as soon as that MR is merged.
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
                    warn!("No data in response after JoinRequest but status code 2XX");
                    // ignore
                }
            },
            441 => {
                warn!("User name was taken, this should not happen and might indicate an error in the server at {sender}");
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            442 => info!("The lobby of the server at {sender} is full, disconnecting"),
            code if code / 10 == 44 => {
                warn!("Unsuccessful, but received unknown status code {code} with data {data:?}");
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            code if code / 100 == 5 => {
                error!("Server error {code} from {sender}, disconnecting");
                commands.insert_resource(NextState(GameState::Unconnected));
            }
            code => todo!("Handle illegal error code {code}"),
        }
    }
}
