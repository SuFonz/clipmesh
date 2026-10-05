//! # ClipMesh desktop
//!
//! The Tauri host for Windows, Linux and macOS. It owns three things:
//!
//! * **Assembly** - [`state::AppState`] builds the identity, the trust store,
//!   the platform clipboard and the network service, then injects them into
//!   [`clipmesh_core::SyncManager`]. This is the only place where concrete
//!   providers meet the engine.
//! * **The IPC surface** - [`commands`] exposes the commands from
//!   `docs/IPC.md`, and `forward_events` turns the engine's event stream into
//!   `clipmesh://…` events for the webview.
//! * **Desktop chrome** - [`tray`] and the window close behaviour.
//!
//! Rust does the work: clipboard watching, TCP, TLS, mDNS, identity. Tauri
//! supplies the window, the tray, settings and autostart, and the Vue frontend
//! renders whatever the engine publishes.

#![warn(missing_docs)]

pub mod commands;
pub mod state;

#[cfg(feature = "desktop-tray")]
mod tray;

use std::sync::Arc;

use tauri::{Emitter as _, Manager as _, RunEvent, WindowEvent};
use tokio::sync::broadcast;

use clipmesh_core::{CoreEvent, SharedSyncManager};

use crate::state::AppState;

/// Start the desktop application.
///
/// # Panics
/// Panics if Tauri cannot create the window, which means the bundled assets are
/// missing - a build problem, not a runtime condition worth recovering from.
#[cfg(not(target_os = "android"))]
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::build()?;

            // Decide before the window is shown, so a tray-first start does not
            // flash a window on screen first.
            let start_hidden = state.engine.settings().start_minimized;
            let engine = Arc::clone(&state.engine);
            app.manage(state);

            #[cfg(feature = "desktop-tray")]
            tray::build(&handle, Arc::clone(&engine))?;

            if start_hidden {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            start_engine(handle, engine);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Keep running in the tray: closing the window must not stop
                // clipboard sync. The tray's Quit item is the way out.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::clear_error,
            commands::list_peers,
            commands::list_trusted_devices,
            commands::list_pairing_requests,
            commands::get_history,
            commands::get_settings,
            commands::get_identity,
            commands::start_engine,
            commands::stop_engine,
            commands::update_settings,
            commands::set_device_name,
            commands::request_pairing,
            commands::respond_pairing,
            commands::unpair_device,
            commands::send_clipboard,
            commands::send_text,
            commands::resend_history_item,
            commands::copy_history_item,
            commands::clear_history,
            commands::get_image_thumbnail,
            commands::android_start_service,
            commands::android_stop_service,
            commands::android_service_running,
            commands::android_request_notification_permission,
        ])
        .build(tauri::generate_context!())
        .expect("could not build the ClipMesh window")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // Last window hidden is not a reason to quit: the whole point is
                // to keep syncing from the tray.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Bring the engine up and start forwarding its events to the webview.
pub fn start_engine(app: tauri::AppHandle, engine: SharedSyncManager) {
    tauri::async_runtime::spawn(async move {
        // The consumer loops must be running before the network opens,
        // otherwise the first discovery events would be dropped.
        engine.spawn();

        if let Err(error) = engine.start().await {
            tracing::error!(%error, "the engine could not start");
        }

        forward_events(app, engine).await;
    });
}

/// Turn every [`CoreEvent`] into a `clipmesh://…` event for the frontend.
///
/// Runs for the life of the process. The engine's channel is a broadcast, so a
/// webview that reloads simply misses the events that happened while it was
/// gone and then refills from the `list_*` commands - which is exactly why the
/// events carry snapshots rather than deltas.
pub async fn forward_events(app: tauri::AppHandle, engine: SharedSyncManager) {
    let mut events = engine.subscribe();

    loop {
        match events.recv().await {
            Ok(event) => {
                let name = commands::event_name(&event);
                let payload = event_payload(&event);

                // Keep the tray's check mark honest when the UI changes the
                // setting.
                #[cfg(feature = "desktop-tray")]
                if let CoreEvent::Status(status) = &event {
                    tray::refresh(&app, status.auto_sync);
                }

                if let Err(error) = app.emit(&name, payload) {
                    tracing::debug!(%error, %name, "could not deliver an event to the webview");
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "the webview fell behind the engine event stream");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// The JSON shape each event carries, as specified in `docs/IPC.md` §4.
pub fn event_payload(event: &CoreEvent) -> serde_json::Value {
    use serde_json::json;

    match event {
        CoreEvent::Peers(peers) => json!(peers),
        CoreEvent::Trusted(devices) => json!(devices),
        CoreEvent::PairingRequests(prompts) => json!(prompts),
        CoreEvent::ClipboardReceived(item) => json!(item),
        CoreEvent::ClipboardSent { item, delivered } => {
            json!({ "item": item, "delivered": delivered })
        }
        CoreEvent::History(items) => json!(items),
        CoreEvent::Status(status) => json!(status),
        CoreEvent::Error(message) => json!(message),
    }
}

/// Structured logs on stderr, filtered by `CLIPMESH_LOG`.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_env("CLIPMESH_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_core::{PairingDirection, PairingPrompt, PeerView, StatusView};
    use clipmesh_protocol::{ClipboardItem, DeviceId, Platform, TextPayload};

    #[test]
    fn event_names_match_the_ipc_contract() {
        let cases = [
            (CoreEvent::Peers(Vec::new()), "clipmesh://peers"),
            (CoreEvent::Trusted(Vec::new()), "clipmesh://trusted"),
            (
                CoreEvent::PairingRequests(Vec::new()),
                "clipmesh://pairing-requests",
            ),
            (CoreEvent::History(Vec::new()), "clipmesh://history"),
            (CoreEvent::Error("x".into()), "clipmesh://error"),
        ];
        for (event, expected) in cases {
            assert_eq!(commands::event_name(&event), expected);
        }
    }

    #[test]
    fn peer_events_carry_a_camel_case_array() {
        let peer = PeerView {
            device_id: DeviceId::new(),
            name: "Pixel".into(),
            platform: Platform::Android,
            address: "192.168.1.5:47711".into(),
            fingerprint: "AAAA BBBB".into(),
            trusted: true,
            connected: false,
            pairing: false,
            last_seen: 1,
        };

        let payload = event_payload(&CoreEvent::Peers(vec![peer]));
        assert!(payload.is_array());
        assert_eq!(payload[0]["trusted"], true);
        assert_eq!(payload[0]["platform"], "android");
        assert_eq!(payload[0]["lastSeen"], 1);
    }

    #[test]
    fn clipboard_events_use_the_typescript_discriminator() {
        let item = ClipboardItem::Text(TextPayload::new_local("hello", DeviceId::new()));
        let payload = event_payload(&CoreEvent::ClipboardReceived(Box::new(item)));

        assert_eq!(payload["kind"], "text");
        assert_eq!(payload["content"], "hello");
        assert!(payload["sourceDevice"].is_string());
    }

    #[test]
    fn sent_events_carry_the_delivery_count() {
        let item = ClipboardItem::Text(TextPayload::new_local("hi", DeviceId::new()));
        let payload = event_payload(&CoreEvent::ClipboardSent {
            item: Box::new(item),
            delivered: 2,
        });

        assert_eq!(payload["delivered"], 2);
        assert_eq!(payload["item"]["kind"], "text");
    }

    #[test]
    fn status_events_are_serialisable_for_the_status_bar() {
        let status = StatusView {
            running: true,
            auto_sync: false,
            device_id: DeviceId::new(),
            device_name: "Laptop".into(),
            platform: Platform::Windows,
            fingerprint: "AAAA".into(),
            listen_port: 47711,
            connected_peers: 1,
            trusted_peers: 1,
            last_error: None,
        };
        let payload = event_payload(&CoreEvent::Status(Box::new(status)));
        assert_eq!(payload["autoSync"], false);
        assert_eq!(payload["listenPort"], 47711);
    }

    #[test]
    fn pairing_events_carry_the_dialog() {
        let prompt = PairingPrompt {
            device_id: DeviceId::new(),
            name: "Pixel 9".into(),
            platform: Platform::Android,
            fingerprint: "1111 2222".into(),
            address: "192.168.1.7:47711".into(),
            requested_at: 42,
            direction: PairingDirection::Incoming,
        };
        let payload = event_payload(&CoreEvent::PairingRequests(vec![prompt]));
        assert_eq!(payload[0]["direction"], "incoming");
        assert_eq!(payload[0]["requestedAt"], 42);
    }
}
