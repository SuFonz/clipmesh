//! Device identity primitives that are shared by every layer.
//!
//! These types describe *who* a peer claims to be. Proving that claim (keys,
//! certificates, signatures) is the job of `clipmesh-identity`, and protecting
//! it in transit is the job of `clipmesh-security`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::{ProtocolError, Result};

pub use crate::proto::Platform;

/// Stable identifier of a ClipMesh installation.
///
/// Generated once on first launch and persisted next to the device key. It is
/// never derived from hardware identifiers, so it survives reinstalls only if
/// the user keeps their key material, and it leaks nothing about the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(Uuid);

impl DeviceId {
    /// Mint a fresh random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Raw 16 byte representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }

    /// Rebuild from raw bytes.
    ///
    /// # Errors
    /// Returns [`ProtocolError::InvalidDeviceId`] if the slice is not 16 bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        Uuid::from_slice(bytes)
            .map(Self)
            .map_err(|e| ProtocolError::InvalidDeviceId(e.to_string()))
    }

    /// Parse from the canonical hyphenated string form.
    ///
    /// # Errors
    /// Returns [`ProtocolError::InvalidDeviceId`] if the text is not a UUID.
    pub fn parse(text: &str) -> Result<Self> {
        Uuid::parse_str(text.trim())
            .map(Self)
            .map_err(|e| ProtocolError::InvalidDeviceId(e.to_string()))
    }

    /// Whether this is the all-zero identifier.
    #[must_use]
    pub fn is_nil(&self) -> bool {
        self.0.is_nil()
    }
}

impl Default for DeviceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DeviceId {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl From<Uuid> for DeviceId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<DeviceId> for Uuid {
    fn from(value: DeviceId) -> Self {
        value.0
    }
}

impl Platform {
    /// Every platform this build knows about.
    pub const ALL: [Self; 4] = [Self::Windows, Self::Linux, Self::Macos, Self::Android];

    /// The platform this process is running on.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "android") {
            Self::Android
        } else {
            Self::Linux
        }
    }

    /// Lowercase token used on the wire, in mDNS TXT records and in settings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Android => "android",
            Self::Unspecified => "unknown",
        }
    }

    /// Human readable label for the UI.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Macos => "macOS",
            Self::Android => "Android",
            Self::Unspecified => "Unknown",
        }
    }

    /// Parse the token produced by [`Platform::as_str`], case insensitively.
    /// Unknown input degrades to [`Platform::Unspecified`] instead of failing,
    /// because mDNS records from newer peers must never crash an older client.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "windows" | "win" | "win32" => Self::Windows,
            "linux" => Self::Linux,
            "macos" | "mac" | "darwin" | "osx" => Self::Macos,
            "android" => Self::Android,
            _ => Self::Unspecified,
        }
    }

    /// Whether this platform is a touch first mobile device.
    #[must_use]
    pub const fn is_mobile(self) -> bool {
        matches!(self, Self::Android)
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

impl Serialize for Platform {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self::parse(&text))
    }
}

/// Human readable description of a device, safe to broadcast over mDNS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Stable device identifier.
    pub device_id: DeviceId,
    /// User visible device name.
    pub name: String,
    /// Operating system family.
    pub platform: Platform,
    /// Version of the ClipMesh build running on the device.
    pub app_version: String,
}

impl DeviceInfo {
    /// Build a description for this device.
    #[must_use]
    pub fn new(
        device_id: DeviceId,
        name: impl Into<String>,
        platform: Platform,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            device_id,
            name: name.into(),
            platform,
            app_version: app_version.into(),
        }
    }

    /// Build a description for the device this process runs on.
    #[must_use]
    pub fn local(device_id: DeviceId, name: impl Into<String>) -> Self {
        Self::new(
            device_id,
            name,
            Platform::current(),
            env!("CARGO_PKG_VERSION"),
        )
    }

    /// Replace the display name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Fall back to a readable default when the user never named the device.
    /// mDNS instance names and UI lists both look broken with an empty string.
    #[must_use]
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            self.platform.display_name()
        } else {
            self.name.trim()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_round_trips_through_text_and_bytes() {
        let id = DeviceId::new();
        assert_eq!(DeviceId::parse(&id.to_string()).unwrap(), id);
        assert_eq!(DeviceId::from_slice(id.as_bytes()).unwrap(), id);
    }

    #[test]
    fn device_id_rejects_garbage() {
        assert!(DeviceId::parse("not-a-uuid").is_err());
        assert!(DeviceId::from_slice(&[0u8; 8]).is_err());
    }

    #[test]
    fn platform_tokens_round_trip() {
        for platform in Platform::ALL {
            assert_eq!(Platform::parse(platform.as_str()), platform);
        }
        assert_eq!(Platform::parse("  ANDROID "), Platform::Android);
        assert_eq!(Platform::parse("plan9"), Platform::Unspecified);
    }

    #[test]
    fn platform_serializes_as_a_token() {
        let json = serde_json::to_string(&Platform::Macos).unwrap();
        assert_eq!(json, "\"macos\"");
        assert_eq!(
            serde_json::from_str::<Platform>("\"windows\"").unwrap(),
            Platform::Windows
        );
    }

    #[test]
    fn blank_device_names_fall_back_to_the_platform() {
        let info = DeviceInfo::local(DeviceId::new(), "   ");
        assert_eq!(info.display_name(), Platform::current().display_name());
    }
}
