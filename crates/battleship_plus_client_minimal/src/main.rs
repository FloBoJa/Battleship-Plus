use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use futures::{poll, SinkExt};
use quinn::{ClientConfig, Connection, crypto, Endpoint, RecvStream, SendStream};
use tokio_util::codec::{FramedRead, FramedWrite};
use battleship_plus_common::codec::BattleshipPlusCodec;
use battleship_plus_common::messages::{JoinRequest, JoinResponse, ProtocolMessage, StatusCode};
use futures::StreamExt;
use battleship_plus_common::messages::StatusMessage;
use battleship_plus_common::messages::status_message::Data;

mod client_config;

struct Con {
    reader: FramedRead<RecvStream, BattleshipPlusCodec>,
    writer: FramedWrite<SendStream, BattleshipPlusCodec>
}

#[tokio::main]
async fn main() {
    let client_config = client_config::configure_client();
    let connection = connect(client_addr(), server_addr(), client_config).await;
    let con = getCon(connection).await;
    join(con, "Test").await;

}

async fn join(mut con: Con, username: &str) {
    send_msg_inner(
        &mut con.writer,
        ProtocolMessage::JoinRequest(JoinRequest {
            username: (*username).parse().unwrap(),
        })).await;
    let msg = receive_msg_inner(&mut con.reader).await;

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

    println!("{}", player_id);
}

async fn getCon(connection: Connection) -> Con{
    let (tx, rx) = match connection.open_bi().await {
        Ok(stream) => stream,
        Err(e) => panic!("unable to open bidirectional stream: {e}"),
    };

    let mut reader = FramedRead::new(rx, BattleshipPlusCodec::default());
    let mut writer = FramedWrite::new(tx, BattleshipPlusCodec::default());

    return Con {reader, writer};
}

fn server_addr() -> SocketAddr {
    let server_ip = "bsplus.floboja.net:30305";
    let server: Vec<_> = server_ip
        .to_socket_addrs()
        .expect("Unable to resolve domain")
        .collect();

    return server[0];
}

fn client_addr() -> SocketAddr {
    return SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into();
}

async fn connect (
    bind_addr: SocketAddr,
    addr: SocketAddr,
    client_config: Arc<dyn crypto::ClientConfig>,
) -> Connection {
    let mut ep = Endpoint::client(bind_addr).expect("unable to create Endpoint");
    ep.set_default_client_config(ClientConfig::new(client_config));
    let connection = ep.connect(addr, &addr.ip().to_string())
        .expect("unable to connect to server")
        .await
        .expect("unable to connect to server");

    return connection;
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
