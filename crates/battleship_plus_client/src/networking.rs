use battleship_plus_common::{
    messages::{self, MAXIMUM_MESSAGE_SIZE},
    types, PROTOCOL_VERSION,
};
use bevy::prelude::*;
use bevy_quinnet::{
    client::{
        certificate::{CertificateVerificationMode, TrustOnFirstUseConfig},
        Client, ConnectionConfiguration, ConnectionId, QuinnetClientPlugin,
    },
    server::{
        certificate::CertificateRetrievalMode, QuinnetServerPlugin, Server, ServerConfigurationData,
    },
    shared::QuinnetError,
};
use std::{
    io::ErrorKind::WouldBlock,
    net::{Ipv6Addr, SocketAddr, UdpSocket},
    str::FromStr,
    time::Duration,
};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin::default())
            .add_event::<(messages::ServerConfigResponse, SocketAddr)>()
            .add_startup_system(set_up_advertisement_listener)
            .add_system(listen_for_advertisements)
            .add_system(clean_up_servers)
            .add_system(listen_for_server_configurations)
            .add_system(process_server_configurations);

        if cfg!(feature = "fake_server") {
            app.add_plugin(QuinnetServerPlugin::default())
                .add_startup_system(start_server)
                .add_system(fake_server);
        }
    }
}

fn start_server(mut server: ResMut<Server>) {
    info!("Removing bevy_quinnet's known_hosts file to allow unstable certificate.");
    let _ = std::fs::remove_file(bevy_quinnet::client::DEFAULT_KNOWN_HOSTS_FILE);
    let _ = server.start_endpoint(
        ServerConfigurationData::new("[::]".to_string(), 30305, "[::]".to_string()),
        CertificateRetrievalMode::GenerateSelfSigned,
    );
}

fn fake_server(mut server: ResMut<Server>) {
    let _res = server.endpoint_mut().try_receive_payload();
}

#[derive(Component, Debug)]
pub struct ServerInformation {
    pub address: SocketAddr,
    pub name: String,
    pub config: Option<types::Config>,
    pub last_advertisement_received: Duration,
}

#[derive(Resource)]
struct AdvertisementListener {
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

    commands.insert_resource(AdvertisementListener {
        socket_v4,
        socket_v6,
    });
}

fn join_multicast_v6(multiaddr: &str, socket: &UdpSocket) {
    let multicast_address =
        Ipv6Addr::from_str(multiaddr).expect("Could not parse hard-coded multicast address");

    for interface in pnet_datalink::interfaces() {
        socket
            .join_multicast_v6(&multicast_address, interface.index)
            .unwrap_or_else(|error| {
                warn!("Could not join UDPv6 multicast on interface {interface} : {error}");
            });
    }

    socket.set_multicast_loop_v6(true).unwrap_or_else(|error| {
        warn!("Could not enable UDPv6 multicast loopback: {error}");
    });
}

fn listen_for_advertisements(
    advertisement_listener: Res<AdvertisementListener>,
    mut commands: Commands,
    time: Res<Time>,
    mut servers: Query<&mut ServerInformation>,
    mut client: ResMut<Client>,
) {
    // Listen for IPv6 advertisements.
    if let Some(socket) = advertisement_listener.socket_v6.as_ref() {
        listen_for_advertisements_on(socket, &mut commands, &time, &mut servers, &mut client);
    }

    // Listen for IPv4 advertisements
    if let Some(socket) = advertisement_listener.socket_v4.as_ref() {
        listen_for_advertisements_on(socket, &mut commands, &time, &mut servers, &mut client);
    }
}

fn listen_for_advertisements_on(
    socket: &UdpSocket,
    commands: &mut Commands,
    time: &Res<Time>,
    servers: &mut Query<&mut ServerInformation>,
    client: &mut ResMut<Client>,
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

        let advertisement = match message.inner_message() {
            messages::packet_payload::ProtocolMessage::ServerAdvertisement(value) => value,
            _ => {
                debug!(
                    "Received non-advertisement Battleship Plus message on the advertisement port."
                );
                continue;
            }
        };

        process_advertisement(advertisement, sender, commands, time, servers, client);
    }
}

fn process_advertisement(
    advertisement: &messages::ServerAdvertisement,
    sender: SocketAddr,
    commands: &mut Commands,
    time: &Res<Time>,
    servers: &mut Query<&mut ServerInformation>,
    client: &mut ResMut<Client>,
) {
    // Update server if it already has a ServerInformation.
    if let Some(mut server) = servers.iter_mut().find(|server| {
        server.address.ip() == sender.ip() && server.address.port() == advertisement.port as u16
    }) {
        server.name = advertisement.display_name.clone();
        server.last_advertisement_received = time.elapsed();
    } else {
        let server_address = SocketAddr::new(sender.ip(), advertisement.port as u16);
        request_config(server_address, commands, client);

        commands.spawn(ServerInformation {
            address: server_address,
            name: advertisement.display_name.clone(),
            config: None,
            last_advertisement_received: time.elapsed(),
        });
    }
}

#[derive(Component)]
struct ConnectionRecord {
    connection_id: ConnectionId,
    server_address: SocketAddr,
}

fn request_config(
    server_address: SocketAddr,
    commands: &mut Commands,
    client: &mut ResMut<Client>,
) {
    // Bind to UDPv4 if the server communicates on it.
    let local_address = if server_address.is_ipv4() {
        "0.0.0.0"
    } else {
        "[::]"
    }
    .to_string();

    let connection_configuration = ConnectionConfiguration::new(
        server_address.ip().to_string(),
        server_address.port(),
        local_address,
        0,
    );

    let certificate_mode =
        CertificateVerificationMode::TrustOnFirstUse(TrustOnFirstUseConfig::default());
    let connection_id = client.open_connection(connection_configuration, certificate_mode);

    commands.spawn(ConnectionRecord {
        connection_id,
        server_address,
    });

    let message = messages::Message::new(
        PROTOCOL_VERSION,
        messages::packet_payload::ProtocolMessage::ServerConfigRequest(
            messages::ServerConfigRequest {},
        ),
    )
    .expect("Request should be constructed properly");

    if let Err(error) = client.connection().send_payload(message.encode()) {
        warn!("Failed to send server configuration request to {server_address}: {error}");
    }
}

fn listen_for_server_configurations(
    mut event: EventWriter<(messages::ServerConfigResponse, SocketAddr)>,
    mut client: ResMut<Client>,
    connection_records: Query<&ConnectionRecord>,
) {
    for (connection_id, connection) in client.connections_mut() {
        if !connection.is_connected() {
            continue;
        }

        let payload = match connection.receive_payload() {
            Ok(Some(value)) => value.to_vec(),
            Ok(None) => continue,
            Err(QuinnetError::ChannelClosed) => continue,
            Err(error) => {
                warn!("Unexpected error occurred while receiving packet: {error}");
                continue;
            }
        };

        let message = match messages::Message::decode(&mut payload.as_slice()) {
            Ok(value) => value,
            Err(error) => {
                debug!("Could not decode incoming message: {error}");
                continue;
            }
        };

        let sender = match connection_records
            .iter()
            .find(|record| &record.connection_id == connection_id)
        {
            Some(record) => record.server_address,
            None => {
                warn!("Received data over unknown connection");
                continue;
            }
        };
        match message.inner_message() {
            messages::packet_payload::ProtocolMessage::ServerConfigResponse(message) => {
                event.send((message.to_owned(), sender));
            }
            other => {
                warn!("Received unimplemented message type {:?}", other);
            }
        }
    }
}

fn process_server_configurations(
    mut event: EventReader<(messages::ServerConfigResponse, SocketAddr)>,
    mut servers: Query<&mut ServerInformation>,
) {
    for (response, sender) in event.iter() {
        let mut server = match servers.iter_mut().find(|server| &server.address == sender) {
            Some(server) => server,
            None => continue,
        };
        if response.config.is_none() {
            debug!("Received empty ServerConfigResponse from {sender}");
            continue;
        }
        server.config = response.config.to_owned();
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
