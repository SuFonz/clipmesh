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
    /// A **text** notification, or the fallback shape for an image whose pixels
    /// could not be read back: there is no preview bitmap and no share action,
    /// and Kotlin drops whatever image it had staged for the previous one.
    ///
    /// There is no "this is an image" flag on the wire. The notification shows
    /// the preview line either way, and the flag that used to sit here could not
    /// have worked: Kotlin's `var isImage` generates `setImage`, so Jackson looked
    /// for `image` and ignored what was sent.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn show_received(&self, title: &str, preview: &str) -> Result<(), String> {
        self.call("showReceived", ShowReceivedArgs { title, preview })
    }

    /// Show the "clipboard received" notification for an image.
    ///
    /// Kotlin stages the PNG in the cache directory it exposes through the
    /// FileProvider, draws it as the notification's preview, and hangs a share
    /// action on the notification. Base64 across the JNI boundary again - the
    /// plugin API marshals JSON - and the notification is the one place where a
    /// received image has to become a file before anything else can happen.
    ///
    /// `entry_id` names the staged file, so a second image cannot overwrite the
    /// first one while a chooser still holds its URI.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn show_received_image(
        &self,
        entry_id: &str,
        title: &str,
        preview: &str,
        png_base64: &str,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.call(
            "showReceivedImage",
            ShowReceivedImageArgs {
                entry_id,
                title,
                preview,
                png: png_base64,
                width,
                height,
            },
        )
    }

    /// Hand an image to Android's share sheet.
    ///
    /// The bytes are the stored copy of a history image: Kotlin writes them into
    /// the FileProvider-backed cache directory and opens a chooser for the
    /// resulting `content://` URI, so nothing ever sees a `file://` path.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn share_image(&self, entry_id: &str, png_base64: &str) -> Result<(), String> {
        self.call(
            "shareImage",
            ShareImageArgs {
                entry_id,
                png: png_base64,
            },
        )
    }

    /// Whether the media-read permission the screenshot watcher needs is held.
    ///
    /// The query half of [`Self::set_screenshot_sync`], for the same reason
    /// [`Self::notification_permission`] exists: the settings switch is
    /// *displayed* from this answer, and displaying must not put a system dialog
    /// in front of the user - nor, for that matter, start or stop anything. It
    /// reports whether the observer is registered as well, because "the setting
    /// says on" and "screenshots are actually being watched" are different
    /// things.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn screenshot_permission(&self) -> Result<ScreenshotState, String> {
        self.call("screenshotPermission", ())
    }

    /// Turn the screenshot watcher on or off.
    ///
    /// `request_permission` is what separates the two callers:
    ///
    ///  * the settings switch, which passes `true` - the user just asked for the
    ///    feature, which is the only moment a media permission may be requested;
    ///  * the host at startup, which passes `false` - it is restoring a setting
    ///    the user already agreed to, and a system dialog on launch is exactly
    ///    what "request permissions when they are needed" forbids.
    ///
    /// Registering only happens when the permission is actually held, so the
    /// answer reports both the permission and whether anything is watching.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn set_screenshot_sync(
        &self,
        enabled: bool,
        request_permission: bool,
    ) -> Result<ScreenshotState, String> {
        self.call(
            "setScreenshotSync",
            SetScreenshotSyncArgs {
                enabled,
                request_permission,
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
        self.call::<NotificationPermission>("requestNotificationPermission", ())
            .map(|response| response.granted)
    }

    /// Whether the notification permission is currently granted (Android 13+).
    ///
    /// The query half of [`Self::request_notification_permission`], and the
    /// reason there are two: the settings toggle is *displayed* from this, and a
    /// display must not put the system dialog in front of the user. Below
    /// Android 13 there is no runtime permission and Kotlin answers `true`.
    ///
    /// # Errors
    /// Returns the JNI failure as a string.
    pub fn notification_permission(&self) -> Result<bool, String> {
        self.call::<NotificationPermission>("notificationPermission", ())
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

/// What Kotlin answers when asked about the notification permission.
///
/// Both the query and the prompt resolve this same `{"granted": …}` object, and
/// the UI compares the two answers - one shape keeps them from drifting apart.
#[derive(Deserialize)]
struct NotificationPermission {
    /// Whether POST_NOTIFICATIONS is held (always true below Android 13).
    granted: bool,
}

/// Whether the screenshot watcher may read the device's images, and whether it
/// is actually watching.
///
/// More than one boolean, because the states are not "yes/no":
///
///  * `granted` - the full grant (`READ_MEDIA_IMAGES` on 33+,
///    `READ_EXTERNAL_STORAGE` below that). The whole library is readable.
///  * `partial` - Android 14 lets the user hand over *some* photos instead of all
///    of them. `READ_MEDIA_VISUAL_USER_SELECTED` is granted and the real media
///    permission is denied, and `MediaStore` then answers with only the selected
///    items. The watcher refuses to start in this state - it would look alive
///    while missing most screenshots - and the settings screen says so instead of
///    showing a switch that mostly does nothing.
///  * `watching` - an observer is registered right now. Derived from the two
///    above plus the setting, and reported rather than assumed, so a switch drawn
///    from this answer cannot lie: a permission revoked in the system settings
///    makes it read off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotState {
    /// Whether the full media-read permission is held.
    pub granted: bool,
    /// Whether only the user-selected subset is readable (Android 14+).
    #[serde(default)]
    pub partial: bool,
    /// Whether the observer is registered.
    #[serde(default)]
    pub watching: bool,
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
    /// Where the payload came from: the notification action or the screenshot
    /// watcher.
    ///
    /// They travel the same road - a process-wide handoff collected by the same
    /// poll - but they are not the same thing to the user, so the log line and
    /// the error a failed send produces have to say which one it was. Absent
    /// means `clipboard`, which is also what every pre-screenshot Kotlin build
    /// sends.
    #[serde(default)]
    pub source: PickupSource,
}

/// What put a payload in the handoff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickupSource {
    /// The notification's "broadcast clipboard" action.
    #[default]
    Clipboard,
    /// A screenshot the observer noticed.
    Screenshot,
}

impl PickupSource {
    /// How to name this source in a log line or an error.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Clipboard => "the clipboard",
            Self::Screenshot => "the screenshot",
        }
    }
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowReceivedImageArgs<'a> {
    entry_id: &'a str,
    title: &'a str,
    preview: &'a str,
    png: &'a str,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareImageArgs<'a> {
    entry_id: &'a str,
    png: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetScreenshotSyncArgs {
    enabled: bool,
    request_permission: bool,
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
    fn a_permission_answer_must_carry_the_granted_flag() {
        // Both `ClipMeshPlugin.notificationPermission` and its prompt
        // counterpart resolve this object, and the settings toggle is drawn from
        // the query's answer. A renamed key would otherwise only fail on a
        // device, as a toggle that reads "off" while the service is running.
        let granted: NotificationPermission = serde_json::from_str(r#"{"granted":true}"#).unwrap();
        assert!(granted.granted);

        let denied: NotificationPermission = serde_json::from_str(r#"{"granted":false}"#).unwrap();
        assert!(!denied.granted);

        assert!(
            serde_json::from_str::<NotificationPermission>(r#"{"allowed":true}"#).is_err(),
            "a missing `granted` is a Kotlin/Rust contract break, not a default"
        );
    }

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
    fn a_pickup_says_whether_it_was_a_screenshot() {
        // Both sources travel the same handoff; only this field tells them
        // apart, and it decides the wording of the log and of a failed send.
        let screenshot: BroadcastPickup = serde_json::from_str(
            r#"{"requested":true,"serviceRunning":true,"source":"screenshot",
                "payload":{"kind":"image","png":"","width":1,"height":1}}"#,
        )
        .unwrap();
        assert_eq!(screenshot.source, PickupSource::Screenshot);
        assert_eq!(screenshot.source.describe(), "the screenshot");

        let clipboard: BroadcastPickup = serde_json::from_str(
            r#"{"requested":true,"serviceRunning":true,"source":"clipboard"}"#,
        )
        .unwrap();
        assert_eq!(clipboard.source, PickupSource::Clipboard);

        // Absent is the old Kotlin build's answer, and has to keep meaning
        // "the notification button".
        let old: BroadcastPickup =
            serde_json::from_str(r#"{"requested":true,"serviceRunning":true}"#).unwrap();
        assert_eq!(old.source, PickupSource::Clipboard);
    }

    #[test]
    fn a_permission_answer_has_to_say_what_was_granted() {
        // `{"granted": true}` without the flags is every answer from a build
        // before Android 14 and before the observer state was reported, and must
        // keep meaning "full access".
        let full: ScreenshotState = serde_json::from_str(r#"{"granted":true}"#).unwrap();
        assert!(full.granted);
        assert!(!full.partial);
        assert!(!full.watching);

        let partial: ScreenshotState =
            serde_json::from_str(r#"{"granted":false,"partial":true,"watching":false}"#).unwrap();
        assert!(!partial.granted);
        assert!(partial.partial);

        assert!(
            serde_json::from_str::<ScreenshotState>(r#"{"partial":true}"#).is_err(),
            "a missing `granted` is a Kotlin/Rust contract break, not a default"
        );
    }

    #[test]
    fn a_screenshot_sync_answer_carries_the_permission_and_the_observer() {
        let on: ScreenshotState =
            serde_json::from_str(r#"{"granted":true,"partial":false,"watching":true}"#).unwrap();
        assert_eq!(
            on,
            ScreenshotState {
                granted: true,
                partial: false,
                watching: true,
            }
        );

        // A denied request leaves the switch off and the observer unregistered.
        let denied: ScreenshotState =
            serde_json::from_str(r#"{"granted":false,"watching":false}"#).unwrap();
        assert!(!denied.granted);
        assert!(!denied.watching);
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
