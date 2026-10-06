//! The "you received something" notification, and the pixels behind it.
//!
//! ## Why a task of its own
//!
//! Nothing on the Rust side posted a notification before this: the Kotlin
//! `showReceived` command existed, the `AndroidClipboardHost::show_received`
//! trait method existed, and no caller connected them - so a received item was
//! written to the clipboard and recorded in the history in complete silence
//! unless the user happened to be looking at the app.
//!
//! The engine publishes `CoreEvent::ClipboardReceived` for every item that
//! arrives, and `clipmesh_desktop_lib::forward_events` turns it into a webview
//! event - which only helps a webview that is running. This subscribes to the
//! same broadcast channel from the Android host instead, so the notification is
//! posted while the app is in the background behind the foreground service,
//! which is when it matters:
//!
//! ```text
//!   accept_remote ──► CoreEvent::ClipboardReceived ──┬──► webview (clipmesh://…)
//!                                                    └──► this task ──► Kotlin notification
//! ```
//!
//! ## Text
//!
//! Text is already on the clipboard by the time this runs: `accept_remote`
//! writes it there before it emits, and Android 10's restriction is on
//! *reading* the clipboard from the background, not on writing to it. The
//! notification is therefore not how the content is delivered - it is the
//! receipt, and the fallback: a write that failed is reported as an engine error
//! *and* still produces this notification, because the item is in the history
//! either way.
//!
//! ## Images
//!
//! An image needs one thing a notification cannot carry by itself: a file. A
//! notification action cannot hand a chooser a `Bitmap`, and Android will not
//! accept a `file://` path from one, so the stored PNG is passed to Kotlin as
//! base64, staged under the FileProvider root, and shared as a `content://` URI.
//! The pixels come from the same `<state>/images/<id>.png` copy the history
//! thumbnails are served from - an image that made it into the history has a
//! file, and an image with no file (the store write failed, or the user cleared
//! the history in between) still gets a notification, just without a preview.

use std::sync::Arc;

use base64::Engine as _;
use tauri::Wry;

use clipmesh_core::{ImageStore as _, SharedSyncManager};
use clipmesh_desktop_lib::images::ImageCache;
use clipmesh_protocol::ClipboardItem;

use crate::plugin::NativeBridge;

/// How much text the notification shows before it truncates.
///
/// The same order as the Kotlin host's own previews; a notification body is a
/// couple of lines on a phone, and a 4 MiB paste is not improved by more of it.
const PREVIEW_CHARS: usize = 160;

/// What a notification says when the source device is not known by name.
const FALLBACK_TITLE: &str = "Clipboard received";

/// Post a notification for every item that arrives, for the life of the process.
///
/// Runs until the engine's channel closes, which happens when the engine is
/// dropped - i.e. at process exit.
pub fn spawn(bridge: Arc<NativeBridge<Wry>>, engine: SharedSyncManager, images: Arc<ImageCache>) {
    tauri::async_runtime::spawn(async move {
        let mut events = engine.subscribe();

        loop {
            match events.recv().await {
                Ok(clipmesh_core::CoreEvent::ClipboardReceived(item)) => {
                    notify(&bridge, &engine, &images, &item);
                }
                // Every other event belongs to the webview; this task only
                // cares about what arrived from a peer.
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "the notification task fell behind the engine");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Post the notification for one received item.
///
/// Failures are logged rather than reported: a notification that could not be
/// shown (the permission is denied, most likely) must not turn into an error
/// banner about content the user has already received.
fn notify(
    bridge: &NativeBridge<Wry>,
    engine: &SharedSyncManager,
    images: &ImageCache,
    item: &ClipboardItem,
) {
    let title = source_title(engine, item);

    let result = match item {
        ClipboardItem::Text(payload) => {
            bridge.show_received(&title, &payload.preview(PREVIEW_CHARS))
        }

        ClipboardItem::Image(meta) => {
            let preview = format!(
                "Image {}x{} ({} KiB)",
                meta.width,
                meta.height,
                meta.size / 1024
            );

            match images.get(&meta.id) {
                Some(png) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
                    bridge.show_received_image(
                        &meta.id,
                        &title,
                        &preview,
                        &encoded,
                        meta.width,
                        meta.height,
                    )
                }

                // No stored copy: still say something arrived. The item is in
                // the history, and its row will show the reason there is no
                // preview - silently dropping the notification would look like
                // the transfer failed.
                None => {
                    tracing::warn!(id = %meta.id, "no stored copy to preview; posting a text notification");
                    bridge.show_received(&title, &preview)
                }
            }
        }
    };

    if let Err(error) = result {
        tracing::warn!(%error, "could not post the received notification");
    }
}

/// The notification's title: who sent it, by name when that is known.
///
/// The name lives in the trust store (and, while the device is up, in the peers
/// snapshot); the item itself only carries the id. A peer that is neither
/// trusted nor currently discovered cannot be named - it must have been trusted
/// when it connected, so this is an edge case, and the fallback says nothing
/// rather than showing a UUID.
fn source_title(engine: &SharedSyncManager, item: &ClipboardItem) -> String {
    let source = item.source_device();

    let name = engine
        .trusted_devices()
        .into_iter()
        .find(|device| device.device_id == source)
        .map(|device| device.name)
        .or_else(|| {
            engine
                .peers()
                .into_iter()
                .find(|peer| peer.device_id == source)
                .map(|peer| peer.name)
        });

    match name {
        Some(name) if !name.trim().is_empty() => format!("Received from {name}"),
        _ => FALLBACK_TITLE.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_protocol::{DeviceId, ImageMeta, TextPayload};

    #[test]
    fn a_text_item_previews_with_the_notification_limit() {
        let payload = TextPayload::new_local("a\nb", DeviceId::new());
        assert_eq!(payload.preview(PREVIEW_CHARS), "a b");
    }

    #[test]
    fn an_image_item_builds_the_size_line_the_notification_shows() {
        // The same wording the Kotlin host has always used for the no-preview
        // fallback, so the two paths read alike.
        let meta = ImageMeta::new_local(DeviceId::new(), &[0u8; 4096], 1920, 1080);
        let preview = format!(
            "Image {}x{} ({} KiB)",
            meta.width,
            meta.height,
            meta.size / 1024
        );
        assert_eq!(preview, "Image 1920x1080 (4 KiB)");
    }

    #[test]
    fn the_fallback_title_says_nothing_about_an_unknown_sender() {
        // A UUID in a notification title is noise, so an unresolvable source
        // gets the plain wording instead.
        assert_eq!(FALLBACK_TITLE, "Clipboard received");
    }
}
