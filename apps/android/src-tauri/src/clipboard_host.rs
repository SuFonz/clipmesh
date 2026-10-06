//! The [`AndroidClipboardHost`] implementation: Rust calling into Kotlin.
//!
//! Every method here ends up on the Android main thread inside the plugin. That
//! is deliberate - `ClipboardManager` and the notification manager both require
//! it, and doing the thread hop on the Kotlin side keeps this file free of
//! `Looper` juggling.

use std::sync::Arc;

use base64::Engine as _;

use clipmesh_clipboard::{AndroidClipboardHost, PlatformClipboard};
use clipmesh_core::{CoreError, Result as CoreResult};
use clipmesh_protocol::{ClipboardItem, ClipboardKind};

use crate::plugin::NativeBridge;

/// Talks to the Kotlin plugin registered under `app.cm.clipmesh.bridge`.
pub struct KotlinClipboardHost {
    bridge: Arc<NativeBridge<tauri::Wry>>,
}

impl KotlinClipboardHost {
    /// Wrap the registered plugin handle.
    #[must_use]
    pub fn new(bridge: Arc<NativeBridge<tauri::Wry>>) -> Self {
        Self { bridge }
    }
}

impl AndroidClipboardHost for KotlinClipboardHost {
    fn read_now(&self) -> CoreResult<PlatformClipboard> {
        self.bridge.read_clipboard().map_err(platform_error)
    }

    fn set_text(&self, text: &str) -> CoreResult<()> {
        self.bridge.set_text(text).map_err(platform_error)
    }

    fn set_image(&self, png: &[u8]) -> CoreResult<()> {
        // Base64 exists only across the JNI boundary: the plugin API marshals
        // arguments as JSON. The wire protocol itself carries raw PNG chunks.
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        self.bridge
            .set_image_base64(&encoded)
            .map_err(platform_error)
    }

    fn show_received(&self, item: &ClipboardItem, source: &str) -> CoreResult<()> {
        // Kept because the trait asks for it, but not the live path any more: the
        // notification is posted by `crate::received`, which also needs the pixels
        // of an image (for the preview and the share action) and therefore cannot
        // go through a method that only takes metadata.
        let preview = match item {
            ClipboardItem::Text(payload) => payload.preview(120),
            ClipboardItem::Image(meta) => {
                format!("Image {}x{} ({} KiB)", meta.width, meta.height, meta.size / 1024)
            }
        };

        self.bridge
            .show_received(source, &preview)
            .map_err(platform_error)
    }

    fn ensure_foreground_service(&self) -> CoreResult<()> {
        self.bridge
            .start_foreground_service()
            .map_err(platform_error)
    }

    fn stop_foreground_service(&self) -> CoreResult<()> {
        self.bridge
            .stop_foreground_service()
            .map_err(platform_error)
    }

    fn is_foreground_service_running(&self) -> bool {
        // A failure here means the plugin is not registered yet; reporting
        // "not running" is the safe answer and lets the UI offer to start it.
        self.bridge
            .is_foreground_service_running()
            .unwrap_or_else(|error| {
                tracing::debug!(%error, "could not query the foreground service");
                false
            })
    }
}

impl std::fmt::Debug for KotlinClipboardHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KotlinClipboardHost").finish_non_exhaustive()
    }
}

/// Turn a JNI error string into a clipboard error the engine understands.
fn platform_error(message: String) -> CoreError {
    CoreError::Clipboard(message)
}

/// The discriminator the Kotlin side uses when it pushes content up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text content.
    Text,
    /// Image content.
    Image,
}

impl From<ClipboardKind> for Kind {
    fn from(kind: ClipboardKind) -> Self {
        match kind {
            ClipboardKind::Text => Self::Text,
            ClipboardKind::Image => Self::Image,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_protocol::{DeviceId, TextPayload};

    #[test]
    fn kinds_map_from_the_protocol() {
        assert_eq!(Kind::from(ClipboardKind::Text), Kind::Text);
        assert_eq!(Kind::from(ClipboardKind::Image), Kind::Image);
    }

    #[test]
    fn notifications_show_a_readable_preview() {
        let item = ClipboardItem::Text(TextPayload::new_local(
            "line one\nline two",
            DeviceId::new(),
        ));
        let (preview, is_image) = match &item {
            ClipboardItem::Text(payload) => (payload.preview(120), false),
            ClipboardItem::Image(_) => (String::new(), true),
        };
        assert_eq!(preview, "line one line two");
        assert!(!is_image);
    }
}
