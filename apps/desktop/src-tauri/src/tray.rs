//! The system tray icon and its menu.
//!
//! The tray is built in Rust rather than declared in `tauri.conf.json`, because
//! a declarative tray cannot carry the click handlers that make it useful - and
//! "broadcast my clipboard right now" is the whole reason the tray exists.
//!
//! Closing the main window hides it instead of quitting. A clipboard syncher
//! that dies when you close its window cannot sync anything, so the tray's
//! **Quit** item is the explicit way out.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use clipmesh_core::{SettingsPatch, SharedSyncManager};

/// Menu item identifiers.
mod id {
    pub const SHOW: &str = "tray.show";
    pub const BROADCAST: &str = "tray.broadcast";
    pub const AUTO_SYNC: &str = "tray.auto-sync";
    pub const QUIT: &str = "tray.quit";
}

/// Handles to the menu items whose state the app updates later.
///
/// Kept in Tauri's managed state rather than in a global, so it is dropped with
/// the app and cannot outlive the runtime it belongs to.
pub struct TrayHandles {
    auto_sync: CheckMenuItem<tauri::Wry>,
}

/// Install the tray icon.
///
/// # Errors
/// Returns a Tauri error when the icon or menu cannot be created, which in
/// practice means the bundled icons are missing.
pub fn build(app: &AppHandle, engine: SharedSyncManager) -> tauri::Result<()> {
    let auto_sync = engine.settings().auto_sync;

    let show = MenuItem::with_id(app, id::SHOW, "Show ClipMesh", true, None::<&str>)?;
    let broadcast = MenuItem::with_id(
        app,
        id::BROADCAST,
        "Broadcast clipboard now",
        true,
        None::<&str>,
    )?;
    let auto = CheckMenuItem::with_id(
        app,
        id::AUTO_SYNC,
        "Sync automatically",
        true,
        auto_sync,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, id::QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &broadcast,
            &PredefinedMenuItem::separator(app)?,
            &auto,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("the window icon is missing".to_owned()))?;

    app.manage(TrayHandles {
        auto_sync: auto.clone(),
    });

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("ClipMesh")
        .menu(&menu)
        // Left click raises the window; the menu is on right click, which is
        // what desktop users expect from a tray application.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            id::SHOW => reveal_window(app),
            id::BROADCAST => broadcast_now(app),
            id::AUTO_SYNC => toggle_auto_sync(app, &auto),
            id::QUIT => quit_app(app),
            other => tracing::debug!(id = other, "unhandled tray menu item"),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Keep the check mark in step with settings changed elsewhere in the UI.
pub fn refresh(app: &AppHandle, auto_sync: bool) {
    if let Some(handles) = app.try_state::<TrayHandles>() {
        let _ = handles.auto_sync.set_checked(auto_sync);
    }
}

fn broadcast_now(app: &AppHandle) {
    let engine = app.state::<crate::state::AppState>().engine.clone();
    tauri::async_runtime::spawn(async move {
        match engine.send_clipboard().await {
            Ok(outcome) => {
                tracing::info!(delivered = outcome.delivered, "broadcast from the tray");
            }
            Err(error) => {
                // Most often "the clipboard is empty" - not worth a dialog, but
                // the user pressed a button so it must reach the log.
                tracing::warn!(%error, "the tray broadcast did nothing");
            }
        }
    });
}

fn toggle_auto_sync(app: &AppHandle, item: &CheckMenuItem<tauri::Wry>) {
    let engine = app.state::<crate::state::AppState>().engine.clone();
    // The check mark already flipped when the user clicked; ask for the
    // opposite of what the engine currently thinks.
    let enabled = !engine.settings().auto_sync;
    let item = item.clone();

    tauri::async_runtime::spawn(async move {
        let patch = SettingsPatch {
            auto_sync: Some(enabled),
            ..SettingsPatch::default()
        };
        match engine.update_settings(patch).await {
            Ok(settings) => {
                let _ = item.set_checked(settings.auto_sync);
            }
            Err(error) => {
                tracing::warn!(%error, "could not toggle automatic sync");
                // Put the check mark back where it was.
                let _ = item.set_checked(!enabled);
            }
        }
    });
}

fn quit_app(app: &AppHandle) {
    let engine = app.state::<crate::state::AppState>().engine.clone();
    let handle = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(error) = engine.stop().await {
            tracing::warn!(%error, "the engine did not stop cleanly");
        }
        handle.exit(0);
    });
}

fn reveal_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
