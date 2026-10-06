//! Tracks which devices we can see right now.
//!
//! This is a pure bookkeeping structure: it holds *observation* state
//! (seen over mDNS, connected, pairing in flight) and deliberately does not
//! hold trust decisions. Trust lives in exactly one place, the trust store in
//! `clipmesh-identity`, and is projected in when the views are built. Duplicating
//! it here would create a second source of truth that could drift.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use clipmesh_protocol::{now_millis, DeviceId, DeviceInfo};

use crate::event::{PairingPrompt, PeerView};
use crate::provider::PeerAddress;

/// How long a peer stays in the list after its last mDNS announcement.
///
/// mDNS `ServiceRemoved` events are not reliable across sleep/wake, so the list
/// is also aged out by time.
pub const DEFAULT_PEER_TTL: Duration = Duration::from_secs(90);

/// How long a pairing prompt waits for the user before it is dropped.
pub const DEFAULT_PAIRING_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
struct PeerEntry {
    info: DeviceInfo,
    fingerprint: String,
    address: SocketAddr,
    connected: bool,
    pairing: bool,
    /// Monotonic clock reading, used for ageing out.
    last_seen: Instant,
    /// Wall clock reading, shown in the UI.
    last_seen_millis: i64,
}

#[derive(Debug, Clone)]
struct PendingPrompt {
    prompt: PairingPrompt,
    raised_at: Instant,
}

/// The set of devices currently visible on the network.
#[derive(Debug)]
pub struct DeviceRegistry {
    peers: HashMap<DeviceId, PeerEntry>,
    prompts: HashMap<DeviceId, PendingPrompt>,
    peer_ttl: Duration,
    pairing_ttl: Duration,
}

impl DeviceRegistry {
    /// Create a registry with the given ageing rules.
    #[must_use]
    pub fn new(peer_ttl: Duration, pairing_ttl: Duration) -> Self {
        Self {
            peers: HashMap::new(),
            prompts: HashMap::new(),
            peer_ttl,
            pairing_ttl,
        }
    }

    /// Record that a peer was seen.
    ///
    /// Returns `true` when the resulting view differs from what we had, so the
    /// caller only emits a UI event on a real change.
    pub fn observe(&mut self, peer: &PeerAddress) -> bool {
        let now = Instant::now();
        let now_millis = now_millis();

        match self.peers.get_mut(&peer.device_id) {
            Some(entry) => {
                let changed = entry.info.name != peer.device.name
                    || entry.info.platform != peer.device.platform
                    || entry.address != peer.address
                    || entry.fingerprint != peer.fingerprint;
                entry.info = peer.device.clone();
                entry.fingerprint = peer.fingerprint.clone();
                entry.address = peer.address;
                entry.last_seen = now;
                entry.last_seen_millis = now_millis;
                changed
            }
            None => {
                self.peers.insert(
                    peer.device_id,
                    PeerEntry {
                        info: peer.device.clone(),
                        fingerprint: peer.fingerprint.clone(),
                        address: peer.address,
                        connected: false,
                        pairing: false,
                        last_seen: now,
                        last_seen_millis: now_millis,
                    },
                );
                true
            }
        }
    }

    /// Drop a peer that announced it is going away.
    pub fn forget(&mut self, device: DeviceId) -> bool {
        self.prompts.remove(&device);
        self.peers.remove(&device).is_some()
    }

    /// Mark a peer as connected or disconnected.
    pub fn set_connected(&mut self, device: DeviceId, connected: bool) -> bool {
        match self.peers.get_mut(&device) {
            Some(entry) if entry.connected != connected => {
                entry.connected = connected;
                if connected {
                    entry.last_seen = Instant::now();
                    entry.last_seen_millis = now_millis();
                }
                true
            }
            _ => false,
        }
    }

    /// Mark that a pairing exchange is in flight with a peer.
    pub fn set_pairing(&mut self, device: DeviceId, pairing: bool) -> bool {
        match self.peers.get_mut(&device) {
            Some(entry) if entry.pairing != pairing => {
                entry.pairing = pairing;
                true
            }
            _ => false,
        }
    }

    /// Remember a pairing request that is waiting for the user.
    ///
    /// Replaces any previous prompt for the same device: a peer that retries
    /// should not stack up dialogs on the other side.
    pub fn raise_prompt(&mut self, prompt: PairingPrompt) -> bool {
        let device = prompt.device_id;
        let previous = self.prompts.insert(
            device,
            PendingPrompt {
                prompt,
                raised_at: Instant::now(),
            },
        );
        let changed = previous
            .as_ref()
            .is_none_or(|old| old.prompt != self.prompts[&device].prompt);
        if self.peers.contains_key(&device) {
            self.set_pairing(device, true);
        }
        changed
    }

    /// Remove a prompt once the user answered it.
    pub fn resolve_prompt(&mut self, device: DeviceId) -> bool {
        let removed = self.prompts.remove(&device).is_some();
        self.set_pairing(device, false);
        removed
    }

    /// Whether a prompt is pending for this device.
    #[must_use]
    pub fn has_prompt(&self, device: DeviceId) -> bool {
        self.prompts.contains_key(&device)
    }

    /// Prompts awaiting a decision, oldest first.
    #[must_use]
    pub fn prompts(&self, is_trusted: impl Fn(DeviceId) -> bool) -> Vec<PairingPrompt> {
        let mut prompts: Vec<PairingPrompt> = self
            .prompts
            .values()
            .filter(|pending| !is_trusted(pending.prompt.device_id))
            .map(|pending| pending.prompt.clone())
            .collect();
        prompts.sort_by_key(|prompt| prompt.requested_at);
        prompts
    }

    /// Drop peers that have not been seen for a while and prompts nobody
    /// answered. Returns `true` when something was removed.
    pub fn prune(&mut self) -> bool {
        let now = Instant::now();
        let peer_ttl = self.peer_ttl;
        let pairing_ttl = self.pairing_ttl;

        let before = self.peers.len() + self.prompts.len();
        self.peers.retain(|_, entry| {
            // A peer we hold a session with is not stale, however long ago it was
            // last announced. mDNS only re-announces periodically, so judging a
            // live connection by `last_seen` alone would make connected devices
            // vanish from the list.
            entry.connected || now.duration_since(entry.last_seen) < peer_ttl
        });
        self.prompts
            .retain(|_, pending| now.duration_since(pending.raised_at) < pairing_ttl);
        self.peers.len() + self.prompts.len() != before
    }

    /// Number of peers being tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether no peer is visible.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// The advertised address of a peer, if we have seen it.
    #[must_use]
    pub fn peer_of(&self, device: DeviceId) -> Option<PeerAddress> {
        self.peers.get(&device).map(|entry| PeerAddress {
            device_id: device,
            device: entry.info.clone(),
            fingerprint: entry.fingerprint.clone(),
            address: entry.address,
        })
    }

    /// Build the snapshot handed to the UI, newest activity first.
    #[must_use]
    pub fn views(&self, is_trusted: impl Fn(DeviceId) -> bool) -> Vec<PeerView> {
        let mut views: Vec<PeerView> = self
            .peers
            .iter()
            .map(|(device_id, entry)| PeerView {
                device_id: *device_id,
                name: entry.info.display_name().to_owned(),
                platform: entry.info.platform,
                address: entry.address.to_string(),
                fingerprint: entry.fingerprint.clone(),
                trusted: is_trusted(*device_id),
                connected: entry.connected,
                pairing: entry.pairing,
                last_seen: entry.last_seen_millis,
            })
            .collect();
        // Connected peers first, then most recently seen.
        views.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| b.last_seen.cmp(&a.last_seen))
        });
        views
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_PEER_TTL, DEFAULT_PAIRING_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::PairingDirection;
    use clipmesh_protocol::Platform;

    fn peer(name: &str) -> PeerAddress {
        let device_id = DeviceId::new();
        PeerAddress {
            device_id,
            device: DeviceInfo::new(device_id, name, Platform::Android, "0.1.0"),
            fingerprint: "AAAA BBBB".into(),
            address: "10.0.0.5:47711".parse().unwrap(),
        }
    }

    fn prompt(device_id: DeviceId) -> PairingPrompt {
        PairingPrompt {
            device_id,
            name: "Pixel".into(),
            platform: Platform::Android,
            fingerprint: "AAAA BBBB".into(),
            address: "10.0.0.5:47711".into(),
            requested_at: now_millis(),
            direction: PairingDirection::Incoming,
        }
    }

    #[test]
    fn observing_a_new_peer_reports_a_change() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Pixel");
        assert!(registry.observe(&found));
        assert_eq!(registry.len(), 1);
        // Seeing the exact same announcement again is not a change.
        assert!(!registry.observe(&found));
    }

    #[test]
    fn a_renamed_peer_reports_a_change() {
        let mut registry = DeviceRegistry::default();
        let mut found = peer("Pixel");
        registry.observe(&found);
        found.device.name = "Pixel 9 Pro".into();
        assert!(registry.observe(&found));
        assert_eq!(registry.views(|_| false)[0].name, "Pixel 9 Pro");
    }

    #[test]
    fn connection_state_only_reports_real_transitions() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Laptop");
        registry.observe(&found);

        assert!(registry.set_connected(found.device_id, true));
        assert!(!registry.set_connected(found.device_id, true));
        assert!(registry.set_connected(found.device_id, false));
        // Unknown devices are ignored rather than inserted.
        assert!(!registry.set_connected(DeviceId::new(), true));
    }

    #[test]
    fn forgetting_a_peer_clears_its_prompt() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Phone");
        registry.observe(&found);
        registry.raise_prompt(prompt(found.device_id));
        assert!(registry.has_prompt(found.device_id));

        assert!(registry.forget(found.device_id));
        assert!(!registry.has_prompt(found.device_id));
        assert!(registry.is_empty());
    }

    #[test]
    fn prompts_hide_devices_that_are_already_trusted() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Phone");
        registry.observe(&found);
        registry.raise_prompt(prompt(found.device_id));

        assert_eq!(registry.prompts(|_| false).len(), 1);
        assert!(registry.prompts(|_| true).is_empty());
    }

    #[test]
    fn raising_a_prompt_marks_the_peer_as_pairing() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Phone");
        registry.observe(&found);
        registry.raise_prompt(prompt(found.device_id));

        assert!(registry.views(|_| false)[0].pairing);
        assert!(registry.resolve_prompt(found.device_id));
        assert!(!registry.views(|_| false)[0].pairing);
    }

    #[test]
    fn connected_peers_sort_first() {
        let mut registry = DeviceRegistry::default();
        let idle = peer("Idle");
        let busy = peer("Busy");
        registry.observe(&idle);
        std::thread::sleep(Duration::from_millis(2));
        registry.observe(&busy);
        // `busy` is newer, but `idle` is connected: connection wins.
        registry.set_connected(idle.device_id, true);

        let views = registry.views(|_| false);
        assert_eq!(views[0].name, "Idle");
    }

    #[test]
    fn views_project_trust_from_the_caller() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Phone");
        registry.observe(&found);

        let trusted = found.device_id;
        assert!(registry.views(move |id| id == trusted)[0].trusted);
        assert!(!registry.views(|_| false)[0].trusted);
    }

    #[test]
    fn stale_peers_and_abandoned_prompts_are_pruned() {
        let mut registry = DeviceRegistry::new(Duration::from_millis(1), Duration::from_millis(1));
        let found = peer("Gone");
        registry.observe(&found);
        registry.raise_prompt(prompt(found.device_id));

        std::thread::sleep(Duration::from_millis(5));
        assert!(registry.prune());
        assert!(registry.is_empty());
        assert!(!registry.prune());
    }

    #[test]
    fn address_lookup_serves_the_connector() {
        let mut registry = DeviceRegistry::default();
        let found = peer("Phone");
        registry.observe(&found);
        let looked_up = registry.peer_of(found.device_id).unwrap();
        assert_eq!(looked_up.address, "10.0.0.5:47711".parse().unwrap());
        assert_eq!(looked_up.device.name, "Phone");
        assert_eq!(registry.peer_of(DeviceId::new()), None);
    }
}
