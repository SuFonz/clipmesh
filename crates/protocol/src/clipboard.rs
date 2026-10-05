//! Clipboard payload model shared by the transport, the sync engine and the UI.
//!
//! The protobuf schema describes what travels on the wire; the types here are
//! the validated, strongly typed Rust view of the same data. Conversion is
//! always explicit so that a malformed peer message can never reach the
//! platform clipboard code.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::device::DeviceId;
use crate::error::{ProtocolError, Result};
use crate::proto;

/// Size of a single image chunk.
///
/// 64 KiB keeps a 4K screenshot at ~160 frames, which streams smoothly over
/// TCP without ever holding more than one chunk in memory per side.
pub const IMAGE_CHUNK_BYTES: usize = 64 * 1024;

/// Largest text clipboard we accept, in bytes.
pub const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;

/// Largest image clipboard we accept, in bytes.
pub const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// The only image encoding ClipMesh puts on the wire.
///
/// PNG is lossless, universally supported by every platform clipboard API and
/// avoids the JPEG artefacts that make round tripped screenshots ugly.
pub const IMAGE_MIME_PNG: &str = "image/png";

/// Which flavour of clipboard content we are dealing with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardKind {
    /// UTF-8 text.
    Text,
    /// A PNG encoded image.
    Image,
}

impl ClipboardKind {
    /// Lowercase token used in logs and the UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

impl std::fmt::Display for ClipboardKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Compute the digest used to integrity check image payloads.
#[must_use]
pub fn sha256_of(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// A piece of text that was (or will be) placed on a clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPayload {
    /// Deduplication key, minted by the origin device.
    pub id: String,
    /// Device that originally copied the text.
    pub source_device: DeviceId,
    /// Origin timestamp, unix milliseconds.
    pub timestamp: i64,
    /// The text itself.
    pub content: String,
}

impl TextPayload {
    /// Wrap freshly copied local text.
    #[must_use]
    pub fn new_local(content: impl Into<String>, source_device: DeviceId) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_device,
            timestamp: crate::now_millis(),
            content: content.into(),
        }
    }

    /// Size of the payload in bytes.
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        self.content.len() as u64
    }

    /// Whether the payload stays within [`MAX_TEXT_BYTES`].
    #[must_use]
    pub fn is_within_limits(&self) -> bool {
        self.size_bytes() <= MAX_TEXT_BYTES
    }

    /// A single line preview suitable for a notification or a list row.
    #[must_use]
    pub fn preview(&self, max_chars: usize) -> String {
        let flattened: String = self
            .content
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let trimmed = flattened.trim();
        if trimmed.chars().count() <= max_chars {
            return trimmed.to_owned();
        }
        let mut out: String = trimmed.chars().take(max_chars).collect();
        out.push('…');
        out
    }

    /// Convert to the wire representation.
    #[must_use]
    pub fn to_proto(&self) -> proto::ClipboardText {
        proto::ClipboardText {
            id: self.id.clone(),
            source_device: self.source_device.to_string(),
            timestamp: self.timestamp,
            content: self.content.clone(),
        }
    }

    /// Validate and convert a wire message.
    ///
    /// # Errors
    /// Fails when the source device id is not a UUID or the text is oversized.
    pub fn from_proto(message: &proto::ClipboardText) -> Result<Self> {
        let payload = Self {
            id: message.id.clone(),
            source_device: DeviceId::parse(&message.source_device)?,
            timestamp: message.timestamp,
            content: message.content.clone(),
        };
        if !payload.is_within_limits() {
            return Err(ProtocolError::PayloadTooLarge {
                kind: "text",
                max: MAX_TEXT_BYTES,
                actual: payload.size_bytes(),
            });
        }
        Ok(payload)
    }
}

/// Metadata describing an image payload that is streamed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    /// Deduplication key, minted by the origin device.
    pub id: String,
    /// Device that originally copied the image.
    pub source_device: DeviceId,
    /// Origin timestamp, unix milliseconds.
    pub timestamp: i64,
    /// Always `image/png` in protocol v1.
    pub mime: String,
    /// Total payload size in bytes.
    pub size: u64,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// sha256 of the full payload.
    ///
    /// Never serialised to the UI: it is 32 numbers that mean nothing to a
    /// person, and the payload it checks has already been verified by the time
    /// an entry reaches the history list.
    #[serde(skip_serializing, default)]
    pub sha256: [u8; 32],
}

impl ImageMeta {
    /// Describe an in-memory PNG payload.
    #[must_use]
    pub fn new_local(source_device: DeviceId, data: &[u8], width: u32, height: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_device,
            timestamp: crate::now_millis(),
            mime: IMAGE_MIME_PNG.to_owned(),
            size: data.len() as u64,
            width,
            height,
            sha256: sha256_of(data),
        }
    }

    /// Whether the payload stays within [`MAX_IMAGE_BYTES`].
    #[must_use]
    pub fn is_within_limits(&self) -> bool {
        self.size <= MAX_IMAGE_BYTES
    }

    /// Number of chunks this payload will be split into.
    #[must_use]
    pub fn chunk_count(&self) -> u64 {
        self.size.div_ceil(IMAGE_CHUNK_BYTES as u64)
    }

    /// Check a received payload against the advertised digest.
    ///
    /// # Errors
    /// Returns [`ProtocolError::ChecksumMismatch`] when the digest differs.
    pub fn verify(&self, data: &[u8]) -> Result<()> {
        if data.len() as u64 != self.size || sha256_of(data) != self.sha256 {
            return Err(ProtocolError::ChecksumMismatch {
                id: self.id.clone(),
            });
        }
        Ok(())
    }

    /// Convert to the wire representation.
    #[must_use]
    pub fn to_proto(&self) -> proto::ClipboardImage {
        proto::ClipboardImage {
            id: self.id.clone(),
            source_device: self.source_device.to_string(),
            timestamp: self.timestamp,
            mime: self.mime.clone(),
            size: self.size,
            width: self.width,
            height: self.height,
            sha256: self.sha256.to_vec(),
        }
    }

    /// Validate and convert a wire message.
    ///
    /// # Errors
    /// Fails on a bad device id, a non-32-byte digest or an oversized payload.
    pub fn from_proto(message: &proto::ClipboardImage) -> Result<Self> {
        if message.sha256.len() != 32 {
            return Err(ProtocolError::InvalidFieldLength {
                field: "sha256",
                expected: 32,
                actual: message.sha256.len(),
            });
        }
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(&message.sha256);

        let meta = Self {
            id: message.id.clone(),
            source_device: DeviceId::parse(&message.source_device)?,
            timestamp: message.timestamp,
            mime: message.mime.clone(),
            size: message.size,
            width: message.width,
            height: message.height,
            sha256,
        };
        if !meta.is_within_limits() {
            return Err(ProtocolError::PayloadTooLarge {
                kind: "image",
                max: MAX_IMAGE_BYTES,
                actual: meta.size,
            });
        }
        Ok(meta)
    }
}

/// A complete image: metadata plus the decoded PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePayload {
    /// Description of the payload.
    pub meta: ImageMeta,
    /// Raw PNG bytes.
    pub data: Vec<u8>,
}

impl ImagePayload {
    /// Build from metadata and bytes, verifying the digest.
    ///
    /// # Errors
    /// Returns [`ProtocolError::ChecksumMismatch`] when `data` does not match.
    pub fn new(meta: ImageMeta, data: Vec<u8>) -> Result<Self> {
        meta.verify(&data)?;
        Ok(Self { meta, data })
    }
}

/// A clipboard entry as shown in history lists: everything except the pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClipboardItem {
    /// A text entry.
    Text(TextPayload),
    /// An image entry, metadata only.
    Image(ImageMeta),
}

impl ClipboardItem {
    /// Deduplication key.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Text(payload) => &payload.id,
            Self::Image(meta) => &meta.id,
        }
    }

    /// Device the entry originated from.
    #[must_use]
    pub fn source_device(&self) -> DeviceId {
        match self {
            Self::Text(payload) => payload.source_device,
            Self::Image(meta) => meta.source_device,
        }
    }

    /// Origin timestamp, unix milliseconds.
    #[must_use]
    pub fn timestamp(&self) -> i64 {
        match self {
            Self::Text(payload) => payload.timestamp,
            Self::Image(meta) => meta.timestamp,
        }
    }

    /// Which flavour of content this is.
    #[must_use]
    pub fn kind(&self) -> ClipboardKind {
        match self {
            Self::Text(_) => ClipboardKind::Text,
            Self::Image(_) => ClipboardKind::Image,
        }
    }
}

/// Full clipboard content: either text or an image with its pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    /// Text content.
    Text(TextPayload),
    /// Image content including pixels.
    Image(ImagePayload),
}

impl ClipboardContent {
    /// Deduplication key.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Text(payload) => &payload.id,
            Self::Image(payload) => &payload.meta.id,
        }
    }

    /// Device the content originated from.
    #[must_use]
    pub fn source_device(&self) -> DeviceId {
        match self {
            Self::Text(payload) => payload.source_device,
            Self::Image(payload) => payload.meta.source_device,
        }
    }

    /// Which flavour of content this is.
    #[must_use]
    pub fn kind(&self) -> ClipboardKind {
        match self {
            Self::Text(_) => ClipboardKind::Text,
            Self::Image(_) => ClipboardKind::Image,
        }
    }

    /// Metadata-only view, for history entries and notifications.
    #[must_use]
    pub fn to_item(&self) -> ClipboardItem {
        match self {
            Self::Text(payload) => ClipboardItem::Text(payload.clone()),
            Self::Image(payload) => ClipboardItem::Image(payload.meta.clone()),
        }
    }

    /// Approximate size in bytes.
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Text(payload) => payload.size_bytes(),
            Self::Image(payload) => payload.meta.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> DeviceId {
        DeviceId::new()
    }

    #[test]
    fn text_survives_a_proto_round_trip() {
        let original = TextPayload::new_local("hello 世界", device());
        let restored = TextPayload::from_proto(&original.to_proto()).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn oversized_text_is_rejected_on_decode() {
        let mut message = TextPayload::new_local("x", device()).to_proto();
        message.content = "x".repeat((MAX_TEXT_BYTES + 1) as usize);
        assert!(matches!(
            TextPayload::from_proto(&message),
            Err(ProtocolError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn image_survives_a_proto_round_trip_and_verifies() {
        let data = vec![0xABu8; 4096];
        let meta = ImageMeta::new_local(device(), &data, 64, 64);
        let restored = ImageMeta::from_proto(&meta.to_proto()).unwrap();
        assert_eq!(meta, restored);
        assert!(restored.verify(&data).is_ok());
        assert_eq!(restored.chunk_count(), 1);
    }

    #[test]
    fn tampered_images_fail_verification() {
        let data = vec![7u8; 1024];
        let meta = ImageMeta::new_local(device(), &data, 32, 32);
        let mut tampered = data.clone();
        tampered[10] = 8;
        assert!(matches!(
            meta.verify(&tampered),
            Err(ProtocolError::ChecksumMismatch { .. })
        ));
        assert!(matches!(
            meta.verify(&data[..100]),
            Err(ProtocolError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn image_meta_rejects_a_short_digest() {
        let data = vec![1u8; 16];
        let mut message = ImageMeta::new_local(device(), &data, 4, 4).to_proto();
        message.sha256.truncate(31);
        assert!(matches!(
            ImageMeta::from_proto(&message),
            Err(ProtocolError::InvalidFieldLength { field: "sha256", .. })
        ));
    }

    #[test]
    fn chunk_count_covers_a_4k_screenshot() {
        let data = vec![0u8; 10 * 1024 * 1024];
        let meta = ImageMeta::new_local(device(), &data, 3840, 2160);
        assert_eq!(meta.chunk_count(), 160);
    }

    #[test]
    fn previews_collapse_control_characters() {
        let mut payload = TextPayload::new_local("line one\nline two", device());
        assert_eq!(payload.preview(64), "line one line two");
        payload.content = "abcdefghij".repeat(10);
        assert_eq!(payload.preview(5), "abcde…");
    }
}
