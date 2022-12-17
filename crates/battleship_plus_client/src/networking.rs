use battleship_plus_common::{
    messages::{self, MAXIMUM_MESSAGE_SIZE},
    types, ProstMessage,
};
use bevy::prelude::*;
use bevy_quinnet::client::QuinnetClientPlugin;
use std::{
    io::ErrorKind::WouldBlock,
    net::{Ipv6Addr, SocketAddr, UdpSocket},
    str::FromStr,
    time::Duration,
};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin {})
            .add_startup_system(set_up_advertisement_listener)
            .add_system(listen_for_advertisements)
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
    let socket_v6 = match UdpSocket::bind("[::]:30303") {
        Ok(socket) => {
            join_multicast_v6("ff02::1", &socket);
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

    let socket_v4 = match UdpSocket::bind("0.0.0.0:30303") {
        Ok(socket) => {
            socket.set_nonblocking(true).unwrap_or_else(|error| {
                warn!("Could not set UDPv4 port to non-blocking: {error}");
            });
            Some(socket)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            // This is OS-specific. Some OSs necessitate both UDPv4 and UDPv6
            // to be bound, some do not allow it.
            info!(
                "Not able to bind UDPv4 advertisement listening socket, \
                 probably because UDPv6 advertisement listening blocks it."
            );
            None
        }
        Err(error) => {
            warn!("Cannot listen for UDPv4 server advertisements: {error}");
            None
        }
    };

    commands.insert_resource(AnnouncementListener {
        socket_v4,
        socket_v6,
    });
}

fn join_multicast_v6(multiaddr: &str, socket: &UdpSocket) {
    let multicast_address =
        Ipv6Addr::from_str(multiaddr).expect("Could not parse hard-coded multicast address");

    socket
        .join_multicast_v6(&multicast_address, 0)
        .unwrap_or_else(|error| {
            warn!("Could not join UDPv6 multicast: {error}");
        });

    socket.set_multicast_loop_v6(true).unwrap_or_else(|error| {
        warn!("Could not enable UDPv6 multicast loopback: {error}");
    });
}

fn listen_for_advertisements(
    advertisement_listener: Res<AnnouncementListener>,
    mut commands: Commands,
    time: Res<Time>,
    mut servers: Query<&mut ServerInformation>,
) {
    // Listen for IPv6 advertisements.
    if let Some(socket) = advertisement_listener.socket_v6.as_ref() {
        listen_for_advertisements_on(socket, &mut commands, &time, &mut servers);
    }

    // Listen for IPv4 advertisements
    if let Some(socket) = advertisement_listener.socket_v4.as_ref() {
        listen_for_advertisements_on(socket, &mut commands, &time, &mut servers);
    }
}

fn listen_for_advertisements_on(
    socket: &UdpSocket,
    commands: &mut Commands,
    time: &Res<Time>,
    servers: &mut Query<&mut ServerInformation>,
) {
    let mut buffer = vec![0; MAXIMUM_MESSAGE_SIZE];

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

        let message = match messages::Message::decode(&mut buffer.as_slice()) {
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

        process_advertisement(advertisement, sender, commands, time, servers);
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
        .find(|server| server.ip == sender.ip() && server.port == advertisement.port)
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
