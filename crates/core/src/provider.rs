//! The three abstractions the engine needs from the outside world.
//!
//! `clipmesh-core` never links against a windowing toolkit, a network stack or
//! an operating system clipboard. It only knows these traits, which is what
//! makes the whole engine testable with fakes and portable to a new platform
//! by implementing three traits.
//!
//! ```text
//!        ┌──────────────────────────────┐
//!        │        clipmesh-core         │
//!        │  SyncManager / DeviceManager │
//!        └───────┬──────────┬───────────┘
//!    ClipboardProvider  NetworkProvider  IdentityProvider
//!                │          │                │
//!         arboard /   mdns-sd + TCP     Ed25519 key +
//!         Android     + rustls TLS      trust store
//! ```

use std::net::SocketAddr;

use async_trait::async_trait;
use clipmesh_identity::{DeviceCertificate, Fingerprint};
use clipmesh_protocol::{ClipboardContent, DeviceId, DeviceInfo, Envelope, Platform};
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Something that happened to the local system clipboard.
#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    /// The user (or another application) copied something.
    Changed(ClipboardContent),
    /// The platform watcher broke; the engine keeps running and reports it.
    Error(String),
}

/// Read, write and observe the system clipboard.
///
/// Implementations are shared across threads, so `write` may be called while
/// `read` is in flight. Platform backends that cannot do that (the Windows
/// clipboard is a single global lock) serialize internally.
#[async_trait]
pub trait ClipboardProvider: Send + Sync + 'static {
    /// Current clipboard content, or `None` when the clipboard holds something
    /// ClipMesh does not carry (files, rich text, an empty selection...).
    async fn read(&self) -> Result<Option<ClipboardContent>>;

    /// Put content on the system clipboard.
    ///
    /// The engine calls this when a remote payload arrives. Implementations
    /// must make sure the resulting change is *not* reported again through
    /// [`ClipboardProvider::watch`] as a user copy, otherwise two devices echo
    /// the same text at each other forever.
    async fn write(&self, content: &ClipboardContent) -> Result<()>;

    /// Stream of local clipboard changes, for the lifetime of the process.
    ///
    /// The stream must never terminate on a transient error: yield
    /// [`ClipboardEvent::Error`] and keep polling.
    fn watch(&self) -> BoxStream<'static, ClipboardEvent>;

    /// Tell the watcher that the next change is ours. Called by the engine
    /// right before [`ClipboardProvider::write`] when a backend cannot detect
    /// this on its own.
    fn suppress_next_change(&self) {}
}

/// A device we can see on the local network but may not have connected to yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerAddress {
    /// Stable device identifier advertised over mDNS.
    pub device_id: DeviceId,
    /// Human readable description of the peer.
    pub device: DeviceInfo,
    /// Certificate fingerprint advertised over mDNS. Verified against the
    /// real certificate during the TLS handshake; a mismatch aborts the
    /// connection instead of trusting the broadcast.
    pub fingerprint: String,
    /// Where to reach the peer.
    pub address: SocketAddr,
}

/// A live, mutually authenticated session.
///
/// Carries the identity material proven during the handshake, because the
/// engine needs all of it to pin a newly paired device. Trust itself is *not*
/// stored here: it is always looked up in the trust store, which is the single
/// source of truth.
#[derive(Debug, Clone)]
pub struct PeerSession {
    /// Stable device identifier of the peer.
    pub device_id: DeviceId,
    /// Human readable name the peer claimed.
    pub name: String,
    /// Operating system family the peer claimed.
    pub platform: Platform,
    /// Remote socket address.
    pub address: SocketAddr,
    /// Whether the peer is in our trust store.
    pub trusted: bool,
    /// When the session was established, unix milliseconds.
    pub since_millis: i64,
    /// The peer's Ed25519 public key, checked against its certificate.
    pub public_key: [u8; 32],
    /// Fingerprint of the peer's certificate.
    pub fingerprint: Fingerprint,
    /// The peer's self-signed certificate, as presented over TLS.
    pub certificate: DeviceCertificate,
}

/// Traffic coming out of the network layer.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// mDNS reported a device.
    Discovered(Box<PeerAddress>),
    /// mDNS reported a device went away.
    Lost(DeviceId),
    /// A session finished its handshake and is ready for payloads.
    Connected(Box<PeerSession>),
    /// A session ended.
    Disconnected {
        /// Which peer.
        device: DeviceId,
        /// Why, for the log and the UI.
        reason: String,
    },
    /// An authenticated envelope arrived.
    Message {
        /// Which peer sent it.
        device: DeviceId,
        /// The message itself.
        envelope: Box<Envelope>,
    },
    /// A non fatal transport problem.
    Error(String),
}

/// Discover peers, hold TLS sessions open and move envelopes around.
///
/// The trait is intentionally push oriented: the engine subscribes once with
/// [`NetworkProvider::events`] and reacts. Implementations own their tasks.
#[async_trait]
pub trait NetworkProvider: Send + Sync + 'static {
    /// Start advertising over mDNS and accept inbound connections.
    async fn start(&self) -> Result<()>;

    /// Open a session to a discovered peer.
    ///
    /// During pairing this is the one allowed way to talk to an untrusted
    /// device; see `clipmesh_security::TrustPolicy`.
    async fn connect(&self, peer: &PeerAddress) -> Result<()>;

    /// Send one envelope to one peer.
    async fn send(&self, device: DeviceId, envelope: Envelope) -> Result<()>;

    /// Drop the session with a peer, if there is one.
    ///
    /// Used when the user unpairs a device: the certificate may still be
    /// acceptable to the TLS layer, but the session it protects must not
    /// outlive the trust decision.
    async fn disconnect(&self, device: DeviceId) -> Result<()>;

    /// Send one envelope to every connected, trusted peer.
    ///
    /// Returns the peers the message was queued for, so the UI can say
    /// "delivered to 3 devices".
    async fn broadcast(&self, envelope: Envelope) -> Result<Vec<DeviceId>>;

    /// Devices with a live session right now.
    fn connected_peers(&self) -> Vec<DeviceId>;

    /// Whether a session to `device` is live.
    fn is_connected(&self, device: DeviceId) -> bool {
        self.connected_peers().contains(&device)
    }

    /// Stream of network events, for the lifetime of the process.
    fn events(&self) -> BoxStream<'static, NetworkEvent>;

    /// Stop listening, drop every session and stop advertising.
    async fn shutdown(&self) -> Result<()>;
}

/// Sign and verify with the device's long term Ed25519 key.
///
/// Private keys never leave this abstraction: the engine can ask for a
/// signature but can never read the key material.
pub trait IdentityProvider: Send + Sync + 'static {
    /// This device's identifier.
    fn device_id(&self) -> DeviceId;

    /// This device's long term public key, 32 raw bytes.
    fn public_key(&self) -> [u8; 32];

    /// Human readable fingerprint of this device's certificate.
    fn fingerprint(&self) -> &Fingerprint;

    /// Sign a message with the device key.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;

    /// Verify a signature made by `public_key`.
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_addresses_serialize_for_the_frontend() {
        let peer = PeerAddress {
            device_id: DeviceId::new(),
            device: DeviceInfo::local(DeviceId::new(), "Laptop"),
            fingerprint: "AAAA BBBB".into(),
            address: "192.168.1.10:47711".parse().unwrap(),
        };
        let json = serde_json::to_value(&peer).unwrap();
        assert!(json.get("deviceId").is_some());
        assert!(json.get("fingerprint").is_some());
        assert_eq!(json["device"]["platform"], "windows");
    }

    #[test]
    fn user_actionable_errors_are_classified() {
        use crate::error::CoreError;
        assert!(CoreError::NotTrusted(DeviceId::new()).is_user_actionable());
        assert!(!CoreError::Other("bug".into()).is_user_actionable());
    }
}
