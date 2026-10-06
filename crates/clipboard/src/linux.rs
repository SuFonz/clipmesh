//! Linux clipboard access.
//!
//! There is no cheap change counter on X11 or Wayland, so the watcher compares
//! content signatures instead (see [`crate::desktop::watch_loop`]). That means
//! every poll tick reads the clipboard, which is why the interval is 250 ms and
//! not something more eager.
//!
//! Wayland note: a client may only read the clipboard while it has focus, so
//! background sync is best effort there. X11 and XWayland behave normally.

use clipmesh_protocol::DeviceId;

pub use crate::desktop::DesktopClipboard;

/// The provider type for this platform.
pub type PlatformClipboard = DesktopClipboard;

/// Build the Linux clipboard provider.
#[must_use]
pub fn create(device_id: DeviceId) -> PlatformClipboard {
    DesktopClipboard::new(device_id)
}

/// No sequence number exists, so the watcher falls back to content comparison.
#[must_use]
pub const fn change_token() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_no_change_counter() {
        assert!(change_token().is_none());
    }
}
