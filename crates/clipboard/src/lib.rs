//! # clipmesh-clipboard
//!
//! Platform clipboard access behind one trait.
//!
//! | target | implementation |
//! | --- | --- |
//! | Windows | [`desktop`] via `arboard`, change detection via the clipboard sequence number |
//! | Linux | [`desktop`] via `arboard`, change detection by comparing content signatures |
//! | macOS | [`desktop`] via `arboard`, change detection by comparing content signatures |
//! | Android | [`android`], delegated to the Kotlin plugin because the platform forbids background clipboard reads |
//!
//! ## Images are always PNG
//!
//! Every platform hands pixels over in its own layout; this crate normalises all
//! of them to PNG before they reach the protocol layer, so a screenshot copied
//! on Windows arrives on Android as the same bytes a PNG decoder expects. Raw
//! RGBA from `arboard` is re-encoded here, and never put on the wire directly.
//!
//! ## Echo suppression
//!
//! Writing remote content onto the clipboard makes the platform watcher fire as
//! if the user had copied it. Both implementations mark the write so the change
//! is not reported back, but the engine's `EchoSuppressor` remains the
//! authoritative check - this is only an optimisation.

#![warn(missing_docs)]

#[cfg(not(target_os = "android"))]
pub mod desktop;

// Deliberately *not* gated on `target_os = "android"`.
//
// This module has no Android dependencies at all - it is a trait plus a state
// machine, and the platform calls live in the Kotlin plugin. Compiling it
// everywhere means the Android Tauri host can be type-checked on a developer's
// laptop, which is otherwise impossible without a full NDK toolchain.
pub mod android;

/// The platform module for the target being compiled.
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
pub mod platform;

/// The platform module for the target being compiled.
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
pub mod platform;

/// The platform module for the target being compiled.
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
pub mod platform;

#[cfg(not(target_os = "android"))]
pub use desktop::{decode_png, encode_png, read_clipboard, write_clipboard, DesktopClipboard};

pub use android::{
    notify_received, AndroidClipboardHost, AndroidClipboardProvider, PlatformClipboard,
};

use std::sync::Arc;

use clipmesh_core::ClipboardProvider;
use clipmesh_protocol::DeviceId;

/// Build the clipboard provider for a desktop target.
///
/// Android is deliberately not covered: its provider needs a Kotlin host, so
/// the Android app constructs [`android::AndroidClipboardProvider`] directly
/// with the implementation the Tauri plugin provides.
#[cfg(not(target_os = "android"))]
#[must_use]
pub fn create_provider(device_id: DeviceId) -> Arc<dyn ClipboardProvider> {
    Arc::new(platform::create(device_id))
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn the_factory_returns_a_provider() {
        let provider = create_provider(DeviceId::new());
        // The only thing we can assert without touching the real clipboard is
        // that it is a live object behind the trait.
        assert!(Arc::strong_count(&provider) >= 1);
    }
}
