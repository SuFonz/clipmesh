//! The desktop clipboard, shared by Windows, Linux and macOS.
//!
//! ## Why `arboard`
//!
//! It is the one crate that reads and writes both text and images on all three
//! desktop platforms without pulling in a windowing toolkit. It gives us
//! `ImageData` as raw RGBA8, which we re-encode to PNG ourselves so that the
//! wire format is identical no matter which platform produced the pixels.
//!
//! ## One operation at a time
//!
//! The Windows clipboard is a single global resource that one thread opens at a
//! time. `arboard` retries internally (five attempts, 5 ms apart) and then gives
//! up with `ClipboardOccupied`, so this type serialises every operation behind
//! an async mutex and runs it on a blocking thread. Nothing here awaits while
//! holding the clipboard.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use tokio::sync::broadcast;

use clipmesh_core::{ClipboardEvent, ClipboardProvider, ContentSignature, CoreError, Result};
use clipmesh_protocol::{ClipboardContent, DeviceId, ImageMeta, ImagePayload, TextPayload};

/// How often the watcher looks for a change.
///
/// On Windows the check is a single `GetClipboardSequenceNumber` call, which is
/// free. On Linux and macOS there is no counter, so the tick compares content
/// signatures - still cheap, but this is the reason the interval is 250 ms
/// rather than something aggressive.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How many clipboard operations may queue up before the oldest change events
/// are dropped. Changes are level-triggered snapshots, so dropping is safe.
const CHANGE_CHANNEL_CAPACITY: usize = 16;

/// Clipboard access for desktop platforms.
pub struct DesktopClipboard {
    device_id: DeviceId,
    /// Set just before we write, so the watcher can skip the change it causes.
    suppress_next: Arc<AtomicBool>,
    /// Serialises every clipboard operation.
    operation: tokio::sync::Mutex<()>,
    poll_interval: Duration,
}

impl DesktopClipboard {
    /// Create a provider for `device_id`.
    #[must_use]
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            suppress_next: Arc::new(AtomicBool::new(false)),
            operation: tokio::sync::Mutex::new(()),
            poll_interval: POLL_INTERVAL,
        }
    }

    /// Override the watcher's polling interval. Used by tests.
    #[must_use]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Read the clipboard on a blocking thread, with a bounded retry.
    async fn read_blocking(&self) -> Result<Option<ClipboardContent>> {
        let _guard = self.operation.lock().await;
        let device_id = self.device_id;

        tokio::task::spawn_blocking(move || read_clipboard(device_id))
            .await
            .map_err(|error| CoreError::Clipboard(format!("clipboard task failed: {error}")))?
    }
}

#[async_trait]
impl ClipboardProvider for DesktopClipboard {
    async fn read(&self) -> Result<Option<ClipboardContent>> {
        self.read_blocking().await
    }

    async fn write(&self, content: &ClipboardContent) -> Result<()> {
        let _guard = self.operation.lock().await;

        // Set before the write so a watcher tick that lands in between already
        // knows the change is ours.
        self.suppress_next.store(true, Ordering::SeqCst);

        let owned = content.clone();
        let result = tokio::task::spawn_blocking(move || write_clipboard(&owned))
            .await
            .map_err(|error| CoreError::Clipboard(format!("clipboard task failed: {error}")))?;

        if result.is_err() {
            // The write never happened, so do not swallow the next real change.
            self.suppress_next.store(false, Ordering::SeqCst);
        }
        result
    }

    fn watch(&self) -> BoxStream<'static, ClipboardEvent> {
        let (sender, receiver) = broadcast::channel(CHANGE_CHANNEL_CAPACITY);
        let device_id = self.device_id;
        let suppress = Arc::clone(&self.suppress_next);
        let interval = self.poll_interval;

        tokio::spawn(async move {
            watch_loop(device_id, suppress, interval, sender).await;
        });

        Box::pin(futures::stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((event, receiver)),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }))
    }

    fn suppress_next_change(&self) {
        self.suppress_next.store(true, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for DesktopClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopClipboard")
            .field("device_id", &self.device_id)
            .field("poll_interval", &self.poll_interval)
            .finish_non_exhaustive()
    }
}

/// The polling loop behind [`ClipboardProvider::watch`].
async fn watch_loop(
    device_id: DeviceId,
    suppress: Arc<AtomicBool>,
    interval: Duration,
    sender: broadcast::Sender<ClipboardEvent>,
) {
    let mut last_token = crate::platform::change_token();
    let mut last_signature: Option<ContentSignature> = None;
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        if sender.receiver_count() == 0 {
            // The engine went away. Stop burning cycles on clipboard reads.
            return;
        }

        let token = crate::platform::change_token();
        let counter_says_changed = match (token, last_token) {
            (Some(now), Some(previous)) => now != previous,
            // No counter on this platform, or we just started: assume it may
            // have changed and let the signature check decide.
            _ => true,
        };
        last_token = token;

        if !counter_says_changed {
            continue;
        }

        // Our own write: report nothing and stop suppressing.
        if suppress.swap(false, Ordering::SeqCst) {
            continue;
        }

        let content = match tokio::task::spawn_blocking(move || read_clipboard(device_id)).await {
            Ok(Ok(content)) => content,
            Ok(Err(error)) => {
                let _ = sender.send(ClipboardEvent::Error(error.to_string()));
                continue;
            }
            Err(error) => {
                let _ = sender.send(ClipboardEvent::Error(format!(
                    "clipboard task failed: {error}"
                )));
                continue;
            }
        };

        let Some(content) = content else {
            continue;
        };

        // A counter can tick without the content changing (another clipboard
        // manager touching it). Compare signatures so the engine is not woken
        // for a no-op.
        let signature = ContentSignature::of(&content);
        if last_signature == Some(signature) {
            continue;
        }
        last_signature = Some(signature);

        if sender.send(ClipboardEvent::Changed(content)).is_err() {
            return;
        }
    }
}

/// Read whatever the clipboard currently holds.
///
/// Text wins over an image when both are present. Copying a selection that
/// contains a picture usually puts text, HTML *and* a bitmap on the clipboard,
/// and the text is the part the user meant; a screenshot or a "copy image"
/// action sets no text at all, so images still come through.
///
/// # Errors
/// Returns [`CoreError::Clipboard`] when the clipboard cannot be opened or the
/// image cannot be encoded.
pub fn read_clipboard(device_id: DeviceId) -> Result<Option<ClipboardContent>> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| CoreError::Clipboard(format!("could not open the clipboard: {error}")))?;

    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => {
            return Ok(Some(ClipboardContent::Text(TextPayload::new_local(
                text, device_id,
            ))))
        }
        Ok(_) => {}
        Err(arboard::Error::ContentNotAvailable) => {}
        Err(error) => {
            return Err(CoreError::Clipboard(format!(
                "could not read text from the clipboard: {error}"
            )))
        }
    }

    match clipboard.get_image() {
        Ok(image) => {
            let width = u32::try_from(image.width).unwrap_or(u32::MAX);
            let height = u32::try_from(image.height).unwrap_or(u32::MAX);

            let png = encode_png(image.bytes.as_ref(), width, height)?;
            let meta = ImageMeta::new_local(device_id, &png, width, height);

            Ok(Some(ClipboardContent::Image(ImagePayload {
                meta,
                data: png,
            })))
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(CoreError::Clipboard(format!(
            "could not read an image from the clipboard: {error}"
        ))),
    }
}

/// Put content on the clipboard.
///
/// # Errors
/// Returns [`CoreError::Clipboard`] when the clipboard is occupied or the
/// payload cannot be decoded.
pub fn write_clipboard(content: &ClipboardContent) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| CoreError::Clipboard(format!("could not open the clipboard: {error}")))?;

    match content {
        ClipboardContent::Text(payload) => clipboard
            .set_text(payload.content.as_str())
            .map_err(|error| CoreError::Clipboard(format!("could not write text: {error}"))),
        ClipboardContent::Image(payload) => {
            let (rgba, width, height) = decode_png(&payload.data)?;

            clipboard
                .set_image(arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: std::borrow::Cow::Owned(rgba),
                })
                .map_err(|error| CoreError::Clipboard(format!("could not write an image: {error}")))
        }
    }
}

/// Encode raw RGBA8 pixels as PNG.
///
/// # Errors
/// Returns [`CoreError::Clipboard`] when the buffer size does not match the
/// dimensions or the encoder fails.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    use image::ImageEncoder as _;

    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(CoreError::Clipboard(format!(
            "the platform handed over {} bytes for a {width}x{height} RGBA image, expected {expected}",
            rgba.len()
        )));
    }

    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|error| CoreError::Clipboard(format!("could not encode PNG: {error}")))?;
    Ok(out)
}

/// Decode PNG bytes back to RGBA8 pixels.
///
/// # Errors
/// Returns [`CoreError::Clipboard`] when the payload is not a PNG.
pub fn decode_png(png: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| CoreError::Clipboard(format!("could not decode PNG: {error}")))?
        .to_rgba8();

    let (width, height) = (decoded.width(), decoded.height());
    Ok((decoded.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_round_trips_through_png() {
        let width = 5;
        let height = 4;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[(x * 20) as u8, (y * 30) as u8, 128, 255]);
            }
        }

        let png = encode_png(&rgba, width, height).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "must be a real PNG");

        let (decoded, w, h) = decode_png(&png).unwrap();
        assert_eq!((w, h), (width, height));
        assert_eq!(decoded, rgba, "pixels must survive the round trip exactly");
    }

    #[test]
    fn a_mismatched_buffer_is_rejected() {
        assert!(encode_png(&[0u8; 10], 4, 4).is_err());
    }

    #[test]
    fn garbage_is_not_a_png() {
        assert!(decode_png(&[1, 2, 3, 4]).is_err());
    }
}
