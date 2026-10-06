//! Certificate fingerprints.
//!
//! A fingerprint is the sha256 of the DER encoded certificate. It is what the
//! user actually *sees* during pairing and what gets pinned in the trust store,
//! so the formatting matters: it has to be readable aloud over a phone call and
//! comparable at a glance between two screens.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::{IdentityError, Result};

/// Number of bytes in a fingerprint.
pub const FINGERPRINT_LENGTH: usize = 32;

/// How many groups [`Fingerprint::short`] keeps.
const SHORT_GROUPS: usize = 4;

/// A certificate fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; FINGERPRINT_LENGTH]);

impl Fingerprint {
    /// Fingerprint of a DER encoded certificate.
    #[must_use]
    pub fn of_certificate_der(der: &[u8]) -> Self {
        let digest: [u8; FINGERPRINT_LENGTH] = Sha256::digest(der).into();
        Self(digest)
    }

    /// Wrap raw fingerprint bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; FINGERPRINT_LENGTH]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; FINGERPRINT_LENGTH] {
        &self.0
    }

    /// Lowercase hex without separators, for storage and mDNS TXT records.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Grouped uppercase hex for humans: `A1B2 C3D4 …` (16 groups of 4).
    #[must_use]
    pub fn to_grouped(&self) -> String {
        let hex = hex::encode_upper(self.0);
        hex.as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The first four groups, for lists where the full string would not fit.
    ///
    /// 64 bits of prefix is plenty to tell two devices in one household apart.
    #[must_use]
    pub fn short(&self) -> String {
        self.to_grouped()
            .split(' ')
            .take(SHORT_GROUPS)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Constant time comparison, used wherever a mismatch would leak
    /// information about a pinned value.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }

    /// Parse hex with optional whitespace, colons or dashes.
    ///
    /// # Errors
    /// Returns [`IdentityError::InvalidFingerprint`] when the input does not
    /// contain exactly 64 hex digits.
    pub fn parse(text: &str) -> Result<Self> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ':' && *c != '-')
            .collect();

        if cleaned.len() != FINGERPRINT_LENGTH * 2 {
            return Err(IdentityError::InvalidFingerprint(format!(
                "expected {} hex digits, got {}",
                FINGERPRINT_LENGTH * 2,
                cleaned.len()
            )));
        }

        let decoded = hex::decode(&cleaned)
            .map_err(|error| IdentityError::InvalidFingerprint(error.to_string()))?;

        let mut bytes = [0u8; FINGERPRINT_LENGTH];
        bytes.copy_from_slice(&decoded);
        Ok(Self(bytes))
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_grouped())
    }
}

impl FromStr for Fingerprint {
    type Err = IdentityError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Serialized as the compact hex form so it round-trips through mDNS records,
/// JSON settings and log lines identically.
impl Serialize for Fingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable_for_the_same_input() {
        let a = Fingerprint::of_certificate_der(b"certificate bytes");
        let b = Fingerprint::of_certificate_der(b"certificate bytes");
        assert!(a.matches(&b));
        assert_eq!(a.to_hex(), b.to_hex());
    }

    #[test]
    fn different_certificates_give_different_fingerprints() {
        let a = Fingerprint::of_certificate_der(b"certificate one");
        let b = Fingerprint::of_certificate_der(b"certificate two");
        assert!(!a.matches(&b));
    }

    #[test]
    fn the_grouped_form_is_readable_and_complete() {
        let fingerprint = Fingerprint::of_certificate_der(b"x");
        let grouped = fingerprint.to_grouped();
        // 32 bytes = 64 hex digits = 16 groups of four.
        assert_eq!(grouped.split(' ').count(), 16);
        assert!(grouped.split(' ').all(|group| group.len() == 4));
        assert_eq!(fingerprint.short().split(' ').count(), 4);
    }

    #[test]
    fn every_rendering_parses_back() {
        let fingerprint = Fingerprint::of_certificate_der(b"round trip");
        assert_eq!(Fingerprint::parse(&fingerprint.to_hex()).unwrap(), fingerprint);
        assert_eq!(
            Fingerprint::parse(&fingerprint.to_grouped()).unwrap(),
            fingerprint
        );
        assert_eq!(
            Fingerprint::parse(&fingerprint.to_hex().to_uppercase()).unwrap(),
            fingerprint
        );
        // Colon separated, as printed by openssl.
        let colonised = fingerprint
            .to_hex()
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap().to_owned())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(Fingerprint::parse(&colonised).unwrap(), fingerprint);
    }

    #[test]
    fn malformed_fingerprints_are_rejected() {
        assert!(Fingerprint::parse("").is_err());
        assert!(Fingerprint::parse("AABB").is_err());
        assert!(Fingerprint::parse(&"Z".repeat(64)).is_err());
    }

    #[test]
    fn fingerprints_serialize_as_compact_hex() {
        let fingerprint = Fingerprint::of_certificate_der(b"json");
        let json = serde_json::to_string(&fingerprint).unwrap();
        assert_eq!(json, format!("\"{}\"", fingerprint.to_hex()));
        let restored: Fingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, fingerprint);
    }
}
