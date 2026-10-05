//! The Android-only half of the IPC surface.
//!
//! Everything else - status, peers, pairing, history, settings, clipboard
//! sending - is the shared command layer in `clipmesh_desktop_lib::commands`,
//! registered by the same `generate_handler!` list. Only the commands that talk
//! to Android itself live here.

use tauri::State;

use clipmesh_clipboard::PlatformClipboard;

use crate::plugin::ClipboardPayload;
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
/// This is the other half of the "broadcast clipboard" notification action: the
/// service reads the clipboard on the main thread and invokes this, which wakes
/// the engine exactly as a desktop clipboard change would.
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
