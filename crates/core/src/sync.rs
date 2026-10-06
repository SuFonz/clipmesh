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
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use clipmesh_protocol::{
    sha256_of, ClipboardContent, ClipboardItem, ClipboardKind, MAX_IMAGE_BYTES,
};

use crate::error::Result;

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

/// One history entry, as it is held in memory and written to `history.json`.
///
/// The path deliberately does not live in [`ClipboardItem`]: that type is what
/// travels to peers and to the frontend, and a local filesystem path has no
/// business in either. This wrapper exists only so `history.json` can remember
/// where a stored image sits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEntry {
    item: ClipboardItem,
    /// Where the pixels are, relative to the state directory, exactly as the
    /// store reported it. Absent for text, and for an image whose store write
    /// failed - hence the default when reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image_path: Option<PathBuf>,
}

/// Bounded, newest-first list of clipboard entries.
#[derive(Debug)]
pub struct History {
    capacity: usize,
    entries: VecDeque<StoredEntry>,
}

/// How many history entries the engine keeps.
pub const DEFAULT_HISTORY_CAPACITY: usize = 50;

/// On-disk format version of `history.json`.
///
/// A file written by a newer ClipMesh is ignored rather than misread, the same
/// way the trust store refuses one.
const HISTORY_VERSION: u32 = 1;

/// What [`History::push`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    /// Whether the visible list changed.
    ///
    /// `false` means the entry was already the newest one, which lets the
    /// caller skip re-emitting an unchanged snapshot.
    pub changed: bool,
    /// The entry the capacity bound pushed out, if any.
    ///
    /// Handed back because the caller holds something on that entry's behalf -
    /// its pixels - and that has to go with it.
    pub evicted: Option<ClipboardItem>,
}

/// The on-disk shape of `history.json`, when writing.
#[derive(Debug, Serialize)]
struct HistoryFileRef<'a> {
    version: u32,
    items: Vec<&'a StoredEntry>,
}

/// The on-disk shape of `history.json`, when reading.
///
/// There is no older shape to support: the file is not a contract with anybody
/// and a history nobody can read is worth less than a fresh one.
#[derive(Debug, Deserialize)]
struct HistoryFile {
    version: u32,
    items: Vec<StoredEntry>,
}

/// Whether a recorded image path may be resolved inside the state directory.
///
/// The file is ours, but it is also plain JSON that a user can edit, and a path
/// that climbs out of the state directory has no business being resolved at all.
fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

impl History {
    /// Create a history bounded to `capacity` entries.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    /// Load a persisted list, falling back to an empty one.
    ///
    /// A missing file is normal - nothing has been copied yet. So is a file
    /// that cannot be read as the current format, which covers a truncated
    /// write, a hand-edited file and one from a newer ClipMesh: it is logged,
    /// the history starts empty, and the next save replaces it. There is no
    /// migration and no compatibility shim, deliberately unlike
    /// [`clipmesh_identity::TrustStore::load`] - a damaged trust store would
    /// silently un-pair devices, while a damaged history costs a list and
    /// nothing more, and refusing to start the engine over it would be worse.
    #[must_use]
    pub fn load(path: &Path, capacity: usize) -> Self {
        let mut history = Self::new(capacity);

        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return history,
            Err(error) => {
                tracing::warn!(
                    %error,
                    path = %path.display(),
                    "could not read the clipboard history; starting empty"
                );
                return history;
            }
        };

        let file: HistoryFile = match serde_json::from_str(&contents) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(
                    %error,
                    path = %path.display(),
                    "the clipboard history is not in a readable format; starting empty"
                );
                return history;
            }
        };

        if file.version > HISTORY_VERSION {
            tracing::warn!(
                path = %path.display(),
                version = file.version,
                supported = HISTORY_VERSION,
                "the clipboard history was written by a newer ClipMesh; starting empty"
            );
            return history;
        }

        // Image paths are relative to the directory the history lives in, so
        // that moving or restoring the state directory keeps them valid.
        let root = path.parent();
        let mut lost_images = 0usize;

        for entry in file.items {
            // The file is written newest first, so appending preserves the
            // order. Going through `push` instead would report evictions to a
            // caller that has nowhere to put them, and could exceed the
            // capacity the file was written under.
            if history.entries.len() >= history.capacity {
                break;
            }
            if history
                .entries
                .iter()
                .any(|existing| existing.item.id() == entry.item.id())
            {
                continue;
            }

            let image_path = match &entry.image_path {
                None => None,
                Some(relative) => {
                    if !is_safe_relative(relative) {
                        tracing::warn!(
                            path = %relative.display(),
                            "dropping a history entry with an image path outside the state directory"
                        );
                        continue;
                    }
                    // A runtime condition rather than a format problem: the
                    // user deleted the file, or a restore did not bring it.
                    if !root.is_some_and(|root| root.join(relative).is_file()) {
                        lost_images += 1;
                        continue;
                    }
                    Some(relative.clone())
                }
            };

            history.entries.push_back(StoredEntry {
                item: entry.item,
                image_path,
            });
        }

        if lost_images > 0 {
            tracing::warn!(
                entries = lost_images,
                "dropped history entries whose image is no longer stored"
            );
        }

        history
    }

    /// Persist the list atomically.
    ///
    /// Metadata only: text entries carry their text, image entries carry their
    /// [`clipmesh_protocol::ImageMeta`] and the path of their pixels, which
    /// never travels anywhere else.
    ///
    /// # Errors
    /// Returns [`crate::CoreError`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = HistoryFileRef {
            version: HISTORY_VERSION,
            items: self.entries.iter().collect(),
        };
        let json = serde_json::to_string_pretty(&file)?;

        // Write and rename, like the settings and the trust store: a crash
        // half way through must never leave the user with a truncated history.
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, json)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Add an entry, moving an existing entry with the same id to the front.
    ///
    /// `image_path` is where the entry's pixels were stored, as the store
    /// reported it. It is `None` for text, and for an image whose store write
    /// failed - such an entry stays in the list as metadata only.
    pub fn push(&mut self, item: ClipboardItem, image_path: Option<PathBuf>) -> PushOutcome {
        let id = item.id().to_owned();
        if self
            .entries
            .front()
            .is_some_and(|front| front.item.id() == id)
        {
            return PushOutcome {
                changed: false,
                evicted: None,
            };
        }
        self.entries.retain(|existing| existing.item.id() != id);
        self.entries.push_front(StoredEntry { item, image_path });

        let mut evicted = None;
        while self.entries.len() > self.capacity {
            evicted = self.entries.pop_back().map(|entry| entry.item);
        }
        PushOutcome {
            changed: true,
            evicted,
        }
    }

    /// Newest first.
    #[must_use]
    pub fn to_vec(&self) -> Vec<ClipboardItem> {
        self.entries
            .iter()
            .map(|entry| entry.item.clone())
            .collect()
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop an entry by id, handing it back so the caller can release whatever
    /// it stored for that entry.
    pub fn remove(&mut self, id: &str) -> Option<ClipboardItem> {
        let position = self
            .entries
            .iter()
            .position(|entry| entry.item.id() == id)?;
        self.entries.remove(position).map(|entry| entry.item)
    }

    /// Drop everything, handing back what was dropped.
    pub fn clear(&mut self) -> Vec<ClipboardItem> {
        self.entries.drain(..).map(|entry| entry.item).collect()
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

    fn entry_ids(history: &History) -> Vec<String> {
        history
            .to_vec()
            .iter()
            .map(|item| item.id().to_owned())
            .collect()
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

        assert!(history.push(first.clone(), None).changed);
        assert!(history.push(second.clone(), None).changed);
        // Pushing the same item again is a no-op and reports "unchanged".
        assert!(history.push(first.clone(), None).changed);
        assert!(!history.push(first.clone(), None).changed);

        let items = history.to_vec();
        let ids: Vec<&str> = items.iter().map(ClipboardItem::id).collect();
        assert_eq!(ids, vec![first.id(), second.id()]);
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::new(2);
        for index in 0..5 {
            history.push(
                ClipboardItem::Text(TextPayload::new_local(
                    format!("item {index}"),
                    DeviceId::new(),
                )),
                None,
            );
        }
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn push_hands_back_the_entry_it_evicted() {
        let mut history = History::new(2);
        let oldest = ClipboardItem::Text(TextPayload::new_local("oldest", DeviceId::new()));
        let newest = ClipboardItem::Text(TextPayload::new_local("newest", DeviceId::new()));
        let evicting = ClipboardItem::Text(TextPayload::new_local("evicting", DeviceId::new()));

        history.push(oldest.clone(), None);
        let outcome = history.push(newest.clone(), None);
        assert!(outcome.changed);
        assert_eq!(outcome.evicted, None, "nothing is dropped before capacity");

        let outcome = history.push(evicting.clone(), None);
        assert_eq!(
            outcome.evicted.as_ref().map(ClipboardItem::id),
            Some(oldest.id()),
            "the entry at the back is the one that fell off"
        );
        let ids: Vec<String> = history.to_vec().iter().map(|i| i.id().to_owned()).collect();
        assert_eq!(ids, vec![evicting.id(), newest.id()]);
    }

    #[test]
    fn history_can_drop_a_single_entry() {
        let mut history = History::default();
        let item = ClipboardItem::Text(TextPayload::new_local("secret", DeviceId::new()));
        history.push(item.clone(), None);

        assert_eq!(history.remove(item.id()).as_ref(), Some(&item));
        assert!(history.is_empty());
        // Removing something that is not there is not an error.
        assert_eq!(history.remove(item.id()), None);
    }

    #[test]
    fn clearing_hands_back_every_entry() {
        let mut history = History::new(3);
        let first = ClipboardItem::Text(TextPayload::new_local("one", DeviceId::new()));
        let second = ClipboardItem::Text(TextPayload::new_local("two", DeviceId::new()));
        history.push(first.clone(), None);
        history.push(second.clone(), None);

        let dropped = history.clear();
        assert_eq!(dropped.len(), 2);
        assert!(history.is_empty());
        assert!(history.clear().is_empty());
    }

    #[test]
    fn a_missing_history_file_starts_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let mut history = History::load(&path, 50);
        assert!(history.is_empty());
        assert!(!path.exists(), "loading must not create the file");

        // ...and the list is writable from there.
        history.push(
            ClipboardItem::Text(TextPayload::new_local("first", DeviceId::new())),
            None,
        );
        history.save(&path).unwrap();
        assert_eq!(History::load(&path, 50).len(), 1);
    }

    #[test]
    fn an_image_remembers_where_its_pixels_are() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        // The store reports paths relative to the state directory, with `/`
        // separators so the same file means the same thing on every platform.
        // It has to be there when the history is read back.
        let relative = PathBuf::from("images/cached.png");
        std::fs::create_dir_all(directory.path().join("images")).unwrap();
        std::fs::write(directory.path().join(&relative), b"png").unwrap();

        let meta = ImageMeta::new_local(DeviceId::new(), b"png", 4, 4);
        let id = meta.id.clone();
        let mut history = History::new(50);
        history.push(ClipboardItem::Image(meta), Some(relative.clone()));
        history.save(&path).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["items"][0]["imagePath"], "images/cached.png");
        assert!(
            json["items"][0]["item"].get("imagePath").is_none(),
            "the path must not leak into the entry itself"
        );

        let restored = History::load(&path, 50);
        let ids: Vec<String> = restored
            .to_vec()
            .iter()
            .map(|item| item.id().to_owned())
            .collect();
        assert_eq!(ids, vec![id]);
    }

    #[test]
    fn an_entry_whose_image_is_gone_is_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let kept = ClipboardItem::Text(TextPayload::new_local("kept", DeviceId::new()));
        let vanished = ClipboardItem::Image(ImageMeta::new_local(DeviceId::new(), b"png", 4, 4));
        let mut history = History::new(50);
        history.push(vanished.clone(), Some(PathBuf::from("images/vanished.png")));
        history.push(kept.clone(), None);
        history.save(&path).unwrap();

        // No `images/vanished.png` on disk: the row would be one the user can
        // neither preview nor copy, so it does not come back.
        let restored = History::load(&path, 50);
        assert_eq!(restored.to_vec(), vec![kept]);
    }

    #[test]
    fn an_image_path_that_climbs_out_of_the_state_directory_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        std::fs::write(directory.path().join("outside.png"), b"png").unwrap();

        let escapee = ClipboardItem::Image(ImageMeta::new_local(DeviceId::new(), b"png", 4, 4));
        let mut history = History::new(50);
        history.push(escapee, Some(PathBuf::from("../outside.png")));
        history.save(&path).unwrap();

        assert!(History::load(&path, 50).is_empty());
    }

    #[test]
    fn an_entry_without_a_recorded_path_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        // What an image looks like when its store write failed: still worth
        // listing, just without pixels.
        let item = ClipboardItem::Image(ImageMeta::new_local(DeviceId::new(), b"png", 4, 4));
        let json = serde_json::json!({ "version": 1, "items": [{ "item": item }] });
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let id = item.id().to_owned();
        let restored = History::load(&path, 50);
        assert_eq!(entry_ids(&restored), vec![id.clone()]);
        // ...and writing it back keeps it readable.
        restored.save(&path).unwrap();
        assert_eq!(entry_ids(&History::load(&path, 50)), vec![id]);
    }

    #[test]
    fn history_round_trips_through_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let mut history = History::new(50);
        let newest = ClipboardItem::Text(TextPayload::new_local("newest", DeviceId::new()));
        let middle = ClipboardItem::Image(ImageMeta::new_local(DeviceId::new(), &[1, 2, 3], 4, 4));
        let oldest = ClipboardItem::Text(TextPayload::new_local("oldest", DeviceId::new()));
        history.push(oldest.clone(), None);
        history.push(middle.clone(), None);
        history.push(newest.clone(), None);
        history.save(&path).unwrap();

        let restored = History::load(&path, 50);
        // Compared by id: an image's digest is deliberately not written to
        // disk, so the entry comes back without it (see
        // `ImagePayload::from_stored`).
        assert_eq!(
            entry_ids(&restored),
            vec![
                newest.id().to_owned(),
                middle.id().to_owned(),
                oldest.id().to_owned()
            ]
        );
        assert_eq!(
            restored.to_vec()[0],
            newest,
            "text survives a round trip unchanged"
        );
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let mut history = History::new(5);
        history.push(
            ClipboardItem::Text(TextPayload::new_local("atomic", DeviceId::new())),
            None,
        );
        history.save(&path).unwrap();

        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_corrupt_history_file_is_ignored_not_fatal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        // Unlike the trust store, this must not be an error: the engine has to
        // start even when the file is damaged.
        let history = History::load(&path, 50);
        assert!(history.is_empty());
    }

    #[test]
    fn a_history_file_from_a_newer_clipmesh_is_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        std::fs::write(
            &path,
            r#"{"version": 99, "items": [{"item": {"kind": "text", "id": "x", "sourceDevice": "00000000-0000-4000-8000-000000000000", "timestamp": 1, "content": "from the future"}}]}"#,
        )
        .unwrap();

        assert!(History::load(&path, 50).is_empty());
    }

    #[test]
    fn a_history_file_that_does_not_match_the_format_starts_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        // A bare entry is what an earlier, unreleased shape looked like. It is
        // not migrated: the history starts over instead.
        let item = ClipboardItem::Text(TextPayload::new_local("old shape", DeviceId::new()));
        let json = serde_json::json!({ "version": 1, "items": [item] });
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let history = History::load(&path, 50);
        assert!(history.is_empty());
    }

    #[test]
    fn loading_truncates_to_the_capacity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let mut history = History::new(5);
        for index in 0..5 {
            history.push(
                ClipboardItem::Text(TextPayload::new_local(
                    format!("item {index}"),
                    DeviceId::new(),
                )),
                None,
            );
        }
        history.save(&path).unwrap();

        let expected: Vec<String> = history
            .to_vec()
            .iter()
            .map(|item| item.id().to_owned())
            .collect();
        let restored = History::load(&path, 3);
        let ids: Vec<String> = restored
            .to_vec()
            .iter()
            .map(|item| item.id().to_owned())
            .collect();

        // The newest entries survive, because that is the order the file is in.
        assert_eq!(ids, expected[..3].to_vec());
    }

    #[test]
    fn loading_drops_duplicate_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        // A hand-edited or half-merged file can repeat an entry; loading must
        // not list it twice.
        let item = ClipboardItem::Text(TextPayload::new_local("twice", DeviceId::new()));
        let entry = serde_json::json!({ "item": item });
        let json = serde_json::json!({ "version": 1, "items": [entry, entry] });
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let restored = History::load(&path, 50);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.to_vec(), vec![item]);
    }
}
