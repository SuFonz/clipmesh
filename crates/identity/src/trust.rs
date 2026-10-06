//! The trust store: the list of devices the user has explicitly paired with.
//!
//! This is the single source of truth for "may this device read my clipboard".
//! Everything about it is deliberately conservative:
//!
//! * A device is trusted only after a human said so (`TrustedDevice`).
//! * The certificate *and* its fingerprint are pinned, so a device that
//!   reinstalls and regenerates its key is treated as a stranger instead of
//!   silently continuing to sync.
//! * Verification compares fingerprints in constant time and never falls back
//!   to "well, the device id matches" - device ids are public.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clipmesh_protocol::{now_millis, DeviceId, Platform};
use serde::{Deserialize, Serialize};

use crate::certificate::DeviceCertificate;
use crate::error::{IdentityError, Result};
use crate::fingerprint::Fingerprint;

/// On-disk format version, so a future change can migrate instead of crash.
const STORE_VERSION: u32 = 1;

/// A device the user decided to trust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDevice {
    /// Stable device identifier.
    pub device_id: DeviceId,
    /// Display name at the time of pairing.
    pub name: String,
    /// Operating system family.
    pub platform: Platform,
    /// The device's Ed25519 public key, lowercase hex. Pinned.
    pub public_key: String,
    /// The device's self-signed certificate. Pinned.
    pub certificate: DeviceCertificate,
    /// sha256 of the certificate. Pinned, and what the user compared.
    pub fingerprint: Fingerprint,
    /// When the user accepted, unix milliseconds.
    pub trusted_at: i64,
    /// Last time we completed a handshake with it, unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<i64>,
}

impl TrustedDevice {
    /// Build a record from a verified certificate and public key.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the certificate does not
    /// carry exactly `public_key`.
    pub fn new(
        name: impl Into<String>,
        platform: Platform,
        certificate: DeviceCertificate,
        public_key: &[u8],
    ) -> Result<Self> {
        let device_id = certificate.device_id()?;
        let embedded = certificate.public_key_bytes()?;

        if embedded != public_key {
            return Err(IdentityError::PublicKeyMismatch { device: device_id });
        }

        Ok(Self {
            device_id,
            name: name.into(),
            platform,
            public_key: hex::encode(public_key),
            fingerprint: certificate.fingerprint(),
            certificate,
            trusted_at: now_millis(),
            last_seen_at: None,
        })
    }

    /// The pinned public key as raw bytes.
    ///
    /// # Errors
    /// Returns [`IdentityError::InvalidPublicKey`] when the stored hex is not a
    /// 32 byte key, which means the store was edited by hand.
    pub fn public_key_bytes(&self) -> Result<[u8; 32]> {
        let decoded = hex::decode(&self.public_key)
            .map_err(|error| IdentityError::InvalidPublicKey(error.to_string()))?;
        decoded.try_into().map_err(|_| {
            IdentityError::InvalidPublicKey(format!(
                "trusted device {0} has a {1} byte key",
                self.device_id,
                self.public_key.len() / 2
            ))
        })
    }

    /// Record that we just talked to this device.
    pub fn touch(&mut self) {
        self.last_seen_at = Some(now_millis());
    }
}

/// The trust store itself.
#[derive(Debug)]
pub struct TrustStore {
    path: Option<PathBuf>,
    devices: BTreeMap<DeviceId, TrustedDevice>,
}

impl TrustStore {
    /// Load from disk, or start empty when the file does not exist yet.
    ///
    /// # Errors
    /// Returns [`IdentityError::TrustStore`] when the file exists but cannot be
    /// parsed. A corrupt trust store is *never* silently replaced with an empty
    /// one: that would quietly drop every pairing the user ever made.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        if !path.exists() {
            return Ok(Self {
                path: Some(path),
                devices: BTreeMap::new(),
            });
        }

        let contents = std::fs::read_to_string(&path)?;
        let file: StoreFile = serde_json::from_str(&contents)?;

        if file.version > STORE_VERSION {
            return Err(IdentityError::TrustStore(format!(
                "{} was written by a newer ClipMesh (format {} > {STORE_VERSION})",
                path.display(),
                file.version
            )));
        }

        let mut devices = BTreeMap::new();
        for device in file.devices {
            // Cross-check the pinned fingerprint against the pinned certificate.
            // If they disagree the file was tampered with or corrupted, and we
            // refuse to start rather than guess which half to believe.
            let derived = device.certificate.fingerprint();
            if !derived.matches(&device.fingerprint) {
                return Err(IdentityError::TrustStore(format!(
                    "entry for {} has fingerprint {} but its certificate hashes to {}",
                    device.device_id,
                    device.fingerprint.to_hex(),
                    derived.to_hex()
                )));
            }
            devices.insert(device.device_id, device);
        }

        Ok(Self {
            path: Some(path),
            devices,
        })
    }

    /// A store that never touches the filesystem, for tests.
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            devices: BTreeMap::new(),
        }
    }

    /// Where this store persists to, if anywhere.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Persist the store atomically.
    ///
    /// Writes a temporary file in the same directory and renames it over the
    /// target, so a crash mid-write can never leave a half written trust store
    /// behind (which would lose every pairing).
    ///
    /// # Errors
    /// Returns [`IdentityError::Io`] / [`IdentityError::Serde`] on failure.
    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = StoreFile {
            version: STORE_VERSION,
            devices: self.devices.values().cloned().collect(),
        };
        let json = serde_json::to_string_pretty(&file)?;

        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, json)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Add or replace a trusted device.
    ///
    /// Returns `false` when the store already held an identical entry, so the
    /// caller can skip a redundant save.
    ///
    /// # Errors
    /// Propagates persistence failures.
    pub fn trust(&mut self, device: TrustedDevice) -> Result<bool> {
        let unchanged = self
            .devices
            .get(&device.device_id)
            .is_some_and(|existing| existing == &device);

        if unchanged {
            return Ok(false);
        }

        self.devices.insert(device.device_id, device);
        self.save()?;
        Ok(true)
    }

    /// Remove a device from the trust list. Returns whether it was there.
    ///
    /// # Errors
    /// Propagates persistence failures.
    pub fn forget(&mut self, device: DeviceId) -> Result<bool> {
        if self.devices.remove(&device).is_none() {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    /// Whether a device id is trusted.
    #[must_use]
    pub fn is_trusted(&self, device: DeviceId) -> bool {
        self.devices.contains_key(&device)
    }

    /// Look up a trusted device.
    #[must_use]
    pub fn get(&self, device: DeviceId) -> Option<&TrustedDevice> {
        self.devices.get(&device)
    }

    /// Every trusted device, most recently paired first.
    #[must_use]
    pub fn all(&self) -> Vec<TrustedDevice> {
        let mut devices: Vec<TrustedDevice> = self.devices.values().cloned().collect();
        devices.sort_by(|a, b| b.trusted_at.cmp(&a.trusted_at));
        devices
    }

    /// Number of trusted devices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// Whether no device is trusted yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Record that a handshake with a trusted device succeeded.
    ///
    /// # Errors
    /// Propagates persistence failures.
    pub fn touch(&mut self, device: DeviceId) -> Result<()> {
        if let Some(entry) = self.devices.get_mut(&device) {
            entry.touch();
            self.save()?;
        }
        Ok(())
    }

    /// Verify that a peer presenting `certificate` really is the device we
    /// pinned, and hand back its record.
    ///
    /// This is the function that decides whether an inbound TLS session is
    /// allowed to reach the clipboard. It checks three independent things:
    ///
    /// 1. the device id is in the store,
    /// 2. the certificate fingerprint matches the pinned one (constant time),
    /// 3. the claimed public key matches the pinned one *and* the certificate.
    ///
    /// # Errors
    /// Returns [`IdentityError::NotTrusted`], [`IdentityError::FingerprintMismatch`]
    /// or [`IdentityError::PublicKeyMismatch`].
    pub fn verify_peer(
        &self,
        device_id: DeviceId,
        certificate: &DeviceCertificate,
        public_key: &[u8],
    ) -> Result<&TrustedDevice> {
        let pinned = self
            .devices
            .get(&device_id)
            .ok_or(IdentityError::NotTrusted(device_id))?;

        let presented = certificate.fingerprint();
        if !pinned.fingerprint.matches(&presented) {
            return Err(IdentityError::FingerprintMismatch {
                device: device_id,
                expected: pinned.fingerprint.short(),
                actual: presented.short(),
            });
        }

        let pinned_key = pinned.public_key_bytes()?;
        if pinned_key.as_slice() != public_key {
            return Err(IdentityError::PublicKeyMismatch { device: device_id });
        }

        // The certificate must actually attest to the key being claimed.
        let attested = certificate.public_key_bytes()?;
        if attested.as_slice() != public_key || attested != pinned_key {
            return Err(IdentityError::PublicKeyMismatch { device: device_id });
        }

        Ok(pinned)
    }

    /// Remove every trusted device.
    ///
    /// # Errors
    /// Propagates persistence failures.
    pub fn clear(&mut self) -> Result<()> {
        self.devices.clear();
        self.save()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreFile {
    version: u32,
    devices: Vec<TrustedDevice>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::DeviceKey;

    fn peer(name: &str) -> (DeviceId, DeviceCertificate, [u8; 32]) {
        let key = DeviceKey::generate();
        let device_id = DeviceId::new();
        let certificate = DeviceCertificate::self_signed(&key, device_id, name).unwrap();
        (device_id, certificate, key.public_key_bytes())
    }

    fn trusted(name: &str) -> TrustedDevice {
        let (_, certificate, public_key) = peer(name);
        TrustedDevice::new(name, Platform::Linux, certificate, &public_key).unwrap()
    }

    #[test]
    fn an_empty_store_trusts_nobody() {
        let store = TrustStore::in_memory();
        assert!(store.is_empty());
        assert!(!store.is_trusted(DeviceId::new()));
    }

    #[test]
    fn trusting_a_device_makes_it_verifiable() {
        let mut store = TrustStore::in_memory();
        let device = trusted("Laptop");
        let device_id = device.device_id;
        let certificate = device.certificate.clone();
        let public_key = device.public_key_bytes().unwrap();

        assert!(store.trust(device).unwrap());
        assert!(store.is_trusted(device_id));
        assert_eq!(store.len(), 1);

        let verified = store
            .verify_peer(device_id, &certificate, &public_key)
            .unwrap();
        assert_eq!(verified.device_id, device_id);
    }

    #[test]
    fn re_trusting_an_identical_device_is_a_no_op() {
        let mut store = TrustStore::in_memory();
        let device = trusted("Laptop");
        assert!(store.trust(device.clone()).unwrap());
        assert!(!store.trust(device).unwrap());
    }

    #[test]
    fn an_unknown_device_is_rejected() {
        let store = TrustStore::in_memory();
        let (device_id, certificate, public_key) = peer("Stranger");
        assert!(matches!(
            store.verify_peer(device_id, &certificate, &public_key),
            Err(IdentityError::NotTrusted(_))
        ));
    }

    #[test]
    fn a_known_device_id_with_a_new_key_is_rejected() {
        // This is the reinstall / impersonation case: same device id, different
        // certificate. It must NOT be accepted just because the id matches.
        let mut store = TrustStore::in_memory();
        let device = trusted("Laptop");
        let device_id = device.device_id;
        store.trust(device).unwrap();

        let impostor_key = DeviceKey::generate();
        let impostor_certificate =
            DeviceCertificate::self_signed(&impostor_key, device_id, "Laptop").unwrap();

        assert!(matches!(
            store.verify_peer(device_id, &impostor_certificate, &impostor_key.public_key_bytes()),
            Err(IdentityError::FingerprintMismatch { .. })
        ));
    }

    #[test]
    fn the_pinned_certificate_with_a_different_claimed_key_is_rejected() {
        let mut store = TrustStore::in_memory();
        let device = trusted("Laptop");
        let device_id = device.device_id;
        let certificate = device.certificate.clone();
        store.trust(device).unwrap();

        let other_key = DeviceKey::generate().public_key_bytes();
        assert!(matches!(
            store.verify_peer(device_id, &certificate, &other_key),
            Err(IdentityError::PublicKeyMismatch { .. })
        ));
    }

    #[test]
    fn forgetting_a_device_removes_it() {
        let mut store = TrustStore::in_memory();
        let device = trusted("Phone");
        let device_id = device.device_id;
        store.trust(device).unwrap();

        assert!(store.forget(device_id).unwrap());
        assert!(!store.forget(device_id).unwrap());
        assert!(!store.is_trusted(device_id));
    }

    #[test]
    fn devices_are_listed_newest_first() {
        let mut store = TrustStore::in_memory();
        let older = trusted("Older");
        let mut newer = trusted("Newer");
        newer.trusted_at = older.trusted_at + 1000;

        store.trust(older).unwrap();
        store.trust(newer).unwrap();

        let names: Vec<String> = store.all().into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["Newer", "Older"]);
    }

    #[test]
    fn the_store_survives_a_save_and_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted_devices.json");

        let device = trusted("Persisted");
        let device_id = device.device_id;
        let certificate = device.certificate.clone();
        let public_key = device.public_key_bytes().unwrap();

        let mut store = TrustStore::load(&path).unwrap();
        store.trust(device).unwrap();

        let reloaded = TrustStore::load(&path).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert!(reloaded
            .verify_peer(device_id, &certificate, &public_key)
            .is_ok());
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let store = TrustStore::load(directory.path().join("nothing-here.json")).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn a_corrupt_store_is_reported_not_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted_devices.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        assert!(TrustStore::load(&path).is_err());
    }

    #[test]
    fn a_tampered_fingerprint_is_detected_on_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted_devices.json");

        let mut store = TrustStore::load(&path).unwrap();
        store.trust(trusted("Victim")).unwrap();

        // Swap the pinned fingerprint for an unrelated one.
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let forged = Fingerprint::of_certificate_der(b"forged");
        json["devices"][0]["fingerprint"] = serde_json::Value::String(forged.to_hex());
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        assert!(matches!(
            TrustStore::load(&path),
            Err(IdentityError::TrustStore(_))
        ));
    }

    #[test]
    fn clearing_empties_the_store() {
        let mut store = TrustStore::in_memory();
        store.trust(trusted("A")).unwrap();
        store.trust(trusted("B")).unwrap();
        store.clear().unwrap();
        assert!(store.is_empty());
    }
}
