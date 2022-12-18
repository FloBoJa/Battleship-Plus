use std::borrow::Borrow;
use std::net::{IpAddr, SocketAddr};

use log::{debug, warn};
use tokio::net::UdpSocket;
use tokio::time;

use battleship_plus_common::PROTOCOL_VERSION;

use crate::config_provider::ConfigProvider;

pub(crate) async fn start_announcement_timer(cfg: &dyn ConfigProvider) {
    if !cfg.server_config().enable_announcements_v4 && cfg.server_config().enable_announcements_v6 {
        return;
    }

    let sock_v4;
    if cfg.server_config().enable_announcements_v4 {
        let s = UdpSocket::bind(SocketAddr::new(
            IpAddr::from(cfg.server_config().game_address_v4.ip().octets()),
            0,
        ))
        .await
        .expect("unable to create IPv4 announcement socket");
        s.set_broadcast(true)
            .expect("unable to enable broadcasting on IPv4 announcement socket");
        sock_v4 = Some(s);
    } else {
        sock_v4 = None
    };

    let sock_v6;
    if cfg.server_config().enable_announcements_v6 {
        let s = UdpSocket::bind(SocketAddr::new(
            IpAddr::from(cfg.server_config().game_address_v6.ip().octets()),
            0,
        ))
        .await
        .expect("unable to create IPv6 announcement socket");
        s.set_broadcast(true)
            .expect("unable to enable broadcasting on IPv6 announcement socket");
        sock_v6 = Some(s);
    } else {
        sock_v6 = None
    };

    let mut timer = time::interval(cfg.server_config().announcement_interval);

    let server_name = cfg.game_config().server_name.clone();
    let game_port_v4 = cfg.server_config().game_address_v4.port();
    let game_port_v6 = cfg.server_config().game_address_v6.port();
    let announce_v4 = cfg.server_config().announcement_address_v4;
    let announce_v6 = cfg.server_config().announcement_address_v6;

    tokio::spawn(async move {
        loop {
            timer.tick().await;

            if sock_v4.as_ref().is_some() {
                match dispatch_announcement(
                    sock_v4.as_ref().unwrap().borrow(),
                    game_port_v4,
                    server_name.as_str(),
                    announce_v4.into(),
                )
                .await
                {
                    Ok(_) => debug!("IPv4 advertisement dispatched"),
                    Err(e) => warn!("unable to dispatch IPv4 advertisement: {}", e),
                };
            }

            if sock_v6.as_ref().is_some() {
                match dispatch_announcement(
                    sock_v6.as_ref().unwrap().borrow(),
                    game_port_v6,
                    server_name.as_str(),
                    announce_v6.into(),
                )
                .await
                {
                    Ok(_) => debug!("IPv6 advertisement dispatched"),
                    Err(e) => warn!("unable to dispatch IPv6 advertisement: {}", e),
                };
            }
        }
    });
}

pub(crate) async fn dispatch_announcement(
    socket: &UdpSocket,
    port: u16,
    display_name: &str,
    dst: SocketAddr,
) -> Result<(), String> {
    let message =
        battleship_plus_common::messages::packet_payload::ProtocolMessage::ServerAdvertisement(
            battleship_plus_common::messages::ServerAdvertisement {
                port: port as u32,
                display_name: String::from(display_name),
            },
        );

    let msg = match battleship_plus_common::messages::Message::new(PROTOCOL_VERSION, message) {
        Ok(msg) => msg,
        Err(e) => return Err(format!("unable to encode advertisement message: {}", e)),
    };

    match socket.send_to(msg.encode().as_slice(), dst).await {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("unable to send advertisement message: {}", e)),
    }
}
