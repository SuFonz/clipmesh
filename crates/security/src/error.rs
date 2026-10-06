//! Errors produced by the security layer.

use clipmesh_identity::IdentityError;

/// Convenience alias used across the crate.
pub type Result<T, E = SecurityError> = std::result::Result<T, E>;

/// Everything that can go wrong while establishing or using a secure session.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SecurityError {
    /// Key, certificate or trust store failure.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// The TLS configuration could not be assembled.
    #[error("TLS configuration error: {0}")]
    Tls(String),

    /// The TLS handshake failed, or the peer violated the protocol.
    #[error("TLS handshake failed: {0}")]
    Handshake(String),

    /// The peer's certificate was unacceptable.
    #[error("peer certificate rejected: {0}")]
    Certificate(String),

    /// We connected expecting a specific device and got a different one.
    #[error("expected to reach device {expected}, but reached {actual}")]
    UnexpectedDevice {
        /// Device we asked for.
        expected: String,
        /// Device we got.
        actual: String,
    },

    /// The application level identity signature did not verify.
    #[error("peer identity could not be proven: {0}")]
    IdentityVerification(String),

    /// A message arrived outside the state machine's expectations.
    #[error("session state is {actual}, expected {expected}")]
    InvalidState {
        /// Expected state.
        expected: &'static str,
        /// Actual state.
        actual: &'static str,
    },

    /// The TLS exporter secret could not be read.
    #[error("channel binding unavailable: {0}")]
    ChannelBinding(String),

    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
