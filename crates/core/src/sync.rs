//! Loop prevention and clipboard bookkeeping.
//!
//! Two devices with clipboard sync enabled will happily bounce the same text
//! back and forth forever unless the engine is careful. Two mechanisms stop
//! that, and both live here:
//!
//! 1. **Deduplication** - every payload carries the UUID minted by the device
//!    that first copied it. A device that has already handled an id never
//!    applies or forwards it again. [`DedupCache`].
//! 2. **Echo suppression** - when the engine writes remote content onto the
//!    local clipboard, the platform watcher fires as if the user had copied
//!    it. We remember the digest we just wrote and ignore that one change.
//!    [`EchoSuppressor`].
//!
//! Neither relies on wall clock ordering, so a device with a wrong clock can
//! not create a loop.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use clipmesh_protocol::{
    sha256_of, ClipboardContent, ClipboardItem, ClipboardKind, MAX_IMAGE_BYTES,
};

/// How many recently seen payload ids we remember.
///
/// At a few clipboard events per minute this covers days of history; the
/// memory cost is a UUID string per entry.
pub const DEFAULT_DEDUP_CAPACITY: usize = 512;

/// How long an echo stays suppressed.
///
/// Long enough for a slow platform clipboard to propagate the change, short
/// enough that a user copying the exact same thing again a minute later is
/// still synced.
pub const DEFAULT_ECHO_WINDOW: Duration = Duration::from_secs(10);

/// Remembers payload ids so each one is handled exactly once.
#[derive(Debug)]
pub struct DedupCache {
    capacity: usize,
    order: VecDeque<String>,
    seen: HashSet<String>,
}

impl DedupCache {
    /// Create a cache that remembers at most `capacity` ids.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
        }
    }

    /// Record an id, returning `true` the first time it is seen.
    ///
    /// Returning `false` means "already handled, drop it": that is the single
    /// check the engine performs before applying or forwarding a payload.
    pub fn insert(&mut self, id: &str) -> bool {
        if self.seen.contains(id) {
            return false;
        }

        self.seen.insert(id.to_owned());
        self.order.push_back(id.to_owned());

        while self.order.len() > self.capacity {
            if let Some(evicted) = self.order.pop_front() {
                self.seen.remove(&evicted);
            }
        }
        true
    }

    /// Whether an id has been seen, without recording it.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.seen.contains(id)
    }

    /// Number of remembered ids.
    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Whether nothing has been seen yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.order.clear();
        self.seen.clear();
    }
}

impl Default for DedupCache {
    fn default() -> Self {
        Self::new(DEFAULT_DEDUP_CAPACITY)
    }
}

/// Cheap content identity: kind, length and digest.
///
/// Comparing digests avoids holding a second copy of a 10 MiB screenshot just
/// to notice that we were the one who put it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentSignature {
    /// Text or image.
    pub kind: ClipboardKind,
    /// Payload length in bytes.
    pub len: u64,
    /// sha256 of the payload bytes.
    pub digest: [u8; 32],
}

impl ContentSignature {
    /// Derive the signature of some clipboard content.
    #[must_use]
    pub fn of(content: &ClipboardContent) -> Self {
        match content {
            ClipboardContent::Text(payload) => Self {
                kind: ClipboardKind::Text,
                len: payload.size_bytes(),
                digest: sha256_of(payload.content.as_bytes()),
            },
            ClipboardContent::Image(payload) => Self {
                kind: ClipboardKind::Image,
                len: payload.meta.size,
                digest: sha256_of(&payload.data),
            },
        }
    }
}

/// Ignores the clipboard change that our own write caused.
#[derive(Debug)]
pub struct EchoSuppressor {
    window: Duration,
    pending: Option<(ContentSignature, Instant)>,
}

impl EchoSuppressor {
    /// Create a suppressor with the given tolerance window.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            pending: None,
        }
    }

    /// Remember what we are about to write onto the clipboard.
    pub fn record_write(&mut self, content: &ClipboardContent) {
        self.pending = Some((ContentSignature::of(content), Instant::now()));
    }

    /// Check whether an observed clipboard change is our own echo.
    ///
    /// Returns `true` when the change matches the recorded write and the
    /// window has not expired; the record is consumed either way, so a genuine
    /// user copy of identical content immediately afterwards still syncs if it
    /// arrives after this one.
    pub fn is_echo(&mut self, content: &ClipboardContent) -> bool {
        let Some((signature, recorded_at)) = self.pending.take() else {
            return false;
        };

        if recorded_at.elapsed() > self.window {
            return false;
        }

        // An image payload arriving from the platform is re-encoded to PNG
        // before we sign it, so only compare the bytes we can see.
        ContentSignature::of(content) == signature
    }

    /// Drop any pending suppression, for example when the write failed.
    pub fn clear(&mut self) {
        self.pending = None;
    }
}

impl Default for EchoSuppressor {
    fn default() -> Self {
        Self::new(DEFAULT_ECHO_WINDOW)
    }
}

/// What the user allows to be synchronised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPolicy {
    /// Push local clipboard changes automatically.
    pub auto_sync: bool,
    /// Synchronise text.
    pub sync_text: bool,
    /// Synchronise images.
    pub sync_images: bool,
    /// Refuse images larger than this.
    pub max_image_bytes: u64,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            auto_sync: true,
            sync_text: true,
            sync_images: true,
            max_image_bytes: MAX_IMAGE_BYTES,
        }
    }
}

impl SyncPolicy {
    /// Whether this content may be sent and applied.
    ///
    /// The size check matters on mobile: a device on a metered link should be
    /// able to say "text only" without the engine silently pushing screenshots.
    #[must_use]
    pub fn accepts(&self, content: &ClipboardContent) -> bool {
        match content {
            ClipboardContent::Text(payload) => self.sync_text && payload.is_within_limits(),
            ClipboardContent::Image(payload) => {
                self.sync_images && payload.meta.size <= self.max_image_bytes
            }
        }
    }
}

/// Bounded, newest-first list of clipboard entries.
#[derive(Debug)]
pub struct History {
    capacity: usize,
    items: VecDeque<ClipboardItem>,
}

/// How many history entries the engine keeps.
pub const DEFAULT_HISTORY_CAPACITY: usize = 50;

impl History {
    /// Create a history bounded to `capacity` entries.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            items: VecDeque::new(),
        }
    }

    /// Add an entry, moving an existing entry with the same id to the front.
    ///
    /// Returns `false` when the entry was already the newest one, which lets
    /// the caller skip re-emitting an unchanged snapshot.
    pub fn push(&mut self, item: ClipboardItem) -> bool {
        let id = item.id().to_owned();
        if self.items.front().is_some_and(|front| front.id() == id) {
            return false;
        }
        self.items.retain(|existing| existing.id() != id);
        self.items.push_front(item);

        while self.items.len() > self.capacity {
            self.items.pop_back();
        }
        true
    }

    /// Newest first.
    #[must_use]
    pub fn to_vec(&self) -> Vec<ClipboardItem> {
        self.items.iter().cloned().collect()
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the history is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drop an entry by id.
    pub fn remove(&mut self, id: &str) {
        self.items.retain(|item| item.id() != id);
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_protocol::{DeviceId, ImageMeta, ImagePayload, TextPayload};

    fn text(content: &str) -> ClipboardContent {
        ClipboardContent::Text(TextPayload::new_local(content, DeviceId::new()))
    }

    fn image(bytes: &[u8]) -> ClipboardContent {
        let meta = ImageMeta::new_local(DeviceId::new(), bytes, 8, 8);
        ClipboardContent::Image(ImagePayload::new(meta, bytes.to_vec()).unwrap())
    }

    #[test]
    fn dedup_accepts_an_id_once() {
        let mut cache = DedupCache::default();
        assert!(cache.insert("a"));
        assert!(!cache.insert("a"));
        assert!(cache.insert("b"));
        assert!(cache.contains("a"));
    }

    #[test]
    fn dedup_evicts_the_oldest_ids() {
        let mut cache = DedupCache::new(2);
        assert!(cache.insert("a"));
        assert!(cache.insert("b"));
        assert!(cache.insert("c"));
        assert_eq!(cache.len(), 2);
        assert!(!cache.contains("a"));
        // "a" is forgotten, so a late duplicate is treated as new. That is the
        // intended trade-off: bounded memory over unbounded history.
        assert!(cache.insert("a"));
    }

    #[test]
    fn dedup_survives_a_capacity_of_zero() {
        let mut cache = DedupCache::new(0);
        assert!(cache.insert("a"));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn our_own_write_is_recognised_as_an_echo() {
        let mut suppressor = EchoSuppressor::default();
        let content = text("copied remotely");
        suppressor.record_write(&content);
        assert!(suppressor.is_echo(&content));
    }

    #[test]
    fn an_echo_is_only_suppressed_once() {
        let mut suppressor = EchoSuppressor::default();
        let content = text("copied remotely");
        suppressor.record_write(&content);
        assert!(suppressor.is_echo(&content));
        assert!(!suppressor.is_echo(&content));
    }

    #[test]
    fn different_content_is_not_an_echo() {
        let mut suppressor = EchoSuppressor::default();
        suppressor.record_write(&text("from remote"));
        assert!(!suppressor.is_echo(&text("typed by the user")));
    }

    #[test]
    fn identical_content_copied_by_the_user_is_not_an_echo() {
        let mut suppressor = EchoSuppressor::default();
        assert!(!suppressor.is_echo(&text("nothing was written")));
    }

    #[test]
    fn a_stale_echo_window_expires() {
        let mut suppressor = EchoSuppressor::new(Duration::from_millis(0));
        let content = text("slow platform");
        suppressor.record_write(&content);
        std::thread::sleep(Duration::from_millis(2));
        assert!(!suppressor.is_echo(&content));
    }

    #[test]
    fn images_are_signatured_by_their_bytes() {
        assert_ne!(
            ContentSignature::of(&image(&[1, 2, 3])),
            ContentSignature::of(&image(&[1, 2, 4]))
        );
        assert_eq!(
            ContentSignature::of(&image(&[1, 2, 3])),
            ContentSignature::of(&image(&[1, 2, 3]))
        );
    }

    #[test]
    fn policy_can_disable_images_but_keep_text() {
        let policy = SyncPolicy {
            sync_images: false,
            ..SyncPolicy::default()
        };
        assert!(policy.accepts(&text("hi")));
        assert!(!policy.accepts(&image(&[0u8; 32])));
    }

    #[test]
    fn policy_enforces_the_image_size_ceiling() {
        let policy = SyncPolicy {
            max_image_bytes: 16,
            ..SyncPolicy::default()
        };
        assert!(policy.accepts(&image(&[0u8; 16])));
        assert!(!policy.accepts(&image(&[0u8; 17])));
    }

    #[test]
    fn history_keeps_the_newest_first_and_deduplicates() {
        let mut history = History::new(3);
        let first = ClipboardItem::Text(TextPayload::new_local("one", DeviceId::new()));
        let second = ClipboardItem::Text(TextPayload::new_local("two", DeviceId::new()));

        assert!(history.push(first.clone()));
        assert!(history.push(second.clone()));
        // Pushing the same item again is a no-op and reports "unchanged".
        assert!(history.push(first.clone()));
        assert!(!history.push(first.clone()));

        let items = history.to_vec();
        let ids: Vec<&str> = items.iter().map(ClipboardItem::id).collect();
        assert_eq!(ids, vec![first.id(), second.id()]);
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::new(2);
        for index in 0..5 {
            history.push(ClipboardItem::Text(TextPayload::new_local(
                format!("item {index}"),
                DeviceId::new(),
            )));
        }
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn history_can_drop_a_single_entry() {
        let mut history = History::default();
        let item = ClipboardItem::Text(TextPayload::new_local("secret", DeviceId::new()));
        history.push(item.clone());
        history.remove(item.id());
        assert!(history.is_empty());
    }
}
