//! Collecting the clipboard broadcast that the notification action asked for.
//!
//! ## Why a task, and why it polls
//!
//! The notification's button cannot do the work itself. Android 10 and later
//! hand the clipboard only to an app in the foreground, so the action opens a
//! transparent activity (`BroadcastActivity` in the Kotlin bridge) that reads
//! the clipboard while it holds focus and finishes. Nothing about that can reach
//! the engine on its own: Tauri's Android API is one-way (Rust calls Kotlin;
//! Kotlin cannot call Rust), and the plugin's `trigger` needs a JS listener this
//! plugin is not registered for. The webview cannot do it either, because the
//! whole point is that the app stays in the background with only the foreground
//! service and this Rust host alive.
//!
//! So the activity leaves the request in a process-wide Kotlin holder and this
//! task, which lives as long as the process does, collects it:
//!
//! ```text
//!   notification action ──► BroadcastActivity ──► BroadcastHandoff
//!                                                      │
//!                          takeBroadcast ◄─────────────┘   (this task)
//!                                │
//!              engine.send_explicit(…) ──► peers
//! ```
//!
//! ## When it polls
//!
//! A tap is possible only while the notification exists, and the notification is
//! posted by the foreground service and dies with it - which Kotlin reports back
//! with every answer. While it is up the task ticks every [`ACTIVE_POLL`], so a
//! tap is picked up almost at once; while it is down there is no button to press,
//! so the task drops to [`IDLE_POLL`] instead of spinning at the fast rate for
//! the whole life of the process. Neither state can lose a request: Kotlin keeps
//! one for fifteen seconds, deliberately longer than the slow tick.

use std::sync::Arc;
use std::time::Duration;

use tauri::Wry;

use clipmesh_clipboard::AndroidClipboardProvider;
use clipmesh_core::SharedSyncManager;
use clipmesh_protocol::ClipboardContent;

use crate::plugin::{BroadcastPickup, ClipboardPayload, NativeBridge};

/// How often to look for a request while the notification action exists.
const ACTIVE_POLL: Duration = Duration::from_millis(500);

/// How often to look while the foreground service - and with it the
/// notification that carries the action - is down.
const IDLE_POLL: Duration = Duration::from_secs(5);

/// How long to wait before reading the clipboard again on the visible path.
const VISIBLE_SETTLE: Duration = Duration::from_millis(400);

/// How many times the visible path reads before believing an empty clipboard.
const VISIBLE_ATTEMPTS: usize = 3;

/// Start collecting notification broadcasts.
///
/// Runs for the life of the process, which on Android is as long as the
/// foreground service holds it.
pub fn spawn(
    bridge: Arc<NativeBridge<Wry>>,
    clipboard: Arc<AndroidClipboardProvider>,
    engine: SharedSyncManager,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let pickup = match bridge.take_broadcast() {
                Ok(pickup) => pickup,
                Err(error) => {
                    tracing::warn!(%error, "could not collect a notification broadcast");
                    // The bridge is broken, not slow: back off to the idle rate
                    // rather than hammering a failing JNI call twice a second.
                    tokio::time::sleep(IDLE_POLL).await;
                    continue;
                }
            };

            if pickup.requested {
                deliver(&bridge, &clipboard, &engine, &pickup).await;
            }

            let interval = if pickup.service_running {
                ACTIVE_POLL
            } else {
                IDLE_POLL
            };
            tokio::time::sleep(interval).await;
        }
    });
}

/// Perform one requested broadcast.
///
/// The two paths differ in where the content comes from and in what the user has
/// to be left with afterwards, which is exactly what `visible` says.
async fn deliver(
    bridge: &NativeBridge<Wry>,
    clipboard: &AndroidClipboardProvider,
    engine: &SharedSyncManager,
    pickup: &BroadcastPickup,
) {
    if pickup.visible {
        deliver_visible(bridge, clipboard, engine).await;
        return;
    }

    match &pickup.payload {
        Some(payload) => deliver_read(clipboard, engine, payload).await,
        // Defensive: the transparent activity only deposits a request it has
        // content for. Nothing to send, and nobody to tell either - that path
        // never showed the app.
        None => tracing::warn!("a broadcast was requested without any content"),
    }
}

/// Send content that the transparent activity read while it held focus.
async fn deliver_read(
    clipboard: &AndroidClipboardProvider,
    engine: &SharedSyncManager,
    payload: &ClipboardPayload,
) {
    let platform = match payload.clone().into_platform() {
        Ok(platform) => platform,
        Err(error) => return report(clipboard, format!("could not read the clipboard: {error}")),
    };

    // `stage` records it as the current clipboard for later reads. Going through
    // `push` instead would report a *change*, which the engine filters through
    // `autoSync` - and that would leave this button dead for exactly the users
    // who turned automatic sync off and press it by hand.
    let Some(content) = clipboard.stage(platform) else {
        return report(clipboard, empty_clipboard());
    };

    send(clipboard, engine, content).await;
}

/// The fallback: the app was brought forward, so read through it.
///
/// The activity has only just been started, and Android refuses a clipboard read
/// until its window has focus, so an empty first read means "not yet" at least as
/// often as it means "empty". Reading a few times costs nothing and keeps the
/// fallback from reporting a failure the user cannot act on.
async fn deliver_visible(
    bridge: &NativeBridge<Wry>,
    clipboard: &AndroidClipboardProvider,
    engine: &SharedSyncManager,
) {
    for attempt in 0..VISIBLE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(VISIBLE_SETTLE).await;
        }

        match engine.read_clipboard().await {
            Ok(Some(content)) => {
                if send(clipboard, engine, content).await {
                    // Only after a successful broadcast: a failed one leaves the
                    // user in the app, looking at the reason.
                    if let Err(error) = bridge.leave_app() {
                        tracing::warn!(%error, "could not send the app back to the background");
                    }
                }
                return;
            }
            Ok(None) => continue,
            Err(error) => {
                return report(clipboard, format!("could not read the clipboard: {error}"));
            }
        }
    }

    report(clipboard, empty_clipboard());
}

/// Push content to every connected trusted peer.
///
/// Returns whether it went out, which is what the visible path keys its ending
/// off.
async fn send(
    clipboard: &AndroidClipboardProvider,
    engine: &SharedSyncManager,
    content: ClipboardContent,
) -> bool {
    match engine.send_explicit(content).await {
        Ok(outcome) => {
            tracing::info!(
                delivered = outcome.delivered,
                "broadcast the clipboard for the notification action"
            );
            true
        }
        Err(error) => {
            report(
                clipboard,
                format!("could not broadcast the clipboard: {error}"),
            );
            false
        }
    }
}

/// What Android hands back when the clipboard holds nothing we carry.
///
/// Worded like the engine's own error for the same situation, so the message
/// reads the same however the user got there.
fn empty_clipboard() -> String {
    "the clipboard is empty or holds something ClipMesh does not carry".to_owned()
}

/// Report a failure through the engine's error channel.
///
/// The failure usually happens with the app in the background and nobody
/// watching, so a log line is not enough: this becomes the error the UI shows the
/// next time the user opens ClipMesh.
fn report(clipboard: &AndroidClipboardProvider, message: String) {
    tracing::warn!(%message, "the notification broadcast failed");
    clipboard.push_error(message);
}
