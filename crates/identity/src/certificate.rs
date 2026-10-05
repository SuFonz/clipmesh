//! Self-signed device certificates.
//!
//! ClipMesh has no CA. Each device mints its own short-lived-to-decade X.509
//! certificate, signed by its own Ed25519 key, and peers decide whether to
//! believe it by comparing fingerprints out of band (see
//! `docs/ARCHITECTURE.md` §4.2).
//!
//! The certificate exists for three reasons:
//!
//! 1. TLS needs one - rustls will not do raw public keys.
//! 2. It binds the device id (`CN`) to the public key in one attestation.
//! 3. It gives the user something short and stable to compare: the fingerprint.

use rcgen::{CertificateParams, DnType, KeyPair, PKCS_ED25519};
use rustls_pki_types::PrivatePkcs8KeyDer;
use time::{Duration, OffsetDateTime};
use x509_parser::prelude::{FromDer, X509Certificate};

use clipmesh_protocol::DeviceId;

use crate::error::{IdentityError, Result};
use crate::fingerprint::Fingerprint;
use crate::key::{DeviceKey, PUBLIC_KEY_LENGTH};

/// The only DNS name ClipMesh certificates are ever issued for.
///
/// Real host names are deliberately *not* used: peers are reached by IP and
/// authenticated by fingerprint, so a hostname SAN would only add knobs that
/// nobody sets correctly.
pub const CERTIFICATE_SAN: &str = "clipmesh.local";

/// Certificate lifetime.
///
/// Long, because the trust decision is fingerprint based rather than
/// expiry based: a device that is reinstalled generates a new key and the user
/// pairs again. Ten years means the certificate never silently expires in a
/// drawer and breaks sync for a non-technical user.
pub const CERTIFICATE_VALIDITY_DAYS: i64 = 3650;

/// Longest device name we copy into the certificate subject.
const MAX_SUBJECT_NAME_CHARS: usize = 64;

/// A DER encoded, self-signed device certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCertificate {
    der: Vec<u8>,
    pem: String,
}

impl DeviceCertificate {
    /// Mint a certificate for `device_id`, signed by the device's own key.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] if the key cannot be loaded or
    /// the certificate cannot be generated.
    pub fn self_signed(key: &DeviceKey, device_id: DeviceId, device_name: &str) -> Result<Self> {
        let pkcs8 = key.to_pkcs8_der()?;
        let key_der = PrivatePkcs8KeyDer::from(pkcs8.to_vec());

        let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(&key_der, &PKCS_ED25519).map_err(
            |error| IdentityError::Certificate(format!("rcgen rejected the device key: {error}")),
        )?;

        let mut params = CertificateParams::new(vec![CERTIFICATE_SAN.to_owned()]).map_err(
            |error| IdentityError::Certificate(format!("invalid subject alt name: {error}")),
        )?;

        params
            .distinguished_name
            .push(DnType::CommonName, device_id.to_string());
        params.distinguished_name.push(
            DnType::OrganizationalUnitName,
            sanitize_subject_component(device_name),
        );
        params.distinguished_name.push(DnType::OrganizationName, "ClipMesh");
        params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
        params.not_after = OffsetDateTime::now_utc() + Duration::days(CERTIFICATE_VALIDITY_DAYS);

        let certificate = params.self_signed(&key_pair).map_err(|error| {
            IdentityError::Certificate(format!("could not self-sign the certificate: {error}"))
        })?;

        Ok(Self {
            der: certificate.der().to_vec(),
            pem: certificate.pem(),
        })
    }

    /// Wrap an existing DER certificate.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the bytes are not a
    /// parseable X.509 certificate.
    pub fn from_der(der: Vec<u8>) -> Result<Self> {
        let parsed = Self {
            pem: pem_from_der(&der)?,
            der,
        };
        // Fail fast on garbage rather than at the first accessor call.
        parsed.common_name()?;
        Ok(parsed)
    }

    /// Wrap an existing PEM certificate.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the PEM cannot be decoded.
    pub fn from_pem(pem: &str) -> Result<Self> {
        let der = pem_to_der(pem)?;
        Self::from_der(der)
    }

    /// DER bytes, what TLS and the fingerprint use.
    #[must_use]
    pub fn der(&self) -> &[u8] {
        &self.der
    }

    /// An owned copy of the DER bytes, for handing to rustls.
    #[must_use]
    pub fn to_der(&self) -> Vec<u8> {
        self.der.clone()
    }

    /// PEM text, for the UI and for the on-disk trust store.
    #[must_use]
    pub fn pem(&self) -> &str {
        &self.pem
    }

    /// sha256 fingerprint of the DER form.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of_certificate_der(&self.der)
    }

    /// The 32 byte Ed25519 public key embedded in the certificate.
    ///
    /// This is what ties an identity claim to a certificate: a peer that
    /// presents certificate `C` while claiming public key `K` is rejected
    /// unless `C` actually carries `K`.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the certificate cannot be
    /// parsed or does not carry a 32 byte key.
    pub fn public_key_bytes(&self) -> Result<[u8; PUBLIC_KEY_LENGTH]> {
        let certificate = parse(&self.der)?;
        let raw: &[u8] = certificate.public_key().subject_public_key.data.as_ref();

        raw.try_into().map_err(|_| {
            IdentityError::Certificate(format!(
                "certificate carries a {} byte public key, expected {PUBLIC_KEY_LENGTH}",
                raw.len()
            ))
        })
    }

    /// The certificate subject's common name, which is the device id.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when there is no readable CN.
    pub fn common_name(&self) -> Result<String> {
        let certificate = parse(&self.der)?;
        certificate
            .subject()
            .iter_common_name()
            .next()
            .and_then(|attribute| attribute.as_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| {
                IdentityError::Certificate("certificate has no common name".to_owned())
            })
    }

    /// The device id this certificate was issued for.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the common name is not a
    /// device id.
    pub fn device_id(&self) -> Result<DeviceId> {
        DeviceId::parse(&self.common_name()?)
            .map_err(|error| IdentityError::Certificate(format!("bad device id in CN: {error}")))
    }

    /// Whether the certificate is currently within its validity window.
    ///
    /// Used as a sanity check during pairing, *not* as a trust decision: a
    /// pinned certificate stays trusted even if its clock window is odd,
    /// because the user already made that decision.
    ///
    /// # Errors
    /// Returns [`IdentityError::Certificate`] when the certificate cannot be
    /// parsed.
    pub fn is_within_validity(&self) -> Result<bool> {
        let certificate = parse(&self.der)?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        Ok(now >= certificate.validity().not_before.timestamp()
            && now <= certificate.validity().not_after.timestamp())
    }
}

/// Serialized as PEM: readable in a JSON diff, and a single string instead of
/// an array of a thousand numbers.
impl serde::Serialize for DeviceCertificate {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.pem)
    }
}

impl<'de> serde::Deserialize<'de> for DeviceCertificate {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let pem = String::deserialize(deserializer)?;
        Self::from_pem(&pem).map_err(serde::de::Error::custom)
    }
}

fn parse(der: &[u8]) -> Result<X509Certificate<'_>> {
    X509Certificate::from_der(der).map(|(_, certificate)| certificate).map_err(|error| {
        IdentityError::Certificate(format!("could not parse X.509 certificate: {error}"))
    })
}

fn sanitize_subject_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SUBJECT_NAME_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "ClipMesh device".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn pem_from_der(der: &[u8]) -> Result<String> {
    use base64::Engine as _;

    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    Ok(pem)
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;

    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");

    base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|error| IdentityError::Certificate(format!("invalid certificate PEM: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certificate() -> (DeviceKey, DeviceId, DeviceCertificate) {
        let key = DeviceKey::generate();
        let device_id = DeviceId::new();
        let certificate = DeviceCertificate::self_signed(&key, device_id, "Test Laptop").unwrap();
        (key, device_id, certificate)
    }

    #[test]
    fn the_certificate_carries_the_device_key() {
        let (key, _, certificate) = certificate();
        assert_eq!(
            certificate.public_key_bytes().unwrap(),
            key.public_key_bytes()
        );
    }

    #[test]
    fn the_common_name_is_the_device_id() {
        let (_, device_id, certificate) = certificate();
        assert_eq!(certificate.common_name().unwrap(), device_id.to_string());
        assert_eq!(certificate.device_id().unwrap(), device_id);
    }

    #[test]
    fn a_fresh_certificate_is_within_its_validity_window() {
        let (_, _, certificate) = certificate();
        assert!(certificate.is_within_validity().unwrap());
    }

    #[test]
    fn der_and_pem_round_trip() {
        let (_, device_id, original) = certificate();
        let from_der = DeviceCertificate::from_der(original.to_der()).unwrap();
        let from_pem = DeviceCertificate::from_pem(original.pem()).unwrap();

        assert_eq!(from_der.fingerprint(), original.fingerprint());
        assert_eq!(from_pem.fingerprint(), original.fingerprint());
        assert_eq!(from_der.device_id().unwrap(), device_id);
        assert!(original.pem().starts_with("-----BEGIN CERTIFICATE-----"));
    }

    #[test]
    fn every_device_gets_a_distinct_certificate() {
        let (_, _, first) = certificate();
        let (_, _, second) = certificate();
        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn the_fingerprint_matches_a_manual_sha256() {
        use sha2::{Digest, Sha256};
        let (_, _, certificate) = certificate();
        let expected: [u8; 32] = Sha256::digest(certificate.der()).into();
        assert_eq!(certificate.fingerprint().as_bytes(), &expected);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(DeviceCertificate::from_der(vec![0u8; 64]).is_err());
        assert!(DeviceCertificate::from_pem("not a pem").is_err());
    }

    #[test]
    fn certificates_serialize_as_pem() {
        let (_, _, certificate) = certificate();
        let json = serde_json::to_string(&certificate).unwrap();
        assert!(json.contains("BEGIN CERTIFICATE"));
        let restored: DeviceCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fingerprint(), certificate.fingerprint());
    }

    #[test]
    fn long_device_names_are_truncated_not_rejected() {
        let key = DeviceKey::generate();
        let long_name = "x".repeat(500);
        let certificate =
            DeviceCertificate::self_signed(&key, DeviceId::new(), &long_name).unwrap();
        assert!(certificate.public_key_bytes().is_ok());
    }
}
