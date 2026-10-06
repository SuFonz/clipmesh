//! The pixels of history images, kept under the state directory.
//!
//! The engine only ever holds an image while it is moving: the clipboard is
//! overwritten within seconds and history entries are metadata. This is where
//! the bytes survive, so an image in the history can still be previewed,
//! restored to the clipboard or sent to a peer after a restart.
//!
//! One file per entry, named after the entry id, under `images/`. There is no
//! size cap and no eviction beyond the history's own bound: an entry that
//! leaves the list takes its file with it, and an entry that is still listed
//! keeps its picture.

use std::path::{Path, PathBuf};

use clipmesh_core::ImageStore;

/// Directory under the state root holding the image files.
pub const IMAGES_DIR_NAME: &str = "images";

/// The image files of one ClipMesh instance.
///
/// Holds the state root rather than a directory of its own, because the path it
/// reports to the engine has to be relative to that root: `history.json`
/// records it that way, so restoring a backup somewhere else still resolves.
#[derive(Debug, Clone)]
pub struct ImageCache {
    root: PathBuf,
}

impl ImageCache {
    /// Keep the pixels of this instance's history under `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where the files live.
    #[must_use]
    pub fn directory(&self) -> PathBuf {
        self.root.join(IMAGES_DIR_NAME)
    }

    /// The absolute file an entry's pixels are in, if the id can name a file.
    #[must_use]
    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        relative_path(id).map(|relative| self.root.join(relative))
    }

    /// The stored PNG for `id`, if there is one.
    ///
    /// # Errors
    /// Returns the io error for anything other than "there is no such file".
    pub fn read(&self, id: &str) -> std::io::Result<Option<Vec<u8>>> {
        let Some(path) = self.path_for(id) else {
            return Ok(None);
        };

        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, relative: &Path, png: &[u8]) -> Result<(), ImageCacheError> {
        let path = self.root.join(relative);

        // Created lazily: a user who never copies an image never gets the
        // directory.
        std::fs::create_dir_all(self.directory())?;

        // Write and rename, like every other state file: the same entry can be
        // copied again while its file is being read, and half a PNG is not a
        // PNG.
        let temporary = path.with_extension("png.tmp");
        std::fs::write(&temporary, png)?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    }

    fn remove(&self, id: &str) -> std::io::Result<()> {
        let Some(path) = self.path_for(id) else {
            return Ok(());
        };

        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            // Already gone: an entry whose write failed, or one that was
            // cleared twice.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl ImageStore for ImageCache {
    fn put(&self, id: &str, png: &[u8]) -> Option<PathBuf> {
        let Some(relative) = relative_path(id) else {
            tracing::warn!(%id, "refusing to store an image under an unusable id");
            return None;
        };

        match self.write(&relative, png) {
            Ok(()) => Some(relative),
            Err(error) => {
                // A missing copy costs a preview and a restore, nothing more;
                // the entry stays in the history without its pixels.
                tracing::warn!(%error, %id, "could not store an image for the history");
                None
            }
        }
    }

    fn get(&self, id: &str) -> Option<Vec<u8>> {
        match self.read(id) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%error, %id, "could not read a stored image");
                None
            }
        }
    }

    fn forget(&self, id: &str) {
        if let Err(error) = self.remove(id) {
            tracing::warn!(%error, %id, "could not drop a stored image");
        }
    }
}

/// Where the pixels of `id` live, relative to the state root.
///
/// Built with `/` separators rather than the platform's: this string is written
/// to `history.json`, and a state directory restored on another operating
/// system has to keep resolving. Every platform reads `/` as a separator.
///
/// Ids arrive from peers, so this is a path traversal guard as much as a
/// naming rule: `../../authorized_keys` must never become a file name. Every id
/// ClipMesh mints is a UUID, so the accepted alphabet is deliberately narrow.
fn relative_path(id: &str) -> Option<PathBuf> {
    let usable = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    usable.then(|| PathBuf::from(format!("{IMAGES_DIR_NAME}/{id}.png")))
}

/// Why an image could not be stored.
#[derive(Debug, thiserror::Error)]
enum ImageCacheError {
    /// The file could not be written.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(directory: &tempfile::TempDir) -> ImageCache {
        ImageCache::new(directory.path())
    }

    #[test]
    fn a_stored_image_comes_back_byte_for_byte() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);

        let relative = store.put("7f3c", b"not really a png").unwrap();

        assert_eq!(relative, PathBuf::from("images/7f3c.png"));
        assert_eq!(store.get("7f3c").as_deref(), Some(&b"not really a png"[..]));
        assert_eq!(
            std::fs::read(directory.path().join("images/7f3c.png")).unwrap(),
            b"not really a png"
        );
    }

    #[test]
    fn the_directory_is_only_created_when_something_is_stored() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);

        assert!(!store.directory().exists());
        assert!(store.get("nothing").is_none());
        assert!(!store.directory().exists());

        store.put("7f3c", b"png").unwrap();
        assert!(store.directory().is_dir());
    }

    #[test]
    fn storing_leaves_no_temporary_file_behind() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);

        store.put("7f3c", b"png").unwrap();
        store.put("7f3c", b"png again").unwrap();

        assert_eq!(store.get("7f3c").as_deref(), Some(&b"png again"[..]));
        assert!(!store.directory().join("7f3c.png.tmp").exists());
    }

    #[test]
    fn forgetting_drops_the_file_and_tolerates_a_missing_one() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);

        store.put("7f3c", b"png").unwrap();
        store.forget("7f3c");
        assert!(store.get("7f3c").is_none());
        // Clearing twice, or forgetting an entry whose write failed, is fine.
        store.forget("7f3c");
        store.forget("never-stored");
    }

    #[test]
    fn an_id_that_could_escape_the_directory_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);

        for id in ["../../authorized_keys", "a/b", "", "..", "with space"] {
            assert!(store.put(id, b"png").is_none(), "{id:?}");
            assert!(store.path_for(id).is_none(), "{id:?}");
            assert!(store.get(id).is_none(), "{id:?}");
        }

        store.forget("../../authorized_keys");
        assert!(!directory.path().join("authorized_keys").exists());
    }
}
