//! Session level message classification.
//!
//! `clipmesh-protocol` knows how to turn bytes into an [`Envelope`]; this
//! module knows what an envelope means to a *session* - whether it belongs to
//! the handshake, is a keepalive, or should be handed to the engine.

use clipmesh_protocol::{Envelope, MessageKind, Result};

/// What a received envelope is, from the session's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// Part of the identity handshake.
    Handshake(MessageKind),
    /// A keepalive that the session answers itself.
    KeepAlive,
    /// A protocol level error report from the peer.
    PeerError,
    /// Something the engine should look at.
    Payload(MessageKind),
}

impl Incoming {
    /// The message kind behind this classification.
    #[must_use]
    pub const fn kind(self) -> MessageKind {
        match self {
            Self::Handshake(kind) | Self::Payload(kind) => kind,
            Self::KeepAlive => MessageKind::Ping,
            Self::PeerError => MessageKind::Error,
        }
    }

    /// Whether the engine should see this envelope.
    #[must_use]
    pub const fn is_payload(self) -> bool {
        matches!(self, Self::Payload(_))
    }
}

/// Classify an envelope.
///
/// # Errors
/// Returns [`ProtocolError`] when the envelope is empty or carries an
/// incompatible protocol version.
pub fn classify(envelope: &Envelope) -> Result<Incoming> {
    let kind = envelope.ensure_valid()?;

    Ok(match kind {
        MessageKind::Hello | MessageKind::HelloAck => Incoming::Handshake(kind),
        MessageKind::Ping | MessageKind::Pong => Incoming::KeepAlive,
        MessageKind::Error => Incoming::PeerError,
        _ => Incoming::Payload(kind),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_protocol::{DeviceId, Payload, TextPayload};

    fn envelope(payload: Payload) -> Envelope {
        Envelope::new(payload)
    }

    #[test]
    fn handshake_messages_are_recognised() {
        let hello = envelope(Payload::Hello(clipmesh_protocol::Hello::new(
            &clipmesh_protocol::DeviceInfo::local(DeviceId::new(), "x"),
            vec![0; 32],
            "fp",
            vec![0; 32],
            vec![0; 64],
        )));
        assert_eq!(
            classify(&hello).unwrap(),
            Incoming::Handshake(MessageKind::Hello)
        );
    }

    #[test]
    fn keepalives_are_handled_by_the_session() {
        assert_eq!(classify(&Envelope::ping()).unwrap(), Incoming::KeepAlive);
        assert_eq!(classify(&Envelope::pong(1)).unwrap(), Incoming::KeepAlive);
    }

    #[test]
    fn clipboard_messages_go_to_the_engine() {
        let text = TextPayload::new_local("hi", DeviceId::new());
        let envelope = Envelope::text(text.to_proto());
        let classified = classify(&envelope).unwrap();
        assert!(classified.is_payload());
        assert_eq!(classified.kind(), MessageKind::ClipboardText);
    }

    #[test]
    fn peer_errors_are_reported_distinctly() {
        let envelope = Envelope::error(clipmesh_protocol::ErrorCode::Internal, "boom");
        assert_eq!(classify(&envelope).unwrap(), Incoming::PeerError);
    }

    #[test]
    fn an_empty_envelope_is_a_protocol_error() {
        let mut envelope = Envelope::ack("some-id", true, "");
        envelope.payload = None;
        assert!(classify(&envelope).is_err());
    }
}
