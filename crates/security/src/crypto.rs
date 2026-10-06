//! Domain separated digests and the challenge/response primitives.
//!
//! Every signed value in ClipMesh is `sha256(domain ‖ part₀ ‖ part₁ …)` where
//! each part is prefixed with its own length. Two properties fall out of that:
//!
//! * **Domain separation** - a signature made for a pairing accept can never be
//!   replayed as a session hello, even though both are Ed25519 signatures over
//!   a mix of a nonce and a device id.
//! * **No concatenation ambiguity** - `("ab", "c")` and `("a", "bc")` hash
//!   differently, so an attacker cannot shift bytes between fields.

use rand::Rng as _;
use sha2::{Digest, Sha256};

/// Domain prefix for a session `Hello` signature.
pub const HELLO_DOMAIN: &[u8] = b"clipmesh-hello-v1";

/// Domain prefix for a `HelloAck` signature.
pub const HELLO_ACK_DOMAIN: &[u8] = b"clipmesh-hello-ack-v1";

/// Domain prefix for a pairing accept signature.
pub const PAIR_ACCEPT_DOMAIN: &[u8] = b"clipmesh-pair-accept-v1";

/// Length of every challenge and of the channel binding value.
pub const CHALLENGE_LENGTH: usize = 32;

/// TLS exporter label used to derive the channel binding value.
///
/// Per RFC 5705 / RFC 8446 §7.5 the label is not secret; what matters is that
/// the derived bytes are unique to this TLS connection.
pub const CHANNEL_BINDING_LABEL: &[u8] = b"EXPORTER-clipmesh-identity";

/// Optional exporter context. Empty, and passed explicitly rather than as
/// `None`, so both implementations derive the same value.
pub const CHANNEL_BINDING_CONTEXT: &[u8] = b"";

/// A fresh 32 byte challenge from the OS CSPRNG.
#[must_use]
pub fn random_challenge() -> [u8; CHALLENGE_LENGTH] {
    let mut bytes = [0u8; CHALLENGE_LENGTH];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// `sha256(domain ‖ len ‖ part …)`.
#[must_use]
pub fn domain_digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u32).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u32).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// What a `Hello` signature covers.
///
/// Binding the challenge *and* the TLS exporter means the signature is only
/// valid on this exact connection: an attacker who relays the TLS byte stream
/// still cannot make the signature verify on their own connection, because
/// their exporter secret differs.
#[must_use]
pub fn hello_signing_input(challenge: &[u8], channel_binding: &[u8]) -> [u8; 32] {
    domain_digest(HELLO_DOMAIN, &[challenge, channel_binding])
}

/// What a `HelloAck` signature covers.
#[must_use]
pub fn hello_ack_signing_input(peer_challenge: &[u8], channel_binding: &[u8]) -> [u8; 32] {
    domain_digest(HELLO_ACK_DOMAIN, &[peer_challenge, channel_binding])
}

/// What a pairing acceptance signature covers.
///
/// There is no channel binding here: the accept is sent over a connection that
/// is already TLS protected, and the nonce from the request is what makes it
/// single-use.
#[must_use]
pub fn pair_accept_signing_input(nonce: &[u8], responder_device_id: &str) -> [u8; 32] {
    domain_digest(
        PAIR_ACCEPT_DOMAIN,
        &[nonce, responder_device_id.as_bytes()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_domain_separated() {
        let challenge = [7u8; 32];
        let binding = [9u8; 32];
        assert_ne!(
            hello_signing_input(&challenge, &binding),
            hello_ack_signing_input(&challenge, &binding),
            "hello and hello_ack must not be interchangeable"
        );
        assert_ne!(
            hello_signing_input(&challenge, &binding),
            pair_accept_signing_input(&challenge, "device"),
        );
    }

    #[test]
    fn digests_are_field_bounded() {
        // ("ab","c") must not collide with ("a","bc").
        let first = domain_digest(b"d", &[b"ab", b"c"]);
        let second = domain_digest(b"d", &[b"a", b"bc"]);
        assert_ne!(first, second);

        // Field order matters.
        assert_ne!(
            domain_digest(b"d", &[b"x", b"y"]),
            domain_digest(b"d", &[b"y", b"x"])
        );
    }

    #[test]
    fn digests_are_deterministic() {
        let a = hello_signing_input(b"nonce", b"binding");
        let b = hello_signing_input(b"nonce", b"binding");
        assert_eq!(a, b);
    }

    #[test]
    fn a_different_channel_binding_changes_the_digest() {
        let same_challenge = [1u8; 32];
        assert_ne!(
            hello_signing_input(&same_challenge, &[2u8; 32]),
            hello_signing_input(&same_challenge, &[3u8; 32]),
            "the signature must be bound to one TLS connection"
        );
    }

    #[test]
    fn challenges_are_random() {
        let first = random_challenge();
        let second = random_challenge();
        assert_ne!(first, second);
        assert_ne!(first, [0u8; CHALLENGE_LENGTH]);
    }

    #[test]
    fn pair_accept_binds_the_responder() {
        assert_ne!(
            pair_accept_signing_input(b"nonce", "device-a"),
            pair_accept_signing_input(b"nonce", "device-b")
        );
    }
}
