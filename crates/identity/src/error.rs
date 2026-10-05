//! Errors produced by the identity layer.

use clipmesh_protocol::DeviceId;

/// Convenience alias used across the crate.
pub type Result<T, E = IdentityError> = std::result::Result<T, E>;

/// Everything that can go wrong with keys, certificates and trust.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// A private or public key could not be encoded or decoded.
    #[error("key encoding error: {0}")]
    KeyEncoding(String),

    /// Bytes handed in as a public key are not a valid one.
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),

    /// A signature did not verify.
    #[error("signature verification failed: {0}")]
    Signature(String),

    /// Certificate generation, parsing or extraction failed.
    #[error("certificate error: {0}")]
    Certificate(String),

    /// The on-disk trust store could not be read or written.
    #[error("trust store error: {0}")]
    TrustStore(String),

    /// We were asked to trust or talk to a device we have never paired with.
    #[error("device {0} is not trusted")]
    NotTrusted(DeviceId),

    /// A trusted device presented a different certificate than the one we
    /// pinned at pairing time. This is the signal that either the device was
    /// reinstalled or somebody is impersonating it - both require the user to
    /// pair again.
    #[error("device {device} presented fingerprint {actual}, but we pinned {expected}")]
    FingerprintMismatch {
        /// Which device.
        device: DeviceId,
        /// Fingerprint recorded at pairing time.
        expected: String,
        /// Fingerprint seen now.
        actual: String,
    },

    /// A trusted device presented a public key that is not the one we pinned.
    #[error("device {device} presented a public key that differs from the pinned one")]
    PublicKeyMismatch {
        /// Which device.
        device: DeviceId,
    },

    /// A fingerprint string could not be parsed.
    #[error("invalid fingerprint: {0}")]
    InvalidFingerprint(String),

    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Persisted state could not be (de)serialized.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
