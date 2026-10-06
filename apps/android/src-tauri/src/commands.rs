//! The Android-only half of the IPC surface.
//!
//! Everything else - status, peers, pairing, history, settings, clipboard
//! sending - is the shared command layer in `clipmesh_desktop_lib::commands`,
//! registered by the same `generate_handler!` list. Only the commands that talk
//! to Android itself live here.

use tauri::State;

use base64::Engine as _;

use clipmesh_clipboard::PlatformClipboard;
use clipmesh_core::ImageStore as _;

use crate::plugin::{ClipboardPayload, ScreenshotState};
use crate::NativeState;

/// Shorthand for a command result.
type Ipc<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    tracing::warn!(%message, "android command failed");
    message
}

/// Convert what Kotlin sent into what the provider wants.
///
/// # Errors
/// Returns an error when the base64 image payload cannot be decoded.
fn to_platform(payload: ClipboardPayload) -> Ipc<PlatformClipboard> {
    payload.into_platform()
}

/// Start the foreground service that keeps ClipMesh alive in the background.
#[tauri::command]
pub async fn android_start_service(state: State<'_, NativeState>) -> Ipc<()> {
    state
        .clipboard
        .host()
        .ensure_foreground_service()
        .map_err(fail)
}

/// Stop the foreground service.
#[tauri::command]
pub async fn android_stop_service(state: State<'_, NativeState>) -> Ipc<()> {
    state
        .clipboard
        .host()
        .stop_foreground_service()
        .map_err(fail)
}

/// Whether the foreground service is running.
#[tauri::command]
pub async fn android_service_running(state: State<'_, NativeState>) -> Ipc<bool> {
    Ok(state.clipboard.host().is_foreground_service_running())
}

/// Send the app to the back of the task stack after a visible broadcast.
///
/// The notification's action normally reads the clipboard from a transparent
/// activity that shows nothing, and the Rust host calls this itself when it has
/// to fall back to bringing the real activity forward. This command is the same
/// thing for the frontend, which is where a UI that wants to leave on purpose
/// would call it.
#[tauri::command]
pub async fn android_leave_app(state: State<'_, NativeState>) -> Ipc<()> {
    let bridge = state.bridge.clone();
    bridge.leave_app().map_err(fail)
}

/// Whether the notification permission is granted (Android 13 and later).
///
/// The query half of [`android_request_notification_permission`]: the settings
/// toggle is displayed from this answer, so it must not show the system dialog.
#[tauri::command]
pub async fn android_notification_permission(state: State<'_, NativeState>) -> Ipc<bool> {
    let bridge = state.bridge.clone();
    bridge
        .notification_permission()
        .map_err(|error| fail(format!("could not read the permission: {error}")))
}

/// Ask for the notification permission (Android 13 and later).
#[tauri::command]
pub async fn android_request_notification_permission(state: State<'_, NativeState>) -> Ipc<bool> {
    let bridge = state.bridge.clone();
    bridge
        .request_notification_permission()
        .map_err(|error| fail(format!("could not request the permission: {error}")))
}

/// Record clipboard content that Kotlin read for us.
///
/// This reports a **change**, not an explicit send: the engine's local change
/// handling drops it while `autoSync` is off, which is right for something the
/// platform noticed by itself and wrong for the notification's broadcast button.
/// That button therefore goes through [`crate::broadcast`] and the engine's
/// explicit send instead, and this command has no caller in the UI today.
#[tauri::command(rename_all = "camelCase")]
pub async fn android_push_clipboard(
    state: State<'_, NativeState>,
    payload: ClipboardPayload,
) -> Ipc<()> {
    let content = to_platform(payload)?;
    state.clipboard.push(content);
    Ok(())
}

/// Report a platform failure so it reaches the UI as a normal error event.
#[tauri::command]
pub async fn android_report_error(state: State<'_, NativeState>, message: String) -> Ipc<()> {
    state.clipboard.push_error(message);
    Ok(())
}

/// Whether the screenshot watcher may read this device's images, and whether it
/// is watching.
///
/// **Queries only** - no dialog, and no side effects. The settings switch is
/// drawn from this answer, and drawing a switch must not ask the user anything
/// or start anything.
#[tauri::command]
pub async fn android_screenshot_permission(state: State<'_, NativeState>) -> Ipc<ScreenshotState> {
    let bridge = state.bridge.clone();
    bridge
        .screenshot_permission()
        .map_err(|error| fail(format!("could not read the permission: {error}")))
}

/// Turn screenshot sync on or off.
///
/// `enabled` is the user's decision, and the permission is requested right here
/// when it is `true`: there is no way to read an image out of `MediaStore`
/// without it, and asking at any other moment would be a permission prompt the
/// user did not ask for. The answer carries the permission *and* whether the
/// observer ended up registered, so a UI cannot show a switch that lies.
#[tauri::command]
pub async fn android_set_screenshot_sync(
    state: State<'_, NativeState>,
    enabled: bool,
) -> Ipc<ScreenshotState> {
    let bridge = state.bridge.clone();
    bridge
        .set_screenshot_sync(enabled, true)
        .map_err(|error| fail(format!("could not change screenshot sync: {error}")))
}

/// Hand a history image to Android's share sheet.
///
/// The pixels come from the stored `<state>/images/<id>.png` copy - the same one
/// the history thumbnails are served from - so an entry can be shared long after
/// the clipboard has moved on. `entry_id` names the staged file Kotlin writes,
/// which is what keeps a second share from overwriting a URI a chooser still
/// holds.
#[tauri::command(rename_all = "camelCase")]
pub async fn android_share_image(state: State<'_, NativeState>, id: String) -> Ipc<()> {
    let bridge = state.bridge.clone();
    let images = std::sync::Arc::clone(&state.images);
    let entry_id = id.clone();

    // Reading a multi-megabyte PNG and base64-encoding it is not work for the
    // runtime's worker threads - the same rule `get_image_thumbnail` follows.
    let encoded = tokio::task::spawn_blocking(move || match images.get(&id) {
        Some(png) => Ok(base64::engine::general_purpose::STANDARD.encode(png)),
        None => Err("there is no stored copy of that image".to_owned()),
    })
    .await
    .map_err(|error| fail(format!("the share task panicked: {error}")))??;

    bridge
        .share_image(&entry_id, &encoded)
        .map_err(|error| fail(format!("could not share the image: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_payloads_convert() {
        let payload = ClipboardPayload::Text {
            text: "hi".into(),
        };
        assert_eq!(
            to_platform(payload).unwrap(),
            PlatformClipboard::Text("hi".into())
        );
    }

    #[test]
    fn image_payloads_decode_their_base64() {
        use base64::Engine as _;

        let png = vec![0x89, b'P', b'N', b'G'];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&png);

        let payload = ClipboardPayload::Image {
            png: encoded,
            width: 2,
            height: 3,
        };

        assert_eq!(
            to_platform(payload).unwrap(),
            PlatformClipboard::Image {
                png,
                width: 2,
                height: 3
            }
        );
    }

    #[test]
    fn invalid_base64_is_rejected() {
        let payload = ClipboardPayload::Image {
            png: "not base64!!".into(),
            width: 1,
            height: 1,
        };
        assert!(to_platform(payload).is_err());
    }

    #[test]
    fn empty_payloads_are_carried_through() {
        assert_eq!(
            to_platform(ClipboardPayload::Empty).unwrap(),
            PlatformClipboard::Empty
        );
    }
}
