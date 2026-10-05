//! Errors produced by the ClipMesh wire protocol layer.

/// Convenience alias used across the crate.
pub type Result<T, E = ProtocolError> = std::result::Result<T, E>;

/// Everything that can go wrong while encoding, decoding or validating a
/// protocol message.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// The frame could not be decoded into an `Envelope`.
    #[error("failed to decode protobuf message: {0}")]
    Decode(#[from] prost::DecodeError),

    /// The envelope could not be serialized.
    #[error("failed to encode protobuf message: {0}")]
    Encode(#[from] prost::EncodeError),

    /// A frame announced more bytes than we are willing to buffer. This is a
    /// hard limit so a hostile peer cannot make us allocate gigabytes.
    #[error("frame of {actual} bytes exceeds the {max} byte limit")]
    FrameTooLarge {
        /// Configured maximum.
        max: usize,
        /// Size announced by the peer.
        actual: usize,
    },

    /// A zero length frame is never legal: every frame is one envelope.
    #[error("received an empty frame")]
    EmptyFrame,

    /// The peer closed the connection cleanly between frames.
    #[error("connection closed by peer")]
    ConnectionClosed,

    /// Transport level failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The envelope carried no payload at all.
    #[error("envelope contains no payload")]
    EmptyEnvelope,

    /// The peer speaks a protocol version we do not implement.
    #[error("unsupported protocol version {peer} (this build speaks {local})")]
    VersionMismatch {
        /// Version advertised by the peer.
        peer: u32,
        /// Version implemented by this build.
        local: u32,
    },

    /// The message arrived where it was not expected in the state machine.
    #[error("unexpected message: expected {expected}, got {actual}")]
    UnexpectedMessage {
        /// What the state machine wanted.
        expected: &'static str,
        /// What actually arrived.
        actual: &'static str,
    },

    /// A device identifier was not a valid UUID.
    #[error("invalid device id: {0}")]
    InvalidDeviceId(String),

    /// A field had an invalid length (for example a 31 byte sha256 digest).
    #[error("invalid {field}: expected {expected} bytes, got {actual}")]
    InvalidFieldLength {
        /// Name of the offending field.
        field: &'static str,
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },

    /// The received payload did not match its advertised hash.
    #[error("checksum mismatch for payload {id}")]
    ChecksumMismatch {
        /// Identifier of the payload that failed verification.
        id: String,
    },

    /// A clipboard payload exceeded the configured ceiling.
    #[error("{kind} payload of {actual} bytes exceeds the {max} byte limit")]
    PayloadTooLarge {
        /// `text` or `image`.
        kind: &'static str,
        /// Configured maximum.
        max: u64,
        /// Actual size.
        actual: u64,
    },
}
