//! Session lifecycle and the application level identity handshake.
//!
//! TLS proves that the peer holds the private key for *some* certificate. This
//! module turns that into "this is device X, and it is on the other end of
//! *this* connection":
//!
//! ```text
//! TCP connect
//!   ↓
//! TLS handshake            (mutual auth, policy checked - see tls.rs)
//!   ↓
//! Hello / HelloAck         (Ed25519 signature over challenge ‖ TLS exporter)
//!   ↓
//! Active                   (payloads allowed)
//! ```
//!
//! The exporter secret is what makes the handshake non-relayable. Both ends
//! derive the same bytes from the TLS master secret, so a signature made for
//! one connection simply does not verify on another - an attacker in the middle
//! cannot forward an honest `Hello` onto their own session.

use clipmesh_identity::{verify_signature, DeviceCertificate, DeviceIdentity, Fingerprint};
use clipmesh_protocol::{
    DeviceId, Hello, HelloAck, PairAccept, Platform, PROTOCOL_VERSION,
};

use crate::crypto::{
    hello_ack_signing_input, hello_signing_input, pair_accept_signing_input, random_challenge,
    CHALLENGE_LENGTH,
};
use crate::error::{SecurityError, Result};

/// Where a connection is in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Socket is open, nothing negotiated yet.
    TcpConnected,
    /// TLS finished; the peer certificate passed the trust policy.
    TlsEstablished,
    /// The `Hello` exchange proved who the peer is.
    IdentityVerified,
    /// Payloads are flowing.
    Active,
    /// The session is finished and must not be reused.
    Closed,
}

impl SessionState {
    /// Stable lowercase name, for logs and error messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TcpConnected => "tcp_connected",
            Self::TlsEstablished => "tls_established",
            Self::IdentityVerified => "identity_verified",
            Self::Active => "active",
            Self::Closed => "closed",
        }
    }

    /// Whether payloads may be exchanged in this state.
    #[must_use]
    pub const fn allows_payloads(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Guards the allowed transitions, so no code path can skip identity
/// verification and start syncing on a bare TLS session.
#[derive(Debug, Clone)]
pub struct SessionStateMachine {
    state: SessionState,
}

impl SessionStateMachine {
    /// A new connection, before TLS.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::TcpConnected,
        }
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Whether payloads may flow.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.state.allows_payloads()
    }

    /// Move to the next state.
    ///
    /// # Errors
    /// Returns [`SecurityError::InvalidState`] for any transition that would
    /// skip a security step, or move after the session closed.
    pub fn transition(&mut self, next: SessionState) -> Result<()> {
        let allowed = matches!(
            (self.state, next),
            (SessionState::TcpConnected, SessionState::TlsEstablished)
                | (SessionState::TlsEstablished, SessionState::IdentityVerified)
                | (SessionState::IdentityVerified, SessionState::Active)
                | (_, SessionState::Closed)
        );

        if !allowed {
            return Err(SecurityError::InvalidState {
                expected: match self.state {
                    SessionState::TcpConnected => "tls_established",
                    SessionState::TlsEstablished => "identity_verified",
                    SessionState::IdentityVerified => "active",
                    SessionState::Active => "closed",
                    SessionState::Closed => "nothing (session is closed)",
                },
                actual: next.as_str(),
            });
        }

        self.state = next;
        Ok(())
    }
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// The 32 random bytes this side contributes to the handshake.
///
/// Kept in a newtype so it cannot be confused with the channel binding value -
/// both are 32 bytes and both feed the same signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeChallenge([u8; CHALLENGE_LENGTH]);

impl HandshakeChallenge {
    /// Fresh challenge from the OS CSPRNG.
    #[must_use]
    pub fn generate() -> Self {
        Self(random_challenge())
    }

    /// Wrap existing bytes, for tests and for the responder side.
    #[must_use]
    pub fn from_bytes(bytes: [u8; CHALLENGE_LENGTH]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; CHALLENGE_LENGTH] {
        &self.0
    }
}

/// Who the peer proved to be.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    /// Stable device identifier, taken from the TLS certificate.
    pub device_id: DeviceId,
    /// Display name the peer claimed.
    pub name: String,
    /// Platform the peer claimed.
    pub platform: Platform,
    /// The peer's Ed25519 public key, checked against its certificate.
    pub public_key: [u8; 32],
    /// Fingerprint of the peer's certificate.
    pub fingerprint: Fingerprint,
    /// The certificate the peer presented during the TLS handshake.
    pub certificate: DeviceCertificate,
    /// The challenge the peer sent, needed to answer with a `HelloAck`.
    pub challenge: HandshakeChallenge,
}

/// Build our `Hello`.
///
/// # Errors
/// Returns [`SecurityError::ChannelBinding`] if TLS was not established yet.
pub fn build_hello(
    identity: &DeviceIdentity,
    challenge: &HandshakeChallenge,
    channel_binding: &[u8],
) -> Result<Hello> {
    let input = hello_signing_input(challenge.as_bytes(), channel_binding);
    let signature = identity.sign(&input);

    Ok(Hello::new(
        &identity.info(),
        identity.public_key().to_vec(),
        identity.fingerprint().to_hex(),
        challenge.as_bytes().to_vec(),
        signature.to_vec(),
    ))
}

/// Check a peer's `Hello` against the certificate it presented over TLS.
///
/// Four independent things must line up:
///
/// 1. the protocol version,
/// 2. the device id in the `Hello` equals the one in the certificate,
/// 3. the public key in the `Hello` is the one inside the certificate,
/// 4. the signature verifies over `challenge ‖ channel_binding`.
///
/// Step 3 is what stops a peer from claiming somebody else's key, and step 4 is
/// what stops the message from being replayed onto another connection.
///
/// # Errors
/// Returns [`SecurityError::IdentityVerification`] or
/// [`SecurityError::UnexpectedDevice`] when anything does not line up.
pub fn verify_hello(
    hello: &Hello,
    channel_binding: &[u8],
    observed: &DeviceCertificate,
) -> Result<PeerIdentity> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(SecurityError::IdentityVerification(format!(
            "peer speaks protocol version {}, this build speaks {PROTOCOL_VERSION}",
            hello.protocol_version
        )));
    }

    let certificate_device = observed.device_id().map_err(|error| {
        SecurityError::IdentityVerification(format!("peer certificate is unusable: {error}"))
    })?;

    let claimed_device = DeviceId::parse(&hello.device_id).map_err(|error| {
        SecurityError::IdentityVerification(format!("peer sent a malformed device id: {error}"))
    })?;

    if claimed_device != certificate_device {
        return Err(SecurityError::UnexpectedDevice {
            expected: certificate_device.to_string(),
            actual: claimed_device.to_string(),
        });
    }

    let certificate_key = observed.public_key_bytes().map_err(|error| {
        SecurityError::IdentityVerification(format!("peer certificate is unusable: {error}"))
    })?;

    if hello.public_key.as_slice() != certificate_key {
        return Err(SecurityError::IdentityVerification(
            "the public key in the Hello is not the one in the peer's certificate".to_owned(),
        ));
    }

    let expected_fingerprint = observed.fingerprint().to_hex();
    if !hello.fingerprint.is_empty() && hello.fingerprint != expected_fingerprint {
        return Err(SecurityError::IdentityVerification(format!(
            "peer advertised fingerprint {} but its certificate hashes to {expected_fingerprint}",
            hello.fingerprint
        )));
    }

    let input = hello_signing_input(&hello.challenge, channel_binding);
    verify_signature(&hello.public_key, &input, &hello.signature).map_err(|error| {
        SecurityError::IdentityVerification(format!(
            "the Hello signature did not verify - the peer does not hold the private key for \
             its certificate, or the message was relayed onto another connection: {error}"
        ))
    })?;

    let challenge_bytes: [u8; CHALLENGE_LENGTH] =
        hello.challenge.as_slice().try_into().map_err(|_| {
            SecurityError::IdentityVerification(format!(
                "peer sent a {} byte challenge, expected {CHALLENGE_LENGTH}",
                hello.challenge.len()
            ))
        })?;

    Ok(PeerIdentity {
        device_id: claimed_device,
        name: hello.name.clone(),
        platform: hello.platform(),
        public_key: certificate_key,
        fingerprint: observed.fingerprint(),
        certificate: observed.clone(),
        challenge: HandshakeChallenge::from_bytes(challenge_bytes),
    })
}

/// Answer a peer's `Hello`.
#[must_use]
pub fn build_hello_ack(
    identity: &DeviceIdentity,
    peer_challenge: &HandshakeChallenge,
    channel_binding: &[u8],
    trusted: bool,
) -> HelloAck {
    let input = hello_ack_signing_input(peer_challenge.as_bytes(), channel_binding);
    let signature = identity.sign(&input);

    HelloAck::new(identity.device_id().to_string(), signature.to_vec(), trusted)
}

/// Check the peer's `HelloAck`.
///
/// The signature covers *our* challenge, so a valid answer proves the peer saw
/// the `Hello` we just sent on this connection - it cannot be a recorded reply
/// from an earlier session.
///
/// # Errors
/// Returns [`SecurityError::IdentityVerification`] when the answer is not a
/// valid proof for `expected_device`.
pub fn verify_hello_ack(
    ack: &HelloAck,
    our_challenge: &HandshakeChallenge,
    channel_binding: &[u8],
    peer_public_key: &[u8],
    expected_device: DeviceId,
) -> Result<()> {
    if ack.protocol_version != PROTOCOL_VERSION {
        return Err(SecurityError::IdentityVerification(format!(
            "peer answered with protocol version {}, this build speaks {PROTOCOL_VERSION}",
            ack.protocol_version
        )));
    }

    let answering_device = DeviceId::parse(&ack.device_id).map_err(|error| {
        SecurityError::IdentityVerification(format!("peer sent a malformed device id: {error}"))
    })?;

    if answering_device != expected_device {
        return Err(SecurityError::UnexpectedDevice {
            expected: expected_device.to_string(),
            actual: answering_device.to_string(),
        });
    }

    let input = hello_ack_signing_input(our_challenge.as_bytes(), channel_binding);
    verify_signature(peer_public_key, &input, &ack.signature).map_err(|error| {
        SecurityError::IdentityVerification(format!(
            "the HelloAck signature did not verify: {error}"
        ))
    })
}

/// Build the proof that we accepted a pairing request.
///
/// # Errors
/// Never fails today; returns `Result` so a future key type can.
pub fn build_pair_accept(
    identity: &DeviceIdentity,
    request_nonce: &[u8],
    trusted: bool,
) -> Result<PairAccept> {
    let input = pair_accept_signing_input(request_nonce, &identity.device_id().to_string());
    let signature = identity.sign(&input);

    if trusted {
        Ok(PairAccept::accepted(
            &identity.info(),
            identity.public_key().to_vec(),
            identity.certificate().to_der(),
            identity.fingerprint().to_hex(),
            signature.to_vec(),
        ))
    } else {
        Ok(PairAccept::rejected(&identity.info(), "rejected by the user"))
    }
}

/// Check the proof inside a `PairAccept`.
///
/// # Errors
/// Returns [`SecurityError::IdentityVerification`] when the responder did not
/// prove ownership of the key it advertised.
pub fn verify_pair_accept(
    accept: &PairAccept,
    request_nonce: &[u8],
    expected_device: DeviceId,
) -> Result<()> {
    let responder = DeviceId::parse(&accept.device_id).map_err(|error| {
        SecurityError::IdentityVerification(format!("malformed device id in PairAccept: {error}"))
    })?;

    if responder != expected_device {
        return Err(SecurityError::UnexpectedDevice {
            expected: expected_device.to_string(),
            actual: responder.to_string(),
        });
    }

    let input = pair_accept_signing_input(request_nonce, &responder.to_string());
    verify_signature(&accept.public_key, &input, &accept.signature).map_err(|error| {
        SecurityError::IdentityVerification(format!(
            "the PairAccept signature did not verify: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_protocol::DeviceInfo;

    const BINDING: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn peer() -> (DeviceIdentity, DeviceCertificate) {
        let identity = DeviceIdentity::generate("Peer").unwrap();
        let certificate = identity.certificate().clone();
        (identity, certificate)
    }

    #[test]
    fn the_state_machine_refuses_to_skip_identity_verification() {
        let mut machine = SessionStateMachine::new();
        assert_eq!(machine.state(), SessionState::TcpConnected);

        // Straight to Active must not be possible.
        assert!(machine.transition(SessionState::Active).is_err());
        assert!(machine.transition(SessionState::IdentityVerified).is_err());

        machine.transition(SessionState::TlsEstablished).unwrap();
        machine.transition(SessionState::IdentityVerified).unwrap();
        assert!(!machine.is_active());
        machine.transition(SessionState::Active).unwrap();
        assert!(machine.is_active());
    }

    #[test]
    fn a_closed_session_cannot_be_revived() {
        let mut machine = SessionStateMachine::new();
        machine.transition(SessionState::Closed).unwrap();
        assert!(machine.transition(SessionState::TlsEstablished).is_err());
        assert!(machine.transition(SessionState::Active).is_err());
    }

    #[test]
    fn a_hello_verifies_against_the_certificate_that_sent_it() {
        let (identity, certificate) = peer();
        let challenge = HandshakeChallenge::generate();
        let hello = build_hello(&identity, &challenge, BINDING).unwrap();

        let verified = verify_hello(&hello, BINDING, &certificate).unwrap();
        assert_eq!(verified.device_id, identity.device_id());
        assert_eq!(verified.public_key, identity.public_key());
        assert_eq!(verified.fingerprint, *identity.fingerprint());
        assert_eq!(verified.challenge, challenge);
        assert_eq!(verified.name, "Peer");
    }

    #[test]
    fn a_hello_relayed_onto_another_connection_fails() {
        // The channel binding is what a man in the middle cannot reproduce.
        let (identity, certificate) = peer();
        let challenge = HandshakeChallenge::generate();
        let hello = build_hello(&identity, &challenge, BINDING).unwrap();

        let other_connection = b"ffffffffffffffffffffffffffffffff";
        assert!(matches!(
            verify_hello(&hello, other_connection, &certificate),
            Err(SecurityError::IdentityVerification(_))
        ));
    }

    #[test]
    fn a_hello_signed_by_another_device_fails() {
        let (victim, certificate) = peer();
        let attacker = DeviceIdentity::generate("Attacker").unwrap();
        let challenge = HandshakeChallenge::generate();

        // The attacker builds a Hello but has to claim the victim's identity
        // to match the certificate that will be presented.
        let mut forged = build_hello(&attacker, &challenge, BINDING).unwrap();
        forged.device_id = victim.device_id().to_string();
        forged.public_key = victim.public_key().to_vec();
        forged.fingerprint = victim.fingerprint().to_hex();

        assert!(matches!(
            verify_hello(&forged, BINDING, &certificate),
            Err(SecurityError::IdentityVerification(_))
        ));
    }

    #[test]
    fn claiming_a_key_that_is_not_in_the_certificate_fails() {
        let (identity, certificate) = peer();
        let challenge = HandshakeChallenge::generate();
        let mut hello = build_hello(&identity, &challenge, BINDING).unwrap();

        hello.public_key = DeviceIdentity::generate("Other").unwrap().public_key().to_vec();
        assert!(matches!(
            verify_hello(&hello, BINDING, &certificate),
            Err(SecurityError::IdentityVerification(_))
        ));
    }

    #[test]
    fn a_hello_claiming_another_device_id_fails() {
        let (identity, certificate) = peer();
        let challenge = HandshakeChallenge::generate();
        let mut hello = build_hello(&identity, &challenge, BINDING).unwrap();

        hello.device_id = DeviceId::new().to_string();
        assert!(matches!(
            verify_hello(&hello, BINDING, &certificate),
            Err(SecurityError::UnexpectedDevice { .. })
        ));
    }

    #[test]
    fn a_wrong_protocol_version_is_refused() {
        let (identity, certificate) = peer();
        let challenge = HandshakeChallenge::generate();
        let mut hello = build_hello(&identity, &challenge, BINDING).unwrap();

        hello.protocol_version = PROTOCOL_VERSION + 1;
        assert!(matches!(
            verify_hello(&hello, BINDING, &certificate),
            Err(SecurityError::IdentityVerification(_))
        ));
    }

    #[test]
    fn a_hello_ack_proves_the_peer_saw_our_challenge() {
        let (responder, _) = peer();
        let our_challenge = HandshakeChallenge::generate();

        let ack = build_hello_ack(&responder, &our_challenge, BINDING, true);
        assert!(verify_hello_ack(
            &ack,
            &our_challenge,
            BINDING,
            &responder.public_key(),
            responder.device_id()
        )
        .is_ok());
    }

    #[test]
    fn a_hello_ack_for_a_different_challenge_is_refused() {
        let (responder, _) = peer();
        let our_challenge = HandshakeChallenge::generate();
        let ack = build_hello_ack(&responder, &HandshakeChallenge::generate(), BINDING, true);

        assert!(verify_hello_ack(
            &ack,
            &our_challenge,
            BINDING,
            &responder.public_key(),
            responder.device_id()
        )
        .is_err());
    }

    #[test]
    fn a_hello_ack_from_another_device_is_refused() {
        let (responder, _) = peer();
        let impostor = DeviceIdentity::generate("Impostor").unwrap();
        let our_challenge = HandshakeChallenge::generate();
        let ack = build_hello_ack(&responder, &our_challenge, BINDING, true);

        assert!(matches!(
            verify_hello_ack(
                &ack,
                &our_challenge,
                BINDING,
                &impostor.public_key(),
                responder.device_id()
            ),
            Err(SecurityError::IdentityVerification(_))
        ));

        assert!(matches!(
            verify_hello_ack(&ack, &our_challenge, BINDING, &responder.public_key(), DeviceId::new()),
            Err(SecurityError::UnexpectedDevice { .. })
        ));
    }

    #[test]
    fn a_pair_accept_is_bound_to_the_nonce_and_responder() {
        let (responder, _) = peer();
        let nonce = [3u8; 32];
        let accept = build_pair_accept(&responder, &nonce, true).unwrap();

        assert!(accept.accepted);
        assert!(verify_pair_accept(&accept, &nonce, responder.device_id()).is_ok());
        // A different nonce must not verify.
        assert!(verify_pair_accept(&accept, &[4u8; 32], responder.device_id()).is_err());
        // Neither must a different responder.
        assert!(verify_pair_accept(&accept, &nonce, DeviceId::new()).is_err());
    }

    #[test]
    fn a_rejected_pair_request_carries_no_signature() {
        let (responder, _) = peer();
        let accept = build_pair_accept(&responder, &[0u8; 32], false).unwrap();
        assert!(!accept.accepted);
        assert!(accept.signature.is_empty());
        assert!(verify_pair_accept(&accept, &[0u8; 32], responder.device_id()).is_err());
    }

    #[test]
    fn info_is_carried_through_the_handshake() {
        let identity = DeviceIdentity::generate("Living Room PC").unwrap();
        let certificate = identity.certificate().clone();
        let challenge = HandshakeChallenge::generate();
        let hello = build_hello(&identity, &challenge, BINDING).unwrap();

        let verified = verify_hello(&hello, BINDING, &certificate).unwrap();
        let expected: DeviceInfo = identity.info();
        assert_eq!(verified.name, expected.name);
        assert_eq!(verified.platform, expected.platform);
    }
}
