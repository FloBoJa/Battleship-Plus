use battleship_plus_common::{codec::BattleshipPlusCodec, messages, types};
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use bevy_quinnet::{
    client::{
        certificate::{CertificateVerificationMode, TrustOnFirstUseConfig},
        Client, ConnectionConfiguration, ConnectionId, QuinnetClientPlugin,
    },
    server::{
        certificate::CertificateRetrievalMode, QuinnetServerPlugin, Server, ServerConfigurationData,
    },
    shared::{AsyncRuntime, QuinnetError},
};
use futures::stream::StreamExt;
use std::{
    net::{Ipv6Addr, SocketAddr},
    str::FromStr,
    sync::mpsc,
    time::Duration,
};
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin::default())
            .add_event::<(messages::ServerConfigResponse, SocketAddr)>()
            .add_startup_system(set_up_advertisement_listener)
            .add_system(receive_advertisements)
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

#[derive(Component)]
struct AdvertisementReceiver(SyncCell<mpsc::Receiver<(messages::ServerAdvertisement, SocketAddr)>>);

impl AdvertisementReceiver {
    fn new(
        receiver: mpsc::Receiver<(messages::ServerAdvertisement, SocketAddr)>,
    ) -> AdvertisementReceiver {
        AdvertisementReceiver(SyncCell::new(receiver))
    }

    fn get(&mut self) -> &mut mpsc::Receiver<(messages::ServerAdvertisement, SocketAddr)> {
        self.0.get()
    }
}

fn set_up_advertisement_listener(mut commands: Commands, runtime: Res<AsyncRuntime>) {
    let socket_v6 = match runtime.block_on(UdpSocket::bind("[::]:30303")) {
        Ok(socket) => {
            join_multicast_v6("ff02::1", &socket);
            Some(socket)
        }
        Err(error) => {
            warn!("Cannot listen for UDPv6 server advertisements: {error}");
            None
        }
    };
    let (sender_v6, receiver_v6) =
        mpsc::sync_channel::<(messages::ServerAdvertisement, SocketAddr)>(10);
    if let Some(socket) = socket_v6 {
        commands.spawn(AdvertisementReceiver::new(receiver_v6));
        runtime.spawn(listen_for_advertisements(socket, sender_v6));
    }

    let socket_v4 = match runtime.block_on(UdpSocket::bind("0.0.0.0:30303")) {
        Ok(socket) => Some(socket),
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
    let (sender_v4, receiver_v4) =
        mpsc::sync_channel::<(messages::ServerAdvertisement, SocketAddr)>(10);
    if let Some(socket) = socket_v4 {
        commands.spawn(AdvertisementReceiver::new(receiver_v4));
        runtime.spawn(listen_for_advertisements(socket, sender_v4));
    }
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

async fn listen_for_advertisements(
    socket: UdpSocket,
    channel_sender: mpsc::SyncSender<(messages::ServerAdvertisement, SocketAddr)>,
) {
    let mut socket = UdpFramed::new(socket, BattleshipPlusCodec::default());
    while let Some(result) = socket.next().await {
        let (advertisement, sender) = match result {
            Ok((Some(messages::ProtocolMessage::ServerAdvertisement(advertisement)), sender)) => {
                (advertisement, sender)
            }
            Ok((Some(_), sender)) => {
                debug!("Received non-advertisement message from {sender} over UDP");
                continue;
            }
            Ok((None, sender)) => {
                debug!("Received empty advertisement packet from {sender}");
                continue;
            }
            Err(error) => {
                error!("Could not receive advertisement: {error}\n\
                       TODO: Do not stop listening for advertisements upon receiving an illegal message, just discard the UDP packet.");
                continue;
            }
        };

        channel_sender
            .send((advertisement, sender))
            .expect("Internal advertisement channel should be open");
    }
}

fn receive_advertisements(
    mut receivers: Query<&mut AdvertisementReceiver>,
    mut commands: Commands,
    time: Res<Time>,
    mut servers: Query<&mut ServerInformation>,
    mut client: ResMut<Client>,
) {
    for mut receiver in receivers.iter_mut() {
        loop {
            match receiver.get().recv_timeout(Duration::from_millis(1)) {
                Ok((advertisement, sender)) => process_advertisement(
                    advertisement,
                    sender,
                    &mut commands,
                    &time,
                    &mut servers,
                    &mut client,
                ),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("Internal advertisement channel closed")
                }
            };
        }
    }
}

fn process_advertisement(
    advertisement: messages::ServerAdvertisement,
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

    let message = messages::packet_payload::ProtocolMessage::ServerConfigRequest(
        messages::ServerConfigRequest {},
    );

    if let Err(error) = client.connection().send_message(message) {
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

        let message = match connection.receive_message() {
            Ok(Some(Some(value))) => value,
            Ok(Some(None)) => {
                warn!("Received empty PacketPayload");
                continue;
            }
            Ok(None) => continue,
            Err(QuinnetError::ChannelClosed) => continue,
            Err(error) => {
                warn!("Unexpected error occurred while receiving packet: {error}");
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

        match message {
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
