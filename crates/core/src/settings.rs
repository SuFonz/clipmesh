//! User settings, persisted as `settings.json`.
//!
//! One struct serves both platforms. The desktop-only and Android-only fields
//! (`start_minimized`, `launch_at_login`, `android_foreground_service`) are
//! simply ignored by the platform they do not apply to, which keeps a single
//! settings file and a single IPC contract instead of two divergent ones.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use clipmesh_protocol::MAX_IMAGE_BYTES;

use crate::error::Result;
use crate::sync::SyncPolicy;

/// Smallest image we will bother synchronising.
pub const MIN_IMAGE_BYTES: u64 = 64 * 1024;

/// Largest image limit a user may configure.
pub const MAX_IMAGE_BYTES_LIMIT: u64 = 512 * 1024 * 1024;

/// Largest history a user may configure.
pub const MAX_HISTORY_CAPACITY: usize = 500;

/// Interface language.
///
/// The wire/storage tags are the stable ones the UI speaks (`system`, `zh-CN`,
/// `en`), not the Rust variant names, so renaming a variant cannot silently
/// migrate anyone's `settings.json`.
///
/// The *type* is the validation: there is no fourth state to clamp away.
/// [`Language::from_tag`] maps every unrecognised tag (`"fr"`, `"ja"`, or a
/// value from a newer build) to [`Language::System`], so a hand-edited file
/// degrades instead of failing the whole `Settings::load` — the same "never let
/// an out-of-range value in, but never brick the file either" rule the numeric
/// fields follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Follow the operating system / browser locale. Resolved in the frontend.
    #[default]
    System,
    /// Simplified Chinese.
    Chinese,
    /// English.
    English,
}

impl Language {
    /// The stable tag stored in `settings.json` and sent over IPC.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Chinese => "zh-CN",
            Self::English => "en",
        }
    }

    /// Narrow a tag to a known language; anything else becomes [`Self::System`].
    #[must_use]
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "zh-CN" => Self::Chinese,
            "en" => Self::English,
            _ => Self::System,
        }
    }
}

impl Serialize for Language {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.tag())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_tag(&String::deserialize(deserializer)?))
    }
}

/// Everything the user can change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Device name broadcast over mDNS and shown to peers.
    pub device_name: String,
    /// Push local clipboard changes to peers automatically.
    pub auto_sync: bool,
    /// Synchronise text.
    pub sync_text: bool,
    /// Synchronise images.
    pub sync_images: bool,
    /// Refuse images larger than this.
    pub max_image_bytes: u64,
    /// Desktop: start hidden in the tray.
    pub start_minimized: bool,
    /// Desktop: launch when the user logs in.
    pub launch_at_login: bool,
    /// Android: keep the foreground service running.
    pub android_foreground_service: bool,
    /// How many clipboard entries to keep.
    pub history_capacity: usize,
    /// Interface language: `system` follows the OS, otherwise an explicit tag.
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_name: String::new(),
            auto_sync: true,
            sync_text: true,
            sync_images: true,
            max_image_bytes: MAX_IMAGE_BYTES,
            start_minimized: false,
            launch_at_login: false,
            android_foreground_service: true,
            history_capacity: crate::sync::DEFAULT_HISTORY_CAPACITY,
            language: Language::System,
        }
    }
}

impl Settings {
    /// Load from disk, falling back to defaults when the file does not exist.
    ///
    /// A *corrupt* file is an error rather than a silent reset: silently
    /// discarding the user's configuration is worse than telling them.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the file exists but cannot be parsed.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let mut settings: Self = serde_json::from_str(&contents)?;
        settings.clamp();
        Ok(settings)
    }

    /// Persist atomically.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, json)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Apply a partial update from the UI.
    ///
    /// Returns whether anything actually changed, so the caller can skip a
    /// pointless disk write (and a pointless mDNS re-announcement).
    pub fn apply(&mut self, patch: &SettingsPatch) -> bool {
        let before = self.clone();

        if let Some(name) = &patch.device_name {
            self.device_name = name.trim().to_owned();
        }
        if let Some(value) = patch.auto_sync {
            self.auto_sync = value;
        }
        if let Some(value) = patch.sync_text {
            self.sync_text = value;
        }
        if let Some(value) = patch.sync_images {
            self.sync_images = value;
        }
        if let Some(value) = patch.max_image_bytes {
            self.max_image_bytes = value;
        }
        if let Some(value) = patch.start_minimized {
            self.start_minimized = value;
        }
        if let Some(value) = patch.launch_at_login {
            self.launch_at_login = value;
        }
        if let Some(value) = patch.android_foreground_service {
            self.android_foreground_service = value;
        }
        if let Some(value) = patch.history_capacity {
            self.history_capacity = value;
        }
        if let Some(value) = patch.language {
            self.language = value;
        }

        self.clamp();
        *self != before
    }

    /// Force every value back into a sane range.
    ///
    /// Called after loading and after every patch, because this file is
    /// user-editable and a `maxImageBytes` of zero would silently disable
    /// images with no explanation in the UI.
    ///
    /// `language` is deliberately absent here: [`Language`] has no
    /// out-of-range value to fix, because unknown tags already collapse to
    /// [`Language::System`] while deserialising.
    pub fn clamp(&mut self) {
        self.max_image_bytes = self
            .max_image_bytes
            .clamp(MIN_IMAGE_BYTES, MAX_IMAGE_BYTES_LIMIT);
        self.history_capacity = self
            .history_capacity
            .clamp(1, MAX_HISTORY_CAPACITY);
        if self.device_name.len() > 64 {
            self.device_name.truncate(64);
        }
        self.device_name = self.device_name.trim().to_owned();
    }

    /// The engine's view of what may be synchronised.
    #[must_use]
    pub fn sync_policy(&self) -> SyncPolicy {
        SyncPolicy {
            auto_sync: self.auto_sync,
            sync_text: self.sync_text,
            sync_images: self.sync_images,
            max_image_bytes: self.max_image_bytes,
        }
    }
}

/// A partial update, mirroring `Partial<SettingsView>` on the TypeScript side.
///
/// Every field is optional; absent means "leave it alone".
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    /// New device name.
    pub device_name: Option<String>,
    /// Toggle automatic pushing.
    pub auto_sync: Option<bool>,
    /// Toggle text synchronisation.
    pub sync_text: Option<bool>,
    /// Toggle image synchronisation.
    pub sync_images: Option<bool>,
    /// New image size ceiling.
    pub max_image_bytes: Option<u64>,
    /// Desktop: start hidden.
    pub start_minimized: Option<bool>,
    /// Desktop: launch at login.
    pub launch_at_login: Option<bool>,
    /// Android: keep the foreground service.
    pub android_foreground_service: Option<bool>,
    /// New history capacity.
    pub history_capacity: Option<usize>,
    /// New interface language.
    pub language: Option<Language>,
}

impl SettingsPatch {
    /// Whether this patch would change nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.device_name.is_none()
            && self.auto_sync.is_none()
            && self.sync_text.is_none()
            && self.sync_images.is_none()
            && self.max_image_bytes.is_none()
            && self.start_minimized.is_none()
            && self.launch_at_login.is_none()
            && self.android_foreground_service.is_none()
            && self.history_capacity.is_none()
            && self.language.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_permissive_and_bounded() {
        let settings = Settings::default();
        assert!(settings.auto_sync);
        assert!(settings.sync_text && settings.sync_images);
        assert!(settings.sync_policy().accepts(&clipmesh_protocol::ClipboardContent::Text(
            clipmesh_protocol::TextPayload::new_local("hi", clipmesh_protocol::DeviceId::new())
        )));
    }

    #[test]
    fn a_missing_file_yields_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::load(&directory.path().join("settings.json")).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn settings_round_trip_through_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");

        let mut settings = Settings::default();
        settings.device_name = "Studio Mac".into();
        settings.sync_images = false;
        settings.max_image_bytes = 1024 * 1024;
        settings.save(&path).unwrap();

        let restored = Settings::load(&path).unwrap();
        assert_eq!(restored.device_name, "Studio Mac");
        assert!(!restored.sync_images);
        assert_eq!(restored.max_image_bytes, 1024 * 1024);
    }

    #[test]
    fn a_corrupt_file_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(Settings::load(&path).is_err());
    }

    #[test]
    fn patches_touch_only_the_fields_they_mention() {
        let mut settings = Settings::default();
        let patch = SettingsPatch {
            sync_images: Some(false),
            ..SettingsPatch::default()
        };

        assert!(settings.apply(&patch));
        assert!(!settings.sync_images);
        assert!(settings.sync_text, "unspecified fields must be preserved");
        assert!(settings.auto_sync);
    }

    #[test]
    fn a_no_op_patch_reports_no_change() {
        let mut settings = Settings::default();
        assert!(!settings.apply(&SettingsPatch::default()));
        assert!(!settings.apply(&SettingsPatch {
            auto_sync: Some(true),
            ..SettingsPatch::default()
        }));
    }

    #[test]
    fn out_of_range_values_are_clamped_not_rejected() {
        let mut settings = Settings::default();
        settings.apply(&SettingsPatch {
            max_image_bytes: Some(1),
            history_capacity: Some(10_000),
            ..SettingsPatch::default()
        });

        assert_eq!(settings.max_image_bytes, MIN_IMAGE_BYTES);
        assert_eq!(settings.history_capacity, MAX_HISTORY_CAPACITY);
    }

    #[test]
    fn device_names_are_trimmed_and_capped() {
        let mut settings = Settings::default();
        settings.apply(&SettingsPatch {
            device_name: Some(format!("  {}  ", "x".repeat(200))),
            ..SettingsPatch::default()
        });
        assert_eq!(settings.device_name.len(), 64);
        assert!(!settings.device_name.starts_with(' '));
    }

    #[test]
    fn an_empty_patch_is_detected() {
        assert!(SettingsPatch::default().is_empty());
        assert!(!SettingsPatch {
            auto_sync: Some(false),
            ..SettingsPatch::default()
        }
        .is_empty());
    }

    #[test]
    fn a_clamped_file_loads_cleanly() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"maxImageBytes": 0, "historyCapacity": 999999}"#).unwrap();

        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.max_image_bytes, MIN_IMAGE_BYTES);
        assert_eq!(settings.history_capacity, MAX_HISTORY_CAPACITY);
        // Fields absent from the file keep their defaults.
        assert!(settings.auto_sync);
    }

    #[test]
    fn language_defaults_to_following_the_system() {
        assert_eq!(Settings::default().language, Language::System);
        // Absent from the file => same default, so an old settings.json keeps working.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"autoSync": false}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, Language::System);
    }

    #[test]
    fn language_round_trips_through_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");

        let mut settings = Settings::default();
        settings.language = Language::English;
        settings.save(&path).unwrap();

        // The file carries the stable tag, not the Rust variant name.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains(r#""language": "en""#), "unexpected file: {raw}");
        assert_eq!(Settings::load(&path).unwrap().language, Language::English);

        settings.language = Language::Chinese;
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, Language::Chinese);
    }

    #[test]
    fn an_unknown_language_never_gets_in() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"language": "fr"}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, Language::System);

        // A patch is narrowed by the same rule: known tags pass through,
        // anything else degrades to `system` instead of reaching the file.
        let known: SettingsPatch = serde_json::from_str(r#"{"language":"zh-CN"}"#).unwrap();
        assert_eq!(known.language, Some(Language::Chinese));
        let unknown: SettingsPatch = serde_json::from_str(r#"{"language":"klingon"}"#).unwrap();
        assert_eq!(unknown.language, Some(Language::System));
    }

    #[test]
    fn changing_the_language_is_reported_as_a_change() {
        let mut settings = Settings::default();
        settings.language = Language::Chinese;

        assert!(!settings.apply(&SettingsPatch {
            language: Some(Language::Chinese),
            ..SettingsPatch::default()
        }));
        assert!(settings.apply(&SettingsPatch {
            language: Some(Language::English),
            ..SettingsPatch::default()
        }));
        assert_eq!(settings.language, Language::English);
        assert!(settings.sync_images, "other fields must be untouched");
        assert!(!SettingsPatch {
            language: Some(Language::System),
            ..SettingsPatch::default()
        }
        .is_empty());
    }
}
