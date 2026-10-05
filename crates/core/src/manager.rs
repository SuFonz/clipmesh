//! The sync engine.
//!
//! `SyncManager` is the only place where the three providers, the trust store,
//! the deduplication cache and the history meet. Everything above it (Tauri,
//! Android) calls methods on this type; everything below it is a trait object
//! that the platform layer injected at startup.
//!
//! ## Threading model
//!
//! The manager is shared as an `Arc` and is `Send + Sync`. Mutable state sits
//! behind `parking_lot` locks, never held across an `await`: each handler
//! copies what it needs out of a lock, drops the guard, and only then touches
//! the network or the clipboard.
//!
//! Two background loops are spawned by [`SyncManager::spawn`]:
//!
//! * the **network loop** turns [`NetworkEvent`]s into engine actions,
//! * the **clipboard loop** turns local clipboard changes into broadcasts.
//!
//! ## What the engine will and will not forward
//!
//! Payloads are sent to every connected, trusted peer directly. There is no
//! store-and-forward and no relaying: on a LAN every device discovers every
//! other device over mDNS, so a direct broadcast is enough, and not relaying
//! removes a whole class of loops and trust questions. [`DedupCache`] still
//! runs, because a peer echoing our own payload back is a real failure mode.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use parking_lot::{Mutex, RwLock};
use tokio::sync::broadcast;

use clipmesh_identity::{DeviceIdentity, TrustStore, TrustedDevice};
use clipmesh_protocol::{
    now_millis, ClipboardContent, ClipboardItem, DeviceId, Envelope, ImageChunk, ImageMeta,
    ImagePayload, Payload, Platform, TextPayload, IMAGE_CHUNK_BYTES,
};
use clipmesh_security::{build_pair_accept, random_challenge, verify_pair_accept};

use crate::device::DeviceRegistry;
use crate::error::{CoreError, Result};
use crate::event::{
    CoreEvent, PairingDirection, PairingPrompt, PeerView, StatusView, TrustedDeviceView,
};
use crate::provider::{
    ClipboardEvent, ClipboardProvider, NetworkEvent, NetworkProvider, PeerAddress, PeerSession,
};
use crate::settings::{Settings, SettingsPatch};
use crate::sync::{DedupCache, EchoSuppressor, History};

/// Capacity of the event channel.
///
/// Deep enough that a UI which is busy re-rendering cannot lose a clipboard
/// event, shallow enough that a UI which is gone cannot grow the process
/// without bound.
const EVENT_CHANNEL_CAPACITY: usize = 128;

/// How long an unanswered pairing attempt is kept before it is forgotten.
const PAIRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// How long a partially received image is kept before it is dropped.
const IMAGE_ASSEMBLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Everything the engine needs, injected by the platform layer.
pub struct SyncManagerOptions {
    /// This device's identity.
    pub identity: Arc<DeviceIdentity>,
    /// Devices the user has trusted.
    ///
    /// Shared with the network service rather than owned, so that pairing a
    /// device and unpairing it are visible to the TLS policy immediately.
    pub trust: crate::SharedTrustStore,
    /// Platform clipboard access.
    pub clipboard: Arc<dyn ClipboardProvider>,
    /// Discovery and transport.
    pub network: Arc<dyn NetworkProvider>,
    /// User settings.
    pub settings: Settings,
    /// Where to persist settings. `None` keeps them in memory only.
    pub settings_path: Option<PathBuf>,
}

/// A pairing we started and are waiting to hear back about.
#[derive(Debug, Clone)]
struct OutgoingPairing {
    nonce: Vec<u8>,
    started_at: Instant,
}

/// An image being reassembled from chunks.
#[derive(Debug)]
struct IncomingImage {
    meta: ImageMeta,
    buffer: Vec<u8>,
    started_at: Instant,
}

/// The result of pushing content to the mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOutcome {
    /// Identifier of what was sent.
    pub id: String,
    /// How many peers accepted the whole payload.
    pub delivered: usize,
}

/// The sync engine.
pub struct SyncManager {
    identity: Arc<DeviceIdentity>,
    trust: Arc<RwLock<TrustStore>>,
    clipboard: Arc<dyn ClipboardProvider>,
    network: Arc<dyn NetworkProvider>,
    settings: RwLock<Settings>,
    settings_path: Option<PathBuf>,

    registry: Mutex<DeviceRegistry>,
    dedup: Mutex<DedupCache>,
    echo: Mutex<EchoSuppressor>,
    history: Mutex<History>,
    sessions: Mutex<HashMap<DeviceId, PeerSession>>,
    outgoing: Mutex<HashMap<DeviceId, OutgoingPairing>>,
    incoming_nonces: Mutex<HashMap<DeviceId, Vec<u8>>>,
    incoming_images: Mutex<HashMap<DeviceId, IncomingImage>>,

    events: broadcast::Sender<CoreEvent>,
    running: AtomicBool,
    listen_port: AtomicU16,
    last_error: Mutex<Option<String>>,
}

impl SyncManager {
    /// Build the engine. Call [`SyncManager::spawn`] to start the loops and
    /// [`SyncManager::start`] to open the network.
    #[must_use]
    pub fn new(options: SyncManagerOptions) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let history_capacity = options.settings.history_capacity;

        Arc::new(Self {
            identity: options.identity,
            trust: options.trust,
            clipboard: options.clipboard,
            network: options.network,
            settings: RwLock::new(options.settings),
            settings_path: options.settings_path,
            registry: Mutex::new(DeviceRegistry::default()),
            dedup: Mutex::new(DedupCache::default()),
            echo: Mutex::new(EchoSuppressor::default()),
            history: Mutex::new(History::new(history_capacity)),
            sessions: Mutex::new(HashMap::new()),
            outgoing: Mutex::new(HashMap::new()),
            incoming_nonces: Mutex::new(HashMap::new()),
            incoming_images: Mutex::new(HashMap::new()),
            events,
            running: AtomicBool::new(false),
            listen_port: AtomicU16::new(0),
            last_error: Mutex::new(None),
        })
    }

    /// Subscribe to engine events. Every UI attaches exactly one subscriber.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    /// This device's identity.
    #[must_use]
    pub fn identity(&self) -> &Arc<DeviceIdentity> {
        &self.identity
    }

    /// Whether discovery and the listener are up.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Record the port the listener actually bound to, for the status bar.
    pub fn set_listen_port(&self, port: u16) {
        self.listen_port.store(port, Ordering::Relaxed);
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Spawn the network and clipboard consumer loops.
    ///
    /// Call once, before [`SyncManager::start`].
    pub fn spawn(self: &Arc<Self>) {
        let network_tasks = Arc::clone(self);
        tokio::spawn(async move { network_tasks.network_loop().await });

        let clipboard_tasks = Arc::clone(self);
        tokio::spawn(async move { clipboard_tasks.clipboard_loop().await });
    }

    /// Open the network: advertise over mDNS and accept connections.
    ///
    /// Idempotent.
    ///
    /// # Errors
    /// Returns [`CoreError::Network`] when discovery or the listener fails.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }

        self.network.start().await?;
        self.running.store(true, Ordering::Relaxed);
        tracing::info!(device = %self.identity.device_id(), "clipmesh engine started");
        self.emit_status();
        Ok(())
    }

    /// Close the network and forget every session. Idempotent.
    ///
    /// # Errors
    /// Returns [`CoreError::Network`] if the listener cannot be shut down.
    pub async fn stop(&self) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }

        self.network.shutdown().await?;
        self.running.store(false, Ordering::Relaxed);

        self.sessions.lock().clear();
        let known: Vec<DeviceId> = self
            .registry
            .lock()
            .views(|_| false)
            .into_iter()
            .map(|view| view.device_id)
            .collect();
        for device in known {
            self.registry.lock().set_connected(device, false);
        }

        tracing::info!("clipmesh engine stopped");
        self.emit_status();
        self.emit_peers();
        Ok(())
    }

    async fn network_loop(self: Arc<Self>) {
        let mut events = self.network.events();
        while let Some(event) = events.next().await {
            self.handle_network_event(event).await;
        }
        tracing::warn!("the network event stream ended; the engine is now deaf");
    }

    async fn clipboard_loop(self: Arc<Self>) {
        let mut changes = self.clipboard.watch();
        while let Some(event) = changes.next().await {
            match event {
                ClipboardEvent::Changed(content) => self.handle_local_change(content).await,
                ClipboardEvent::Error(message) => {
                    self.report(format!("clipboard watcher: {message}"));
                }
            }
        }
        tracing::warn!("the clipboard watcher ended; local changes will no longer sync");
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// A snapshot of the engine for the status bar.
    #[must_use]
    pub fn status(&self) -> StatusView {
        StatusView {
            running: self.is_running(),
            auto_sync: self.settings.read().auto_sync,
            device_id: self.identity.device_id(),
            device_name: self.identity.name(),
            platform: Platform::current(),
            fingerprint: self.identity.fingerprint().to_grouped(),
            listen_port: self.listen_port.load(Ordering::Relaxed),
            connected_peers: self.sessions.lock().len(),
            trusted_peers: self.trust.read().len(),
            last_error: self.last_error.lock().clone(),
        }
    }

    /// Devices currently visible on the network.
    #[must_use]
    pub fn peers(&self) -> Vec<PeerView> {
        let trust = self.trust.read();
        self.registry.lock().views(|device| trust.is_trusted(device))
    }

    /// Devices the user has trusted.
    #[must_use]
    pub fn trusted_devices(&self) -> Vec<TrustedDeviceView> {
        let sessions = self.sessions.lock();
        self.trust
            .read()
            .all()
            .into_iter()
            .map(|device| TrustedDeviceView {
                online: sessions.contains_key(&device.device_id),
                device_id: device.device_id,
                name: device.name,
                platform: device.platform,
                fingerprint: device.fingerprint.to_grouped(),
                trusted_at: device.trusted_at,
            })
            .collect()
    }

    /// Pairing requests waiting for the user.
    #[must_use]
    pub fn pairing_requests(&self) -> Vec<PairingPrompt> {
        let trust = self.trust.read();
        self.registry
            .lock()
            .prompts(|device| trust.is_trusted(device))
    }

    /// Clipboard history, newest first.
    #[must_use]
    pub fn history(&self) -> Vec<ClipboardItem> {
        self.history.lock().to_vec()
    }

    /// Current settings.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Whether a device is trusted.
    #[must_use]
    pub fn is_trusted(&self, device: DeviceId) -> bool {
        self.trust.read().is_trusted(device)
    }

    // -----------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------

    /// Apply a settings patch, persist it and push the effects out.
    ///
    /// Renaming the device restarts the network service, because the mDNS
    /// announcement carries the name and there is no way to amend it in place.
    ///
    /// # Errors
    /// Returns [`CoreError`] when persisting fails or the restart fails.
    pub async fn update_settings(self: &Arc<Self>, patch: SettingsPatch) -> Result<Settings> {
        let (changed, renamed, policy_changed) = {
            let mut settings = self.settings.write();
            let renamed = patch
                .device_name
                .as_ref()
                .is_some_and(|name| name.trim() != settings.device_name);
            let before = settings.sync_policy();
            let changed = settings.apply(&patch);
            let policy_changed = changed && settings.sync_policy() != before;
            (changed, renamed, policy_changed)
        };

        if renamed {
            let name = self.settings.read().device_name.clone();
            if let Err(error) = self.identity.set_name(name) {
                // The identity file may be read-only in a sandboxed install;
                // the in-memory name still changes, which is what the UI shows.
                tracing::warn!(%error, "could not persist the device name");
            }
        }

        if changed {
            if let Some(path) = &self.settings_path {
                self.settings.read().save(path)?;
            }

            if policy_changed {
                let capacity = self.settings.read().history_capacity;
                *self.history.lock() = History::new(capacity);
                self.emit_history();
            }

            if renamed && self.is_running() {
                // Re-announce under the new name.
                self.stop().await?;
                self.start().await?;
            }

            self.emit_status();
        }

        Ok(self.settings.read().clone())
    }

    /// Ask a discovered device to pair with us.
    ///
    /// # Errors
    /// Returns [`CoreError::NotConnected`] when the device has not been seen,
    /// or a network error when the connection or the request fails.
    pub async fn request_pairing(self: &Arc<Self>, device_id: DeviceId) -> Result<()> {
        if self.is_trusted(device_id) {
            return Err(CoreError::InvalidState("that device is already trusted"));
        }

        let peer = self
            .registry
            .lock()
            .peer_of(device_id)
            .ok_or(CoreError::NotConnected(device_id))?;

        let nonce = random_challenge().to_vec();
        self.outgoing.lock().insert(
            device_id,
            OutgoingPairing {
                nonce,
                started_at: Instant::now(),
            },
        );

        self.registry.lock().raise_prompt(PairingPrompt {
            device_id,
            name: peer.device.display_name().to_owned(),
            platform: peer.device.platform,
            fingerprint: peer.fingerprint.clone(),
            address: peer.address.to_string(),
            requested_at: now_millis(),
            direction: PairingDirection::Outgoing,
        });
        self.emit_pairing_requests();
        self.emit_peers();

        if self.network.is_connected(device_id) {
            self.send_pair_request(device_id).await?;
        } else {
            // The request goes out as soon as the session reports Connected.
            self.network.connect(&peer).await?;
        }
        Ok(())
    }

    /// Answer an incoming pairing request.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidState`] when there is no such request, or a
    /// network error when the answer cannot be delivered.
    pub async fn respond_pairing(self: &Arc<Self>, device_id: DeviceId, accept: bool) -> Result<()> {
        let prompt = self
            .registry
            .lock()
            .prompts(|_| false)
            .into_iter()
            .find(|prompt| prompt.device_id == device_id)
            .ok_or(CoreError::InvalidState("no pairing request for that device"))?;

        let nonce = self
            .incoming_nonces
            .lock()
            .remove(&device_id)
            .unwrap_or_default();

        let session = self.sessions.lock().get(&device_id).cloned();

        let reply = build_pair_accept(&self.identity, &nonce, accept)?;
        if let Err(error) = self
            .network
            .send(device_id, Envelope::new(Payload::PairAccept(reply)))
            .await
        {
            tracing::warn!(%device_id, %error, "could not deliver the pairing answer");
        }

        self.registry.lock().resolve_prompt(device_id);

        if accept {
            let session = session.ok_or(CoreError::NotConnected(device_id))?;
            let device = TrustedDevice::new(
                prompt.name.clone(),
                session.platform,
                session.certificate.clone(),
                &session.public_key,
            )?;
            self.trust.write().trust(device)?;
            tracing::info!(%device_id, name = %prompt.name, "paired a new device");
            self.emit_trusted();
        }

        self.emit_pairing_requests();
        self.emit_peers();
        self.emit_status();
        Ok(())
    }

    /// Forget a device and drop its session.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the trust store cannot be persisted.
    pub async fn unpair(&self, device_id: DeviceId) -> Result<()> {
        if !self.trust.write().forget(device_id)? {
            return Err(CoreError::NotTrusted(device_id));
        }

        self.registry.lock().resolve_prompt(device_id);
        self.sessions.lock().remove(&device_id);

        if let Err(error) = self.network.disconnect(device_id).await {
            tracing::warn!(%device_id, %error, "could not drop the session after unpairing");
        }

        tracing::info!(%device_id, "unpaired a device");
        self.emit_trusted();
        self.emit_peers();
        self.emit_status();
        Ok(())
    }

    /// Read the local clipboard and broadcast it.
    ///
    /// This is what the Android notification action and the desktop tray item
    /// call: it works regardless of whether automatic sync is enabled, because
    /// the user asked for it explicitly.
    ///
    /// # Errors
    /// Returns [`CoreError::Clipboard`] when there is nothing to send.
    pub async fn send_clipboard(self: &Arc<Self>) -> Result<SendOutcome> {
        let content = self.clipboard.read().await?.ok_or_else(|| {
            CoreError::Clipboard(
                "the clipboard is empty or holds something ClipMesh does not carry".to_owned(),
            )
        })?;

        if !self.settings.read().sync_policy().accepts(&content) {
            return Err(CoreError::Clipboard(
                "this kind of content is turned off in settings".to_owned(),
            ));
        }

        self.dedup.lock().insert(content.id());
        self.record_history(content.to_item());
        self.publish(&content).await
    }

    /// Broadcast arbitrary text.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the text is too large or the network fails.
    pub async fn send_text(self: &Arc<Self>, content: String) -> Result<SendOutcome> {
        let payload = TextPayload::new_local(content, self.identity.device_id());
        if !payload.is_within_limits() {
            return Err(CoreError::Clipboard(format!(
                "text is larger than the {} byte limit",
                clipmesh_protocol::MAX_TEXT_BYTES
            )));
        }

        let content = ClipboardContent::Text(payload);
        self.dedup.lock().insert(content.id());
        self.record_history(content.to_item());
        self.publish(&content).await
    }

    /// Read the system clipboard through the injected provider.
    ///
    /// The UI uses this to build a preview of whatever is on the clipboard
    /// right now, without duplicating the platform logic.
    ///
    /// # Errors
    /// Returns [`CoreError::Clipboard`] when the platform refuses.
    pub async fn read_clipboard(&self) -> Result<Option<ClipboardContent>> {
        self.clipboard.read().await
    }

    /// Send a history entry again.
    ///
    /// Images are re-read from the clipboard when they are still there; a
    /// payload that has since been replaced cannot be re-sent, because ClipMesh
    /// deliberately does not keep pixels in memory or on disk.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the entry is unknown or cannot be re-sent.
    pub async fn resend_history_item(self: &Arc<Self>, id: &str) -> Result<SendOutcome> {
        let item = self
            .history
            .lock()
            .to_vec()
            .into_iter()
            .find(|item| item.id() == id)
            .ok_or_else(|| CoreError::Other(format!("no history entry with id {id}")))?;

        match item {
            ClipboardItem::Text(payload) => {
                let content = ClipboardContent::Text(payload);
                self.dedup.lock().insert(content.id());
                self.publish(&content).await
            }
            ClipboardItem::Image(meta) => {
                let current = self.clipboard.read().await?;
                match current {
                    Some(ClipboardContent::Image(payload)) if payload.meta.id == meta.id => {
                        self.publish(&ClipboardContent::Image(payload)).await
                    }
                    _ => Err(CoreError::Clipboard(
                        "that image is no longer on the clipboard, so it cannot be re-sent"
                            .to_owned(),
                    )),
                }
            }
        }
    }

    /// Put a history entry back on the local clipboard without sending it.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the entry is unknown or the clipboard is
    /// unavailable.
    pub async fn copy_history_item(&self, id: &str) -> Result<()> {
        let item = self
            .history
            .lock()
            .to_vec()
            .into_iter()
            .find(|item| item.id() == id)
            .ok_or_else(|| CoreError::Other(format!("no history entry with id {id}")))?;

        match item {
            ClipboardItem::Text(payload) => {
                let content = ClipboardContent::Text(payload);
                self.echo.lock().record_write(&content);
                self.clipboard.write(&content).await
            }
            ClipboardItem::Image(_) => Err(CoreError::Clipboard(
                "image entries cannot be restored from history yet".to_owned(),
            )),
        }
    }

    /// Empty the history.
    pub fn clear_history(&self) {
        self.history.lock().clear();
        self.emit_history();
    }

    // -----------------------------------------------------------------------
    // Network events
    // -----------------------------------------------------------------------

    async fn handle_network_event(self: &Arc<Self>, event: NetworkEvent) {
        match event {
            NetworkEvent::Discovered(peer) => {
                let changed = { self.registry.lock().observe(&peer) };
                if changed {
                    self.emit_peers();
                    self.emit_status();
                }
                self.maybe_connect(&peer);
            }
            NetworkEvent::Lost(device) => {
                self.sessions.lock().remove(&device);
                let changed = { self.registry.lock().forget(device) };
                if changed {
                    self.emit_peers();
                    self.emit_status();
                }
            }
            NetworkEvent::Connected(session) => self.handle_connected(*session).await,
            NetworkEvent::Disconnected { device, reason } => {
                self.handle_disconnected(device, &reason);
            }
            NetworkEvent::Message { device, envelope } => {
                if let Err(error) = self.handle_message(device, *envelope).await {
                    if error.is_user_actionable() {
                        // "They declined", "they are not trusted": the user
                        // asked for this and deserves to see the answer.
                        self.report(error.to_string());
                    } else {
                        tracing::warn!(%device, %error, "could not handle a peer message");
                    }
                }
            }
            NetworkEvent::Error(message) => self.report(message),
        }
    }

    /// Dial a trusted peer we just discovered.
    ///
    /// Both devices see each other over mDNS, so without a rule they would both
    /// dial at the same moment and end up with two sessions. The tie-break is
    /// deterministic and needs no negotiation: **the device with the smaller
    /// id dials, the other one listens.**
    ///
    /// Untrusted peers are never dialled here - that only happens when the user
    /// asks to pair.
    fn maybe_connect(self: &Arc<Self>, peer: &PeerAddress) {
        if !self.is_running()
            || !self.is_trusted(peer.device_id)
            || self.network.is_connected(peer.device_id)
            || self.identity.device_id() >= peer.device_id
        {
            return;
        }

        let this = Arc::clone(self);
        let peer = peer.clone();
        tokio::spawn(async move {
            if let Err(error) = this.network.connect(&peer).await {
                // A peer that just went away is normal; anything else will be
                // retried on its next mDNS announcement.
                tracing::debug!(device = %peer.device_id, %error, "could not connect to a trusted peer");
            }
        });
    }

    async fn handle_connected(self: &Arc<Self>, session: PeerSession) {        let device_id = session.device_id;
        let trusted = self.is_trusted(device_id);

        tracing::info!(
            %device_id,
            name = %session.name,
            trusted,
            address = %session.address,
            "peer connected"
        );

        self.sessions.lock().insert(device_id, session);
        self.registry.lock().set_connected(device_id, true);
        if trusted {
            let _ = self.trust.write().touch(device_id);
        }

        self.emit_peers();
        self.emit_status();

        // If this connection was opened to ask for pairing, send the request
        // now that there is a session to send it on.
        let awaiting_pairing = { self.outgoing.lock().contains_key(&device_id) };
        if awaiting_pairing {
            if let Err(error) = self.send_pair_request(device_id).await {
                self.outgoing.lock().remove(&device_id);
                self.registry.lock().resolve_prompt(device_id);
                self.emit_pairing_requests();
                self.report(format!("could not send the pairing request: {error}"));
            }
        }
    }

    fn handle_disconnected(self: &Arc<Self>, device_id: DeviceId, reason: &str) {
        self.sessions.lock().remove(&device_id);
        self.incoming_images.lock().remove(&device_id);
        self.incoming_nonces.lock().remove(&device_id);
        self.registry.lock().set_connected(device_id, false);
        // A pairing that never completed is not worth keeping around.
        let abandoned = { self.outgoing.lock().remove(&device_id).is_some() };
        if abandoned {
            self.registry.lock().resolve_prompt(device_id);
            self.emit_pairing_requests();
        }

        tracing::info!(%device_id, reason, "peer disconnected");
        self.emit_peers();
        self.emit_status();
    }

    async fn send_pair_request(&self, device_id: DeviceId) -> Result<()> {
        let pending = self.outgoing.lock().get(&device_id).cloned();
        let Some(pending) = pending else {
            return Ok(());
        };

        let request = clipmesh_protocol::PairRequest::new(
            &self.identity.info(),
            self.identity.public_key().to_vec(),
            self.identity.certificate().to_der(),
            self.identity.fingerprint().to_hex(),
            pending.nonce,
        );

        self.network
            .send(device_id, Envelope::new(Payload::PairRequest(request)))
            .await
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    async fn handle_message(self: &Arc<Self>, device_id: DeviceId, envelope: Envelope) -> Result<()> {
        let kind = envelope.ensure_valid()?;
        let trusted = self.is_trusted(device_id);

        // An untrusted peer gets exactly one thing: the right to ask to pair.
        // Everything else is dropped, which is what stops a stranger on the
        // network from pushing content onto our clipboard.
        if !trusted && !matches!(kind, clipmesh_protocol::MessageKind::PairRequest) {
            tracing::debug!(%device_id, %kind, "ignored a message from an untrusted device");
            return Ok(());
        }

        match envelope.payload {
            Some(Payload::PairRequest(request)) => self.handle_pair_request(device_id, request),
            Some(Payload::PairAccept(accept)) => self.handle_pair_accept(device_id, accept).await,
            Some(Payload::ClipboardText(text)) => {
                let payload = TextPayload::from_proto(&text)?;
                self.accept_remote(device_id, ClipboardContent::Text(payload))
                    .await;
                Ok(())
            }
            Some(Payload::ClipboardImage(meta)) => {
                let meta = ImageMeta::from_proto(&meta)?;
                if meta.source_device == self.identity.device_id() {
                    return Ok(());
                }
                tracing::debug!(%device_id, id = %meta.id, size = meta.size, "receiving an image");
                self.incoming_images.lock().insert(
                    device_id,
                    IncomingImage {
                        buffer: Vec::with_capacity(meta.size.min(4 * 1024 * 1024) as usize),
                        meta,
                        started_at: Instant::now(),
                    },
                );
                Ok(())
            }
            Some(Payload::ImageChunk(chunk)) => {
                self.handle_image_chunk(device_id, chunk).await;
                Ok(())
            }
            Some(Payload::Ping(ping)) => {
                self.network
                    .send(device_id, Envelope::pong(ping.timestamp))
                    .await?;
                Ok(())
            }
            Some(Payload::Pong(_)) => Ok(()),
            Some(Payload::Error(error)) => {
                self.report(format!(
                    "peer {} reported {:?}: {}",
                    device_id,
                    error.code(),
                    error.message
                ));
                Ok(())
            }
            Some(Payload::Ack(ack)) => {
                tracing::debug!(%device_id, id = %ack.id, ok = ack.ok, "peer acknowledged");
                Ok(())
            }
            // The handshake messages belong to the network layer; seeing one
            // here means the peer sent it twice.
            Some(Payload::Hello(_)) | Some(Payload::HelloAck(_)) => Ok(()),
            None => Ok(()),
        }
    }

    fn handle_pair_request(&self, device_id: DeviceId, request: clipmesh_protocol::PairRequest) -> Result<()> {
        let session = self.sessions.lock().get(&device_id).cloned();
        let Some(session) = session else {
            return Err(CoreError::NotConnected(device_id));
        };

        // The request must describe the same device the TLS handshake proved.
        // Without this an attacker could ask us to pin *somebody else's*
        // certificate and then impersonate that device.
        if request.fingerprint != session.fingerprint.to_hex() {
            return Err(CoreError::Other(format!(
                "device {device_id} asked to pair with a fingerprint that is not the one its \
                 connection proved"
            )));
        }
        if request.public_key.as_slice() != session.public_key {
            return Err(CoreError::Other(format!(
                "device {device_id} asked to pair with a public key that is not the one its \
                 connection proved"
            )));
        }
        if request.device_id != device_id.to_string() {
            return Err(CoreError::Other(format!(
                "device {device_id} asked to pair under a different device id"
            )));
        }

        self.incoming_nonces
            .lock()
            .insert(device_id, request.nonce.clone());

        let prompt = PairingPrompt {
            device_id,
            name: if request.name.trim().is_empty() {
                session.name.clone()
            } else {
                request.name.clone()
            },
            platform: session.platform,
            fingerprint: session.fingerprint.to_grouped(),
            address: session.address.to_string(),
            requested_at: now_millis(),
            direction: PairingDirection::Incoming,
        };

        tracing::info!(%device_id, name = %prompt.name, "a device wants to pair");
        let raised = { self.registry.lock().raise_prompt(prompt) };
        if raised {
            self.emit_pairing_requests();
            self.emit_peers();
        }
        Ok(())
    }

    async fn handle_pair_accept(self: &Arc<Self>, device_id: DeviceId, accept: clipmesh_protocol::PairAccept) -> Result<()> {
        let pending = self.outgoing.lock().remove(&device_id);
        let Some(pending) = pending else {
            tracing::debug!(%device_id, "ignoring an unsolicited PairAccept");
            return Ok(());
        };

        self.registry.lock().resolve_prompt(device_id);
        self.emit_pairing_requests();
        self.emit_peers();

        if !accept.accepted {
            let reason = if accept.reason.is_empty() {
                "the other device declined".to_owned()
            } else {
                accept.reason.clone()
            };
            tracing::info!(%device_id, %reason, "pairing was declined");
            return Err(CoreError::PairingRejected {
                device: device_id,
                reason,
            });
        }

        verify_pair_accept(&accept, &pending.nonce, device_id)?;

        let session = self
            .sessions
            .lock()
            .get(&device_id)
            .cloned()
            .ok_or(CoreError::NotConnected(device_id))?;

        // The answer must come from the identity the connection already proved.
        if accept.public_key.as_slice() != session.public_key {
            return Err(CoreError::Other(format!(
                "device {device_id} accepted the pairing with a different public key than its \
                 connection proved"
            )));
        }
        if accept.fingerprint != session.fingerprint.to_hex() {
            return Err(CoreError::Other(format!(
                "device {device_id} accepted the pairing with a certificate we did not see"
            )));
        }

        let name = if accept.name.trim().is_empty() {
            session.name.clone()
        } else {
            accept.name.clone()
        };

        let device = TrustedDevice::new(
            name,
            accept.platform(),
            session.certificate.clone(),
            &session.public_key,
        )?;
        self.trust.write().trust(device)?;

        tracing::info!(%device_id, "pairing accepted; the device is now trusted");
        self.emit_trusted();
        self.emit_status();
        Ok(())
    }

    async fn handle_image_chunk(self: &Arc<Self>, device_id: DeviceId, chunk: ImageChunk) {
        let assembled = {
            let mut images = self.incoming_images.lock();
            let Some(state) = images.get_mut(&device_id) else {
                return;
            };

            if state.meta.id != chunk.id {
                tracing::debug!(%device_id, "dropping a chunk for a superseded image");
                return;
            }
            if chunk.offset != state.buffer.len() as u64 {
                tracing::warn!(
                    %device_id,
                    expected = state.buffer.len(),
                    got = chunk.offset,
                    "image chunk arrived out of order; dropping the transfer"
                );
                images.remove(&device_id);
                return;
            }
            if state.buffer.len() as u64 + chunk.data.len() as u64 > state.meta.size {
                tracing::warn!(%device_id, "image chunk overflows the declared size");
                images.remove(&device_id);
                return;
            }

            state.buffer.extend_from_slice(&chunk.data);
            let complete = chunk.last || state.buffer.len() as u64 == state.meta.size;
            if complete {
                images.remove(&device_id)
            } else {
                None
            }
        };

        let Some(state) = assembled else {
            return;
        };

        match ImagePayload::new(state.meta, state.buffer) {
            Ok(payload) => {
                self.accept_remote(device_id, ClipboardContent::Image(payload))
                    .await;
            }
            Err(error) => {
                self.report(format!(
                    "an image from {device_id} failed its integrity check: {error}"
                ));
            }
        }
    }

    /// Apply content that arrived from a peer.
    async fn accept_remote(self: &Arc<Self>, device_id: DeviceId, content: ClipboardContent) {
        // Our own payload coming back: drop it before touching anything.
        if content.source_device() == self.identity.device_id() {
            return;
        }
        if !self.dedup.lock().insert(content.id()) {
            tracing::trace!(%device_id, id = content.id(), "ignoring a duplicate payload");
            return;
        }

        if self.settings.read().sync_policy().accepts(&content) {
            // Writing to the clipboard will make the platform watcher fire as
            // if the user had copied it; remember the write so we do not send
            // it straight back.
            self.echo.lock().record_write(&content);
            if let Err(error) = self.clipboard.write(&content).await {
                self.report(format!("could not write to the clipboard: {error}"));
            }
        }

        let item = content.to_item();
        tracing::debug!(%device_id, id = item.id(), kind = %item.kind(), "applied remote content");
        self.record_history(item.clone());
        self.emit(CoreEvent::ClipboardReceived(Box::new(item)));
    }

    /// Handle a change to the local clipboard.
    async fn handle_local_change(self: &Arc<Self>, content: ClipboardContent) {
        if !self.is_running() {
            return;
        }

        // Was this change caused by our own write of remote content?
        if self.echo.lock().is_echo(&content) {
            tracing::trace!(id = content.id(), "suppressed an echo of our own clipboard write");
            return;
        }

        let policy = self.settings.read().sync_policy();
        if !policy.auto_sync || !policy.accepts(&content) {
            return;
        }

        self.dedup.lock().insert(content.id());
        let item = content.to_item();
        self.record_history(item.clone());

        match self.publish(&content).await {
            Ok(outcome) => {
                tracing::debug!(
                    id = item.id(),
                    kind = %item.kind(),
                    delivered = outcome.delivered,
                    "broadcast local content"
                );
                self.emit(CoreEvent::ClipboardSent {
                    item: Box::new(item),
                    delivered: outcome.delivered,
                });
            }
            Err(error) => self.report(format!("could not sync the clipboard: {error}")),
        }
    }

    /// Push content to every connected trusted peer.
    async fn publish(self: &Arc<Self>, content: &ClipboardContent) -> Result<SendOutcome> {
        let id = content.id().to_owned();

        let delivered = match content {
            ClipboardContent::Text(payload) => self
                .network
                .broadcast(Envelope::text(payload.to_proto()))
                .await?
                .len(),

            ClipboardContent::Image(payload) => {
                // Header first, then the pixels as raw binary chunks. A peer
                // that did not accept the header never gets the chunks.
                let targets = self
                    .network
                    .broadcast(Envelope::image(payload.meta.to_proto()))
                    .await?;

                let mut intact = vec![true; targets.len()];
                for (index, slice) in payload.data.chunks(IMAGE_CHUNK_BYTES).enumerate() {
                    let offset = (index * IMAGE_CHUNK_BYTES) as u64;
                    let last = offset + slice.len() as u64 >= payload.meta.size;
                    let envelope = Envelope::image_chunk(ImageChunk {
                        id: payload.meta.id.clone(),
                        offset,
                        data: slice.to_vec(),
                        last,
                    });

                    for (slot, peer) in targets.iter().enumerate() {
                        if !intact[slot] {
                            continue;
                        }
                        if self.network.send(*peer, envelope.clone()).await.is_err() {
                            intact[slot] = false;
                        }
                    }

                    if !intact.iter().any(|ok| *ok) {
                        break;
                    }
                }

                intact.iter().filter(|ok| **ok).count()
            }
        };

        Ok(SendOutcome { id, delivered })
    }

    // -----------------------------------------------------------------------
    // Emission helpers
    // -----------------------------------------------------------------------

    fn emit(&self, event: CoreEvent) {
        // A send error only means nobody is listening yet.
        let _ = self.events.send(event);
    }

    fn emit_status(&self) {
        self.emit(CoreEvent::Status(Box::new(self.status())));
    }

    fn emit_peers(&self) {
        self.emit(CoreEvent::Peers(self.peers()));
    }

    fn emit_trusted(&self) {
        self.emit(CoreEvent::Trusted(self.trusted_devices()));
    }

    fn emit_pairing_requests(&self) {
        self.emit(CoreEvent::PairingRequests(self.pairing_requests()));
    }

    fn emit_history(&self) {
        self.emit(CoreEvent::History(self.history()));
    }

    fn record_history(&self, item: ClipboardItem) {
        let changed = self.history.lock().push(item);
        if changed {
            self.emit_history();
        }
    }

    fn report(&self, message: String) {
        tracing::warn!(%message, "engine error");
        *self.last_error.lock() = Some(message.clone());
        self.emit(CoreEvent::Error(message));
        self.emit_status();
    }

    /// Drop expired pairing attempts and half-received images.
    ///
    /// Called from the network loop's idle path; exposed so the platform layer
    /// can also drive it from a timer when the network is quiet.
    pub fn prune_expired(&self) {
        let now = Instant::now();

        let stale_outgoing: Vec<DeviceId> = self
            .outgoing
            .lock()
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.started_at) > PAIRING_TIMEOUT)
            .map(|(device, _)| *device)
            .collect();
        for device in stale_outgoing {
            self.outgoing.lock().remove(&device);
            self.registry.lock().resolve_prompt(device);
            self.emit_pairing_requests();
        }

        self.incoming_images
            .lock()
            .retain(|_, image| now.duration_since(image.started_at) <= IMAGE_ASSEMBLY_TIMEOUT);

        let pruned = { self.registry.lock().prune() };
        if pruned {
            self.emit_peers();
            self.emit_status();
        }
    }
}

impl std::fmt::Debug for SyncManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncManager")
            .field("device_id", &self.identity.device_id())
            .field("running", &self.is_running())
            .field("trusted", &self.trust.read().len())
            .field("sessions", &self.sessions.lock().len())
            .finish_non_exhaustive()
    }
}

/// Convenience alias for the shared engine handle.
pub type SharedSyncManager = Arc<SyncManager>;
