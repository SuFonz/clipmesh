//! # clipmesh-core
//!
//! The platform independent heart of ClipMesh: the sync engine, the three
//! provider traits it is built on, loop prevention, device bookkeeping and the
//! event stream the UI renders.
//!
//! ## What lives here
//!
//! * [`provider`] - `ClipboardProvider`, `NetworkProvider`, `IdentityProvider`.
//!   Everything platform specific is behind one of these, which is why this
//!   crate compiles and is tested on any machine.
//! * [`manager::SyncManager`] - the orchestrator. Discovery, pairing, the
//!   handshake, sending, receiving and history all meet here.
//! * [`sync`] - deduplication, echo suppression, the sync policy and history.
//! * [`device`] - which devices are visible right now.
//! * [`event`] - snapshot events and the view types the frontend consumes.
//! * [`settings`] - user configuration, persisted as JSON.
//!
//! ## Dependency direction
//!
//! ```text
//! protocol <- identity <- security <- core <- { clipboard, network } <- apps
//!                                    ^
//!                          (traits only; the concrete
//!                           providers are injected)
//! ```
//!
//! `core` never names `arboard`, `mdns-sd` or `rustls`. The platform layer
//! builds the concrete providers and hands them over as trait objects, which is
//! what keeps the engine testable with fakes and portable to Android.
//!
//! ```no_run
//! use std::sync::Arc;
//! use clipmesh_core::{SharedTrustStore, SyncManager, SyncManagerOptions, Settings};
//!
//! # fn build(
//! #     identity: Arc<clipmesh_identity::DeviceIdentity>,
//! #     trust: SharedTrustStore,
//! #     clipboard: Arc<dyn clipmesh_core::ClipboardProvider>,
//! #     network: Arc<dyn clipmesh_core::NetworkProvider>,
//! # ) {
//! let engine = SyncManager::new(SyncManagerOptions {
//!     identity,
//!     trust,
//!     clipboard,
//!     network,
//!     settings: Settings::default(),
//!     settings_path: None,
//! });
//! let mut events = engine.subscribe();
//! engine.spawn();
//! # let _ = (&engine, &mut events);
//! # }
//! ```

#![warn(missing_docs)]

pub mod device;
pub mod error;
pub mod event;
pub mod manager;
pub mod provider;
pub mod settings;
pub mod sync;

use std::sync::Arc;

use clipmesh_identity::{verify_signature, DeviceIdentity};
use clipmesh_protocol::DeviceId;
use parking_lot::RwLock;

pub use clipmesh_identity::TrustStore;
pub use error::{CoreError, Result};
pub use event::{
    CoreEvent, PairingDirection, PairingPrompt, PeerView, StatusView, TrustedDeviceView,
};
pub use manager::{SendOutcome, SharedSyncManager, SyncManager, SyncManagerOptions};
pub use provider::{
    ClipboardEvent, ClipboardProvider, IdentityProvider, NetworkEvent, NetworkProvider,
    PeerAddress, PeerSession,
};
pub use settings::{Settings, SettingsPatch};
pub use sync::{
    ContentSignature, DedupCache, EchoSuppressor, History, SyncPolicy, DEFAULT_DEDUP_CAPACITY,
    DEFAULT_ECHO_WINDOW, DEFAULT_HISTORY_CAPACITY,
};

/// The trust store, shared between the engine and the network layer.
///
/// It is deliberately *one* handle rather than two copies: the TLS policy and
/// the engine must agree on who is trusted, and unpairing a device has to take
/// effect on the very next handshake.
pub type SharedTrustStore = Arc<RwLock<TrustStore>>;

/// `DeviceIdentity` already does everything [`IdentityProvider`] asks for.
///
/// The impl lives here rather than in `clipmesh-identity` so that the identity
/// crate stays free of any dependency on the engine - the trait is defined
/// here, so implementing it here is the only way to keep the graph acyclic.
impl IdentityProvider for DeviceIdentity {
    fn device_id(&self) -> DeviceId {
        DeviceIdentity::device_id(self)
    }

    fn public_key(&self) -> [u8; 32] {
        DeviceIdentity::public_key(self)
    }

    fn fingerprint(&self) -> &clipmesh_identity::Fingerprint {
        DeviceIdentity::fingerprint(self)
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        Ok(DeviceIdentity::sign(self, message).to_vec())
    }

    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        // Note this is a *static* capability: verifying a peer's signature must
        // not depend on our own key, so it delegates to the free function.
        verify_signature(public_key, message, signature).map_err(CoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_identity_satisfies_the_identity_provider_trait() {
        let identity = DeviceIdentity::generate("Trait Test").unwrap();
        let provider: &dyn IdentityProvider = &identity;

        assert_eq!(provider.device_id(), identity.device_id());
        assert_eq!(provider.public_key(), identity.public_key());

        let signature = provider.sign(b"payload").unwrap();
        assert!(provider
            .verify(&provider.public_key(), b"payload", &signature)
            .is_ok());
        assert!(provider
            .verify(&provider.public_key(), b"tampered", &signature)
            .is_err());
    }

    #[test]
    fn verification_is_not_bound_to_our_own_key() {
        let ours = DeviceIdentity::generate("Ours").unwrap();
        let theirs = DeviceIdentity::generate("Theirs").unwrap();

        let signature = theirs.sign(b"hello");
        // We can verify a peer's signature without holding their key.
        assert!(IdentityProvider::verify(&ours, &theirs.public_key(), b"hello", &signature).is_ok());
    }
}
