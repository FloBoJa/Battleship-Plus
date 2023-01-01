use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use quinn::{crypto, ClientConfig, Connection, Endpoint, RecvStream, SendStream};
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};

use battleship_plus_common::codec::BattleshipPlusCodec;
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{
    JoinRequest, JoinResponse, LobbyChangeEvent, ProtocolMessage, StatusMessage,
};
use battleship_plus_common::types::PlayerLobbyState;
use bevy_quinnet::client::certificate::SkipServerVerification;

use crate::config_provider::{default_config_provider, ConfigProvider};
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

    async fn connect(
        bind_addr: SocketAddr,
        addr: SocketAddr,
        client_config: Arc<dyn crypto::ClientConfig>,
    ) -> Connection {
        let mut ep = Endpoint::client(bind_addr).expect("unable to create Endpoint on {addr}");
        ep.set_default_client_config(ClientConfig::new(client_config));
        ep.connect(addr.into(), &addr.ip().to_string())
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
                Ipv4Addr::LOCALHOST,
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
                Ipv6Addr::LOCALHOST,
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
            Err(e) => panic!("unable to open bidirectional stream"),
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
            }) => {
                assert_eq!(code, 200);
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

#[tokio::test]
async fn lobby_e2e() {
    pretty_env_logger::init();

    let _lock = TEST_LOCK.lock().await;
    let cfg = default_config_provider();

    let server_ctrl = spawn_server_task(cfg.clone());

    const CLIENTS: usize = 10;

    let client_config = Arc::new(
        rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    );

    // create clients and connect to socket
    let mut clients = Vec::with_capacity(CLIENTS);
    for i in 0..CLIENTS {
        clients.push(if i < CLIENTS / 2 {
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

    assert_eq!(clients.len(), CLIENTS);

    for (i, c) in clients.iter_mut().enumerate() {
        for j in (2 + i)..(CLIENTS - i - 1) {
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

    // Fuzzy test the following
    // TODO: player disconnect and reconnect and check player ready states
    // TODO: player switch teams and check player ready states
    // TODO: player set themselves ready and unready

    // TODO: check server state switch when a game can start

    todo!();

    server_ctrl.stop().await;
}
