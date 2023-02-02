use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use eyre::Report;
use quinn::{Connecting, Endpoint};

use bevy_quinnet_client::certificate::CertificateVerificationMode;

#[derive(Debug, Clone)]
pub struct QuicSocket {
    ep_v4: Endpoint,
    ep_v6: Endpoint,
}

impl QuicSocket {
    pub fn new() -> io::Result<Self> {
        Ok(QuicSocket {
            ep_v4: Endpoint::client(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0))?,
            ep_v6: Endpoint::client(SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0))?,
        })
    }

    pub fn connect<S: AsRef<str>>(
        &self,
        cert_mode: CertificateVerificationMode,
        alpns: Vec<String>,
        addr: SocketAddr,
        server_name: S,
    ) -> eyre::Result<Connecting> {
        let ep = match addr {
            SocketAddr::V4(_) => &self.ep_v4,
            SocketAddr::V6(_) => &self.ep_v6,
        };

        let cfg = match bevy_quinnet_client::configure_client_standalone(cert_mode, alpns) {
            Ok(cfg) => cfg,
            Err(e) => return Err(Report::msg(e.to_string())),
        };

        match ep.connect_with(cfg, addr, server_name.as_ref()) {
            Ok(connecting) => Ok(connecting),
            Err(e) => Err(Report::new(e)),
        }
    }
}
