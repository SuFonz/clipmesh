//! # ClipMesh for Android
//!
//! The Tauri mobile host. It exists to supply the one thing the desktop host
//! cannot: a clipboard provider backed by Kotlin, because Android refuses to
//! let a background app read the clipboard and requires an `Activity` to write
//! it.
//!
//! Everything else is shared with the desktop build:
//!
//! | concern | where it lives |
//! | --- | --- |
//! | state assembly (keys, trust, engine) | `clipmesh_desktop_lib::state` |
//! | the IPC command layer | `clipmesh_desktop_lib::commands` |
//! | event forwarding | `clipmesh_desktop_lib::forward_events` |
//! | clipboard | [`clipmesh_clipboard::AndroidClipboardProvider`] + Kotlin |
//!
//! Because the desktop crate is compiled with `default-features = false`, the
//! Tauri tray - which cannot build for Android - is left out.

#![warn(missing_docs)]

pub mod broadcast;
pub mod clipboard_host;
pub mod commands;
pub mod plugin;
pub mod received;

use std::sync::{Arc, OnceLock};

use tauri::{Manager as _, Wry};

use clipmesh_clipboard::AndroidClipboardProvider;
use clipmesh_core::SharedSyncManager;
use clipmesh_desktop_lib::images::ImageCache;
use clipmesh_desktop_lib::state::{AppState, SetupError};
use clipmesh_identity::IdentityPaths;

use crate::clipboard_host::KotlinClipboardHost;
use crate::plugin::NativeBridge;

/// State that only the Android host has.
pub struct NativeState {
    /// The clipboard provider, so commands can push content into it.
    pub clipboard: Arc<AndroidClipboardProvider>,
    /// The handle to the Kotlin plugin.
    pub bridge: Arc<NativeBridge<Wry>>,
    /// The pixels behind history images.
    ///
    /// The same store the engine writes an image into, which is what lets a
    /// history entry be shared without a second place to keep pixels.
    pub images: Arc<ImageCache>,
}

/// Start the Android application.
///
/// # Panics
/// Panics when Tauri cannot create the webview, which means the bundled assets
/// are missing - a build problem.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    clipmesh_desktop_lib::init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(plugin::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let bridge = plugin::bridge(&handle)
                .ok_or("the ClipMesh Android plugin did not register itself during setup")?;

            let host = Arc::new(KotlinClipboardHost::new(Arc::clone(&bridge)));

            // The clipboard provider needs this device's id, which only exists
            // once the identity is loaded - so it is built by a factory and the
            // result is captured here for the commands to use.
            let slot: Arc<OnceLock<Arc<AndroidClipboardProvider>>> = Arc::new(OnceLock::new());

            let state = AppState::build_in(state_paths(&handle)?, {
                let slot = Arc::clone(&slot);
                let host: Arc<dyn clipmesh_clipboard::AndroidClipboardHost> = host;
                move |device_id| {
                    let provider = AndroidClipboardProvider::new(device_id, host);
                    let _ = slot.set(Arc::clone(&provider));
                    provider
                }
            })?;

            let engine: SharedSyncManager = Arc::clone(&state.engine);
            let images = Arc::clone(&state.images);
            app.manage(state);

            let clipboard = slot
                .get()
                .cloned()
                .ok_or("the clipboard provider factory did not run")?;

            // Keep the process alive: on Android the engine stops the moment the
            // activity is collected unless a foreground service is holding it.
            if let Err(error) = clipboard.host().ensure_foreground_service() {
                tracing::warn!(
                    %error,
                    "could not start the foreground service; sync will stop when the app leaves \
                     the foreground"
                );
            }

            // The notification's broadcast action is collected by a task of its
            // own rather than by the UI: it is requested while the app is in the
            // background, where the only thing still running is this process
            // behind the foreground service.
            let broadcast_bridge = Arc::clone(&bridge);
            let broadcast_clipboard = Arc::clone(&clipboard);
            let received_bridge = Arc::clone(&bridge);
            let received_images = Arc::clone(&images);

            // Screenshot sync is restored from the setting, **not** re-asked:
            // the permission belongs to the moment the user turns the switch on,
            // so this call is told not to request anything. A permission that
            // was revoked in the system settings since then simply leaves the
            // observer unregistered, and the settings screen reports that.
            if engine.settings().android_screenshot_sync {
                match bridge.set_screenshot_sync(true, false) {
                    Ok(state) if state.watching => {
                        tracing::info!("the screenshot watcher is running");
                    }
                    Ok(state) => tracing::warn!(
                        partial = state.partial,
                        granted = state.granted,
                        "screenshot sync is on but the media permission is not granted"
                    ),
                    Err(error) => {
                        tracing::warn!(%error, "could not start the screenshot watcher");
                    }
                }
            }

            app.manage(NativeState {
                clipboard,
                bridge,
                images: Arc::clone(&images),
            });

            clipmesh_desktop_lib::start_engine(handle, Arc::clone(&engine));
            broadcast::spawn(broadcast_bridge, broadcast_clipboard, engine.clone());
            received::spawn(received_bridge, engine, received_images);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Shared with the desktop build.
            clipmesh_desktop_lib::commands::get_status,
            clipmesh_desktop_lib::commands::clear_error,
            clipmesh_desktop_lib::commands::list_peers,
            clipmesh_desktop_lib::commands::list_trusted_devices,
            clipmesh_desktop_lib::commands::list_pairing_requests,
            clipmesh_desktop_lib::commands::get_history,
            clipmesh_desktop_lib::commands::get_settings,
            clipmesh_desktop_lib::commands::get_identity,
            clipmesh_desktop_lib::commands::start_engine,
            clipmesh_desktop_lib::commands::stop_engine,
            clipmesh_desktop_lib::commands::update_settings,
            clipmesh_desktop_lib::commands::set_device_name,
            clipmesh_desktop_lib::commands::request_pairing,
            clipmesh_desktop_lib::commands::respond_pairing,
            clipmesh_desktop_lib::commands::unpair_device,
            clipmesh_desktop_lib::commands::send_clipboard,
            clipmesh_desktop_lib::commands::send_text,
            clipmesh_desktop_lib::commands::resend_history_item,
            clipmesh_desktop_lib::commands::copy_history_item,
            clipmesh_desktop_lib::commands::clear_history,
            clipmesh_desktop_lib::commands::get_image_thumbnail,
            // Android only. The desktop build answers these with a clear
            // "android commands are only available on Android" instead.
            commands::android_start_service,
            commands::android_stop_service,
            commands::android_service_running,
            commands::android_notification_permission,
            commands::android_leave_app,
            commands::android_request_notification_permission,
            commands::android_push_clipboard,
            commands::android_report_error,
            commands::android_screenshot_permission,
            commands::android_set_screenshot_sync,
            commands::android_share_image,
        ])
        .run(tauri::generate_context!())
        .expect("could not start ClipMesh on Android");
}

/// Where the Android build keeps its keys, trust store and settings.
///
/// [`AppState::build_with`] cannot be used here. It resolves the state directory
/// with [`IdentityPaths::discover`], which goes through `dirs::config_dir()`, and
/// `dirs-sys` reports `None` on Android: its `home_dir` has no fallback there
/// (`#[cfg(target_os = "android")] fn fallback() -> Option<OsString> { None }`),
/// unlike desktop Linux, which falls back to `getpwuid_r`.
///
/// The failure used to surface as an immediate crash on launch with nothing in
/// the log, because a setup error makes `run()` return `Err`, `.expect` turns
/// that into a panic, and `[profile.release]` sets `panic = "abort"`.
///
/// Tauri installs its own `path` plugin into every app
/// (`App::register_core_plugins`), and on Android that plugin's `getConfigDir`
/// answers `Context.getDataDir()`. `app_config_dir` appends the bundle
/// identifier, so this resolves to
/// `/data/user/0/app.cm.clipmesh/app.cm.clipmesh` - private to the app and
/// writable without any runtime permission.
fn state_paths<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<IdentityPaths, SetupError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| SetupError::Paths(error.to_string()))?;

    tracing::info!(dir = %dir.display(), "resolved the Android state directory");

    Ok(IdentityPaths::at(dir))
}
