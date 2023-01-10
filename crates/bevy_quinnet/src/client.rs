use std::string::ToString;
use std::{
    collections::{
        hash_map::{Iter, IterMut},
        HashMap,
    },
    error::Error,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
};

#[cfg(feature = "bevy")]
use bevy::prelude::*;
use futures::sink::SinkExt;
use futures_util::StreamExt;
#[cfg(not(feature = "bevy"))]
use log::{error, info, trace, warn};
use once_cell::sync::Lazy;
use quinn::{ClientConfig, Endpoint};
use rustls::KeyLogFile;
#[cfg(feature = "bevy")]
use serde::Deserialize;
use tokio::sync::broadcast::error::SendError;
use tokio::sync::broadcast::Sender;
use tokio::{
    runtime,
    sync::{
        broadcast,
        mpsc::{
            self,
            error::{TryRecvError, TrySendError},
        },
        oneshot,
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

use battleship_plus_common::{codec::BattleshipPlusCodec, messages::ProtocolMessage};

#[cfg(feature = "bevy")]
use crate::shared::AsyncRuntime;
use crate::shared::{QuinnetError, DEFAULT_KILL_MESSAGE_QUEUE_SIZE, DEFAULT_MESSAGE_QUEUE_SIZE};

use self::certificate::{
    load_known_hosts_store_from_config, CertConnectionAbortEvent, CertInteractionEvent,
    CertTrustUpdateEvent, CertVerificationInfo, CertVerificationStatus, CertVerifierAction,
    CertificateVerificationMode, SkipServerVerification, TofuServerVerification,
};

pub mod certificate;

pub const DEFAULT_INTERNAL_MESSAGE_CHANNEL_SIZE: usize = 100;

pub type ProtectedString = Arc<Mutex<String>>;

pub static DEFAULT_KNOWN_HOSTS_FILE: Lazy<ProtectedString> =
    Lazy::new(|| Arc::new(Mutex::new("quinnet/known_hosts".to_string())));

pub type ConnectionId = u64;

/// Connection event raised when the client just connected to the server. Raised in the CoreStage::PreUpdate stage.
#[derive(Debug, Copy, Clone)]
pub struct ConnectionEvent(pub ConnectionId);

/// ConnectionLost event raised when the client is considered disconnected from the server. Raised in the CoreStage::PreUpdate stage.
#[derive(Debug, Copy, Clone)]
pub struct ConnectionLostEvent(pub ConnectionId);

/// ConnectionErrorEvent event raised when the client failed to connect to the server.
#[derive(Debug, Clone)]
pub struct ConnectionErrorEvent(pub ConnectionId, pub String);

/// Configuration of the client, used when connecting to a server
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bevy", derive(Deserialize))]
pub struct ConnectionConfiguration {
    server_host: String,
    server_scope: Option<u32>,
    server_port: u16,
    local_bind_host: String,
    local_bind_port: u16,
}

impl ConnectionConfiguration {
    /// Creates a new ConnectionConfiguration
    ///
    /// # Arguments
    ///
    /// * `server_host` - Address of the server
    /// * `server_scope` - Scope ID that the server is listening on. Only relevant for
    /// IPv6 link-local connections.
    /// * `server_port` - Port that the server is listening on
    /// * `local_bind_host` - Local address to bind to, which should usually be a wildcard address like `0.0.0.0` or `[::]`, which allow communication with any reachable IPv4 or IPv6 address. See [`Endpoint`] for more precision
    /// * `local_bind_port` - Local port to bind to. Use 0 to get an OS-assigned port.. See [`Endpoint`] for more precision
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy_quinnet::client::ConnectionConfiguration;
    ///
    /// let config = ConnectionConfiguration::new(
    ///         "127.0.0.1".to_string(),
    ///         None,
    ///         6000,
    ///         "0.0.0.0".to_string(),
    ///         0,
    ///     );
    /// ```
    pub fn new(
        server_host: String,
        server_scope: Option<u32>,
        server_port: u16,
        local_bind_host: String,
        local_bind_port: u16,
    ) -> Self {
        Self {
            server_host,
            server_scope,
            server_port,
            local_bind_host,
            local_bind_port,
        }
    }
}

/// Current state of the client driver
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ConnectionState {
    Disconnected,
    Connected,
}

#[derive(Debug)]
pub(crate) enum InternalAsyncMessage {
    Connected,
    LostConnection,
    CertificateInteractionRequest {
        status: CertVerificationStatus,
        info: CertVerificationInfo,
        action_sender: oneshot::Sender<CertVerifierAction>,
    },
    CertificateTrustUpdate(CertVerificationInfo),
    CertificateConnectionAbort {
        status: CertVerificationStatus,
        cert_info: CertVerificationInfo,
    },
    ConnectionError(String),
}

#[derive(Debug)]
pub(crate) struct ConnectionSpawnConfig {
    connection_config: ConnectionConfiguration,
    cert_mode: CertificateVerificationMode,
    to_sync_client: mpsc::Sender<InternalAsyncMessage>,
    close_sender: broadcast::Sender<()>,
    close_receiver: broadcast::Receiver<()>,
    to_server_receiver: mpsc::Receiver<ProtocolMessage>,
    from_server_sender: mpsc::Sender<Option<ProtocolMessage>>,
}

#[derive(Debug)]
pub struct Connection {
    state: ConnectionState,
    // TODO Perf: multiple channels
    sender: mpsc::Sender<ProtocolMessage>,
    receiver: mpsc::Receiver<Option<ProtocolMessage>>,
    close_sender: broadcast::Sender<()>,
    pub(crate) internal_receiver: mpsc::Receiver<InternalAsyncMessage>,
}

impl Connection {
    pub fn send_message(&self, message: ProtocolMessage) -> Result<(), QuinnetError> {
        match self.sender.try_send(message) {
            Ok(_) => Ok(()),
            Err(err) => match err {
                TrySendError::Full(_) => Err(QuinnetError::FullQueue),
                TrySendError::Closed(_) => Err(QuinnetError::ChannelClosed),
            },
        }
    }

    /// Same as [Connection::send_message] but will log the error instead of returning it
    pub fn try_send_message(&self, message: ProtocolMessage) {
        match self.send_message(message) {
            Ok(_) => {}
            Err(err) => error!("try_send_message: {}", err),
        }
    }

    pub fn receive_message(&mut self) -> Result<Option<Option<ProtocolMessage>>, QuinnetError> {
        match self.receiver.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(err) => match err {
                TryRecvError::Empty => Ok(None),
                TryRecvError::Disconnected => Err(QuinnetError::ChannelClosed),
            },
        }
    }

    /// Same as [Connection::receive_message] but will log the error instead of returning it
    pub fn try_receive_message(&mut self) -> Option<Option<ProtocolMessage>> {
        match self.receive_message() {
            Ok(msg) => msg,
            Err(err) => {
                error!("try_receive_message: {}", err);
                None
            }
        }
    }

    /// Disconnect from the server on this connection. This does not send any message to the server, and simply closes all the connection's tasks locally.
    fn disconnect(&mut self) -> Result<(), QuinnetError> {
        let close_send_result = self.close_sender.send(());
        if self.is_connected() && close_send_result.is_err() {
            return Err(QuinnetError::ChannelClosed);
        }
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
}

#[cfg_attr(feature = "bevy", derive(Resource))]
pub struct Client {
    runtime: runtime::Handle,
    connections: HashMap<ConnectionId, Connection>,
    last_gen_id: ConnectionId,
    default_connection_id: Option<ConnectionId>,

    #[cfg(not(feature = "bevy"))]
    event_tx: broadcast::Sender<QuinnetClientEvent>,
    #[cfg(not(feature = "bevy"))]
    certificate_interaction_tx: broadcast::Sender<CertInteractionEvent>,
}

impl Client {
    #[cfg(not(feature = "bevy"))]
    pub fn new_standalone(
        event_tx: Sender<QuinnetClientEvent>,
        certificate_interaction_tx: Sender<CertInteractionEvent>,
    ) -> Self {
        Client {
            connections: HashMap::new(),
            runtime: runtime::Handle::current(),
            last_gen_id: 0,
            default_connection_id: None,
            event_tx,
            certificate_interaction_tx,
        }
    }

    /// Returns the default connection or None.
    pub fn get_connection(&self) -> Option<&Connection> {
        match self.default_connection_id {
            Some(id) => self.connections.get(&id),
            None => None,
        }
    }

    /// Returns the default connection as mut or None.
    pub fn get_connection_mut(&mut self) -> Option<&mut Connection> {
        match self.default_connection_id {
            Some(id) => self.connections.get_mut(&id),
            None => None,
        }
    }

    /// Returns the default connection. **Warning**, this function panics if there is no default connection.
    pub fn connection(&self) -> &Connection {
        self.connections
            .get(&self.default_connection_id.unwrap())
            .unwrap()
    }

    /// Returns the default connection as mut. **Warning**, this function panics if there is no default connection.
    pub fn connection_mut(&mut self) -> &mut Connection {
        self.connections
            .get_mut(&self.default_connection_id.unwrap())
            .unwrap()
    }

    /// Returns the requested connection.
    pub fn get_connection_by_id(&self, id: ConnectionId) -> Option<&Connection> {
        self.connections.get(&id)
    }

    /// Returns the requested connection as mut.
    pub fn get_connection_mut_by_id(&mut self, id: ConnectionId) -> Option<&mut Connection> {
        self.connections.get_mut(&id)
    }

    /// Returns an iterator over all connections
    pub fn connections(&self) -> Iter<ConnectionId, Connection> {
        self.connections.iter()
    }

    /// Returns an iterator over all connections as muts
    pub fn connections_mut(&mut self) -> IterMut<ConnectionId, Connection> {
        self.connections.iter_mut()
    }

    /// Open a connection to a server with the given [ConnectionConfiguration] and [CertificateVerificationMode]. The connection will raise an event when fully connected, see [ConnectionEvent]
    pub fn open_connection(
        &mut self,
        config: ConnectionConfiguration,
        cert_mode: CertificateVerificationMode,
    ) -> ConnectionId {
        let (from_server_sender, from_server_receiver) =
            mpsc::channel::<Option<ProtocolMessage>>(DEFAULT_MESSAGE_QUEUE_SIZE);
        let (to_server_sender, to_server_receiver) =
            mpsc::channel::<ProtocolMessage>(DEFAULT_MESSAGE_QUEUE_SIZE);

        let (to_sync_client, from_async_client) =
            mpsc::channel::<InternalAsyncMessage>(DEFAULT_INTERNAL_MESSAGE_CHANNEL_SIZE);

        // Create a close channel for this connection
        let (close_sender, close_receiver): (broadcast::Sender<()>, broadcast::Receiver<()>) =
            broadcast::channel(DEFAULT_KILL_MESSAGE_QUEUE_SIZE);

        let connection = Connection {
            state: ConnectionState::Disconnected,
            sender: to_server_sender,
            receiver: from_server_receiver,
            close_sender: close_sender.clone(),
            internal_receiver: from_async_client,
        };

        // Async connection
        self.runtime.spawn(async move {
            connection_task(ConnectionSpawnConfig {
                connection_config: config,
                cert_mode,
                to_sync_client,
                close_sender,
                close_receiver,
                to_server_receiver,
                from_server_sender,
            })
            .await
        });

        self.last_gen_id += 1;
        let connection_id = self.last_gen_id;
        self.connections.insert(connection_id, connection);
        if self.default_connection_id.is_none() {
            self.default_connection_id = Some(connection_id);
        }

        connection_id
    }

    /// Set the default connection
    pub fn set_default_connection(&mut self, connection_id: ConnectionId) {
        self.default_connection_id = Some(connection_id);
    }

    /// Get the default Connection Id
    pub fn get_default_connection(&self) -> Option<ConnectionId> {
        self.default_connection_id
    }

    /// Close a specific connection. This will call disconnect on the connection and remove it from the client. This may fail if the [Connection] fails to disconnect or if no [Connection] if found for connection_id
    pub fn close_connection(&mut self, connection_id: ConnectionId) -> Result<(), QuinnetError> {
        match self.connections.remove(&connection_id) {
            Some(mut connection) => Self::close_connection_inner(
                &mut connection,
                connection_id,
                &mut self.default_connection_id,
            ),
            None => Err(QuinnetError::UnknownConnection(connection_id)),
        }
    }

    fn close_connection_inner(
        connection: &mut Connection,
        connection_id: ConnectionId,
        default_id: &mut Option<ConnectionId>,
    ) -> Result<(), QuinnetError> {
        connection.disconnect()?;
        if let Some(id) = default_id {
            if connection_id == *id {
                *default_id = None;
            }
        };
        Ok(())
    }

    /// Calls close_connection on all the open connections.
    pub fn close_all_connections(&mut self) -> Result<(), QuinnetError> {
        for connection_id in self
            .connections
            .keys()
            .cloned()
            .collect::<Vec<ConnectionId>>()
        {
            self.close_connection(connection_id)?;
        }
        Ok(())
    }

    #[cfg(not(feature = "bevy"))]
    // Receive messages from the async client tasks and update the sync client.
    pub fn update_client(&mut self) {
        for (&connection_id, connection) in &mut self.connections {
            while let Ok(message) = connection.internal_receiver.try_recv() {
                if let Err(_) = match message {
                    InternalAsyncMessage::Connected => {
                        connection.state = ConnectionState::Connected;
                        self.event_tx
                            .send(QuinnetClientEvent::Connected(ConnectionEvent(
                                connection_id,
                            )))
                    }
                    InternalAsyncMessage::LostConnection => {
                        connection.state = ConnectionState::Disconnected;
                        self.event_tx
                            .send(QuinnetClientEvent::ConnectionLost(ConnectionLostEvent(
                                connection_id,
                            )))
                    }
                    InternalAsyncMessage::ConnectionError(message) => {
                        trace!("Closing errored connection {connection_id}");
                        if let Err(error) = Self::close_connection_inner(
                            connection,
                            connection_id,
                            &mut self.default_connection_id,
                        ) {
                            warn!("Could not properly close errored connection {connection_id}: {error}");
                        }

                        self.event_tx.send(QuinnetClientEvent::ConnectionError(
                            ConnectionErrorEvent(connection_id, message),
                        ))
                    }
                    InternalAsyncMessage::CertificateTrustUpdate(info) => self.event_tx.send(
                        QuinnetClientEvent::CertTrustUpdate(CertTrustUpdateEvent {
                            connection_id,
                            cert_info: info,
                        }),
                    ),
                    InternalAsyncMessage::CertificateConnectionAbort { status, cert_info } => {
                        self.event_tx.send(QuinnetClientEvent::CertConnectionAbort(
                            CertConnectionAbortEvent {
                                connection_id,
                                status,
                                cert_info,
                            },
                        ))
                    }
                    InternalAsyncMessage::CertificateInteractionRequest {
                        status,
                        info,
                        action_sender,
                    } => {
                        if let Err(SendError(event)) =
                            self.certificate_interaction_tx.send(CertInteractionEvent {
                                connection_id,
                                status,
                                info,
                                action_sender: Arc::new(Mutex::new(Some(action_sender))),
                            })
                        {
                            // in case there are no Receivers for CertificateInteractionRequests, we abort
                            let _ = event
                                .action_sender
                                .lock()
                                .expect("unable to acquire lock on action_sender")
                                .take()
                                .expect("action_sender should be present")
                                .send(CertVerifierAction::AbortConnection);
                        }

                        Ok(0)
                    }
                } {
                    warn!("there are no client event receivers left");
                }
            }
        }
    }
}

fn configure_client(
    cert_mode: CertificateVerificationMode,
    to_sync_client: mpsc::Sender<InternalAsyncMessage>,
) -> Result<ClientConfig, Box<dyn Error>> {
    match cert_mode {
        CertificateVerificationMode::SkipVerification => {
            let mut crypto = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth();
            if cfg!(debug_assertions) {
                crypto.key_log = Arc::new(KeyLogFile::new());
            }

            Ok(ClientConfig::new(Arc::new(crypto)))
        }
        CertificateVerificationMode::SignedByCertificateAuthority => {
            // Taken from quinn::ClientConfig::with_native_roots()
            // VVV
            let mut roots = rustls::RootCertStore::empty();

            match rustls_native_certs::load_native_certs() {
                Ok(certs) => {
                    for cert in certs {
                        if let Err(e) = roots.add(&rustls::Certificate(cert.0)) {
                            warn!("failed to parse trust anchor: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("couldn't load any default trust roots: {}", e);
                }
            };

            let mut crypto = rustls::ClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&rustls::version::TLS13])
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
            crypto.enable_early_data = true;
            // ^^^
            // Taken from quinn::ClientConfig::with_native_roots()

            if cfg!(debug_assertions) {
                crypto.key_log = Arc::new(KeyLogFile::new());
            }

            Ok(ClientConfig::new(Arc::new(crypto)))
        }
        CertificateVerificationMode::TrustOnFirstUse(config) => {
            let (store, store_file) = load_known_hosts_store_from_config(config.known_hosts)?;
            let mut crypto = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_custom_certificate_verifier(TofuServerVerification::new(
                    store,
                    config.verifier_behaviour,
                    to_sync_client,
                    store_file,
                ))
                .with_no_client_auth();
            if cfg!(debug_assertions) {
                crypto.key_log = Arc::new(KeyLogFile::new());
            }

            Ok(ClientConfig::new(Arc::new(crypto)))
        }
    }
    .map(|mut config| {
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_idle_timeout(None);
        transport_config.keep_alive_interval(Some(core::time::Duration::from_secs(30)));
        config.transport_config(Arc::new(transport_config));
        config
    })
}

async fn connection_task(mut spawn_config: ConnectionSpawnConfig) {
    let config = spawn_config.connection_config;
    let server_adr_str = format!("{}:{}", config.server_host, config.server_port);
    let srv_host = config.server_host.clone();
    let local_bind_adr = format!("{}:{}", config.local_bind_host, config.local_bind_port);

    info!("Trying to connect to server on: {} ...", server_adr_str);

    let mut server_addresses = match server_adr_str.to_socket_addrs() {
        Ok(server_addresses) => server_addresses,
        Err(error) => {
            spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::ConnectionError(format!(
                    "Failed to parse server address: {error}"
                )))
                .await
                .expect("Failed to signal connection error to sync client");
            return;
        }
    };

    let server_address =
        // Prefer IPv6 addresses, if available.
        if let Some(mut server_address) = server_addresses.clone().find_map(|socket_address| {if let SocketAddr::V6(socket_address) = socket_address {Some(socket_address)} else {None}}) {
            // Specify scope, if provided.
            if let Some(scope) = config.server_scope {
                server_address.set_scope_id(scope);
            }
            SocketAddr::V6(server_address)
        } else {
            match server_addresses.next() {
                Some(server_address) => server_address,
                None => {
                    spawn_config
                        .to_sync_client
                        .send(InternalAsyncMessage::ConnectionError("Failed to resolve server address".to_string()))
                        .await
                        .expect("Failed to signal connection error to sync client");
                    return;
                }
            }
        };

    let configuration_result =
        configure_client(spawn_config.cert_mode, spawn_config.to_sync_client.clone())
            .map_err(|x| format!("{x}"));
    let client_cfg = match configuration_result {
        Ok(client_cfg) => client_cfg,
        Err(error) => {
            spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::ConnectionError(format!(
                    "Failed to configure client: {error}"
                )))
                .await
                .expect("Failed to signal connection error to sync client");
            return;
        }
    };

    let mut endpoint = match Endpoint::client(local_bind_adr.parse().unwrap()) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::ConnectionError(format!(
                    "Failed to create client endpoint: {error}"
                )))
                .await
                .expect("Failed to signal connection error to sync client");
            return;
        }
    };
    endpoint.set_default_client_config(client_cfg);

    let connection = match endpoint.connect(server_address, &srv_host) {
        Ok(connection) => connection.await,
        Err(error) => {
            spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::ConnectionError(format!(
                    "Failed to connect: {error}"
                )))
                .await
                .expect("Failed to signal connection error to sync client");
            return;
        }
    };

    match connection {
        Err(error) => {
            spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::ConnectionError(format!(
                    "Error while connecting: {error}"
                )))
                .await
                .expect("Failed to signal connection error to sync client");
        }
        Ok(connection) => {
            info!("Connected to {}", connection.remote_address());

            if let Err(error) = spawn_config
                .to_sync_client
                .send(InternalAsyncMessage::Connected)
                .await
            {
                if spawn_config.close_receiver.is_empty() {
                    // No close requested but internal channel closed.
                    error!("Failed to signal connection to sync client with error: {error}");
                }
                return;
            }

            let (send, recv) = match connection.open_bi().await {
                Ok(stream) => stream,
                Err(error) => {
                    spawn_config
                        .to_sync_client
                        .send(InternalAsyncMessage::ConnectionError(format!(
                            "Failed to open bidirectional stream: {error}"
                        )))
                        .await
                        .expect("Failed to signal connection error to sync client");
                    return;
                }
            };

            let mut frame_send = FramedWrite::new(send, BattleshipPlusCodec::default());

            let close_sender_clone = spawn_config.close_sender.clone();
            let _network_sends = tokio::spawn(async move {
                tokio::select! {
                    _ = spawn_config.close_receiver.recv() => {
                        trace!("Sending half of stream forced to disconnect")
                    }
                    _ = async {
                        while let Some(msg_bytes) = spawn_config.to_server_receiver.recv().await {
                            if let Err(err) = frame_send.send(msg_bytes).await {
                                error!("Error while sending, {}", err); // TODO Clean: error handling
                                error!("Client seems disconnected, closing resources");
                                if close_sender_clone.send(()).is_err() {
                                    error!("Failed to close all client streams & resources")
                                }
                                spawn_config.to_sync_client.send(
                                    InternalAsyncMessage::LostConnection)
                                    .await
                                    .expect("Failed to signal connection lost to sync client");
                            }
                        }
                        trace!("Sending half of stream ended")
                    } => {}
                }
                trace!("Sending half of stream closed")
            });

            let mut close_receiver = spawn_config.close_sender.subscribe();
            let _network_reads = tokio::spawn(async move {
                tokio::select! {
                    _ = close_receiver.recv() => {
                        trace!("Receiving half of stream forced to disconnect")
                    }
                    _ = async {
                        let mut frame_recv = FramedRead::new(recv, BattleshipPlusCodec::default());
                        let from_server_sender = spawn_config.from_server_sender.clone();

                        while let Some(Ok(msg_bytes)) = frame_recv.next().await {
                            from_server_sender.send(msg_bytes).await.unwrap(); // TODO Clean: error handling
                        }

                        trace!("Receiving half of stream ended")
                    } => {}
                }
                trace!("Receiving half of stream closed")
            });
        }
    }
}

#[cfg(not(feature = "bevy"))]
#[derive(Debug, Clone)]
pub enum QuinnetClientEvent {
    Connected(ConnectionEvent),
    ConnectionError(ConnectionErrorEvent),
    ConnectionLost(ConnectionLostEvent),
    CertTrustUpdate(CertTrustUpdateEvent),
    CertConnectionAbort(CertConnectionAbortEvent),
}

#[cfg(feature = "bevy")]
// Receive messages from the async client tasks and update the sync client.
fn update_sync_client(
    mut connection_events: EventWriter<ConnectionEvent>,
    mut connection_error_events: EventWriter<ConnectionErrorEvent>,
    mut connection_lost_events: EventWriter<ConnectionLostEvent>,
    mut certificate_interaction_events: EventWriter<CertInteractionEvent>,
    mut cert_trust_update_events: EventWriter<CertTrustUpdateEvent>,
    mut cert_connection_abort_events: EventWriter<CertConnectionAbortEvent>,
    mut client: ResMut<Client>,
) {
    let mut errored_connections = Vec::new();
    for (connection_id, mut connection) in &mut client.connections {
        while let Ok(message) = connection.internal_receiver.try_recv() {
            match message {
                InternalAsyncMessage::Connected => {
                    connection.state = ConnectionState::Connected;
                    connection_events.send(ConnectionEvent(*connection_id));
                }
                InternalAsyncMessage::LostConnection => {
                    connection.state = ConnectionState::Disconnected;
                    connection_lost_events.send(ConnectionLostEvent(*connection_id));
                }
                InternalAsyncMessage::CertificateInteractionRequest {
                    status,
                    info,
                    action_sender,
                } => {
                    certificate_interaction_events.send(CertInteractionEvent {
                        connection_id: *connection_id,
                        status,
                        info,
                        action_sender: Arc::new(Mutex::new(Some(action_sender))),
                    });
                }
                InternalAsyncMessage::CertificateTrustUpdate(info) => {
                    cert_trust_update_events.send(CertTrustUpdateEvent {
                        connection_id: *connection_id,
                        cert_info: info,
                    });
                }
                InternalAsyncMessage::CertificateConnectionAbort { status, cert_info } => {
                    cert_connection_abort_events.send(CertConnectionAbortEvent {
                        connection_id: *connection_id,
                        status,
                        cert_info,
                    });
                }
                InternalAsyncMessage::ConnectionError(message) => {
                    connection_error_events.send(ConnectionErrorEvent(*connection_id, message));
                    errored_connections.push(*connection_id);
                }
            }
        }
    }

    // Close errored connections.
    for connection_id in errored_connections {
        trace!("Closing errored connection {connection_id}");
        if let Err(error) = client.close_connection(connection_id) {
            warn!("Could not properly close errored connection {connection_id}: {error}");
        }
    }
}

#[cfg(feature = "bevy")]
fn create_client(mut commands: Commands, runtime: Res<AsyncRuntime>) {
    commands.insert_resource(Client {
        connections: HashMap::new(),
        runtime: runtime.handle().clone(),
        last_gen_id: 0,
        default_connection_id: None,
    });
}

#[cfg(feature = "bevy")]
#[derive(Default)]
pub struct QuinnetClientPlugin {}

#[cfg(feature = "bevy")]
impl Plugin for QuinnetClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ConnectionEvent>()
            .add_event::<ConnectionLostEvent>()
            .add_event::<CertInteractionEvent>()
            .add_event::<CertTrustUpdateEvent>()
            .add_event::<CertConnectionAbortEvent>()
            .add_event::<ConnectionErrorEvent>()
            // StartupStage::PreStartup so that resources created in commands are available to default startup_systems
            .add_startup_system_to_stage(StartupStage::PreStartup, create_client)
            .add_system_to_stage(CoreStage::PreUpdate, update_sync_client);

        if app.world.get_resource_mut::<AsyncRuntime>().is_none() {
            app.insert_resource(AsyncRuntime(
                runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            ));
        }
    }
}
