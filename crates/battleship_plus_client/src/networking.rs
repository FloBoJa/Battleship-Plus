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
use bevy_inspector_egui::{options::StringAttributes, Inspectable, RegisterInspectable};
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
            .add_event::<ConfigReceivedEvent>()
            .add_event::<messages::EventMessage>()
            .add_event::<ResponseReceivedEvent>()
            .register_inspectable::<ServerInformation>()
            .register_inspectable::<Connection>()
            .add_startup_system(set_up_advertisement_listener)
            .add_system(listen_for_messages)
            .add_system(clean_up_servers.run_in_state(GameState::Unconnected))
            .add_system(receive_advertisements.run_in_state(GameState::Unconnected))
            .add_system(request_server_configurations.run_in_state(GameState::Unconnected))
            .add_system(process_server_configurations.run_in_state(GameState::Unconnected))
            .add_enter_system(GameState::Joining, join_server)
            .add_enter_system(GameState::Unconnected, try_leave_server);
    }
}

// Only happens for responses received from the current server.
#[derive(Deref)]
pub struct ResponseReceivedEvent(pub messages::StatusMessage);

// Happens for all servers, but only in the unconnected state.
pub struct ConfigReceivedEvent(pub messages::StatusMessage, pub SocketAddr);

#[derive(Resource, Deref)]
pub struct CurrentServer(pub Entity);

#[derive(Component, Clone)]
pub struct ServerInformation {
    pub address: SocketAddr,
    pub name: String,
    pub config_requested: bool,
    pub config: Option<types::Config>,
    pub last_advertisement_received: Duration,
}

impl Inspectable for ServerInformation {
    type Attributes = ();

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _: Self::Attributes,
        context: &mut bevy_inspector_egui::Context,
    ) -> bool {
        let mut modified = false;
        modified |= self.name.ui(ui, StringAttributes::default(), context);
        ui.label(format!("Address: {}", self.address));
        modified |= self.last_advertisement_received.ui(ui, (), context);
        ui.label(format!("Config: {:#?}", self.config));
        modified
    }
}

impl ServerInformation {
    fn connect(&self, commands: &mut Commands, server: Entity, client: &mut ResMut<Client>) {
        // Bind to UDPv4 if the server communicates on it.
        let (local_address, server_scope) = match self.address {
            SocketAddr::V4(_) => ("0.0.0.0".to_string(), None),
            SocketAddr::V6(address) => ("[::]".to_string(), Some(address.scope_id())),
        };

        let connection_configuration = ConnectionConfiguration::new(
            self.address.ip().to_string(),
            server_scope,
            self.address.port(),
            local_address,
            0,
        );

        let certificate_mode =
            CertificateVerificationMode::TrustOnFirstUse(TrustOnFirstUseConfig::default());
        let connection_id = client.open_connection(connection_configuration, certificate_mode);

        commands
            .get_entity(server)
            .expect("The server entity must be the parent of this component")
            .insert(Connection(connection_id));
    }
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
            join_multicast_v6("ff02:6261:7474:6c65:7368:6970:706c:7573", &socket);
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

        let server_information = ServerInformation {
            address: server_address,
            name: advertisement.display_name.clone(),
            config: None,
            config_requested: false,
            last_advertisement_received: time.elapsed(),
        };
        let server = commands.spawn(server_information.clone()).id();

        // Connect to the server to request its configuration.
        server_information.connect(commands, server, client);
    }
}

#[derive(Component, Clone, Inspectable, Deref)]
pub struct Connection(ConnectionId);

fn request_server_configurations(
    mut servers: Query<(&mut ServerInformation, &Connection)>,
    mut client: ResMut<Client>,
) {
    let message: messages::ProtocolMessage = messages::ServerConfigRequest {}.into();

    for (mut server_information, connection) in servers.iter_mut() {
        if server_information.config_requested {
            continue;
        }

        let connection = match client.get_connection_mut_by_id(**connection) {
            Some(connection) => connection,
            None => {
                debug!("Did not find connection {} represented by Connection component, it was probably just deleted.", **connection);
                continue;
            }
        };

        if let Err(error) = connection.send_message(message.clone()) {
            warn!(
                "Failed to send server configuration request to {}: {error}",
                server_information.address
            );
        }

        server_information.config_requested = true;
    }
}

fn listen_for_messages(
    servers: Query<(Entity, &ServerInformation, &Connection)>,
    current_server: Option<Res<CurrentServer>>,
    mut client: ResMut<Client>,
    mut response_events: EventWriter<ResponseReceivedEvent>,
    mut config_response_events: EventWriter<ConfigReceivedEvent>,
    mut game_events: EventWriter<messages::EventMessage>,
) {
    match current_server {
        None => {
            for (_, server_information, connection) in servers.iter() {
                listen_for_messages_from(
                    server_information,
                    connection,
                    &mut client,
                    None,
                    Some(&mut config_response_events),
                    None,
                );
            }
        }
        Some(server) => {
            let server_information = servers
                .get_component::<ServerInformation>(server.0)
                .expect("CurrentServer always has a ServerInformation");
            let connection = servers
                .get_component::<Connection>(server.0)
                .expect("CurrentServer always has a connection component");
            listen_for_messages_from(
                server_information,
                connection,
                &mut client,
                Some(&mut response_events),
                None,
                Some(&mut game_events),
            );
        }
    }
}

fn listen_for_messages_from(
    server_information: &ServerInformation,
    Connection(connection_id): &Connection,
    client: &mut ResMut<Client>,
    mut response_events: Option<&mut EventWriter<ResponseReceivedEvent>>,
    mut config_response_events: Option<&mut EventWriter<ConfigReceivedEvent>>,
    mut game_events: Option<&mut EventWriter<messages::EventMessage>>,
) {
    let sender = server_information.address;
    let connection = match client.get_connection_mut_by_id(*connection_id) {
        Some(connection) => connection,
        None => {
            debug!("Did not find connection {connection_id} represented by Connection component, it was probably just deleted.");
            return;
        }
    };
    loop {
        match connection.receive_message() {
            Ok(Some(Some(ProtocolMessage::StatusMessage(status_message)))) => {
                debug!("Received reponse from {sender}: {status_message:?}");
                if let Some(response_events) = &mut response_events {
                    response_events.send(ResponseReceivedEvent(status_message.clone()));
                }
                if let Some(config_response_events) = &mut config_response_events {
                    config_response_events.send(ConfigReceivedEvent(status_message, sender));
                }
            }
            Ok(Some(Some(other_message))) => match EventMessage::try_from(other_message.clone()) {
                Ok(game_event) => {
                    if let Some(game_events) = &mut game_events {
                        game_events.send(game_event)
                    }
                }
                Err(()) => {
                    warn!("Received non-event message without status code: {other_message:?}")
                }
            },
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

fn process_server_configurations(
    mut commands: Commands,
    mut events: EventReader<ConfigReceivedEvent>,
    mut servers: Query<(Entity, &mut ServerInformation, &Connection)>,
    mut client: ResMut<Client>,
) {
    for ConfigReceivedEvent(
        messages::StatusMessage {
            code,
            message,
            data,
        },
        sender,
    ) in events.iter()
    {
        let (entity, mut server, connection) = match servers
            .iter_mut()
            .find(|(_, server, _)| &server.address == sender)
        {
            Some(server) => server,
            None => continue,
        };
        trace!("Received some response after config request during discovery, closing connection.");
        if let Err(error) = client.close_connection(**connection) {
            warn!("Failed to close connection properly: {error}");
        }
        commands.entity(entity).remove::<Connection>();

        if code / 100 != 2 {
            if message.is_empty() {
                warn!("Received error code {code} from server at {sender}");
            } else {
                warn!("Received error code {code} from server at {sender} with message: {message}");
            }
            continue;
        }
        let response = match data {
            Some(messages::status_message::Data::ServerConfigResponse(response)) => response,
            Some(_other_response) => {
                if message.is_empty() {
                    warn!("No data in response after ConfigRequest but status code 2XX");
                } else {
                    warn!("No data in response after ConfigRequest but status code 2XX with message: {message}");
                }
                // ignore
                continue;
            }
            None => continue,
        };
        if response.config.is_none() {
            if message.is_empty() {
                warn!("Received empty ServerConfigResponse from {sender}. This indicates an error in that server");
            } else {
                warn!("Received empty ServerConfigResponse from {sender}. This indicates an error in that server. \
                       The following message was included: {message}");
            }
            continue;
        }

        server.config = response.config.to_owned();
    }
}

fn clean_up_servers(
    mut commands: Commands,
    time: Res<Time>,
    servers: Query<(Entity, &ServerInformation)>,
    connections: Query<(Entity, &Connection)>,
    mut client: ResMut<Client>,
) {
    servers
        .iter()
        .filter(|(_, server)| {
            time.elapsed() - server.last_advertisement_received > Duration::from_secs(10)
        })
        .for_each(|(entity, _)| {
            if let Ok(connection) = connections.get_component::<Connection>(entity) {
                if let Err(error) = client.close_connection(**connection) {
                    warn!("Failed to close connection properly: {error}");
                }
            }
            commands.entity(entity).despawn_recursive()
        });
}

fn join_server(
    mut commands: Commands,
    server: Option<Res<CurrentServer>>,
    server_information: Query<(Entity, &ServerInformation)>,
    connections: Query<(Entity, &Connection)>,
    mut client: ResMut<Client>,
    user_name: Res<crate::lobby::UserName>,
) {
    info!("Joining server");
    let server = server.expect("There must always exist a CurrentServer in GameState::Joining");
    let server_information = server_information
        .get_component::<ServerInformation>(**server)
        .expect("CurrentServer always has a ServerInformation component");

    // Close all connections when joining a server
    for (entity, connection) in connections.iter() {
        if let Err(error) = client.close_connection(**connection) {
            warn!("Failed to close connection properly: {error}");
        }
        commands.entity(entity).remove::<Connection>();
    }

    server_information.connect(&mut commands, **server, &mut client);

    // From now on, the default connection can be used.
    let connection = client
        .get_connection()
        .expect("This connection was just requested");

    let message = messages::JoinRequest {
        username: user_name.clone(),
    };
    if let Err(error) = connection.send_message(message.into()) {
        warn!("Could not send join request: {error}");
        commands.insert_resource(NextState(GameState::Unconnected));
    }
}

// Leaving the CurrentServer, if there is one.
fn try_leave_server(
    mut commands: Commands,
    server: Option<Res<CurrentServer>>,
    connections: Query<(Entity, &Connection)>,
    mut client: ResMut<Client>,
) {
    let server = match server {
        Some(server) => server,
        None => return,
    };
    info!("Leaving server");
    let connection = connections
        .get_component::<Connection>(**server)
        .expect("There must always exist a connection component for the current server");
    if let Err(error) = client.close_connection(**connection) {
        warn!("Failed to close connection properly: {error}");
    }
    commands.entity(**server).remove::<Connection>();
}
