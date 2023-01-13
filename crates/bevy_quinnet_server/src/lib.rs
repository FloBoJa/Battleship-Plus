use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

#[cfg(feature = "bevy")]
use bevy::prelude::*;
use futures::sink::SinkExt;
use futures_util::StreamExt;
#[cfg(not(feature = "bevy"))]
use log::{debug, error, info, trace};
use quinn::{Endpoint as QuinnEndpoint, ServerConfig};
#[cfg(feature = "bevy")]
use serde::Deserialize;
use tokio::runtime::Runtime;
use tokio::{
    runtime,
    sync::{
        broadcast,
        mpsc::{self, error::TryRecvError},
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

use battleship_plus_common::{
    codec::{BattleshipPlusCodec, CodecError},
    messages::ProtocolMessage,
};

use bevy_quinnet_common::{DEFAULT_KILL_MESSAGE_QUEUE_SIZE, DEFAULT_MESSAGE_QUEUE_SIZE};

pub use bevy_quinnet_common::{ClientId, QuinnetError};

pub mod certificate;
use self::certificate::{retrieve_certificate, CertificateRetrievalMode, ServerCertificate};

pub const DEFAULT_INTERNAL_MESSAGE_CHANNEL_SIZE: usize = 100;

#[cfg_attr(feature = "bevy", derive(Resource, Deref, DerefMut))]
pub struct AsyncRuntime(pub Runtime);

/// Connection event raised when a client just connected to the server. Raised in the CoreStage::PreUpdate stage.
pub struct ConnectionEvent {
    /// Id of the client who connected
    pub id: ClientId,
}

/// ConnectionLost event raised when a client is considered disconnected from the server. Raised in the CoreStage::PreUpdate stage.
pub struct ConnectionLostEvent {
    /// Id of the client who lost connection
    pub id: ClientId,
}

/// Configuration of the server, used when the server starts
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bevy", derive(Deserialize))]
pub struct ServerConfigurationData {
    host: String,
    port: u16,
    local_bind_host: String,
}

impl ServerConfigurationData {
    /// Creates a new ServerConfigurationData
    ///
    /// # Arguments
    ///
    /// * `host` - Address of the server
    /// * `port` - Port that the server is listening on
    /// * `local_bind_host` - Local address to bind to, which should usually be a wildcard address like `0.0.0.0` or `[::]`, which allow communication with any reachable IPv4 or IPv6 address. See [`quinn::endpoint::Endpoint`] for more precision
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy_quinnet_server::ServerConfigurationData;
    ///
    /// let config = ServerConfigurationData::new(
    ///         "127.0.0.1".to_string(),
    ///         6000,
    ///         "0.0.0.0".to_string(),
    ///     );
    /// ```
    pub fn new(host: String, port: u16, local_bind_host: String) -> Self {
        Self {
            host,
            port,
            local_bind_host,
        }
    }
}

/// Represents a client message in its binary form
#[derive(Debug)]
pub struct ClientPayload {
    /// Id of the client sending the message
    pub client_id: ClientId,
    /// Content of the message as bytes
    pub msg: Option<ProtocolMessage>,
}

#[derive(Debug)]
pub(crate) enum InternalAsyncMessage {
    ClientConnected(ClientConnection),
    ClientLostConnection(ClientId),
    UnsupportedVersionMessage { client_id: ClientId, version: u8 },
}

#[derive(Debug)]
pub(crate) struct ClientConnection {
    client_id: ClientId,
    sender: mpsc::Sender<ProtocolMessage>,
    close_sender: broadcast::Sender<()>,
}

pub struct Endpoint {
    clients: HashMap<ClientId, ClientConnection>,
    payloads_receiver: mpsc::Receiver<ClientPayload>,
    close_sender: broadcast::Sender<()>,

    #[cfg(not(feature = "bevy"))]
    pub(crate) internal_receiver_closed: bool,
    pub(crate) internal_receiver: mpsc::Receiver<InternalAsyncMessage>,
}

impl Endpoint {
    pub fn send_message(
        &self,
        client_id: ClientId,
        message: ProtocolMessage,
    ) -> Result<(), QuinnetError> {
        if let Some(client) = self.clients.get(&client_id) {
            match client.sender.try_send(message) {
                Ok(_) => Ok(()),
                Err(err) => match err {
                    mpsc::error::TrySendError::Full(_) => Err(QuinnetError::FullQueue),
                    mpsc::error::TrySendError::Closed(_) => Err(QuinnetError::ChannelClosed),
                },
            }
        } else {
            Err(QuinnetError::UnknownClient(client_id))
        }
    }

    pub fn try_send_message(&self, client_id: ClientId, message: ProtocolMessage) {
        match self.send_message(client_id, message) {
            Ok(_) => {}
            Err(err) => error!("try_send_message: {}", err),
        }
    }

    pub fn send_group_message<'a, I: Iterator<Item = &'a ClientId>>(
        &self,
        client_ids: I,
        message: ProtocolMessage,
    ) -> Result<(), QuinnetError> {
        for id in client_ids {
            self.send_message(*id, message.clone())?;
        }
        Ok(())
    }

    pub fn try_send_group_message<'a, I: Iterator<Item = &'a ClientId>>(
        &self,
        client_ids: I,
        message: ProtocolMessage,
    ) {
        match self.send_group_message(client_ids, message) {
            Ok(_) => {}
            Err(err) => error!("try_send_group_message: {}", err),
        }
    }

    pub fn broadcast_message(&self, message: ProtocolMessage) -> Result<(), QuinnetError> {
        for (_, client_connection) in self.clients.iter() {
            match client_connection.sender.try_send(message.clone()) {
                Ok(_) => {}
                Err(err) => {
                    return match err {
                        mpsc::error::TrySendError::Full(_) => Err(QuinnetError::FullQueue),
                        mpsc::error::TrySendError::Closed(_) => Err(QuinnetError::ChannelClosed),
                    }
                }
            };
        }
        Ok(())
    }

    pub fn try_broadcast_message(&self, message: ProtocolMessage) {
        match self.broadcast_message(message) {
            Ok(_) => {}
            Err(err) => error!("try_broadcast_payload: {}", err),
        }
    }

    #[cfg(not(feature = "bevy"))]
    pub async fn next_event(&mut self) -> EndpointEvent {
        tokio::select! {
            biased;
            message = self.internal_receiver.recv(), if !self.internal_receiver_closed => {
                match message {
                    Some(InternalAsyncMessage::ClientConnected(connection)) => {
                        let id = connection.client_id;
                        self.clients.insert(id, connection);
                        EndpointEvent::Connect(id)
                    },
                    Some(InternalAsyncMessage::ClientLostConnection(client_id)) => {
                        self.clients.remove(&client_id);
                        EndpointEvent::Disconnect(client_id)
                    },
                    Some(InternalAsyncMessage::UnsupportedVersionMessage{client_id, version}) => {
                        EndpointEvent::UnsupportedVersionMessage{client_id, version}
                    },
                    None => {
                        self.internal_receiver_closed = true;
                        EndpointEvent::SocketClosed
                    }
                }
            },
            payload = self.payloads_receiver.recv() => {
                match payload {
                    Some(p) => EndpointEvent::Payload(Box::new(p)),
                    None => EndpointEvent::NoMorePayloads,
                }
            }
        }
    }

    pub fn receive_payload(&mut self) -> Result<Option<ClientPayload>, QuinnetError> {
        match self.payloads_receiver.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(err) => match err {
                TryRecvError::Empty => Ok(None),
                TryRecvError::Disconnected => Err(QuinnetError::ChannelClosed),
            },
        }
    }

    pub fn try_receive_payload(&mut self) -> Option<ClientPayload> {
        match self.receive_payload() {
            Ok(payload) => payload,
            Err(err) => {
                error!("try_receive_payload: {}", err);
                None
            }
        }
    }

    pub fn disconnect_client(&mut self, client_id: ClientId) -> Result<(), QuinnetError> {
        match self.clients.remove(&client_id) {
            Some(client_connection) => match client_connection.close_sender.send(()) {
                Ok(_) => Ok(()),
                Err(_) => Err(QuinnetError::ChannelClosed),
            },
            None => Err(QuinnetError::UnknownClient(client_id)),
        }
    }

    pub fn disconnect_all_clients(&mut self) -> Result<(), QuinnetError> {
        for client_id in self.clients.keys().cloned().collect::<Vec<ClientId>>() {
            self.disconnect_client(client_id)?;
        }
        Ok(())
    }

    pub(crate) fn close_incoming_connections_handler(&mut self) -> Result<(), QuinnetError> {
        match self.close_sender.send(()) {
            Ok(_) => Ok(()),
            Err(_) => Err(QuinnetError::ChannelClosed),
        }
    }
}

#[cfg_attr(feature = "bevy", derive(Resource))]
pub struct Server {
    runtime: runtime::Handle,
    endpoint: Option<Endpoint>,
}

impl Server {
    #[cfg(not(feature = "bevy"))]
    pub fn new_standalone() -> Server {
        Server {
            runtime: runtime::Handle::current(),
            endpoint: None,
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        self.endpoint.as_ref().unwrap()
    }

    pub fn endpoint_mut(&mut self) -> &mut Endpoint {
        self.endpoint.as_mut().unwrap()
    }

    pub fn get_endpoint(&self) -> Option<&Endpoint> {
        self.endpoint.as_ref()
    }

    pub fn get_endpoint_mut(&mut self) -> Option<&mut Endpoint> {
        self.endpoint.as_mut()
    }

    /// Run the server with the given [ServerConfigurationData] and [CertificateRetrievalMode]
    pub fn start_endpoint(
        &mut self,
        config: ServerConfigurationData,
        cert_mode: CertificateRetrievalMode,
    ) -> Result<ServerCertificate, QuinnetError> {
        let server_adr_str = format!("{}:{}", config.local_bind_host, config.port);
        let server_addr = server_adr_str
            .to_socket_addrs()?
            .next()
            .expect("Could not resolve host address");

        // Endpoint configuration
        let server_cert = retrieve_certificate(&config.host, cert_mode)?;
        let mut server_config = ServerConfig::with_single_cert(
            server_cert.cert_chain.clone(),
            server_cert.priv_key.clone(),
        )?;
        Arc::get_mut(&mut server_config.transport)
            .ok_or(QuinnetError::LockAcquisitionFailure)?
            .max_idle_timeout(Duration::from_secs(60).try_into().ok());

        let (from_clients_sender, from_clients_receiver) =
            mpsc::channel::<ClientPayload>(DEFAULT_MESSAGE_QUEUE_SIZE);
        let (to_sync_server, from_async_server) =
            mpsc::channel::<InternalAsyncMessage>(DEFAULT_INTERNAL_MESSAGE_CHANNEL_SIZE);
        // Create a close channel for this endpoint
        let (endpoint_close_sender, endpoint_close_receiver) =
            broadcast::channel(DEFAULT_KILL_MESSAGE_QUEUE_SIZE);

        info!("Starting endpoint on: {} ...", server_adr_str);

        self.runtime.spawn(async move {
            endpoint_task(
                server_config,
                server_addr,
                to_sync_server.clone(),
                endpoint_close_receiver,
                from_clients_sender.clone(),
            )
            .await;
        });

        self.endpoint = Some(Endpoint {
            clients: HashMap::new(),
            payloads_receiver: from_clients_receiver,
            close_sender: endpoint_close_sender,
            internal_receiver: from_async_server,
            #[cfg(not(feature = "bevy"))]
            internal_receiver_closed: false,
        });

        Ok(server_cert)
    }

    pub fn stop_endpoint(&mut self) -> Result<(), QuinnetError> {
        match self.endpoint.take() {
            Some(mut endpoint) => {
                endpoint.close_incoming_connections_handler()?;
                endpoint.disconnect_all_clients()
            }
            None => Err(QuinnetError::EndpointAlreadyClosed),
        }
    }

    /// Returns true if the server is currently listening for messages and connections.
    pub fn is_listening(&self) -> bool {
        self.endpoint.is_some()
    }
}

async fn endpoint_task(
    endpoint_config: ServerConfig,
    endpoint_adr: SocketAddr,
    to_sync_server: mpsc::Sender<InternalAsyncMessage>,
    mut close_receiver: broadcast::Receiver<()>,
    from_clients_sender: mpsc::Sender<ClientPayload>,
) {
    let mut client_gen_id: ClientId = 0;
    let mut client_id_mappings = HashMap::new();

    let endpoint = QuinnEndpoint::server(endpoint_config, endpoint_adr)
        .expect("Failed to create the endpoint");

    // Handle incoming connections/clients.
    tokio::select! {
        _ = close_receiver.recv() => {
            trace!("Endpoint incoming connection handler received a request to close")
        }
        _ = async {
            while let Some(connecting) = endpoint.accept().await {
                match connecting.await {
                    Err(err) => error!("An incoming connection failed: {}", err),
                    Ok(connection) => {
                        client_gen_id += 1; // TODO Fix: Better id generation/check
                        let client_id = client_gen_id;
                        client_id_mappings.insert(connection.stable_id(), client_id);

                        handle_client_connection(
                            connection,
                            client_id,
                            &to_sync_server,
                            from_clients_sender.clone(),
                        )
                        .await;
                    },
                }

            }
        } => {}
    }
}

async fn handle_client_connection(
    connection: quinn::Connection,
    client_id: ClientId,
    to_sync_server: &mpsc::Sender<InternalAsyncMessage>,
    from_clients_sender: mpsc::Sender<ClientPayload>,
) {
    info!(
        "New connection from {}, client_id: {}, stable_id : {}",
        connection.remote_address(),
        client_id,
        connection.stable_id()
    );

    // Create a close channel for this client
    let (client_close_sender, client_close_receiver) =
        broadcast::channel(DEFAULT_KILL_MESSAGE_QUEUE_SIZE);

    // Create an ordered reliable send channel for this client
    let (to_client_sender, to_client_receiver) =
        mpsc::channel::<ProtocolMessage>(DEFAULT_MESSAGE_QUEUE_SIZE);

    let to_sync_server_clone_for_sender_task = to_sync_server.clone();
    let to_sync_server_clone_for_receiver_task = to_sync_server.clone();
    let close_sender_for_sender_task = client_close_sender.clone();
    let close_sender_for_receiver_task = client_close_sender.clone();

    tokio::spawn(async move {
        if let Ok((send_stream, recv_stream)) = connection.accept_bi().await {
            tokio::spawn(async move {
                client_sender_task(
                    client_id,
                    send_stream,
                    to_client_receiver,
                    client_close_receiver,
                    close_sender_for_sender_task,
                    to_sync_server_clone_for_sender_task,
                )
                .await
            });

            tokio::spawn(async move {
                client_receiver_task(
                    client_id,
                    recv_stream,
                    close_sender_for_receiver_task.subscribe(),
                    close_sender_for_receiver_task,
                    from_clients_sender,
                    to_sync_server_clone_for_receiver_task,
                )
                .await
            });
        }
    });

    // Signal the sync server of this new connection
    to_sync_server
        .send(InternalAsyncMessage::ClientConnected(ClientConnection {
            client_id,
            sender: to_client_sender,
            close_sender: client_close_sender.clone(),
        }))
        .await
        .expect("Failed to signal connection to sync client");
}

async fn client_sender_task(
    client_id: ClientId,
    send_stream: quinn::SendStream,
    mut to_client_receiver: mpsc::Receiver<ProtocolMessage>,
    mut close_receiver: broadcast::Receiver<()>,
    close_sender: broadcast::Sender<()>,
    to_sync_server: mpsc::Sender<InternalAsyncMessage>,
) {
    let mut framed_send_stream = FramedWrite::new(send_stream, BattleshipPlusCodec::default());

    tokio::select! {
        _ = close_receiver.recv() => {
            trace!("Sending half of stream forced to disconnect for client: {}", client_id)
        }
        _ = async {
            while let Some(message) = to_client_receiver.recv().await {
                // TODO Perf: Batch frames for a send_all
                // TODO Clean: Error handling
                if let Err(err) = framed_send_stream.send(message.clone()).await {
                    error!("Error while sending to client {}: {}", client_id, err);
                    error!("Client {} seems disconnected, closing resources", client_id);
                    if close_sender.send(()).is_err() {
                        error!("Failed to close all client streams & resources for client {}", client_id)
                    }
                };
            }
        } => {}
    }
    trace!("Sending half of stream closed for client: {}", client_id);
    if let Err(e) = to_sync_server
        .send(InternalAsyncMessage::ClientLostConnection(client_id))
        .await
    {
        debug!("Failed to signal connection lost to sync server: {e}")
    }
}

async fn client_receiver_task(
    client_id: ClientId,
    recv_stream: quinn::RecvStream,
    mut close_receiver: broadcast::Receiver<()>,
    close_sender: broadcast::Sender<()>,
    from_clients_sender: mpsc::Sender<ClientPayload>,
    to_sync_server: mpsc::Sender<InternalAsyncMessage>,
) {
    tokio::select! {
        _ = close_receiver.recv() => {
            trace!("Receiving half of stream forced to disconnect for client: {}", client_id)
        }
        _ = async {
            let mut frame_recv = FramedRead::new(recv_stream, BattleshipPlusCodec::default());

            // Spawn a task to receive data on this stream.
            let from_client_sender = from_clients_sender.clone();
            while let Some(result) = frame_recv.next().await {
                match result {
                    Ok(message) => {
                        from_client_sender
                            .send(ClientPayload {
                                client_id,
                                msg: message,
                            })
                            .await
                            .unwrap();// TODO Fix: error event
                    }
                    Err(CodecError::UnsupportedVersion(version)) => {
                        to_sync_server
                            .send(InternalAsyncMessage::UnsupportedVersionMessage{client_id, version})
                            .await
                            .expect("Failed to signal message with unsupported version to sync server");
                    }
                    Err(_other_error) => {
                        break;
                    }
                }
            }
            trace!("Receiving half of stream ended for client: {}", client_id)
        } => {}
    }
    trace!("Receiving half of stream closed for client: {}", client_id);
    if close_sender.send(()).is_err() {
        error!(
            "Failed to close all client streams & resources for client {}",
            client_id
        );

        // per default the writer task should notify the sync server but now we could not reach it
        // so we notify the close on our own behalf.
        let _ = to_sync_server
            .send(InternalAsyncMessage::ClientLostConnection(client_id))
            .await;
    }
}

#[cfg(feature = "bevy")]
fn create_server(mut commands: Commands, runtime: Res<AsyncRuntime>) {
    commands.insert_resource(Server {
        endpoint: None,
        runtime: runtime.handle().clone(),
    });
}

#[cfg(feature = "bevy")]
// Receive messages from the async server tasks and update the sync server.
fn update_sync_server(
    mut server: ResMut<Server>,
    mut connection_events: EventWriter<ConnectionEvent>,
    mut connection_lost_events: EventWriter<ConnectionLostEvent>,
) {
    if let Some(endpoint) = server.get_endpoint_mut() {
        while let Ok(message) = endpoint.internal_receiver.try_recv() {
            match message {
                InternalAsyncMessage::ClientConnected(connection) => {
                    let id = connection.client_id;
                    endpoint.clients.insert(id, connection);
                    connection_events.send(ConnectionEvent { id });
                }
                InternalAsyncMessage::ClientLostConnection(client_id) => {
                    endpoint.clients.remove(&client_id);
                    connection_lost_events.send(ConnectionLostEvent { id: client_id });
                }
                InternalAsyncMessage::UnsupportedVersionMessage { client_id, version } => {
                    warn!("received message with unsupported version {version} on connection {client_id}")
                }
            }
        }
    }
}

#[derive(Default)]
#[cfg(feature = "bevy")]
pub struct QuinnetServerPlugin {}

#[cfg(feature = "bevy")]
impl Plugin for QuinnetServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ConnectionEvent>()
            .add_event::<ConnectionLostEvent>()
            .add_startup_system_to_stage(StartupStage::PreStartup, create_server)
            .add_system_to_stage(CoreStage::PreUpdate, update_sync_server);

        if app.world.get_resource_mut::<AsyncRuntime>().is_none() {
            app.insert_resource(AsyncRuntime(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            ));
        }
    }
}

#[cfg(not(feature = "bevy"))]
#[derive(Debug)]
pub enum EndpointEvent {
    Payload(Box<ClientPayload>),
    Connect(ClientId),
    Disconnect(ClientId),
    UnsupportedVersionMessage { client_id: ClientId, version: u8 },
    SocketClosed,
    NoMorePayloads,
}
