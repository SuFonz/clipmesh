//! Application state shared by every Tauri command.

use std::sync::Arc;

use clipmesh_core::{Settings, SharedSyncManager, SyncManager, SyncManagerOptions};
use clipmesh_core::ClipboardProvider as ClipmeshClipboardProvider;
use clipmesh_identity::{DeviceIdentity, IdentityError, IdentityPaths, TrustStore};
use clipmesh_network::{NetworkConfig, NetworkService};
use clipmesh_protocol::DeviceId;
use parking_lot::RwLock;

/// Everything that can go wrong while bringing the app up.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// Keys, certificates or the trust store could not be loaded.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// The engine could not be assembled.
    #[error(transparent)]
    Core(#[from] clipmesh_core::CoreError),

    /// The platform would not give us a directory to keep state in.
    ///
    /// Reachable on Android: [`IdentityPaths::discover`] goes through
    /// `dirs::config_dir()`, which has no fallback there. The Android host asks
    /// Tauri's path plugin instead and reports the failure through this.
    #[error("could not resolve a state directory: {0}")]
    Paths(String),
}

/// The live application.
///
/// Holds the engine plus the concrete handles the command layer needs beyond
/// the provider traits - the network service (for its bound port) and the
/// identity (for the certificate the user can compare).
pub struct AppState {
    /// The sync engine.
    pub engine: SharedSyncManager,
    /// This device's identity.
    pub identity: Arc<DeviceIdentity>,
    /// Discovery and transport.
    pub network: Arc<NetworkService>,
    /// Where the state files live.
    pub paths: IdentityPaths,
}

impl AppState {
    /// Load everything from disk and wire the providers together, keeping state
    /// in the platform config directory.
    ///
    /// # Errors
    /// Returns [`SetupError::Identity`] when the platform reports no config
    /// directory - which is what happens on Android, where the host resolves its
    /// own directory and calls [`AppState::build_in`] instead. See
    /// [`AppState::build_in`] for everything else that can fail.
    pub fn build_with<F>(clipboard: F) -> Result<Self, SetupError>
    where
        F: FnOnce(DeviceId) -> Arc<dyn ClipmeshClipboardProvider>,
    {
        Self::build_in(IdentityPaths::discover()?, clipboard)
    }

    /// Load everything from disk and wire the providers together, keeping state
    /// in `paths`.
    ///
    /// `clipboard` is a factory rather than a value because a clipboard
    /// provider needs this device's id - and the id comes from the identity,
    /// which is only loaded here.
    ///
    /// The order matters: the trust store is loaded once and shared, because
    /// both the TLS policy and the engine have to see the same pairings.
    ///
    /// # Errors
    /// Returns [`SetupError`] when keys, the trust store or the engine cannot
    /// be created. A corrupt trust store is deliberately fatal rather than
    /// silently replaced - see [`TrustStore::load`].
    pub fn build_in<F>(paths: IdentityPaths, clipboard: F) -> Result<Self, SetupError>
    where
        F: FnOnce(DeviceId) -> Arc<dyn ClipmeshClipboardProvider>,
    {
        paths.ensure_dir()?;

        let identity = Arc::new(DeviceIdentity::load_or_create(
            &paths,
            &default_device_name(),
        )?);
        let device_id = identity.device_id();

        tracing::info!(
            device_id = %device_id,
            fingerprint = %identity.fingerprint().short(),
            state_dir = %paths.root().display(),
            "loaded device identity"
        );

        let trust = TrustStore::load(paths.trust_store())?;
        tracing::info!(trusted = trust.len(), "loaded the trust store");
        let trust = Arc::new(RwLock::new(trust));

        // A broken settings file should not stop the app from starting; the
        // defaults are perfectly usable and the file is rewritten on first save.
        let settings = match Settings::load(&paths.settings()) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "ignoring an unreadable settings file");
                Settings::default()
            }
        };

        if !settings.device_name.is_empty() && settings.device_name != identity.name() {
            if let Err(error) = identity.set_name(settings.device_name.clone()) {
                tracing::warn!(%error, "could not apply the stored device name");
            }
        }

        let clipboard = clipboard(device_id);
        let network = NetworkService::new(
            Arc::clone(&identity),
            Arc::clone(&trust),
            NetworkConfig::default(),
        );

        let engine = SyncManager::new(SyncManagerOptions {
            identity: Arc::clone(&identity),
            trust,
            clipboard,
            network: Arc::clone(&network) as Arc<dyn clipmesh_core::NetworkProvider>,
            settings,
            settings_path: Some(paths.settings()),
        });

        Ok(Self {
            engine,
            identity,
            network,
            paths,
        })
    }

    /// Build with the native desktop clipboard.
    ///
    /// # Errors
    /// See [`AppState::build_with`].
    #[cfg(not(target_os = "android"))]
    pub fn build() -> Result<Self, SetupError> {
        Self::build_with(|device_id| clipmesh_clipboard::create_provider(device_id))
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("device_id", &self.identity.device_id())
            .field("state_dir", &self.paths.root())
            .finish_non_exhaustive()
    }
}

/// A sensible default name for this machine.
///
/// Nothing here is sensitive: the name is broadcast over mDNS and shown to the
/// user's other devices, and the real identity is the key pair.
#[must_use]
pub fn default_device_name() -> String {
    for variable in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(variable) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_owned();
            }
        }
    }
    "ClipMesh device".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_name_is_never_empty() {
        assert!(!default_device_name().trim().is_empty());
    }
}
