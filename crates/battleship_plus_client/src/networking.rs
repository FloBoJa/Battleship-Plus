use bevy::prelude::*;
use bevy_quinnet::client::QuinnetClientPlugin;
use battleship_plus_common::messages;
use std::net::Ipv6Addr;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
  fn build(&self, app : &mut App) {
    app.add_plugin(QuinnetClientPlugin{})
        .add_startup_system(listen_for_announcements)
        .add_system(needs_server)
        .add_system(listen_for_lobby_change);
  }
}

#[derive(Component, Debug)]
pub struct ServerInformation {
  pub ip : std::net::IpAddr, pub port : u32, pub config : messages::Config,
}

fn listen_for_announcements(mut commands: Commands) {
  commands.spawn(ServerInformation{
    ip : Ipv6Addr::new (0, 0, 0, 0, 0, 0, 0, 1).into(),
    port : 30305,
    config : messages::Config{..default()},
  });
}

fn needs_server(servers : Query<&ServerInformation>) {
    for
      server in servers.iter() { println !("{:?}", server); }
}

#[derive(Component, Debug)]
pub struct LobbyState(messages::LobbyTeamState, messages::LobbyTeamState);

fn listen_for_lobby_change(mut commands
                           : Commands, lobby_state
                           : Query<(Entity, &LobbyState)>) {
  if
    !lobby_state.is_empty() {
      commands.get_entity(lobby_state.single() .0)
          .expect("!!!!")
          .despawn_recursive();
    }
  commands.spawn(LobbyState(default(), default()));
  return;
}
