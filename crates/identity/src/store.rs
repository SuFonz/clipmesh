//! Where ClipMesh keeps its state on disk.
//!
//! Everything sensitive lives under one directory so that "unpair everything
//! and start over" is a single `rm -rf`, and so that backups and permissions
//! can be reasoned about in one place.

use std::path::{Path, PathBuf};

use crate::error::{IdentityError, Result};

/// Directory name used under the platform config directory.
pub const APP_DIR_NAME: &str = "ClipMesh";

/// File names inside the state directory.
pub mod file {
    /// Device id, name and creation time.
    pub const IDENTITY: &str = "identity.json";
    /// PKCS#8 DER Ed25519 private key. Mode 0600 on unix.
    pub const PRIVATE_KEY: &str = "device.key";
    /// PEM self-signed certificate.
    pub const CERTIFICATE: &str = "device.crt";
    /// Pinned peer certificates.
    pub const TRUST_STORE: &str = "trusted_devices.json";
    /// User settings.
    pub const SETTINGS: &str = "settings.json";
    /// Clipboard history metadata.
    pub const HISTORY: &str = "history.json";
}

/// Resolved locations of the ClipMesh state files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPaths {
    root: PathBuf,
}

impl IdentityPaths {
    /// Use the platform config directory:
    /// `%APPDATA%\ClipMesh` on Windows, `~/.config/ClipMesh` on Linux and
    /// `~/Library/Application Support/ClipMesh` on macOS.
    ///
    /// **Android has no such directory.** `dirs::config_dir()` returns `None`
    /// there, because `dirs-sys` gives `home_dir` no fallback on that target
    /// (`#[cfg(target_os = "android")] fn fallback() -> Option<OsString> { None }`),
    /// unlike desktop Linux, which falls back to `getpwuid_r`. The Android host
    /// resolves a writable directory through Tauri's path plugin and calls
    /// [`IdentityPaths::at`]; reaching this function on Android is a bug, not a
    /// supported fallback.
    ///
    /// # Errors
    /// Returns [`IdentityError::TrustStore`] when the platform reports no config
    /// directory - expected on Android, a broken environment anywhere else.
    pub fn discover() -> Result<Self> {
        let base = dirs::config_dir().ok_or_else(|| {
            IdentityError::TrustStore(
                "the operating system did not report a config directory".to_owned(),
            )
        })?;
        Ok(Self {
            root: base.join(APP_DIR_NAME),
        })
    }

    /// Use an explicit directory. Tests and portable installs use this.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The state directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `identity.json`.
    #[must_use]
    pub fn identity_file(&self) -> PathBuf {
        self.root.join(file::IDENTITY)
    }

    /// `device.key`.
    #[must_use]
    pub fn private_key(&self) -> PathBuf {
        self.root.join(file::PRIVATE_KEY)
    }

    /// `device.crt`.
    #[must_use]
    pub fn certificate(&self) -> PathBuf {
        self.root.join(file::CERTIFICATE)
    }

    /// `trusted_devices.json`.
    #[must_use]
    pub fn trust_store(&self) -> PathBuf {
        self.root.join(file::TRUST_STORE)
    }

    /// `settings.json`.
    #[must_use]
    pub fn settings(&self) -> PathBuf {
        self.root.join(file::SETTINGS)
    }

    /// `history.json`.
    #[must_use]
    pub fn history(&self) -> PathBuf {
        self.root.join(file::HISTORY)
    }

    /// Create the state directory if it is missing.
    ///
    /// # Errors
    /// Returns [`IdentityError::Io`] when the directory cannot be created.
    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

/// Read a file, treating "does not exist" as `None`.
///
/// # Errors
/// Propagates every error other than `NotFound`.
pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Write a file that only the current user should be able to read.
///
/// On unix the mode is set to 0600 *at creation time* via `OpenOptions`, so the
/// private key is never briefly world readable. On Windows the file inherits
/// the ACL of the per-user config directory, which is already user-only.
///
/// # Errors
/// Returns [`IdentityError::Io`] on failure.
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)?;
    }

    Ok(())
}

/// Serialize `value` as pretty JSON and write it atomically.
///
/// # Errors
/// Returns [`IdentityError::Io`] / [`IdentityError::Serde`] on failure.
pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(value)?;
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, json)?;
    // `rename` replaces the destination on both unix and Windows, so a reader
    // never observes a partially written file.
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        value: u32,
    }

    #[test]
    fn paths_hang_off_one_root() {
        let paths = IdentityPaths::at("/tmp/clipmesh-test");
        assert_eq!(paths.root(), Path::new("/tmp/clipmesh-test"));
        assert!(paths.private_key().starts_with(paths.root()));
        assert!(paths.trust_store().starts_with(paths.root()));
        assert_eq!(
            paths.private_key().file_name().unwrap(),
            std::ffi::OsStr::new("device.key")
        );
    }

    #[test]
    fn discover_is_available_on_this_platform() {
        let paths = IdentityPaths::discover().unwrap();
        assert!(paths.root().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn reading_a_missing_file_yields_none() {
        let directory = tempfile::tempdir().unwrap();
        assert!(read_optional(&directory.path().join("nope")).unwrap().is_none());
    }

    #[test]
    fn json_is_written_atomically_and_leaves_no_temp_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");

        write_json_atomic(&path, &Sample { value: 7 }).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("tmp").exists());

        let restored: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(restored["value"], 7);
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_not_world_readable() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("device.key");
        write_private_file(&path, b"secret").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
