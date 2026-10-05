//! Android clipboard access, delegated to the Kotlin plugin.
//!
//! ## Why this is not a direct implementation
//!
//! Two Android restrictions shape this file:
//!
//! 1. Since Android 10 an app in the background cannot read the clipboard at
//!    all. There is no polling equivalent of the desktop watcher.
//! 2. Clipboard access must happen on the main thread, through a
//!    `ClipboardManager` obtained from a `Context`.
//!
//! So the data flow is inverted: instead of polling, the Kotlin side pushes.
//! The foreground service's notification carries a "broadcast clipboard" action;
//! tapping it reads the clipboard on the main thread and calls
//! [`AndroidClipboardProvider::push`]. Writing a received payload goes the other
//! way, through [`AndroidClipboardHost`].
//!
//! ```text
//!   notification action ──► Kotlin reads clipboard ──► push()  ──► engine
//!   engine ──► write() ──► AndroidClipboardHost ──► Kotlin ClipboardManager
//! ```
//!
//! Keeping the platform calls behind a trait is also what lets `crates/**`
//! stay free of Android APIs, so the whole workspace still builds on a
//! developer's laptop.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use clipmesh_core::{ClipboardEvent, ClipboardProvider, Result};
use clipmesh_protocol::{
    ClipboardContent, ClipboardItem, DeviceId, ImageMeta, ImagePayload, TextPayload,
};

/// How many change events may queue before the oldest are dropped.
const CHANGE_CHANNEL_CAPACITY: usize = 8;

/// What the Kotlin side hands over.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlatformClipboard {
    /// Plain text.
    Text(String),
    /// A PNG encoded image plus its pixel dimensions.
    Image {
        /// PNG bytes.
        png: Vec<u8>,
        /// Pixel width.
        width: u32,
        /// Pixel height.
        height: u32,
    },
    /// Nothing we carry (empty, a file list, rich text only...).
    #[default]
    Empty,
}

/// The Kotlin half of the clipboard, and the notification surface.
///
/// Every method here runs on the platform's main thread; the plugin is
/// responsible for hopping threads if it is called from elsewhere.
pub trait AndroidClipboardHost: Send + Sync + 'static {
    /// Read the clipboard through the platform, on the main thread.
    ///
    /// Returns [`PlatformClipboard::Empty`] both when the clipboard really is
    /// empty *and* when Android refuses the read because the app does not have
    /// focus. The two are indistinguishable from here, and it does not matter:
    /// a background app cannot broadcast anyway.
    ///
    /// # Errors
    /// Platform failure.
    fn read_now(&self) -> Result<PlatformClipboard>;

    /// Put text on the system clipboard.
    ///
    /// # Errors
    /// Platform failure.
    fn set_text(&self, text: &str) -> Result<()>;

    /// Put a PNG image on the system clipboard.
    ///
    /// # Errors
    /// Platform failure.
    fn set_image(&self, png: &[u8]) -> Result<()>;

    /// Show the "received from <device>" notification with a copy action.
    ///
    /// # Errors
    /// Platform failure.
    fn show_received(&self, item: &ClipboardItem, source: &str) -> Result<()>;

    /// Make sure the foreground service is running so the process survives.
    ///
    /// # Errors
    /// Platform failure, for example a missing permission.
    fn ensure_foreground_service(&self) -> Result<()>;

    /// Tear the foreground service down.
    ///
    /// # Errors
    /// Platform failure.
    fn stop_foreground_service(&self) -> Result<()>;

    /// Whether the foreground service is currently running.
    fn is_foreground_service_running(&self) -> bool;
}

/// The Android clipboard provider.
pub struct AndroidClipboardProvider {
    device_id: DeviceId,
    host: Arc<dyn AndroidClipboardHost>,
    events: broadcast::Sender<ClipboardEvent>,
    /// The most recent content the platform pushed. Android will not let a
    /// background thread read the clipboard, so this is our only source for
    /// [`ClipboardProvider::read`].
    last: Mutex<Option<ClipboardContent>>,
}

impl AndroidClipboardProvider {
    /// Build a provider around a Kotlin host.
    #[must_use]
    pub fn new(device_id: DeviceId, host: Arc<dyn AndroidClipboardHost>) -> Arc<Self> {
        let (events, _) = broadcast::channel(CHANGE_CHANNEL_CAPACITY);
        Arc::new(Self {
            device_id,
            host,
            events,
            last: Mutex::new(None),
        })
    }

    /// The Kotlin host, for the Tauri command layer.
    #[must_use]
    pub fn host(&self) -> &Arc<dyn AndroidClipboardHost> {
        &self.host
    }

    /// Record content the platform read for us.
    ///
    /// This is the entry point behind the notification's "broadcast clipboard"
    /// button. It emits a change event, which is what makes the engine pick the
    /// content up and send it - exactly as if the desktop watcher had fired.
    pub fn push(&self, payload: PlatformClipboard) {
        if let Some(content) = self.convert(payload) {
            *self.last.lock() = Some(content.clone());
            // A send error only means nobody is listening right now.
            let _ = self.events.send(ClipboardEvent::Changed(content));
        }
    }

    /// Turn platform content into protocol content.
    fn convert(&self, payload: PlatformClipboard) -> Option<ClipboardContent> {
        match payload {
            // An empty clipboard is not a change worth reporting.
            PlatformClipboard::Empty => None,
            PlatformClipboard::Text(text) if text.is_empty() => None,
            PlatformClipboard::Text(text) => Some(ClipboardContent::Text(TextPayload::new_local(
                text,
                self.device_id,
            ))),
            PlatformClipboard::Image { png, width, height } => {
                let meta = ImageMeta::new_local(self.device_id, &png, width, height);
                Some(ClipboardContent::Image(ImagePayload { meta, data: png }))
            }
        }
    }

    /// Report a platform failure to the engine.
    pub fn push_error(&self, message: impl Into<String>) {
        let _ = self.events.send(ClipboardEvent::Error(message.into()));
    }
}

impl std::fmt::Debug for AndroidClipboardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndroidClipboardProvider")
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ClipboardProvider for AndroidClipboardProvider {
    async fn read(&self) -> Result<Option<ClipboardContent>> {
        // Ask the platform first: the cached value is only a fallback for when
        // Android refuses the read (background) or the plugin is not attached.
        match self.host.read_now() {
            Ok(payload) => {
                let content = self.convert(payload);
                if let Some(content) = &content {
                    *self.last.lock() = Some(content.clone());
                }
                Ok(content)
            }
            Err(error) => {
                tracing::debug!(%error, "the platform refused a clipboard read; using the cached value");
                Ok(self.last.lock().clone())
            }
        }
    }

    async fn write(&self, content: &ClipboardContent) -> Result<()> {
        // Writing is what makes received content usable, so it must succeed -
        // there is no "the platform will notice" fallback on Android.
        match content {
            ClipboardContent::Text(payload) => self.host.set_text(&payload.content)?,
            ClipboardContent::Image(payload) => self.host.set_image(&payload.data)?,
        }

        // Keep `read` consistent with what we just wrote.
        *self.last.lock() = Some(content.clone());
        Ok(())
    }

    fn watch(&self) -> BoxStream<'static, ClipboardEvent> {
        let receiver = self.events.subscribe();
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
}

/// Show the notification that a payload arrived.
///
/// # Errors
/// Returns [`CoreError::Clipboard`] when the platform refuses.
pub fn notify_received(
    host: &dyn AndroidClipboardHost,
    item: &ClipboardItem,
    source: &str,
) -> Result<()> {
    host.show_received(item, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_core::CoreError;
    use futures::{FutureExt as _, StreamExt as _};

    struct FakeHost {
        text: Mutex<Vec<String>>,
        images: Mutex<Vec<Vec<u8>>>,
        /// What the platform clipboard would return.
        current: Mutex<PlatformClipboard>,
        /// Set to simulate an app without focus, where Android refuses reads.
        refuse_reads: std::sync::atomic::AtomicBool,
        service_running: Mutex<bool>,
    }

    impl Default for FakeHost {
        fn default() -> Self {
            Self {
                text: Mutex::new(Vec::new()),
                images: Mutex::new(Vec::new()),
                current: Mutex::new(PlatformClipboard::Empty),
                refuse_reads: std::sync::atomic::AtomicBool::new(false),
                service_running: Mutex::new(false),
            }
        }
    }

    impl AndroidClipboardHost for FakeHost {
        fn read_now(&self) -> Result<PlatformClipboard> {
            if self.refuse_reads.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(CoreError::Clipboard(
                    "the app does not have focus".to_owned(),
                ));
            }
            Ok(self.current.lock().clone())
        }

        fn set_text(&self, text: &str) -> Result<()> {
            self.text.lock().push(text.to_owned());
            *self.current.lock() = PlatformClipboard::Text(text.to_owned());
            Ok(())
        }

        fn set_image(&self, png: &[u8]) -> Result<()> {
            self.images.lock().push(png.to_vec());
            *self.current.lock() = PlatformClipboard::Image {
                png: png.to_vec(),
                width: 1,
                height: 1,
            };
            Ok(())
        }

        fn show_received(&self, _item: &ClipboardItem, _source: &str) -> Result<()> {
            Ok(())
        }

        fn ensure_foreground_service(&self) -> Result<()> {
            *self.service_running.lock() = true;
            Ok(())
        }

        fn stop_foreground_service(&self) -> Result<()> {
            *self.service_running.lock() = false;
            Ok(())
        }

        fn is_foreground_service_running(&self) -> bool {
            *self.service_running.lock()
        }
    }

    fn provider() -> (Arc<AndroidClipboardProvider>, Arc<FakeHost>) {
        let host = Arc::new(FakeHost::default());
        let provider = AndroidClipboardProvider::new(DeviceId::new(), host.clone());
        (provider, host)
    }

    #[tokio::test]
    async fn pushed_text_becomes_the_readable_clipboard() {
        let (provider, host) = provider();
        // Simulate the app being in the background: Android refuses the read, so
        // the provider must fall back to what Kotlin pushed.
        host.refuse_reads
            .store(true, std::sync::atomic::Ordering::Relaxed);

        assert!(provider.read().await.unwrap().is_none());
        provider.push(PlatformClipboard::Text("from the phone".into()));

        let content = provider.read().await.unwrap().unwrap();
        match content {
            ClipboardContent::Text(payload) => assert_eq!(payload.content, "from the phone"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reading_asks_the_platform_first() {
        let (provider, host) = provider();
        *host.current.lock() = PlatformClipboard::Text("copied just now".into());

        // No push happened, so the cache is empty - the value can only have
        // come from the platform.
        match provider.read().await.unwrap().unwrap() {
            ClipboardContent::Text(payload) => assert_eq!(payload.content, "copied just now"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_empty_platform_clipboard_is_not_a_change() {
        let (provider, _host) = provider();
        let mut events = provider.watch();

        provider.push(PlatformClipboard::Empty);
        provider.push(PlatformClipboard::Text(String::new()));

        assert!(provider.read().await.unwrap().is_none());
        assert!(events.next().now_or_never().is_none());
    }

    #[tokio::test]
    async fn a_push_wakes_the_watcher() {
        let (provider, _host) = provider();
        let mut events = provider.watch();

        provider.push(PlatformClipboard::Text("hello".into()));

        let event = events.next().await.unwrap();
        assert!(matches!(event, ClipboardEvent::Changed(_)));
    }
    #[tokio::test]
    async fn writing_goes_through_the_platform_host() {
        let (provider, host) = provider();

        let text = ClipboardContent::Text(TextPayload::new_local("remote", DeviceId::new()));
        provider.write(&text).await.unwrap();

        assert_eq!(host.text.lock().as_slice(), ["remote".to_owned()]);
        // ... and is now what `read` reports.
        assert!(matches!(
            provider.read().await.unwrap(),
            Some(ClipboardContent::Text(_))
        ));
    }

    #[tokio::test]
    async fn images_round_trip_with_their_checksum() {
        let (provider, host) = provider();
        // The push path exists precisely because Android refuses a background
        // read; the cached value is what `read` must fall back to.
        host.refuse_reads
            .store(true, std::sync::atomic::Ordering::Relaxed);

        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4];

        provider.push(PlatformClipboard::Image {
            png: png.clone(),
            width: 2,
            height: 2,
        });

        let content = provider.read().await.unwrap().unwrap();
        match &content {
            ClipboardContent::Image(payload) => {
                assert_eq!(payload.data, png);
                assert_eq!(payload.meta.size, png.len() as u64);
                assert!(payload.meta.verify(&payload.data).is_ok());
            }
            other => panic!("expected an image, got {other:?}"),
        }

        provider.write(&content).await.unwrap();
        assert_eq!(host.images.lock().as_slice(), [png]);
    }

    #[tokio::test]
    async fn platform_errors_reach_the_engine() {
        let (provider, _host) = provider();
        let mut events = provider.watch();

        provider.push_error("the clipboard is locked by another app");

        match events.next().await.unwrap() {
            ClipboardEvent::Error(message) => assert!(message.contains("locked")),
            other => panic!("expected an error event, got {other:?}"),
        }
    }

    #[test]
    fn the_foreground_service_is_driven_through_the_host() {
        let (_provider, host) = provider();
        assert!(!host.is_foreground_service_running());
        host.ensure_foreground_service().unwrap();
        assert!(host.is_foreground_service_running());
        host.stop_foreground_service().unwrap();
        assert!(!host.is_foreground_service_running());
    }
}
