//! What the engine tells the outside world, and the shapes the UI renders.
//!
//! Every event carries a **snapshot** rather than a delta. A UI that renders
//! `CoreEvent::Peers(list)` is always correct, even if it missed an earlier
//! event while the window was hidden - there is no incremental state to get out
//! of sync. The lists are small (a handful of devices, a bounded history), so
//! the extra bytes do not matter.

use serde::{Deserialize, Serialize};

use clipmesh_protocol::{ClipboardItem, DeviceId, Platform};

/// Which side started a pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairingDirection {
    /// A remote device asked to pair with us.
    Incoming,
    /// We asked a remote device to pair with us.
    Outgoing,
}

/// A pairing request waiting for a user decision.
///
/// This is the payload behind the "发现新设备 XXX / 接受 / 拒绝" dialog. It
/// carries the certificate fingerprint so the user can compare it out of band
/// with the other device's screen - that comparison is the actual security
/// boundary of the pairing flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingPrompt {
    /// The device asking (or being asked).
    pub device_id: DeviceId,
    /// Its display name.
    pub name: String,
    /// Its operating system family.
    pub platform: Platform,
    /// Certificate fingerprint, formatted for reading aloud.
    pub fingerprint: String,
    /// Where the request came from.
    pub address: String,
    /// When it arrived, unix milliseconds.
    pub requested_at: i64,
    /// Who initiated.
    pub direction: PairingDirection,
}

/// A device the user has explicitly trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDeviceView {
    /// Stable device identifier.
    pub device_id: DeviceId,
    /// Display name at the time of pairing.
    pub name: String,
    /// Operating system family.
    pub platform: Platform,
    /// Certificate fingerprint pinned at pairing time.
    pub fingerprint: String,
    /// When the user accepted, unix milliseconds.
    pub trusted_at: i64,
    /// Whether a session is live right now.
    pub online: bool,
}

/// A device currently visible on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    /// Stable device identifier.
    pub device_id: DeviceId,
    /// Display name.
    pub name: String,
    /// Operating system family.
    pub platform: Platform,
    /// `ip:port` of the peer.
    pub address: String,
    /// Fingerprint the peer advertises.
    pub fingerprint: String,
    /// Whether the peer is in our trust store.
    pub trusted: bool,
    /// Whether a TLS session is established.
    pub connected: bool,
    /// Whether a pairing exchange is in flight.
    pub pairing: bool,
    /// Last time we saw this peer, unix milliseconds.
    pub last_seen: i64,
}

/// High level engine state, rendered in the status bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    /// Whether discovery and the listener are running.
    pub running: bool,
    /// Whether local clipboard changes are pushed automatically.
    pub auto_sync: bool,
    /// This device's identifier.
    pub device_id: DeviceId,
    /// This device's display name.
    pub device_name: String,
    /// This device's platform.
    pub platform: Platform,
    /// This device's certificate fingerprint.
    pub fingerprint: String,
    /// TCP port the listener bound to.
    pub listen_port: u16,
    /// Number of live sessions.
    pub connected_peers: usize,
    /// Number of trusted devices.
    pub trusted_peers: usize,
    /// Most recent error, cleared on the next successful operation.
    pub last_error: Option<String>,
}

/// Everything the engine publishes.
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// The set of visible devices changed.
    Peers(Vec<PeerView>),
    /// The trust store changed.
    Trusted(Vec<TrustedDeviceView>),
    /// The set of pairing requests awaiting a decision changed.
    PairingRequests(Vec<PairingPrompt>),
    /// Clipboard content arrived from a peer and was applied locally.
    ClipboardReceived(Box<ClipboardItem>),
    /// Local clipboard content was pushed to peers.
    ClipboardSent {
        /// What was sent.
        item: Box<ClipboardItem>,
        /// How many peers accepted it.
        delivered: usize,
    },
    /// The clipboard history changed.
    History(Vec<ClipboardItem>),
    /// Engine status changed.
    Status(Box<StatusView>),
    /// Something went wrong. Never fatal: the engine keeps running.
    Error(String),
}

impl CoreEvent {
    /// Stable lowercase name, used for logging and to select the Tauri event
    /// channel on the frontend.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Peers(_) => "peers",
            Self::Trusted(_) => "trusted",
            Self::PairingRequests(_) => "pairing-requests",
            Self::ClipboardReceived(_) => "clipboard-received",
            Self::ClipboardSent { .. } => "clipboard-sent",
            Self::History(_) => "history",
            Self::Status(_) => "status",
            Self::Error(_) => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_serialize_with_camel_case_keys() {
        let status = StatusView {
            running: true,
            auto_sync: true,
            device_id: DeviceId::new(),
            device_name: "Desktop".into(),
            platform: Platform::Windows,
            fingerprint: "AB CD".into(),
            listen_port: 47711,
            connected_peers: 2,
            trusted_peers: 3,
            last_error: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["autoSync"], true);
        assert_eq!(json["listenPort"], 47711);
        assert_eq!(json["connectedPeers"], 2);
        assert_eq!(json["platform"], "windows");
        assert!(json["lastError"].is_null());
    }

    #[test]
    fn pairing_prompts_carry_the_whole_dialog() {
        let prompt = PairingPrompt {
            device_id: DeviceId::new(),
            name: "Pixel 9".into(),
            platform: Platform::Android,
            fingerprint: "1111 2222 3333".into(),
            address: "192.168.1.7:47711".into(),
            requested_at: 1_700_000_000_000,
            direction: PairingDirection::Incoming,
        };
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(json["direction"], "incoming");
        assert_eq!(json["platform"], "android");
        assert_eq!(json["requestedAt"], 1_700_000_000_000i64);
    }

    #[test]
    fn event_names_are_stable() {
        assert_eq!(CoreEvent::Error("x".into()).name(), "error");
        let sent = CoreEvent::ClipboardSent {
            item: Box::new(ClipboardItem::Text(
                clipmesh_protocol::TextPayload::new_local("hi", DeviceId::new()),
            )),
            delivered: 1,
        };
        assert_eq!(sent.name(), "clipboard-sent");
    }
}
