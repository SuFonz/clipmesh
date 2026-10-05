//! Windows clipboard access.
//!
//! Windows is the easy platform: `GetClipboardSequenceNumber` is a global
//! counter that increments whenever the clipboard changes, and it can be read
//! from any thread without opening the clipboard. That makes change detection a
//! single integer comparison instead of a content read, which is why polling is
//! cheap here.

use clipmesh_protocol::DeviceId;

pub use crate::desktop::DesktopClipboard;

/// The provider type for this platform.
pub type PlatformClipboard = DesktopClipboard;

/// Build the Windows clipboard provider.
#[must_use]
pub fn create(device_id: DeviceId) -> PlatformClipboard {
    DesktopClipboard::new(device_id)
}

/// Read the clipboard sequence number.
///
/// Returns `None` when the OS reports zero, which happens when the calling
/// session has no window station (a service, or a session that is not
/// interactive). Callers treat that as "unknown" and fall back to comparing
/// content.
///
/// The counter can jump by more than one per change - other clipboard managers
/// and Windows' own clipboard history bump it too - so it is only ever compared
/// for equality.
#[must_use]
pub fn change_token() -> Option<u64> {
    clipboard_win::seq_num().map(|value| u64::from(value.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_change_token_is_cheap_and_safe_to_poll() {
        // Reading the sequence number does not open the clipboard, so it is
        // safe from any thread and safe to call in a loop. The value itself is
        // only meaningful relative to a previous reading, so all this asserts
        // is that asking does not fail or panic.
        let first = change_token();
        let second = change_token();
        assert_eq!(first, second, "two reads in a row must agree");
    }

    // NOTE: there is deliberately no test that writes to the real clipboard.
    //
    // Two reasons, both learned the hard way: it destroys whatever the
    // developer had copied, and the Win32 clipboard tolerates exactly one
    // thread at a time - a test harness running several clipboard tests in
    // parallel corrupts the heap rather than failing an assertion. The
    // behaviour that matters is covered by the pure PNG round-trip tests in
    // `crate::desktop`, and by the engine's own deduplication tests.
}
