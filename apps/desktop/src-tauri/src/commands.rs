//! The IPC surface described in `docs/IPC.md`.
//!
//! Every command returns `Result<T, String>`: the frontend shows the string in
//! a toast, so a structured error it cannot render would be worse than a
//! sentence. Errors are logged with their full detail before being flattened.

use std::sync::Arc;

use clipmesh_core::{
    CoreEvent, PairingPrompt, PeerView, SendOutcome, Settings, SettingsPatch, StatusView,
    TrustedDeviceView,
};
use clipmesh_protocol::{ClipboardItem, DeviceId, Platform};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// Shorthand for a command result.
type Ipc<T> = Result<T, String>;

/// Log a failure with full detail, then hand the frontend a sentence.
fn fail(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    tracing::warn!(%message, "command failed");
    message
}

// ---------------------------------------------------------------------------
// Views that only exist at the IPC boundary
// ---------------------------------------------------------------------------

/// What the UI needs to display this device's identity.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    /// Stable device id.
    pub device_id: String,
    /// Display name.
    pub device_name: String,
    /// Operating system family.
    pub platform: Platform,
    /// Grouped fingerprint, for reading aloud.
    pub fingerprint: String,
    /// Public key, lowercase hex.
    pub public_key: String,
    /// The certificate, for a user who wants to check it by hand.
    pub certificate_pem: String,
}

/// The result of pushing content to the mesh.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    /// Identifier of what was sent.
    pub id: String,
    /// How many peers accepted it.
    pub delivered: usize,
}

impl From<SendOutcome> for SendResult {
    fn from(outcome: SendOutcome) -> Self {
        Self {
            id: outcome.id,
            delivered: outcome.delivered,
        }
    }
}

// ---------------------------------------------------------------------------
// State and lists
// ---------------------------------------------------------------------------

/// Current engine status.
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Ipc<StatusView> {
    Ok(state.engine.status())
}

/// Acknowledge the last error so the banner can be dismissed.
///
/// Without this the UI had nothing to call: `lastError` is part of every status
/// snapshot, so clearing it only locally brought it straight back on the next
/// event.
#[tauri::command]
pub fn clear_error(state: State<'_, AppState>) -> Ipc<StatusView> {
    Ok(state.engine.clear_error())
}

/// Devices currently visible on the network.
#[tauri::command]
pub fn list_peers(state: State<'_, AppState>) -> Ipc<Vec<PeerView>> {
    Ok(state.engine.peers())
}

/// Devices the user has trusted.
#[tauri::command]
pub fn list_trusted_devices(state: State<'_, AppState>) -> Ipc<Vec<TrustedDeviceView>> {
    Ok(state.engine.trusted_devices())
}

/// Pairing requests waiting for a decision.
#[tauri::command]
pub fn list_pairing_requests(state: State<'_, AppState>) -> Ipc<Vec<PairingPrompt>> {
    Ok(state.engine.pairing_requests())
}

/// Clipboard history, newest first.
#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Ipc<Vec<ClipboardItem>> {
    Ok(state.engine.history())
}

/// Current settings.
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Ipc<Settings> {
    Ok(state.engine.settings())
}

/// This device's identity.
#[tauri::command]
pub fn get_identity(state: State<'_, AppState>) -> Ipc<IdentityView> {
    Ok(identity_view(&state))
}

fn identity_view(state: &AppState) -> IdentityView {
    IdentityView {
        device_id: state.identity.device_id().to_string(),
        device_name: state.identity.name(),
        platform: Platform::current(),
        fingerprint: state.identity.fingerprint().to_grouped(),
        public_key: state.identity.public_key_hex(),
        certificate_pem: state.identity.certificate().pem().to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Start discovery and the listener. Idempotent.
#[tauri::command]
pub async fn start_engine(state: State<'_, AppState>) -> Ipc<StatusView> {
    state.engine.start().await.map_err(fail)?;
    Ok(state.engine.status())
}

/// Stop the network layer. Idempotent.
#[tauri::command]
pub async fn stop_engine(state: State<'_, AppState>) -> Ipc<StatusView> {
    state.engine.stop().await.map_err(fail)?;
    Ok(state.engine.status())
}

/// Apply a partial settings update.
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Ipc<Settings> {
    state.engine.update_settings(patch).await.map_err(fail)
}

/// Rename this device.
///
/// Implemented as a settings patch so the name is persisted in exactly one
/// place and the mDNS re-announcement happens through the normal path.
#[tauri::command]
pub async fn set_device_name(state: State<'_, AppState>, name: String) -> Ipc<IdentityView> {
    let patch = SettingsPatch {
        device_name: Some(name),
        ..SettingsPatch::default()
    };
    state.engine.update_settings(patch).await.map_err(fail)?;
    Ok(identity_view(&state))
}

// ---------------------------------------------------------------------------
// Pairing and trust
// ---------------------------------------------------------------------------

/// Ask a discovered device to pair.
#[tauri::command(rename_all = "camelCase")]
pub async fn request_pairing(state: State<'_, AppState>, device_id: String) -> Ipc<()> {
    let device = parse_device(&device_id)?;
    state.engine.request_pairing(device).await.map_err(fail)
}

/// Answer a pairing request.
#[tauri::command(rename_all = "camelCase")]
pub async fn respond_pairing(
    state: State<'_, AppState>,
    device_id: String,
    accept: bool,
) -> Ipc<()> {
    let device = parse_device(&device_id)?;
    state
        .engine
        .respond_pairing(device, accept)
        .await
        .map_err(fail)
}

/// Forget a device and drop its session.
#[tauri::command(rename_all = "camelCase")]
pub async fn unpair_device(state: State<'_, AppState>, device_id: String) -> Ipc<()> {
    let device = parse_device(&device_id)?;
    state.engine.unpair(device).await.map_err(fail)
}

fn parse_device(text: &str) -> Ipc<DeviceId> {
    DeviceId::parse(text).map_err(|error| format!("that is not a device id: {error}"))
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

/// Read the local clipboard and broadcast it.
///
/// This is the command behind the tray item and the Android notification
/// action: it works whether or not automatic sync is on, because the user asked
/// for it explicitly.
#[tauri::command]
pub async fn send_clipboard(state: State<'_, AppState>) -> Ipc<SendResult> {
    state.engine.send_clipboard().await.map(SendResult::from).map_err(fail)
}

/// Broadcast some text without touching the clipboard.
#[tauri::command]
pub async fn send_text(state: State<'_, AppState>, content: String) -> Ipc<SendResult> {
    state
        .engine
        .send_text(content)
        .await
        .map(SendResult::from)
        .map_err(fail)
}

/// Send a history entry again.
#[tauri::command(rename_all = "camelCase")]
pub async fn resend_history_item(state: State<'_, AppState>, id: String) -> Ipc<SendResult> {
    state
        .engine
        .resend_history_item(&id)
        .await
        .map(SendResult::from)
        .map_err(fail)
}

/// Put a history entry back on the local clipboard without sending it.
#[tauri::command(rename_all = "camelCase")]
pub async fn copy_history_item(state: State<'_, AppState>, id: String) -> Ipc<()> {
    state.engine.copy_history_item(&id).await.map_err(fail)
}

/// Empty the history.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Ipc<()> {
    state.engine.clear_history();
    Ok(())
}

/// A downscaled preview of a history image.
///
/// Served from the copy the engine stored when the image entered the history,
/// so every row can have a preview rather than only the one still on the
/// clipboard. This is the only place ClipMesh uses base64 - the wire format is
/// raw PNG chunks, and the encoding here exists purely so the webview can
/// render an `<img>` without a custom protocol handler. The image is downscaled
/// first, so a 4K screenshot does not become a 13 MB string.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_image_thumbnail(
    state: State<'_, AppState>,
    id: String,
    max_size: u32,
) -> Ipc<String> {
    let images = Arc::clone(&state.images);
    let limit = max_size.clamp(32, 512);

    // Decoding a screenshot is not work for the runtime's worker threads.
    tokio::task::spawn_blocking(move || match images.read(&id) {
        Ok(Some(png)) => thumbnail_data_url(&png, limit),
        Ok(None) => Err("there is no stored copy of that image".to_owned()),
        Err(error) => Err(fail(error)),
    })
    .await
    .map_err(|error| fail(format!("the preview task panicked: {error}")))?
}

fn thumbnail_data_url(png: &[u8], max_size: u32) -> Ipc<String> {
    use base64::Engine as _;

    let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| fail(format!("could not decode the image: {error}")))?;

    // `thumbnail` preserves the aspect ratio and is much cheaper than `resize`
    // for this use.
    let thumbnail = decoded.thumbnail(max_size, max_size);

    let mut encoded = Vec::new();
    thumbnail
        .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
        .map_err(|error| fail(format!("could not encode the preview: {error}")))?;

    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded)
    ))
}

// ---------------------------------------------------------------------------
// Android-only commands
// ---------------------------------------------------------------------------

/// Not available on desktop.
///
/// Declared so the frontend sees a clear message instead of "command not
/// found", which reads like a bug in the app rather than a platform difference.
#[tauri::command]
pub async fn android_start_service() -> Ipc<()> {
    Err(ANDROID_ONLY.to_owned())
}

/// Not available on desktop.
#[tauri::command]
pub async fn android_stop_service() -> Ipc<()> {
    Err(ANDROID_ONLY.to_owned())
}

/// Not available on desktop.
#[tauri::command]
pub async fn android_service_running() -> Ipc<bool> {
    Err(ANDROID_ONLY.to_owned())
}

/// Not available on desktop.
#[tauri::command]
pub async fn android_request_notification_permission() -> Ipc<bool> {
    Err(ANDROID_ONLY.to_owned())
}

const ANDROID_ONLY: &str = "android commands are only available on Android";

/// The Tauri event name for a core event.
///
/// The names match `docs/IPC.md` exactly; the frontend listens on them.
#[must_use]
pub fn event_name(event: &CoreEvent) -> String {
    format!("clipmesh://{}", event.name())
}
