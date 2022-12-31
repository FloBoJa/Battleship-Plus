use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    str::FromStr,
    sync::mpsc,
    time::Duration,
};

use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use bytes::BytesMut;
use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;

use crate::game_state::GameState;
use battleship_plus_common::{
    codec::BattleshipPlusCodec,
    messages::{self, EventMessage, ProtocolMessage, ServerAdvertisement},
    types,
};
use bevy_quinnet::{
    client::{
        certificate::{CertificateVerificationMode, TrustOnFirstUseConfig},
        Client, ConnectionConfiguration, ConnectionId, QuinnetClientPlugin,
    },
    shared::{AsyncRuntime, QuinnetError},
};
use iyes_loopless::prelude::*;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin::default())
            .add_event::<messages::EventMessage>()
            .add_event::<ResponseReceivedEvent>()
            .init_resource::<CurrentServer>()
            .add_startup_system(set_up_advertisement_listener)
            .add_system(clean_up_servers)
            .add_system(listen_for_messages)
            .add_system(receive_advertisements.run_in_state(GameState::Unconnected))
            .add_system(process_server_configurations.run_in_state(GameState::Unconnected));
    }
}

pub struct ResponseReceivedEvent(pub messages::StatusMessage, pub SocketAddr);

#[derive(Resource, Default)]
pub struct CurrentServer(pub Option<ConnectionRecord>);

#[derive(Component, Debug)]
pub struct ServerInformation {
    pub address: SocketAddr,
    pub name: String,
    pub config: Option<types::Config>,
    pub last_advertisement_received: Duration,
}

#[derive(Component)]
struct AdvertisementReceiver(SyncCell<mpsc::Receiver<(ServerAdvertisement, SocketAddr)>>);

impl AdvertisementReceiver {
    fn new(receiver: mpsc::Receiver<(ServerAdvertisement, SocketAddr)>) -> AdvertisementReceiver {
        AdvertisementReceiver(SyncCell::new(receiver))
    }

    fn get(&mut self) -> &mut mpsc::Receiver<(ServerAdvertisement, SocketAddr)> {
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
    let (sender_v6, receiver_v6) = mpsc::sync_channel::<(ServerAdvertisement, SocketAddr)>(10);
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
    let (sender_v4, receiver_v4) = mpsc::sync_channel::<(ServerAdvertisement, SocketAddr)>(10);
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

const MAX_UDP_SIZE: usize = 64 * 1024;

async fn listen_for_advertisements(
    socket: UdpSocket,
    channel_sender: mpsc::SyncSender<(ServerAdvertisement, SocketAddr)>,
) {
    let mut buffer = BytesMut::zeroed(MAX_UDP_SIZE);
    loop {
        let mut codec = BattleshipPlusCodec::default();
        let mut read_bytes = 0;
        let mut unfinished_sender = None;
        loop {
            let result = if let Some(sender) = unfinished_sender {
                socket
                    .recv(&mut buffer[read_bytes..])
                    .await
                    .map(|datagram_length| (datagram_length, sender))
            } else {
                socket.recv_from(&mut buffer[read_bytes..]).await
            };
            let sender = match result {
                Ok((datagram_length, sender)) => {
                    read_bytes += datagram_length;
                    sender
                }
                Err(_) => todo!(),
            };
            let advertisement = match codec.decode(&mut buffer) {
                Ok(Some(Some(ProtocolMessage::ServerAdvertisement(advertisement)))) => {
                    advertisement
                }
                Ok(None) => {
                    // Read further, ensuring that there is enough space for the next datagram.
                    if read_bytes + MAX_UDP_SIZE > buffer.len() {
                        buffer.resize(buffer.len() - read_bytes - MAX_UDP_SIZE, 0);
                    }
                    unfinished_sender = Some(sender);
                    debug!("{read_bytes}");
                    continue;
                }
                // Discard the datagram, log it, and continue listening in case of an exception.
                Ok(Some(Some(_))) => {
                    debug!("Received non-advertisement message over UDP from {sender}");
                    break;
                }
                Ok(Some(None)) => {
                    debug!("Received empty advertisement packet from {sender}");
                    break;
                }
                Err(error) => {
                    debug!("Could not receive advertisement: {error}");
                    break;
                }
            };

            channel_sender
                .send((advertisement, sender))
                .expect("Internal advertisement channel should be open");
            break;
        }
        // Shrink the buffer, if necessary.
        buffer.resize(MAX_UDP_SIZE, 0);
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
    advertisement: ServerAdvertisement,
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
        let server_address = match sender {
            SocketAddr::V4(sender) => {
                SocketAddrV4::new(sender.ip().to_owned(), advertisement.port as u16).into()
            }
            SocketAddr::V6(sender) => SocketAddrV6::new(
                sender.ip().to_owned(),
                advertisement.port as u16,
                0,
                sender.scope_id(),
            )
            .into(),
        };
        request_config(server_address, commands, client);

        commands.spawn(ServerInformation {
            address: server_address,
            name: advertisement.display_name.clone(),
            config: None,
            last_advertisement_received: time.elapsed(),
        });
    }
}

#[derive(Component, Clone)]
pub struct ConnectionRecord {
    pub connection_id: ConnectionId,
    pub server_address: SocketAddr,
}

fn request_config(
    server_address: SocketAddr,
    commands: &mut Commands,
    client: &mut ResMut<Client>,
) {
    // Bind to UDPv4 if the server communicates on it.
    let (local_address, server_scope) = match server_address {
        SocketAddr::V4(_) => ("0.0.0.0".to_string(), None),
        SocketAddr::V6(server_address) => ("[::]".to_string(), Some(server_address.scope_id())),
    };

    let connection_configuration = ConnectionConfiguration::new(
        server_address.ip().to_string(),
        server_scope,
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

    let message = ProtocolMessage::ServerConfigRequest(messages::ServerConfigRequest {});

    if let Err(error) = client.connection().send_message(message) {
        warn!("Failed to send server configuration request to {server_address}: {error}");
    }
}

fn listen_for_messages(
    mut response_events: EventWriter<ResponseReceivedEvent>,
    mut game_events: EventWriter<messages::EventMessage>,
    mut current_server: Res<CurrentServer>,
    mut client: ResMut<Client>,
    connection_records: Query<&ConnectionRecord>,
) {
    for (connection_id, connection) in client.connections_mut() {
        if !connection.is_connected() {
            continue;
        }

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

        loop {
            match connection.receive_message() {
                Ok(Some(Some(ProtocolMessage::StatusMessage(status_message)))) => {
                    debug!("Received reponse from {sender}: {status_message:?}");
                    response_events.send(ResponseReceivedEvent(status_message, sender));
                }
                Ok(Some(Some(other_message))) => {
                    match &current_server.0 {
                        None => {
                            warn!("Received non-status message before joining a server from {sender}: {other_message:?}");
                            continue;
                        }
                        Some(current_connection) if sender == current_connection.server_address => {
                            // Carry on
                        }
                        Some(_other_connection) => {
                            warn!("Received non-status message from unjoined server at {sender}: {other_message:?}");
                            continue;
                        }
                    };
                    match EventMessage::try_from(other_message.clone()) {
                        Ok(game_event) => game_events.send(game_event),
                        Err(()) => warn!(
                            "Received non-event message without status code: {other_message:?}"
                        ),
                    }
                }
                Ok(Some(None)) => {
                    // A keep-alive message
                }
                Ok(None) => {
                    // Nothing or an incomplete message, retry later
                    break;
                }
                Err(QuinnetError::ChannelClosed) => {}
                Err(error) => {
                    warn!("Unexpected error occurred while receiving packet: {error}");
                }
            };
        }
    }
}

pub enum ResponseError<T> {
    A(T),
}

pub fn receive_response<T>(
    events: &mut EventReader<ResponseReceivedEvent>,
) -> Result<T, ResponseError<T>>
where
    T: messages::Message + Default,
{
    let mut messages = vec![];
    for ResponseReceivedEvent(
        messages::StatusMessage {
            code,
            message,
            data,
        },
        sender,
    ) in events.iter()
    {
        // Return the first message containing the correct response type.
        messages.push((code, data, sender));
    }
    // Otherwise, return the first status message containing a plausible error type (excluding
    // server errors).
    // Then, return the first server error.
    // Finally, return the first status message.
    Ok(T::default())
}

fn process_server_configurations(
    mut events: EventReader<ResponseReceivedEvent>,
    mut servers: Query<&mut ServerInformation>,
) {
    for ResponseReceivedEvent(
        messages::StatusMessage {
            code,
            message,
            data,
        },
        sender,
    ) in events.iter()
    {
        // TODO: Include response.message as soon as that MR is merged.
        if code / 100 != 2 {
            warn!("Received error code {code} from server at {sender}");
            continue;
        }
        let response = match data {
            Some(messages::status_message::Data::ServerConfigResponse(response)) => response,
            Some(_other_response) => {
                warn!("No data in response after ConfigRequest but status code 2XX");
                // ignore
                continue;
            }
            None => continue,
        };
        let mut server = match servers.iter_mut().find(|server| &server.address == sender) {
            Some(server) => server,
            None => continue,
        };
        if response.config.is_none() {
            warn!("Received empty ServerConfigResponse from {sender}. This indicates an error in that server");
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
            time.elapsed() - server.last_advertisement_received > Duration::from_secs(10)
        })
        .for_each(|(entity, _)| commands.entity(entity).despawn_recursive());
}
