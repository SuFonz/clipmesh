//! Errors produced by the network layer.

use clipmesh_identity::IdentityError;
use clipmesh_protocol::{DeviceId, ProtocolError};
use clipmesh_security::SecurityError;

/// Convenience alias used across the crate.
pub type Result<T, E = NetworkError> = std::result::Result<T, E>;

/// Everything discovery or transport can fail at.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NetworkError {
    /// mDNS registration or browsing failed.
    #[error("service discovery error: {0}")]
    Discovery(String),

    /// A socket could not be bound, connected or written to.
    #[error("transport error: {0}")]
    Transport(String),

    /// TLS setup or handshake failure.
    #[error(transparent)]
    Security(#[from] SecurityError),

    /// Key or trust store failure.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// Framing or protobuf failure.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// The peer is not the device we asked for, or is not trusted.
    #[error("device {0} is not trusted")]
    NotTrusted(DeviceId),

    /// We have no session with that device.
    #[error("device {0} is not connected")]
    NotConnected(DeviceId),

    /// The peer failed the identity handshake.
    #[error("peer failed the identity handshake: {0}")]
    Handshake(String),

    /// The service is not running.
    #[error("the network service is not running")]
    NotRunning,

    /// The service is already running.
    #[error("the network service is already running")]
    AlreadyRunning,

    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
