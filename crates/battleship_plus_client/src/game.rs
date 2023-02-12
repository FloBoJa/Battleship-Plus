use std::ptr::null;
use std::thread::sleep;
use std::time::Duration;
use std::option::Option;
use bevy::pbr::LightEntity::Directional;
use bevy::prelude::*;
use bevy::utils::tracing::event;
use futures::future::select;
use iyes_loopless::prelude::*;
use battleship_plus_common::*;
use battleship_plus_common::messages::*;
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::types::*;
use bevy_quinnet_client::Client;
use crate::game_state::GameState;
use crate::networking;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameInfo>()
            .add_system(process_game_events.run_in_state(GameState::Game))
            .add_startup_system(main.run_in_state(GameState::Game))
        ;
    }
}
#[derive(Resource, Default)]
pub struct GameInfo {
    ship_selected_id: u32,
    server_state: ServerState,

}

fn main(
    mut client: ResMut<Client>,
    mut game_info: ResMut<GameInfo>
) {
    //DEBUG

    sleep(Duration::from_secs(1));
    select_ship(&mut game_info, 1);
    request_ship_action_move(&mut client,&mut game_info, MoveProperties{ direction: 0 })

}

fn process_game_events(
    mut events: EventReader<messages::EventMessage>,
) {
    for event in events.iter() {
        match event {
            EventMessage::GameStart(_) => {
                println!("Game Stated!");
            }
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

fn process_game_responses(
    mut events: EventReader<networking::ResponseReceivedEvent>
) {
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

fn process_game_response_data(
    data: &Option<messages::status_message::Data>,
    message: &str,
) {
    match data {
        Some(messages::status_message::Data::ServerStateResponse(_)) => {
            println!("{}", message);
        }
        _ => {}
    }
}

fn request_server_state(
    mut client: ResMut<Client>,
) {
    let con = client.get_connection().expect("");

    if let Err(error) = con.send_message(
        messages::ServerStateRequest{}
            .into(),
    ) {
        error!("Could not send <ServerStateRequest>: {error}");
    } else {
        // oke
    }
}

fn select_ship(
    game_info: &mut ResMut<GameInfo>,
    ship_number: u32,
) {
    game_info.ship_selected_id = ship_number;
}


fn request_ship_action_move(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: MoveProperties,
){

    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: (*game_info).ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::MoveProperties(properties)),
    }) {
        // Move?
    }
}

fn request_ship_action_shoot(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: ShootProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::ShootProperties(properties)),
    }) {
        // Shoot?
    }
}

fn request_ship_action_rotate(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: RotateProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::RotateProperties(properties)),
    }) {
        // Rotate?
    }
}

fn request_ship_action_torpedo(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: TorpedoProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::TorpedoProperties(properties)),
    }) {
        // Torpedo?
    }
}

fn request_ship_action_scout_plane(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: ScoutPlaneProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::ScoutPlaneProperties(properties)),
    }) {
        // ScoutPlane?
    }
}

fn request_ship_action_multi_missile(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: MultiMissileProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::MultiMissileProperties(properties)),
    }) {
        // MultiMissile ?
    }
}

fn request_ship_action_predator_missile(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: PredatorMissileProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::PredatorMissileProperties(properties)),
    }) {
        // PredatorMissile ?
    }
}

fn request_ship_action_engine_boost(
    client: &mut ResMut<Client>,
    game_info: &mut ResMut<GameInfo>,
    properties: EngineBoostProperties,
) {
    if send_ship_action_request(client, messages::ShipActionRequest{
        ship_number: game_info.ship_selected_id,
        action_properties: Some(ship_action_request::ActionProperties::EngineBoostProperties(properties)),
    }) {
        // EngineBoost ?
    }
}

fn send_ship_action_request(
    client: &mut ResMut<Client>,
    message: messages::ShipActionRequest
) -> bool {
    let con = client.get_connection().expect("");

    return if let Err(error) = con.send_message(message.into()) {
        error!("Could not send message <ship_action_request>: {error}");
        false
    } else {
        true
    }
}