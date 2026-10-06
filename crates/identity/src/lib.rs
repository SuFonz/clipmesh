//! # clipmesh-identity
//!
//! Device identity for ClipMesh: an Ed25519 key pair, a self-signed X.509
//! certificate derived from it, a human readable fingerprint, and the trust
//! store of devices the user has paired with.
//!
//! ## The model
//!
//! * **Identity is a key pair**, not an account. [`DeviceIdentity::generate`]
//!   mints one on first launch; it is stored next to the app's config and never
//!   leaves the device.
//! * **A certificate attests to the key.** It is self-signed, carries the
//!   device id in its common name and the public key in its SubjectPublicKeyInfo.
//!   [`DeviceCertificate`]
//! * **A fingerprint is the trust anchor.** Users compare
//!   [`Fingerprint`]s out of band; the trust store pins the certificate *and*
//!   the fingerprint so a reinstalled device is treated as a new device rather
//!   than silently accepted. [`TrustStore`]
//!
//! ```no_run
//! use clipmesh_identity::{DeviceIdentity, IdentityPaths, TrustStore};
//!
//! let paths = IdentityPaths::discover().unwrap();
//! let identity = DeviceIdentity::load_or_create(&paths, "My Laptop").unwrap();
//! println!("this device is {} ({})", identity.device_id(), identity.fingerprint());
//!
//! let store = TrustStore::load(paths.trust_store()).unwrap();
//! println!("{} devices trusted", store.len());
//! ```

#![warn(missing_docs)]

pub mod certificate;
pub mod error;
pub mod fingerprint;
pub mod key;
pub mod store;
pub mod trust;

use std::sync::Arc;

use ed25519_dalek::VerifyingKey;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use clipmesh_protocol::{now_millis, DeviceId, DeviceInfo};

pub use certificate::DeviceCertificate;
pub use error::{IdentityError, Result};
pub use fingerprint::Fingerprint;
pub use key::{verify_signature, DeviceKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
pub use store::IdentityPaths;
pub use trust::{TrustStore, TrustedDevice};

/// On-disk format of `identity.json`.
const IDENTITY_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityFile {
    version: u32,
    device_id: DeviceId,
    name: String,
    created_at: i64,
}

/// This device's identity: key, certificate and display name.
///
/// Cheap to clone behind an [`Arc`]; the name is the only mutable part and is
/// guarded by a lock.
#[derive(Debug)]
pub struct DeviceIdentity {
    key: DeviceKey,
    certificate: DeviceCertificate,
    fingerprint: Fingerprint,
    device_id: DeviceId,
    name: RwLock<String>,
    paths: Option<IdentityPaths>,
}

impl DeviceIdentity {
    /// Create a brand new identity in memory.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] if certificate generation fails.
    pub fn generate(name: impl Into<String>) -> Result<Self> {
        let device_id = DeviceId::new();
        let name = name.into();
        let key = DeviceKey::generate();
        let certificate = DeviceCertificate::self_signed(&key, device_id, &name)?;
        let fingerprint = certificate.fingerprint();

        Ok(Self {
            key,
            certificate,
            fingerprint,
            device_id,
            name: RwLock::new(name),
            paths: None,
        })
    }

    /// Load the identity from `paths`, creating it on first run.
    ///
    /// The device id and name live in `identity.json`, the key in `device.key`
    /// and the certificate in `device.crt`. If the certificate does not match
    /// the key on disk (someone restored a stale backup, or the key was
    /// replaced) it is re-minted rather than left inconsistent.
    ///
    /// # Errors
    /// Returns [`IdentityError`] when the files exist but are unreadable, or
    /// when new material cannot be persisted.
    pub fn load_or_create(paths: &IdentityPaths, default_name: &str) -> Result<Self> {
        paths.ensure_dir()?;

        // --- key -----------------------------------------------------------
        let key = match store::read_optional(&paths.private_key())? {
            Some(der) => DeviceKey::from_pkcs8_der(&der)?,
            None => {
                let key = DeviceKey::generate();
                store::write_private_file(&paths.private_key(), &key.to_pkcs8_der()?)?;
                tracing::info!(path = %paths.private_key().display(), "generated a new device key");
                key
            }
        };

        // --- identity metadata --------------------------------------------
        let meta = match store::read_optional(&paths.identity_file())? {
            Some(bytes) => serde_json::from_slice::<IdentityFile>(&bytes)?,
            None => {
                let meta = IdentityFile {
                    version: IDENTITY_VERSION,
                    device_id: DeviceId::new(),
                    name: default_name.to_owned(),
                    created_at: now_millis(),
                };
                store::write_json_atomic(&paths.identity_file(), &meta)?;
                tracing::info!(device_id = %meta.device_id, "created a new device identity");
                meta
            }
        };

        // --- certificate ---------------------------------------------------
        let public_key = key.public_key_bytes();
        let existing = store::read_optional(&paths.certificate())?
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|pem| DeviceCertificate::from_pem(&pem).ok())
            .filter(|certificate| {
                certificate.device_id().ok() == Some(meta.device_id)
                    && certificate.public_key_bytes().ok() == Some(public_key)
            });

        let certificate = match existing {
            Some(certificate) => certificate,
            None => {
                let certificate =
                    DeviceCertificate::self_signed(&key, meta.device_id, &meta.name)?;
                store::write_private_file(&paths.certificate(), certificate.pem().as_bytes())?;
                tracing::info!(device_id = %meta.device_id, "issued a device certificate");
                certificate
            }
        };

        let fingerprint = certificate.fingerprint();
        Ok(Self {
            key,
            certificate,
            fingerprint,
            device_id: meta.device_id,
            name: RwLock::new(meta.name),
            paths: Some(paths.clone()),
        })
    }

    /// This device's identifier.
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    /// The Ed25519 private key, for TLS and for signing handshakes.
    ///
    /// Exposed because `clipmesh-security` needs it to build the TLS config.
    /// It is not re-exported to the UI or the network layer.
    #[must_use]
    pub fn key(&self) -> &DeviceKey {
        &self.key
    }

    /// The device's certificate.
    #[must_use]
    pub fn certificate(&self) -> &DeviceCertificate {
        &self.certificate
    }

    /// The pinned fingerprint users compare during pairing.
    #[must_use]
    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    /// 32 raw public key bytes.
    #[must_use]
    pub fn public_key(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.key.public_key_bytes()
    }

    /// Public key as lowercase hex, for JSON and the UI.
    #[must_use]
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key())
    }

    /// The public key as an `ed25519_dalek` type, for verification.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.key.verifying_key()
    }

    /// Current display name.
    #[must_use]
    pub fn name(&self) -> String {
        self.name.read().clone()
    }

    /// Rename the device and persist the change.
    ///
    /// The certificate is deliberately *not* re-issued: its subject is the
    /// common name (the device id) plus an informational OU, and re-issuing
    /// would change the fingerprint and invalidate every existing pairing.
    ///
    /// # Errors
    /// Returns [`IdentityError`] when the new name cannot be persisted.
    pub fn set_name(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(IdentityError::TrustStore(
                "the device name cannot be empty".to_owned(),
            ));
        }

        *self.name.write() = trimmed.to_owned();

        if let Some(paths) = &self.paths {
            let meta = IdentityFile {
                version: IDENTITY_VERSION,
                device_id: self.device_id,
                name: trimmed.to_owned(),
                created_at: now_millis(),
            };
            store::write_json_atomic(&paths.identity_file(), &meta)?;
        }
        Ok(())
    }

    /// A protocol level description of this device, for mDNS and handshakes.
    #[must_use]
    pub fn info(&self) -> DeviceInfo {
        DeviceInfo::local(self.device_id, self.name())
    }

    /// Sign a message with the device key.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LENGTH] {
        self.key.sign(message)
    }
}

/// Shared ownership wrapper used by the engine and the Tauri state.
pub type SharedIdentity = Arc<DeviceIdentity>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identities_are_self_consistent() {
        let identity = DeviceIdentity::generate("Test Rig").unwrap();
        assert_eq!(
            identity.certificate().device_id().unwrap(),
            identity.device_id()
        );
        assert_eq!(
            identity.certificate().public_key_bytes().unwrap(),
            identity.public_key()
        );
        assert_eq!(identity.certificate().fingerprint(), *identity.fingerprint());
    }

    #[test]
    fn signatures_verify_under_the_device_public_key() {
        let identity = DeviceIdentity::generate("Signer").unwrap();
        let signature = identity.sign(b"hello");
        assert!(verify_signature(&identity.public_key(), b"hello", &signature).is_ok());
        assert!(verify_signature(&identity.public_key(), b"hell0", &signature).is_err());
    }

    #[test]
    fn an_identity_survives_reloading() {
        let directory = tempfile::tempdir().unwrap();
        let paths = IdentityPaths::at(directory.path());

        let first = DeviceIdentity::load_or_create(&paths, "Persisted").unwrap();
        drop(first);

        let second = DeviceIdentity::load_or_create(&paths, "Ignored").unwrap();
        assert_eq!(second.device_id(), first_id(&paths));
        assert_eq!(second.name(), "Persisted");
        assert_eq!(second.public_key(), first_key(&paths));
    }

    fn first_id(paths: &IdentityPaths) -> DeviceId {
        let raw = std::fs::read_to_string(paths.identity_file()).unwrap();
        serde_json::from_str::<IdentityFile>(&raw).unwrap().device_id
    }

    fn first_key(paths: &IdentityPaths) -> [u8; PUBLIC_KEY_LENGTH] {
        let pem = std::fs::read_to_string(paths.certificate()).unwrap();
        DeviceCertificate::from_pem(&pem)
            .unwrap()
            .public_key_bytes()
            .unwrap()
    }

    #[test]
    fn reloading_keeps_the_same_fingerprint() {
        let directory = tempfile::tempdir().unwrap();
        let paths = IdentityPaths::at(directory.path());

        let first = DeviceIdentity::load_or_create(&paths, "Stable").unwrap();
        let fingerprint = *first.fingerprint();
        drop(first);

        let second = DeviceIdentity::load_or_create(&paths, "Stable").unwrap();
        assert_eq!(*second.fingerprint(), fingerprint);
    }

    #[test]
    fn a_stale_certificate_is_reissued() {
        let directory = tempfile::tempdir().unwrap();
        let paths = IdentityPaths::at(directory.path());
        let identity = DeviceIdentity::load_or_create(&paths, "Replace me").unwrap();
        let device_id = identity.device_id();
        drop(identity);

        // Overwrite the certificate with an unrelated one.
        let other_key = DeviceKey::generate();
        let other = DeviceCertificate::self_signed(&other_key, DeviceId::new(), "Other").unwrap();
        std::fs::write(paths.certificate(), other.pem()).unwrap();

        let reloaded = DeviceIdentity::load_or_create(&paths, "Replace me").unwrap();
        assert_eq!(reloaded.device_id(), device_id);
        assert_ne!(
            reloaded.certificate().fingerprint(),
            other.fingerprint(),
            "a certificate that does not match the key must be re-issued"
        );
        assert_eq!(
            reloaded.certificate().public_key_bytes().unwrap(),
            reloaded.public_key()
        );
    }

    #[test]
    fn renaming_persists_and_keeps_the_identity() {
        let directory = tempfile::tempdir().unwrap();
        let paths = IdentityPaths::at(directory.path());

        let identity = DeviceIdentity::load_or_create(&paths, "Old name").unwrap();
        let fingerprint = *identity.fingerprint();
        identity.set_name("  New name  ").unwrap();
        assert_eq!(identity.name(), "New name");
        drop(identity);

        let reloaded = DeviceIdentity::load_or_create(&paths, "Old name").unwrap();
        assert_eq!(reloaded.name(), "New name");
        assert_eq!(*reloaded.fingerprint(), fingerprint);
    }

    #[test]
    fn blank_names_are_rejected() {
        let identity = DeviceIdentity::generate("Fine").unwrap();
        assert!(identity.set_name("   ").is_err());
        assert_eq!(identity.name(), "Fine");
    }

    #[test]
    fn info_describes_this_platform() {
        let identity = DeviceIdentity::generate("Here").unwrap();
        let info = identity.info();
        assert_eq!(info.device_id, identity.device_id());
        assert_eq!(info.platform, clipmesh_protocol::Platform::current());
    }
}
