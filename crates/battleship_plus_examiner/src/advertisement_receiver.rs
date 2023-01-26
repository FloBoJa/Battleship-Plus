use std::borrow::BorrowMut;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

use futures_util::StreamExt;
use log::{debug, error, warn};
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;

use battleship_plus_common::codec::{BattleshipPlusCodec, CodecError};
use battleship_plus_common::messages::{ProtocolMessage, ServerAdvertisement};

#[derive(Debug)]
pub struct AdvertisementReceiver {
    advertisements: tokio::sync::broadcast::Receiver<(ServerAdvertisement, SocketAddr)>,
}

impl AdvertisementReceiver {
    pub fn new(port: u16) -> Self {
        let (advertisements_tx, advertisements_rx) =
            tokio::sync::broadcast::channel::<(ServerAdvertisement, SocketAddr)>(128);

        tokio::spawn(async move {
            let socket_v6 =
                match UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0)).await {
                    Ok(socket) => Some(UdpFramed::new(socket, BattleshipPlusCodec::default())),
                    Err(e) => {
                        error!("unable to bind IPv6 socket for server advertisements: {e}");
                        None
                    }
                };

            let socket_v4 = match UdpSocket::bind(SocketAddrV6::new(
                Ipv6Addr::UNSPECIFIED,
                port,
                0,
                0,
            ))
            .await
            {
                Ok(socket) => Some(UdpFramed::new(socket, BattleshipPlusCodec::default())),
                Err(e) => {
                    warn!("unable to bind IPv4 socket for server advertisements. Is it blocked by the IPv6 socket?: {e}");
                    None
                }
            };

            for task in [
                tokio::spawn(socket_task(socket_v4, advertisements_tx.clone())),
                tokio::spawn(socket_task(socket_v6, advertisements_tx.clone())),
            ] {
                let _ = task.await;
            }
        });

        Self {
            advertisements: advertisements_rx,
        }
    }

    pub fn poll(&mut self) -> Vec<(ServerAdvertisement, SocketAddr)> {
        let mut advertisements = Vec::with_capacity(0);
        while let Ok(advertisement) = self.advertisements.try_recv() {
            advertisements.push(advertisement);
        }
        advertisements
    }
}

async fn socket_task(
    mut socket: Option<UdpFramed<BattleshipPlusCodec>>,
    advertisements_tx: tokio::sync::broadcast::Sender<(ServerAdvertisement, SocketAddr)>,
) {
    while socket.as_ref().is_some() {
        process_msg(
            socket.as_mut().unwrap().next().await,
            socket.borrow_mut(),
            &advertisements_tx,
        );
    }
}

fn process_msg(
    msg: Option<Result<(Option<ProtocolMessage>, SocketAddr), CodecError>>,
    socket: &mut Option<UdpFramed<BattleshipPlusCodec>>,
    advertisements_tx: &tokio::sync::broadcast::Sender<(ServerAdvertisement, SocketAddr)>,
) {
    match msg {
        None => *socket = None,
        Some(Err(e)) => error!("unable to receive server advertisement: {e}"),
        Some(Ok((Some(ProtocolMessage::ServerAdvertisement(advertisement)), addr))) => {
            if let Err(_) = advertisements_tx.send((advertisement, addr)) {
                debug!("dropping server advertisement because there is no receiver");
                *socket = None;
            }
        }
        Some(Ok((Some(protocol_msg), addr))) => {
            debug!("expected ServerAdvertisement, received {protocol_msg:?} from {addr}");
        }
        Some(Ok((None, addr))) => {
            debug!("expected ServerAdvertisement, received empty ProtocolMessage from {addr}");
        }
    }
}
