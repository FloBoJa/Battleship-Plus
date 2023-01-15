use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use quinn::{crypto, ClientConfig, Connection, Endpoint, RecvStream, SendStream, VarInt};
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};

use battleship_plus_common::codec::BattleshipPlusCodec;
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{
    JoinRequest, JoinResponse, LobbyChangeEvent, PlacementPhase, ProtocolMessage,
    SetReadyStateRequest, SetReadyStateResponse, StatusCode, StatusMessage, TeamSwitchRequest,
    TeamSwitchResponse,
};
use battleship_plus_common::types::{Coordinate, PlayerLobbyState};
use battleship_plus_common::{protocol_name, protocol_name_with_version};

use crate::config_provider::{default_config_provider, ConfigProvider};
use crate::game::data::PlayerID;
use crate::server::spawn_server_task;

type TestLock = Arc<Mutex<()>>;

static TEST_LOCK: Lazy<TestLock> = Lazy::new(|| Arc::new(Mutex::new(())));

#[derive(Debug, Copy, Clone)]
enum Team {
    A,
    B,
}

struct Client {
    connection: Connection,
    reader: FramedRead<RecvStream, BattleshipPlusCodec>,
    writer: FramedWrite<SendStream, BattleshipPlusCodec>,
    state: PlayerLobbyState,
    team: Team,
}

impl Client {
    async fn send(&mut self, msg: ProtocolMessage) {
        Self::send_msg_inner(&mut self.writer, msg).await
    }

    async fn receive(&mut self) -> ProtocolMessage {
        Self::receive_msg_inner(&mut self.reader).await
    }

    async fn switch_team(&mut self) {
        self.send(TeamSwitchRequest::default().into()).await;
        let resp = self.receive().await;
        match resp {
            ProtocolMessage::StatusMessage(StatusMessage {
                code,
                data: Some(Data::TeamSwitchResponse(TeamSwitchResponse {})),
                ..
            }) => {
                assert_eq!(StatusCode::from_i32(code), Some(StatusCode::Ok));
                self.team = match self.team {
                    Team::A => Team::B,
                    Team::B => Team::A,
                };
            }
            _ => panic!("Expected TeamSwitchResponse, got {resp:#?}"),
        }
    }

    async fn switch_team_check_broadcasts(
        broadcast_receiver: impl Iterator<Item = &mut Client>,
        assert_team_a_count: usize,
        assert_team_b_count: usize,
    ) {
        for c in broadcast_receiver {
            let msg = c.receive().await;
            match msg {
                ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
                    team_state_a,
                    team_state_b,
                }) => {
                    assert_eq!(team_state_a.len(), assert_team_a_count);
                    assert_eq!(team_state_b.len(), assert_team_b_count);
                }
                _ => panic!("expected LobbyChangeEvent, got {msg:#?}"),
            }
        }
    }

    async fn set_ready(&mut self, ready_state: bool) {
        self.send(SetReadyStateRequest { ready_state }.into()).await;
        let resp = self.receive().await;
        match resp {
            ProtocolMessage::StatusMessage(StatusMessage {
                code,
                data: Some(Data::SetReadyStateResponse(SetReadyStateResponse {})),
                ..
            }) => {
                assert_eq!(StatusCode::from_i32(code), Some(StatusCode::Ok));
                self.state.ready = ready_state;
            }
            _ => panic!("Expected SetReadyStateResponse, got {resp:#?}"),
        }
    }

    async fn set_ready_check_broadcasts(
        broadcast_receiver: impl Iterator<Item = &mut Client>,
        assert_map: HashMap<PlayerID, bool>,
    ) {
        for c in broadcast_receiver {
            let msg = c.receive().await;
            match msg {
                ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
                    team_state_a,
                    team_state_b,
                }) => {
                    for state in team_state_a.iter().chain(team_state_b.iter()) {
                        assert_eq!(state.ready, *assert_map.get(&state.player_id).unwrap());
                    }
                }
                _ => panic!("expected LobbyChangeEvent, got {msg:#?}"),
            }
        }
    }

    async fn check_preparation_start_broadcast(
        broadcast_receiver: impl Iterator<Item = &mut Client>,
    ) -> HashMap<PlayerID, Coordinate> {
        let mut quadrant_assignments: HashMap<u32, Coordinate> = HashMap::new();

        for c in broadcast_receiver {
            let msg = c.receive().await;
            match msg {
                ProtocolMessage::PlacementPhase(PlacementPhase {
                    corner: Some(corner),
                }) => {
                    assert!(!quadrant_assignments.values().any(|c| c.clone() == corner));
                    quadrant_assignments.insert(c.state.player_id, corner);
                }
                _ => panic!("expected PlacementPhase with corner, got {msg:#?}"),
            }
        }

        quadrant_assignments
    }

    async fn disconnect(self) {
        self.connection.close(VarInt::from_u32(0), &[])
    }

    async fn connect(
        bind_addr: SocketAddr,
        addr: SocketAddr,
        client_config: Arc<dyn crypto::ClientConfig>,
    ) -> Connection {
        let mut ep = Endpoint::client(bind_addr).expect("unable to create Endpoint on {addr}");
        ep.set_default_client_config(ClientConfig::new(client_config));
        ep.connect(addr, &addr.ip().to_string())
            .expect("unable to connect to server")
            .await
            .expect("unable to connect to server")
    }

    async fn connect_ipv4(
        cfg: Arc<dyn ConfigProvider>,
        client_config: Arc<dyn crypto::ClientConfig>,
        username: &str,
    ) -> Client {
        let connection = Self::connect(
            SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into(),
            SocketAddrV4::new(
                match option_env!("TEST_CLIENT_IP4") {
                    Some(ip_str) => Ipv4Addr::from_str(ip_str).expect("unable to parse IPv4"),
                    None => Ipv4Addr::LOCALHOST,
                },
                cfg.server_config().game_address_v4.port(),
            )
            .into(),
            client_config,
        )
        .await;

        Self::join(connection, username).await
    }

    async fn connect_ipv6(
        cfg: Arc<dyn ConfigProvider>,
        client_config: Arc<dyn crypto::ClientConfig>,
        username: &str,
    ) -> Client {
        let connection = Self::connect(
            SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into(),
            SocketAddrV6::new(
                match option_env!("TEST_CLIENT_IP6") {
                    Some(ip_str) => Ipv6Addr::from_str(ip_str).expect("unable to parse IPv6"),
                    None => Ipv6Addr::LOCALHOST,
                },
                cfg.server_config().game_address_v6.port(),
                0,
                0,
            )
            .into(),
            client_config,
        )
        .await;

        Self::join(connection, username).await
    }

    async fn join(connection: Connection, username: &str) -> Client {
        let (tx, rx) = match connection.open_bi().await {
            Ok(stream) => stream,
            Err(e) => panic!("unable to open bidirectional stream: {e}"),
        };

        let mut reader = FramedRead::new(rx, BattleshipPlusCodec::default());
        let mut writer = FramedWrite::new(tx, BattleshipPlusCodec::default());

        Self::send_msg_inner(
            &mut writer,
            ProtocolMessage::JoinRequest(JoinRequest {
                username: username.to_string(),
            }),
        )
        .await;
        let msg = Self::receive_msg_inner(&mut reader).await;

        let player_id = match msg {
            ProtocolMessage::StatusMessage(StatusMessage {
                code,
                data: Some(Data::JoinResponse(JoinResponse { player_id })),
                ..
            }) => {
                assert_eq!(StatusCode::from_i32(code), Some(StatusCode::Ok));
                player_id
            }
            _ => panic!("Expected JoinResponse, got {msg:#?}"),
        };

        let msg = Self::receive_msg_inner(&mut reader).await;

        let predicate = |state: &&PlayerLobbyState| {
            state.player_id == player_id && state.name == *username && !state.ready
        };

        let (state, team) = match msg {
            ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
                team_state_a,
                team_state_b,
            }) => {
                if let Some(state) = team_state_a.iter().find(predicate) {
                    (state.clone(), Team::A)
                } else {
                    let state = team_state_b
                        .iter()
                        .find(predicate)
                        .expect("joined player is not in lobby");

                    (state.clone(), Team::B)
                }
            }
            _ => panic!("Expected JoinResponse, got {msg:#?}"),
        };

        Client {
            connection,
            reader,
            writer,
            state,
            team,
        }
    }

    async fn send_msg_inner(
        writer: &mut FramedWrite<SendStream, BattleshipPlusCodec>,
        msg: ProtocolMessage,
    ) {
        match tokio::time::timeout(Duration::from_secs(5), writer.send(msg)).await {
            Err(e) => panic!("send message timed out: {e}"),
            Ok(res) => res.expect("unable to send {msg:#?}"),
        }
        match tokio::time::timeout(Duration::from_secs(5), writer.flush()).await {
            Err(e) => panic!("flush message timed out: {e}"),
            Ok(res) => res.expect("unable to send {msg:#?}"),
        }
    }

    async fn receive_msg_inner(
        reader: &mut FramedRead<RecvStream, BattleshipPlusCodec>,
    ) -> ProtocolMessage {
        match tokio::time::timeout(Duration::from_secs(5), reader.next()).await {
            Err(e) => panic!("receive message timed out: {e}"),
            Ok(res) => res
                .expect("unable to receive ProtocolMessage")
                .expect("unable to receive ProtocolMessage")
                .expect("unable to receive ProtocolMessage"),
        }
    }
}

/// Implementation of `ServerCertVerifier` that verifies everything as trustworthy.
/// Taken from `bevy_quinnet_client`
pub struct SkipServerVerification;

impl SkipServerVerification {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

#[tokio::test]
async fn lobby_e2e() {
    pretty_env_logger::init_timed();

    let _lock = TEST_LOCK.lock().await;
    let cfg = default_config_provider();

    let server_ctrl = spawn_server_task(cfg.clone());

    const DISCONNECTING_CLIENTS: usize = 4;
    let client_count: usize =
        (cfg.game_config().team_size_a + cfg.game_config().team_size_a) as usize;

    let mut client_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();
    client_config
        .alpn_protocols
        .push(protocol_name_with_version().into_bytes());
    client_config
        .alpn_protocols
        .push(protocol_name().into_bytes());

    let client_config = Arc::new(client_config);

    // create clients and connect to socket
    let mut clients = Vec::with_capacity(client_count);
    for i in 0..client_count + DISCONNECTING_CLIENTS {
        clients.push(if i < client_count / 2 {
            Client::connect_ipv6(
                cfg.clone(),
                client_config.clone(),
                format!("User{i}").as_str(),
            )
            .await
        } else {
            Client::connect_ipv4(
                cfg.clone(),
                client_config.clone(),
                format!("User{i}").as_str(),
            )
            .await
        });
    }
    assert_eq!(clients.len(), client_count + DISCONNECTING_CLIENTS);

    // check for all remaining LobbyChangeEvents
    for (i, c) in clients.iter_mut().enumerate() {
        for j in (2 + i)..=(client_count + DISCONNECTING_CLIENTS) {
            let msg = c.receive().await;

            match msg {
                ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
                    team_state_a,
                    team_state_b,
                }) => {
                    assert_eq!(team_state_a.len() + team_state_b.len(), j);
                }
                _ => panic!("Expected LobbyChangeEvent, got {msg:#?}"),
            }
        }
    }

    let mut team_a = Vec::new();
    let mut team_b = Vec::new();

    for c in clients {
        match c.team {
            Team::A => team_a.push(c),
            Team::B => team_b.push(c),
        }
    }

    // disconnect some clients
    while team_a.len() > cfg.game_config().team_size_a as usize {
        team_a.pop().unwrap().disconnect().await;
    }
    while team_b.len() > cfg.game_config().team_size_b as usize {
        team_b.pop().unwrap().disconnect().await;
    }

    // check for all LobbyChangeEvents
    for c in team_a.iter_mut().chain(team_b.iter_mut()) {
        for j in (0..DISCONNECTING_CLIENTS).rev() {
            let msg = c.receive().await;

            match msg {
                ProtocolMessage::LobbyChangeEvent(LobbyChangeEvent {
                    team_state_a,
                    team_state_b,
                }) => {
                    assert_eq!(team_state_a.len() + team_state_b.len(), client_count + j);
                }
                _ => panic!("Expected LobbyChangeEvent, got {msg:#?}"),
            }
        }
    }

    let team_a_count = team_a.len();
    let team_b_count = team_b.len();

    assert_eq!(team_a_count, cfg.game_config().team_size_a as usize);
    assert_eq!(team_b_count, cfg.game_config().team_size_b as usize);

    // switch one client
    team_a.first_mut().unwrap().switch_team().await;
    Client::switch_team_check_broadcasts(
        team_a.iter_mut().chain(team_b.iter_mut()),
        team_a_count - 1,
        team_b_count + 1,
    )
    .await;

    // switch client back
    team_a.first_mut().unwrap().switch_team().await;
    Client::switch_team_check_broadcasts(
        team_a.iter_mut().chain(team_b.iter_mut()),
        team_a_count,
        team_b_count,
    )
    .await;

    let mut clients = HashMap::with_capacity(client_count);
    for cc in [team_a, team_b] {
        for c in cc {
            clients.insert(c.state.player_id, c);
        }
    }

    let mut assert_map = clients.values().fold(HashMap::new(), |mut map, p| {
        map.insert(p.state.player_id, p.state.ready);
        map
    });

    let ids: Vec<_> = assert_map.keys().cloned().collect();
    for id in ids.iter().skip(1) {
        // set all clients (except one) ready -> unready -> ready
        clients.get_mut(id).unwrap().set_ready(true).await;
        assert_map.insert(*id, true);
        Client::set_ready_check_broadcasts(clients.values_mut(), assert_map.clone()).await;

        clients.get_mut(id).unwrap().set_ready(false).await;
        assert_map.insert(*id, false);
        Client::set_ready_check_broadcasts(clients.values_mut(), assert_map.clone()).await;

        clients.get_mut(id).unwrap().set_ready(true).await;
        assert_map.insert(*id, true);
        Client::set_ready_check_broadcasts(clients.values_mut(), assert_map.clone()).await;
    }

    // set last client ready
    clients
        .get_mut(ids.first().unwrap())
        .unwrap()
        .set_ready(true)
        .await;
    assert_map.insert(*ids.first().unwrap(), true);
    Client::set_ready_check_broadcasts(clients.values_mut(), assert_map.clone()).await;
    Client::check_preparation_start_broadcast(clients.values_mut()).await;

    server_ctrl.stop().await;
}

// TODO Implement: Fuzzy test
// TODO Test: player disconnect and reconnect and check player ready states
// TODO Test: player switch teams and check player ready states
// TODO Test: player set themselves ready and unready
