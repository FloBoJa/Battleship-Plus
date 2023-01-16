use std::borrow::Borrow;
use std::net::{IpAddr, SocketAddr};

use futures::SinkExt;
use log::{trace, warn};
use tokio::net::UdpSocket;
use tokio::time;
use tokio_util::udp::UdpFramed;

use battleship_plus_common::{
    codec::BattleshipPlusCodec,
    messages::{self, packet_payload::ProtocolMessage},
};

use crate::config_provider::ConfigProvider;
use crate::tasks::{upgrade_oneshot, TaskControl};

/// Starts broadcasting game announcements at a fixed interval.
/// When a task is started by this call, it returns a Channel to signal the task to stop and a JoinHandle.
#[cfg(not(feature = "silent"))]
pub(crate) async fn spawn_timer_task(cfg: &dyn ConfigProvider) -> Option<TaskControl> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut stop = upgrade_oneshot(rx);

    if !cfg.server_config().enable_announcements_v4 && !cfg.server_config().enable_announcements_v6
    {
        return None;
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

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.recv() => return,
                _ = timer.tick() => {}
            }

            if sock_v4.as_ref().is_some() {
                match dispatch_announcement(
                    sock_v4.as_ref().unwrap().borrow(),
                    game_port_v4,
                    server_name.as_str(),
                    announce_v4.into(),
                )
                .await
                {
                    Ok(_) => trace!("IPv4 advertisement dispatched"),
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
                    Ok(_) => trace!("IPv6 advertisement dispatched"),
                    Err(e) => warn!("unable to dispatch IPv6 advertisement: {}", e),
                };
            }
        }
    });

    Some(TaskControl::new(tx, handle))
}

#[cfg(feature = "silent")]
pub(crate) async fn spawn_timer_task(cfg: &dyn ConfigProvider) -> Option<TaskControl> {
    None
}

#[cfg(not(feature = "silent"))]
pub(crate) async fn dispatch_announcement(
    socket: &UdpSocket,
    port: u16,
    display_name: &str,
    dst: SocketAddr,
) -> Result<(), String> {
    let message = ProtocolMessage::ServerAdvertisement(messages::ServerAdvertisement {
        port: port as u32,
        display_name: String::from(display_name),
    });

    let mut socket = UdpFramed::new(socket, BattleshipPlusCodec::default());

    match socket.send((message, dst)).await {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("unable to send advertisement message: {e}")),
    }
}
