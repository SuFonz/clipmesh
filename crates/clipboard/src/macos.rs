//! macOS clipboard access.
//!
//! Like Linux there is no sequence number available through a lightweight API,
//! so the watcher compares content signatures. The pasteboard also has a
//! `changeCount`, but reaching it means going through `objc2`, which is a
//! dependency this crate does not otherwise need; the signature comparison
//! costs one clipboard read per tick and is good enough at 250 ms.

use clipmesh_protocol::DeviceId;

pub use crate::desktop::DesktopClipboard;

/// The provider type for this platform.
pub type PlatformClipboard = DesktopClipboard;

/// Build the macOS clipboard provider.
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
