//! Errors surfaced by the sync engine.

use clipmesh_identity::IdentityError;
use clipmesh_protocol::{DeviceId, ProtocolError};
use clipmesh_security::SecurityError;

/// Convenience alias used across the crate.
pub type Result<T, E = CoreError> = std::result::Result<T, E>;

/// Everything the sync engine can fail at.
///
/// The provider traits erase their concrete error types into
/// [`CoreError::Clipboard`] / [`CoreError::Network`] on purpose: the orchestrator
/// only ever reacts to "this subsystem is broken", and keeping the traits
/// object safe (`Arc<dyn ClipboardProvider>`) is worth more than preserving the
/// exact error type at this layer. Each subsystem logs its own detailed error
/// before it is erased.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A protocol level failure.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// A key, certificate or trust store failure.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// A TLS or session failure.
    #[error(transparent)]
    Security(#[from] SecurityError),

    /// The platform clipboard could not be read or written.
    #[error("clipboard unavailable: {0}")]
    Clipboard(String),

    /// Discovery or transport failure.
    #[error("network error: {0}")]
    Network(String),

    /// We refused to talk to a device the user never trusted.
    #[error("device {0} is not trusted")]
    NotTrusted(DeviceId),

    /// We tried to reach a device that has no live session.
    #[error("device {0} is not connected")]
    NotConnected(DeviceId),

    /// The remote user declined the pairing request.
    #[error("pairing with {device} was rejected: {reason}")]
    PairingRejected {
        /// Device that declined.
        device: DeviceId,
        /// Reason it gave.
        reason: String,
    },

    /// We asked for a user decision and none arrived in time.
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),

    /// The clipboard engine is already running (or was never started).
    #[error("invalid engine state: {0}")]
    InvalidState(&'static str),

    /// Transport level failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Persisted state could not be (de)serialized.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Anything that does not deserve its own variant.
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    /// True when the failure is a user facing condition rather than a bug.
    ///
    /// The UI uses this to decide between a toast and an error log entry.
    #[must_use]
    pub fn is_user_actionable(&self) -> bool {
        matches!(
            self,
            Self::NotTrusted(_) | Self::PairingRejected { .. } | Self::Timeout(_)
        )
    }
}
