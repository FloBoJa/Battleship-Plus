use std::{fmt, io, net::AddrParseError, sync::PoisonError};

use base64::Engine;
#[cfg(feature = "bevy")]
use bevy::prelude::{Deref, DerefMut, Resource};
use rcgen::RcgenError;
use tokio::runtime::Runtime;

use crate::client::ConnectionId;

pub const DEFAULT_MESSAGE_QUEUE_SIZE: usize = 150;
pub const DEFAULT_KILL_MESSAGE_QUEUE_SIZE: usize = 10;
pub const DEFAULT_KEEP_ALIVE_INTERVAL_S: u64 = 4;

pub type ClientId = u32;

#[cfg_attr(feature = "bevy", derive(Resource, Deref, DerefMut))]
pub struct AsyncRuntime(pub Runtime);

/// Enum with possibles errors that can occur in Bevy Quinnet
#[derive(thiserror::Error, Debug)]
pub enum QuinnetError {
    #[error("IP/Socket address is invalid")]
    InvalidAddress(#[from] AddrParseError),
    #[error("Failed to generate a self-signed certificate")]
    CertificateGenerationFailed(#[from] RcgenError),
    #[error("Client with id `{0}` is unknown")]
    UnknownClient(ClientId),
    #[error("Connection with id `{0}` is unknown")]
    UnknownConnection(ConnectionId),
    #[error("Endpoint is already closed")]
    EndpointAlreadyClosed,
    #[error("Failed serialization")]
    Serialization,
    #[error("Failed deserialization")]
    Deserialization,
    #[error("The data could not be sent on the channel because the channel is currently full and sending would require blocking")]
    FullQueue,
    #[error("The receiving half of the channel was explicitly closed or has been dropped")]
    ChannelClosed,
    #[error("The hosts file is invalid")]
    InvalidHostFile,
    #[error("Lock acquisition failure")]
    LockAcquisitionFailure,
    #[error("A Certificate action was already sent for a CertificateInteractionEvent")]
    CertificateActionAlreadyApplied,
    #[error("Failed to read/write file(s)")]
    IoError(#[from] io::Error),
    #[error("Rustls protocol error")]
    RustlsError(#[from] rustls::Error),
}

impl<T> From<PoisonError<T>> for QuinnetError {
    fn from(_: PoisonError<T>) -> Self {
        Self::LockAcquisitionFailure
    }
}

/// SHA-256 hash of the certificate data in DER form
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CertificateFingerprint([u8; 32]);

impl CertificateFingerprint {
    pub fn new(buf: [u8; 32]) -> Self {
        CertificateFingerprint(buf)
    }

    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0)
    }
}

impl From<&rustls::Certificate> for CertificateFingerprint {
    fn from(cert: &rustls::Certificate) -> CertificateFingerprint {
        let hash = ring::digest::digest(&ring::digest::SHA256, &cert.0);
        let fingerprint_bytes = hash.as_ref().try_into().unwrap();
        CertificateFingerprint(fingerprint_bytes)
    }
}

impl fmt::Display for CertificateFingerprint {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_base64(), f)
    }
}
