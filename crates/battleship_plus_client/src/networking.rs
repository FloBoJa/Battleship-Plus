use battleship_plus_common::{
    messages::{self, MAXIMUM_MESSAGE_SIZE},
    types, ProstMessage,
};
use bevy::prelude::*;
use bevy_quinnet::client::QuinnetClientPlugin;
use std::{
    io::ErrorKind::WouldBlock,
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin {})
            .add_startup_system(set_up_advertisement_listener)
            .add_system(listen_for_advertisements_v4)
            .add_system(clean_up_servers)
            .add_system(needs_server);
    }
}

#[derive(Component, Debug)]
pub struct ServerInformation {
    pub ip: std::net::IpAddr,
    pub port: u32,
    pub name: String,
    pub config: types::Config,
    pub last_advertisement_received: Duration,
}

#[derive(Resource)]
struct AnnouncementListener {
    pub socket_v4: Option<UdpSocket>,
    pub socket_v6: Option<UdpSocket>,
}

fn set_up_advertisement_listener(mut commands: Commands) {
    let socket_v4 = match UdpSocket::bind("0.0.0.0:30303") {
        Ok(socket) => {
            socket.set_nonblocking(true).unwrap_or_else(|error| {
                warn!("Could not set UDPv4 port to non-blocking: {error}");
            });
            Some(socket)
        }
        Err(error) => {
            warn!("Cannot listen for UDPv4 server advertisements: {error}");
            None
        }
    };

    let socket_v6 = match UdpSocket::bind("[::]:30303") {
        Ok(socket) => {
            socket.set_nonblocking(true).unwrap_or_else(|error| {
                warn!("Could not set UDPv6 port to non-blocking: {error}");
            });
            Some(socket)
        }
        Err(error) => {
            warn!("Cannot listen for UDPv6 server advertisements: {error}");
            None
        }
    };

    commands.insert_resource(AnnouncementListener {
        socket_v4,
        socket_v6,
    });
}

fn listen_for_advertisements_v4(
    advertisement_listener: Res<AnnouncementListener>,
    mut commands: Commands,
    time: Res<Time>,
    mut servers: Query<&mut ServerInformation>,
) {
    if advertisement_listener.socket_v4.is_none() {
        return;
    }

    let socket = advertisement_listener.socket_v4.as_ref().unwrap();

    let mut buffer = Vec::with_capacity(MAXIMUM_MESSAGE_SIZE);
    buffer.resize(MAXIMUM_MESSAGE_SIZE, 0);

    loop {
        let (_message_length, sender) = match socket.recv_from(buffer.as_mut_slice()) {
            Ok(value) => value,
            Err(error) => {
                if error.kind() != WouldBlock {
                    warn!(
                        "Could not receive on advertisement listening socket: {:?}",
                        error
                    );
                }
                // It does not make sense to continue trying (either due to lack of incoming traffic
                // or due to an error), maybe it works in the next call.
                return;
            }
        };

        let message = match messages::Message::decode(&mut buffer.as_mut_slice().as_ref()) {
            Ok(value) => value,
            Err(error) => {
                debug!("Could not decode supposed advertisement: {error}");
                continue;
            }
        };

        match message.op_code() {
            messages::OpCode::ServerAdvertisement => (),
            _ => {
                debug!(
                    "Received non-advertisement Battleship Plus message on the advertisement port."
                );
                continue;
            }
        };

        let advertisement =
            match messages::ServerAdvertisement::decode(message.payload().as_slice()) {
                Ok(value) => value,
                Err(error) => {
                    warn!("Malformed advertisement: {error}");
                    continue;
                }
            };

        process_advertisement(advertisement, sender, &mut commands, &time, &mut servers);
    }
}

fn process_advertisement(
    advertisement: messages::ServerAdvertisement,
    sender: SocketAddr,
    commands: &mut Commands,
    time: &Res<Time>,
    servers: &mut Query<&mut ServerInformation>,
) {
    // Update server if it already has a ServerInformation.
    if let Some(mut server) = servers
        .iter_mut()
        .filter(|server| server.ip == sender.ip() && server.port == advertisement.port)
        .next()
    {
        server.name = advertisement.display_name;
        server.last_advertisement_received = time.elapsed();
    } else {
        // TODO: Request Config.
        commands.spawn(ServerInformation {
            ip: sender.ip(),
            port: advertisement.port,
            name: advertisement.display_name,
            config: default(),
            last_advertisement_received: time.elapsed(),
        });
    }
}

fn clean_up_servers(
    mut commands: Commands,
    time: Res<Time>,
    servers: Query<(Entity, &ServerInformation)>,
) {
    servers
        .iter()
        .filter(|(_, server)| {
            time.elapsed() - server.last_advertisement_received > std::time::Duration::from_secs(10)
        })
        .for_each(|(entity, _)| commands.entity(entity).despawn_recursive());
}

fn needs_server(servers: Query<&ServerInformation>) {
    for server in servers.iter() {
        println!("{:?}", server);
    }
}
