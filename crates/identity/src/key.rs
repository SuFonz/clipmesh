//! The device's long term Ed25519 key.
//!
//! One key pair per installation, generated on first launch and stored as
//! PKCS#8. Everything else in ClipMesh - the certificate, the trust store, the
//! session handshake - hangs off this key, so it is the one piece of state
//! that must never be regenerated silently: doing so would look like a brand
//! new device to every peer.

use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::rng;
use zeroize::Zeroizing;

use crate::error::{IdentityError, Result};

/// Raw public key length, in bytes.
pub const PUBLIC_KEY_LENGTH: usize = 32;

/// Signature length, in bytes.
pub const SIGNATURE_LENGTH: usize = 64;

/// A device's Ed25519 signing key.
///
/// The private half never leaves this struct: callers can ask for signatures
/// and for the public key, but there is no accessor that hands out the secret.
#[derive(Clone)]
pub struct DeviceKey {
    signing: SigningKey,
}

impl DeviceKey {
    /// Generate a fresh key from the operating system CSPRNG.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut rng()),
        }
    }

    /// Load a key from PKCS#8 DER.
    ///
    /// Accepts both the classic v1 encoding (48 bytes for Ed25519) and the v2
    /// encoding with the public key attached (83 bytes), because
    /// `ed25519-dalek` writes v2 by default.
    ///
    /// # Errors
    /// Returns [`IdentityError::KeyEncoding`] when the bytes are not a valid
    /// Ed25519 PKCS#8 private key.
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self> {
        let signing = SigningKey::from_pkcs8_der(der).map_err(|error| {
            IdentityError::KeyEncoding(format!("could not decode PKCS#8 private key: {error}"))
        })?;
        Ok(Self { signing })
    }

    /// Serialize the key as PKCS#8 DER.
    ///
    /// The buffer is wrapped in [`Zeroizing`] so it is wiped when dropped.
    ///
    /// # Errors
    /// Returns [`IdentityError::KeyEncoding`] if the encoding fails.
    pub fn to_pkcs8_der(&self) -> Result<Zeroizing<Vec<u8>>> {
        let der = self
            .signing
            .to_pkcs8_der()
            .map_err(|error| IdentityError::KeyEncoding(error.to_string()))?;
        Ok(Zeroizing::new(der.as_bytes().to_vec()))
    }

    /// The 32 raw public key bytes.
    #[must_use]
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.signing.verifying_key().to_bytes()
    }

    /// The public half, for verification.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// Sign a message.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LENGTH] {
        self.signing.sign(message).to_bytes()
    }
}

impl std::fmt::Debug for DeviceKey {
    /// Deliberately opaque: a `Debug` print of a key must never leak key bytes
    /// into a log file.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKey")
            .field("public_key", &hex::encode(self.public_key_bytes()))
            .finish_non_exhaustive()
    }
}

/// Turn 32 raw bytes into a public key.
///
/// # Errors
/// Returns [`IdentityError::InvalidPublicKey`] when the bytes are the wrong
/// length or do not decode to a point on the curve.
pub fn verifying_key_from_bytes(bytes: &[u8]) -> Result<VerifyingKey> {
    let array: [u8; PUBLIC_KEY_LENGTH] = bytes.try_into().map_err(|_| {
        IdentityError::InvalidPublicKey(format!(
            "expected {PUBLIC_KEY_LENGTH} bytes, got {}",
            bytes.len()
        ))
    })?;

    VerifyingKey::from_bytes(&array).map_err(|error| {
        IdentityError::InvalidPublicKey(format!("not a valid Ed25519 curve point: {error}"))
    })
}

/// Verify a detached signature.
///
/// Uses `verify_strict`, which additionally rejects small-order public keys and
/// non-canonical signatures. Plain `verify` accepts those, and for a protocol
/// where a signature decides whether a device gets to read the clipboard, that
/// permissiveness is not worth the compatibility.
///
/// # Errors
/// Returns [`IdentityError::Signature`] when the key is malformed or the
/// signature does not match.
pub fn verify_signature(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    let key = verifying_key_from_bytes(public_key)?;

    let signature = Signature::from_slice(signature).map_err(|error| {
        IdentityError::Signature(format!(
            "malformed signature (expected {SIGNATURE_LENGTH} bytes): {error}"
        ))
    })?;

    key.verify_strict(message, &signature)
        .map_err(|error| IdentityError::Signature(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_round_trip() {
        let key = DeviceKey::generate();
        let signature = key.sign(b"clipmesh");
        assert!(verify_signature(&key.public_key_bytes(), b"clipmesh", &signature).is_ok());
    }

    #[test]
    fn a_tampered_message_fails_verification() {
        let key = DeviceKey::generate();
        let signature = key.sign(b"clipmesh");
        assert!(verify_signature(&key.public_key_bytes(), b"clipmesg", &signature).is_err());
    }

    #[test]
    fn another_device_cannot_forge_a_signature() {
        let signer = DeviceKey::generate();
        let impostor = DeviceKey::generate();
        let signature = signer.sign(b"clipmesh");
        assert!(verify_signature(&impostor.public_key_bytes(), b"clipmesh", &signature).is_err());
    }

    #[test]
    fn keys_survive_a_pkcs8_round_trip() {
        let key = DeviceKey::generate();
        let der = key.to_pkcs8_der().unwrap();
        let restored = DeviceKey::from_pkcs8_der(&der).unwrap();
        assert_eq!(key.public_key_bytes(), restored.public_key_bytes());

        // A signature from the restored key verifies under the original key.
        let signature = restored.sign(b"persisted");
        assert!(verify_signature(&key.public_key_bytes(), b"persisted", &signature).is_ok());
    }

    #[test]
    fn malformed_keys_are_rejected() {
        assert!(DeviceKey::from_pkcs8_der(&[0u8; 8]).is_err());
        assert!(verifying_key_from_bytes(&[0u8; 31]).is_err());
        let key = DeviceKey::generate();
        assert!(verify_signature(&key.public_key_bytes(), b"x", &[0u8; 12]).is_err());
    }

    #[test]
    fn debug_does_not_leak_the_private_key() {
        let key = DeviceKey::generate();
        let der = key.to_pkcs8_der().unwrap();
        let rendered = format!("{key:?}");
        // The secret scalar must not appear anywhere in the Debug output.
        let secret_hex = hex::encode(&der[16..48]);
        assert!(!rendered.contains(&secret_hex));
        assert!(rendered.contains(&hex::encode(key.public_key_bytes())));
    }
}
