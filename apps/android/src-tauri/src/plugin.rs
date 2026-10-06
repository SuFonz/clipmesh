//! The Rust half of the ClipMesh Android plugin.
//!
//! Tauri mobile plugins have two halves. This is the Rust one: it registers with
//! the Kotlin class [`ANDROID_PLUGIN_CLASS`] and wraps the resulting handle in a
//! typed API, so the rest of the crate never touches JNI or stringly typed
//! method names.
//!
//! ```text
//!   Vue ──invoke──► Tauri command ──► KotlinClipboardHost ──► PluginHandle
//!                                                                │
//!                    AndroidClipboardProvider ◄─────────────────┘
//!                              ▲
//!                    push() from Kotlin (notification action)
//! ```

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

use clipmesh_clipboard::PlatformClipboard;

/// Kotlin package that holds the plugin class.
pub const ANDROID_PLUGIN_PACKAGE: &str = "app.cm.clipmesh.bridge";

/// Kotlin class implementing `app.tauri.plugin.Plugin`.
pub const ANDROID_PLUGIN_CLASS: &str = "ClipMeshPlugin";

/// The plugin's name, as seen from JavaScript and from the config.
pub const PLUGIN_NAME: &str = "clipmesh-bridge";

/// What Kotlin sends back when asked for the clipboard.
///
/// Base64 is used for the image **only** across the JNI boundary, because the
/// plugin API marshals arguments as JSON. The network protocol never sees it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClipboardPayload {
    /// Plain text.
    Text {
        /// The text.
        text: String,
    },
    /// A PNG image.
    Image {
        /// Base64 encoded PNG bytes.
        png: String,
        /// Pixel width.
        width: u32,
        /// Pixel height.
        height: u32,
    },
    /// Nothing ClipMesh carries, or Android refused the read.
    Empty,
}

impl ClipboardPayload {
    /// Convert to what the clipboard provider expects.
    ///
    /// # Errors
    /// Returns an error when the base64 payload is not decodable.
    pub fn into_platform(self) -> Result<PlatformClipboard, String> {
        use base64::Engine as _;

        Ok(match self {
            Self::Text { text } => PlatformClipboard::Text(text),
            Self::Empty => PlatformClipboard::Empty,
            Self::Image { png, width, height } => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(png)
                    .map_err(|error| format!("the plugin sent invalid base64: {error}"))?;
                PlatformClipboard::Image {
                    png: decoded,
                    width,
                    height,
                }
            }
        })
    }
}

/// A typed handle to the Kotlin plugin.
#[derive(Debug)]
pub struct NativeBridge<R: Runtime> {
    handle: PluginHandle<R>,
}

impl<R: Runtime> NativeBridge<R> {
    /// Wrap a registered plugin handle.
    #[must_use]
    pub fn new(handle: PluginHandle<R>) -> Self {
        Self { handle }
    }

    /// Call a `@Command` on the Kotlin side.
    ///
    /// `run_mobile_plugin` only exists in a mobile build of Tauri, so the
    /// fallback keeps the whole crate compiling on a developer's laptop - which
    /// is how the shared command layer gets type-checked at all.
    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        payload: impl Serialize,
    ) -> Result<T, String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin::<T>(method, payload)
                .map_err(|error| error.to_string())
        }

        #[cfg(not(target_os = "android"))]
        {
            // Touching the handle keeps the field live in a desktop build,
            // where the method it exists for is not compiled.
            let _ = (&self.handle, method, payload);
            Err("the Android plugin is only available in a mobile build".to_owned())
        }
    }

    /// Read the clipboard through the platform.
    ///
    /// # Errors
    /// Returns the JNI failure, or "not a mobile build" on desktop.
    pub fn read_clipboard(&self) -> Result<PlatformClipboard, String> {
        let payload: ClipboardPayload = self.call("readClipboard", ())?;
        payload.into_platform()
    }

    /// Put text on the system clipboard.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn set_text(&self, text: &str) -> Result<(), String> {
        self.call("setText", SetTextArgs { text })
    }

    /// Put a base64 encoded PNG on the system clipboard.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn set_image_base64(&self, png_base64: &str) -> Result<(), String> {
        self.call("setImage", SetImageArgs { png: png_base64 })
    }

    /// Show the "clipboard received" notification.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn show_received(&self, title: &str, preview: &str, is_image: bool) -> Result<(), String> {
        self.call(
            "showReceived",
            ShowReceivedArgs {
                title,
                preview,
                is_image,
            },
        )
    }

    /// Start the foreground service that keeps the process alive.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn start_foreground_service(&self) -> Result<(), String> {
        self.call("startService", ())
    }

    /// Stop the foreground service.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn stop_foreground_service(&self) -> Result<(), String> {
        self.call("stopService", ())
    }

    /// Whether the foreground service is running.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn is_foreground_service_running(&self) -> Result<bool, String> {
        #[derive(Deserialize)]
        struct Running {
            running: bool,
        }

        self.call::<Running>("serviceRunning", ())
            .map(|response| response.running)
    }

    /// Ask for the notification permission (Android 13+).
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn request_notification_permission(&self) -> Result<bool, String> {
        #[derive(Deserialize)]
        struct Granted {
            granted: bool,
        }

        self.call::<Granted>("requestNotificationPermission", ())
            .map(|response| response.granted)
    }

    /// Send the app to the back of the task stack.
    ///
    /// The ending of the visible fallback: the notification's action normally
    /// reads the clipboard from a transparent activity that shows nothing, but
    /// when that cannot do its job the real activity is brought forward, and the
    /// user has to land back in whatever they were doing.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn leave_app(&self) -> Result<(), String> {
        // Kotlin answers with a bare `resolve()`, i.e. JSON null, so the result
        // is parsed as an opaque value rather than a struct.
        self.call::<serde_json::Value>("leaveApp", ())?;
        Ok(())
    }

    /// Collect a broadcast that the notification asked for, if any.
    ///
    /// Answering consumes the request: a second call answers `requested: false`
    /// even if the user tapped twice in a row, because the first tap's request
    /// is the one that matters.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn take_broadcast(&self) -> Result<BroadcastPickup, String> {
        self.call("takeBroadcast", ())
    }
}

/// What Kotlin answers when asked whether the notification asked for a broadcast.
///
/// `serviceRunning` is not about this request: the poller uses it to decide how
/// often to ask at all, because the notification - the only thing that can start
/// a request - is posted by that service and dies with it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastPickup {
    /// Whether a broadcast was requested since the last call.
    pub requested: bool,
    /// Whether the user is looking at the app because of this request.
    ///
    /// `false` for the transparent path, which shows nothing and therefore has
    /// nothing to leave afterwards; `true` for the fallback, which brought the
    /// activity forward and has to put it back.
    #[serde(default)]
    pub visible: bool,
    /// What the transparent activity read, when it managed to read anything.
    #[serde(default)]
    pub payload: Option<ClipboardPayload>,
    /// Whether the foreground service is up.
    #[serde(default)]
    pub service_running: bool,
}

#[derive(Serialize)]
struct SetTextArgs<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct SetImageArgs<'a> {
    png: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowReceivedArgs<'a> {
    title: &'a str,
    preview: &'a str,
    is_image: bool,
}

/// Register the plugin.
///
/// On desktop this is a no-op that still builds, so the same `lib.rs` can be
/// read on any machine.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new(PLUGIN_NAME)
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    api.register_android_plugin(ANDROID_PLUGIN_PACKAGE, ANDROID_PLUGIN_CLASS)?;
                // Managed behind an `Arc` so commands and the clipboard host
                // can both hold it; `PluginHandle` itself is not `Clone`.
                app.manage(std::sync::Arc::new(NativeBridge::new(handle)));
                tracing::info!("registered the ClipMesh Android plugin");
            }

            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }

            Ok(())
        })
        .build()
}

/// Fetch the bridge, if the plugin was registered.
#[must_use]
pub fn bridge<R: Runtime>(app: &impl Manager<R>) -> Option<std::sync::Arc<NativeBridge<R>>> {
    app.try_state::<std::sync::Arc<NativeBridge<R>>>()
        .map(|state| std::sync::Arc::clone(&state))
}

#[cfg(test)]
mod tests {
    use super::*;

    // These pin the shapes `ClipMeshPlugin.takeBroadcast` resolves. The Kotlin
    // half cannot be compiled here, so the JSON is the contract that has to be
    // checked from this side - a renamed field would otherwise only fail on a
    // device, where it fails silently: the poll would answer "nothing to do"
    // forever and the notification button would stop working.

    #[test]
    fn an_idle_answer_says_so() {
        // What Kotlin resolves when there is no request: the other keys are
        // absent rather than null.
        let pickup: BroadcastPickup =
            serde_json::from_str(r#"{"requested":false,"serviceRunning":true}"#).unwrap();

        assert!(!pickup.requested);
        assert!(!pickup.visible);
        assert!(pickup.payload.is_none());
        assert!(pickup.service_running);
    }

    #[test]
    fn a_read_answer_carries_its_content() {
        let pickup: BroadcastPickup = serde_json::from_str(
            r#"{"requested":true,"serviceRunning":true,"visible":false,
                "payload":{"kind":"text","text":"from the phone"}}"#,
        )
        .unwrap();

        assert!(pickup.requested);
        assert!(!pickup.visible, "the transparent path shows nothing");
        match pickup.payload {
            Some(ClipboardPayload::Text { text }) => assert_eq!(text, "from the phone"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn a_visible_answer_carries_no_content() {
        // The fallback reads the clipboard through the activity instead, so the
        // poll has to be able to tell the two apart without a payload.
        let pickup: BroadcastPickup =
            serde_json::from_str(r#"{"requested":true,"serviceRunning":true,"visible":true}"#)
                .unwrap();

        assert!(pickup.requested);
        assert!(pickup.visible);
        assert!(pickup.payload.is_none());
    }

    #[test]
    fn an_image_answer_decodes_from_the_same_shape() {
        use base64::Engine as _;

        let png = vec![0x89, b'P', b'N', b'G'];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
        let json = format!(
            r#"{{"requested":true,"serviceRunning":true,"visible":false,
                 "payload":{{"kind":"image","png":"{encoded}","width":4,"height":5}}}}"#
        );

        let pickup: BroadcastPickup = serde_json::from_str(&json).unwrap();
        let content = pickup
            .payload
            .expect("the payload is present")
            .into_platform()
            .unwrap();

        assert_eq!(
            content,
            PlatformClipboard::Image {
                png,
                width: 4,
                height: 5
            }
        );
    }
}
