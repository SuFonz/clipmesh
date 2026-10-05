//! Ergonomic constructors and accessors for protocol envelopes.
//!
//! The generated protobuf types are deliberately dumb data holders. Everything
//! that needs a decision (setting the protocol version, minting a message id,
//! naming a variant, checking compatibility) happens here so that no call site
//! can forget to do it.

use std::fmt;

use uuid::Uuid;

use crate::device::DeviceInfo;
use crate::error::{ProtocolError, Result};

pub use crate::proto::envelope::Payload;
pub use crate::proto::{
    Ack, ClipboardImage, ClipboardText, Envelope, ErrorCode, ErrorMessage, Hello, HelloAck,
    ImageChunk, PairAccept, PairRequest, Ping, Pong,
};

/// Wire protocol version implemented by this build.
///
/// Peers must agree exactly: a mismatch is reported as
/// [`ErrorCode::ProtocolVersionMismatch`] and the connection is closed rather
/// than risking a mis-parsed frame.
pub const PROTOCOL_VERSION: u32 = 1;

/// Which variant an [`Envelope`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKind {
    /// A new device asks to be trusted.
    PairRequest,
    /// The answer to a [`MessageKind::PairRequest`].
    PairAccept,
    /// First message of an authenticated session.
    Hello,
    /// Answer to a [`MessageKind::Hello`].
    HelloAck,
    /// A text clipboard payload.
    ClipboardText,
    /// Metadata for an image clipboard payload.
    ClipboardImage,
    /// One slice of an image payload.
    ImageChunk,
    /// Positive or negative acknowledgement.
    Ack,
    /// A protocol level error report.
    Error,
    /// Keepalive request.
    Ping,
    /// Keepalive answer.
    Pong,
}

impl MessageKind {
    /// Stable lowercase name, used in logs and error messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PairRequest => "pair_request",
            Self::PairAccept => "pair_accept",
            Self::Hello => "hello",
            Self::HelloAck => "hello_ack",
            Self::ClipboardText => "clipboard_text",
            Self::ClipboardImage => "clipboard_image",
            Self::ImageChunk => "image_chunk",
            Self::Ack => "ack",
            Self::Error => "error",
            Self::Ping => "ping",
            Self::Pong => "pong",
        }
    }

    /// Whether this message is part of session establishment.
    #[must_use]
    pub const fn is_handshake(self) -> bool {
        matches!(self, Self::Hello | Self::HelloAck | Self::PairRequest | Self::PairAccept)
    }

    /// Whether this message carries clipboard content.
    #[must_use]
    pub const fn is_clipboard(self) -> bool {
        matches!(
            self,
            Self::ClipboardText | Self::ClipboardImage | Self::ImageChunk
        )
    }
}

impl fmt::Display for MessageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl crate::proto::Envelope {
    /// Wrap a payload, stamping the current protocol version and a fresh
    /// message id.
    #[must_use]
    pub fn new(payload: Payload) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message_id: Uuid::new_v4().to_string(),
            payload: Some(payload),
        }
    }

    /// The variant this envelope carries, if it carries one.
    #[must_use]
    pub fn kind(&self) -> Option<MessageKind> {
        Some(match self.payload.as_ref()? {
            Payload::PairRequest(_) => MessageKind::PairRequest,
            Payload::PairAccept(_) => MessageKind::PairAccept,
            Payload::Hello(_) => MessageKind::Hello,
            Payload::HelloAck(_) => MessageKind::HelloAck,
            Payload::ClipboardText(_) => MessageKind::ClipboardText,
            Payload::ClipboardImage(_) => MessageKind::ClipboardImage,
            Payload::ImageChunk(_) => MessageKind::ImageChunk,
            Payload::Ack(_) => MessageKind::Ack,
            Payload::Error(_) => MessageKind::Error,
            Payload::Ping(_) => MessageKind::Ping,
            Payload::Pong(_) => MessageKind::Pong,
        })
    }

    /// Borrow the payload.
    #[must_use]
    pub fn payload(&self) -> Option<&Payload> {
        self.payload.as_ref()
    }

    /// Validate the envelope and return what it carries.
    ///
    /// # Errors
    /// Fails on a missing payload or an incompatible protocol version.
    pub fn ensure_valid(&self) -> Result<MessageKind> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch {
                peer: self.protocol_version,
                local: PROTOCOL_VERSION,
            });
        }
        self.kind().ok_or(ProtocolError::EmptyEnvelope)
    }

    /// Assert that this envelope carries `expected`.
    ///
    /// # Errors
    /// Returns [`ProtocolError::UnexpectedMessage`] when it does not.
    pub fn expect(&self, expected: MessageKind) -> Result<&Self> {
        let actual = self.ensure_valid()?;
        if actual == expected {
            Ok(self)
        } else {
            Err(ProtocolError::UnexpectedMessage {
                expected: expected.as_str(),
                actual: actual.as_str(),
            })
        }
    }

    /// Wrap a text payload.
    #[must_use]
    pub fn text(message: ClipboardText) -> Self {
        Self::new(Payload::ClipboardText(message))
    }

    /// Wrap image metadata.
    #[must_use]
    pub fn image(message: ClipboardImage) -> Self {
        Self::new(Payload::ClipboardImage(message))
    }

    /// Wrap one image chunk.
    #[must_use]
    pub fn image_chunk(message: ImageChunk) -> Self {
        Self::new(Payload::ImageChunk(message))
    }

    /// Build an acknowledgement for `id`.
    #[must_use]
    pub fn ack(id: impl Into<String>, ok: bool, message: impl Into<String>) -> Self {
        Self::new(Payload::Ack(Ack {
            id: id.into(),
            ok,
            message: message.into(),
        }))
    }

    /// Build an error report.
    #[must_use]
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(Payload::Error(ErrorMessage {
            code: code as i32,
            message: message.into(),
            related_id: String::new(),
        }))
    }

    /// Build a keepalive ping.
    #[must_use]
    pub fn ping() -> Self {
        Self::new(Payload::Ping(Ping {
            timestamp: crate::now_millis(),
        }))
    }

    /// Build the answer to a ping.
    #[must_use]
    pub fn pong(timestamp: i64) -> Self {
        Self::new(Payload::Pong(Pong { timestamp }))
    }
}

impl crate::proto::PairRequest {
    /// Describe this device and ask to be trusted.
    #[must_use]
    pub fn new(
        device: &DeviceInfo,
        public_key: Vec<u8>,
        certificate: Vec<u8>,
        fingerprint: impl Into<String>,
        nonce: Vec<u8>,
    ) -> Self {
        Self {
            device_id: device.device_id.to_string(),
            name: device.display_name().to_owned(),
            platform: device.platform as i32,
            public_key,
            certificate,
            fingerprint: fingerprint.into(),
            timestamp: crate::now_millis(),
            nonce,
        }
    }
}

impl crate::proto::PairAccept {
    /// Accept, proving ownership of `public_key` with `signature`.
    #[must_use]
    pub fn accepted(
        device: &DeviceInfo,
        public_key: Vec<u8>,
        certificate: Vec<u8>,
        fingerprint: impl Into<String>,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            accepted: true,
            device_id: device.device_id.to_string(),
            name: device.display_name().to_owned(),
            platform: device.platform as i32,
            public_key,
            certificate,
            fingerprint: fingerprint.into(),
            reason: String::new(),
            signature,
        }
    }

    /// Reject the request. `reason` is shown to the local user.
    #[must_use]
    pub fn rejected(device: &DeviceInfo, reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            device_id: device.device_id.to_string(),
            name: device.display_name().to_owned(),
            platform: device.platform as i32,
            public_key: Vec::new(),
            certificate: Vec::new(),
            fingerprint: String::new(),
            reason: reason.into(),
            signature: Vec::new(),
        }
    }
}

impl crate::proto::Hello {
    /// Announce our identity on a freshly established TLS session.
    #[must_use]
    pub fn new(
        device: &DeviceInfo,
        public_key: Vec<u8>,
        fingerprint: impl Into<String>,
        challenge: Vec<u8>,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            device_id: device.device_id.to_string(),
            name: device.display_name().to_owned(),
            platform: device.platform as i32,
            public_key,
            fingerprint: fingerprint.into(),
            challenge,
            signature,
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

impl crate::proto::HelloAck {
    /// Answer a [`crate::proto::Hello`].
    #[must_use]
    pub fn new(device_id: impl Into<String>, signature: Vec<u8>, trusted: bool) -> Self {
        Self {
            device_id: device_id.into(),
            signature,
            protocol_version: PROTOCOL_VERSION,
            trusted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::TextPayload;
    use crate::device::DeviceId;

    fn device() -> DeviceInfo {
        DeviceInfo::local(DeviceId::new(), "Test Rig")
    }

    #[test]
    fn new_envelopes_are_valid_and_stamped() {
        let payload = TextPayload::new_local("hi", DeviceId::new());
        let envelope = Envelope::text(payload.to_proto());
        assert_eq!(envelope.protocol_version, PROTOCOL_VERSION);
        assert!(!envelope.message_id.is_empty());
        assert_eq!(envelope.ensure_valid().unwrap(), MessageKind::ClipboardText);
    }

    #[test]
    fn empty_envelopes_are_rejected() {
        let envelope = Envelope {
            protocol_version: PROTOCOL_VERSION,
            message_id: "x".into(),
            payload: None,
        };
        assert!(matches!(
            envelope.ensure_valid(),
            Err(ProtocolError::EmptyEnvelope)
        ));
        assert_eq!(envelope.kind(), None);
    }

    #[test]
    fn version_mismatches_are_rejected() {
        let mut envelope = Envelope::ping();
        envelope.protocol_version = PROTOCOL_VERSION + 1;
        assert!(matches!(
            envelope.ensure_valid(),
            Err(ProtocolError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn expect_reports_the_actual_variant() {
        let envelope = Envelope::ping();
        let error = envelope.expect(MessageKind::Pong).unwrap_err();
        assert!(matches!(
            error,
            ProtocolError::UnexpectedMessage {
                expected: "pong",
                actual: "ping"
            }
        ));
    }

    #[test]
    fn pair_request_carries_the_device_description() {
        let device = device();
        let request = PairRequest::new(&device, vec![1; 32], vec![2; 8], "ABCD EF", vec![9; 32]);
        assert_eq!(request.device_id, device.device_id.to_string());
        assert_eq!(request.name, "Test Rig");
        assert_eq!(request.platform(), crate::Platform::current());
        assert!(request.timestamp > 0);
    }

    #[test]
    fn rejections_are_not_errors() {
        let accept = PairAccept::rejected(&device(), "user said no");
        assert!(!accept.accepted);
        assert_eq!(accept.reason, "user said no");
        assert!(accept.signature.is_empty());
    }

    #[test]
    fn kinds_are_classified() {
        assert!(MessageKind::Hello.is_handshake());
        assert!(!MessageKind::Hello.is_clipboard());
        assert!(MessageKind::ImageChunk.is_clipboard());
        assert_eq!(MessageKind::ClipboardText.as_str(), "clipboard_text");
    }
}
