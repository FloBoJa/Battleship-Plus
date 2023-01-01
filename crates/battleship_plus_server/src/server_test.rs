use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use quinn::{crypto, ClientConfig, Endpoint};
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite};

use battleship_plus_common::codec::BattleshipPlusCodec;
use battleship_plus_common::messages::status_message::Data;
use battleship_plus_common::messages::{JoinRequest, JoinResponse, ProtocolMessage, StatusMessage};
use bevy_quinnet::client::certificate::SkipServerVerification;

use crate::config_provider::{default_config_provider, ConfigProvider};
use crate::server::spawn_server_task;

type TestLock = Arc<Mutex<()>>;

static TEST_LOCK: Lazy<TestLock> = Lazy::new(|| Arc::new(Mutex::new(())));

async fn connect_client_4(
    cfg: Arc<dyn ConfigProvider>,
    client_config: Arc<dyn crypto::ClientConfig>,
) -> quinn::Connection {
    let mut ep = Endpoint::client(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
        .expect("unable to create v4 Endpoint");
    ep.set_default_client_config(ClientConfig::new(client_config));

    let addr = SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        cfg.server_config().game_address_v4.port(),
    );
    ep.connect(addr.into(), &addr.ip().to_string())
        .expect("unable to connect to server")
        .await
        .expect("unable to connect to server")
}

async fn connect_client_6(
    cfg: Arc<dyn ConfigProvider>,
    client_config: Arc<dyn crypto::ClientConfig>,
) -> quinn::Connection {
    let mut ep = Endpoint::client(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into())
        .expect("unable to create v6 Endpoint");
    ep.set_default_client_config(ClientConfig::new(client_config));

    let addr = SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        cfg.server_config().game_address_v4.port(),
        0,
        0,
    );
    ep.connect(addr.into(), &addr.ip().to_string())
        .expect("unable to connect to server")
        .await
        .expect("unable to connect to server")
}

#[tokio::test]
async fn lobby_e2e() {
    env_logger::init();

    let _lock = TEST_LOCK.lock().await;
    let cfg = default_config_provider();

    let server_ctrl = spawn_server_task(cfg.clone());

    const CLIENTS: usize = 10;

    let config = Arc::new(
        rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth(),
    );

    // create clients and connect to socket
    let clients = vec![
        connect_client_4(cfg.clone(), config.clone()).await,
        connect_client_4(cfg.clone(), config.clone()).await,
        connect_client_4(cfg.clone(), config.clone()).await,
        connect_client_4(cfg.clone(), config.clone()).await,
        connect_client_4(cfg.clone(), config.clone()).await,
        connect_client_6(cfg.clone(), config.clone()).await,
        connect_client_6(cfg.clone(), config.clone()).await,
        connect_client_6(cfg.clone(), config.clone()).await,
        connect_client_6(cfg.clone(), config.clone()).await,
        connect_client_6(cfg.clone(), config.clone()).await,
    ];

    assert_eq!(clients.len(), CLIENTS);

    // open streams
    let mut client_streams = Vec::with_capacity(CLIENTS);
    for c in clients {
        client_streams.push(
            c.open_bi()
                .await
                .expect("unable to open bidirectional stream"),
        );
    }

    let mut reader = Vec::with_capacity(CLIENTS);
    let mut writer = Vec::with_capacity(CLIENTS);

    for (tx, rx) in client_streams {
        reader.push(FramedRead::new(rx, BattleshipPlusCodec::default()));
        writer.push(FramedWrite::new(tx, BattleshipPlusCodec::default()));
    }

    let mut player_ids = Vec::with_capacity(CLIENTS);

    // join all clients
    for i in 0..CLIENTS {
        writer[i]
            .send(ProtocolMessage::JoinRequest(JoinRequest {
                username: format!("User{i}"),
            }))
            .await
            .expect("unable to send JoinMessage");
        writer[i].flush().await.expect("unable to send JoinMessage");

        let msg = reader[i]
            .next()
            .await
            .expect("unable to receive JoinResponse")
            .expect("unable to receive JoinResponse")
            .expect("unable to receive JoinResponse");

        match msg {
            ProtocolMessage::StatusMessage(StatusMessage {
                code,
                data: Some(Data::JoinResponse(JoinResponse { player_id })),
            }) => {
                assert_eq!(code, 200);
                player_ids.push(player_id);
            }
            _ => panic!("Expected JoinResponse, got {msg:#?}"),
        }
    }

    server_ctrl.stop().await;
}
