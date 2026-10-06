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
    ImagePayload, MessageKind, Payload, Platform, TextPayload, IMAGE_CHUNK_BYTES,
};
use clipmesh_security::{build_pair_accept, random_challenge, verify_pair_accept};

use crate::device::DeviceRegistry;
use crate::error::{CoreError, Result};
use crate::event::{
    CoreEvent, PairingDirection, PairingPrompt, PeerView, StatusView, TrustedDeviceView,
};
use crate::provider::{
    ClipboardEvent, ClipboardProvider, ImageStore, NetworkEvent, NetworkProvider, PeerAddress,
    PeerSession,
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

/// How often the engine re-checks that it is connected to the trusted devices it
/// can see, and drops expired pairing attempts.
///
/// Five seconds is short enough that a peer which restarts is back before the
/// user notices, and long enough that a dial which is slow to fail cannot pile
/// up. See [`SyncManager::reconcile`].
const RECONCILE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Every how many reconcile ticks the *larger* device id also dials.
///
/// See [`SyncManager::reconcile`]: the tie-break normally lets only the smaller
/// id dial, and this is the fallback for when the other end never gets a fresh
/// discovery event. Six ticks is thirty seconds, by which point the smaller side
/// has had six chances - if any of them worked we are already connected and this
/// never fires.
const BACKUP_DIAL_EVERY: u64 = 6;

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
    /// Where to persist the clipboard history. `None` keeps it in memory only.
    pub history_path: Option<PathBuf>,
    /// Where the pixels of history images are kept.
    ///
    /// `None` disables them: the history still lists images, but nothing can
    /// preview, restore or re-send one.
    pub images: Option<Arc<dyn ImageStore>>,
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
    history_path: Option<PathBuf>,
    images: Option<Arc<dyn ImageStore>>,

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

        // A missing history file is not news; a damaged one is logged inside
        // `load` and treated as empty, because losing a list of copied items is
        // not the security event a damaged trust store would be.
        let history = match &options.history_path {
            Some(path) => History::load(path, history_capacity),
            None => History::new(history_capacity),
        };

        Arc::new(Self {
            identity: options.identity,
            trust: options.trust,
            clipboard: options.clipboard,
            network: options.network,
            settings: RwLock::new(options.settings),
            settings_path: options.settings_path,
            history_path: options.history_path,
            images: options.images,
            registry: Mutex::new(DeviceRegistry::default()),
            dedup: Mutex::new(DedupCache::default()),
            echo: Mutex::new(EchoSuppressor::default()),
            history: Mutex::new(history),
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

        let maintenance = Arc::clone(self);
        tokio::spawn(async move { maintenance.reconcile_loop().await });
    }

    /// Open the network: advertise over mDNS and accept connections.
    ///
    /// Idempotent.
    ///
    /// # Errors
    /// Returns [`CoreError::Network`] when discovery or the listener fails.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // Claim the flag *before* opening the network, and do it atomically so
        // two concurrent callers cannot both get through.
        //
        // The order matters: `maybe_connect` refuses to dial anything while this
        // is false, so flipping it afterwards meant a discovery event that
        // landed during `network.start()` was dropped - and nothing retried it.
        if self.running.swap(true, Ordering::Relaxed) {
            return Ok(());
        }

        if let Err(error) = self.network.start().await {
            // Roll the flag back so a retry is possible.
            self.running.store(false, Ordering::Relaxed);
            return Err(error.into());
        }

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

    /// Keep the session set honest, and expire stale pairing attempts.
    ///
    /// mDNS used to be the only thing that ever triggered a dial, and that is not
    /// enough: a peer which restarts while we are already running does not
    /// necessarily produce a fresh `ServiceResolved` here, because when the
    /// advertised record is unchanged mdns-sd has nothing new to report. The
    /// device then stayed offline - with the dial tie-break meaning we would not
    /// even try, if we happen to have the larger id - until the user restarted
    /// the engine by hand, which builds a new daemon and therefore does produce
    /// events.
    ///
    /// This closes that hole: whatever the reason a session went away, it comes
    /// back within one tick.
    async fn reconcile_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(RECONCILE_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick completes immediately, i.e. before the engine has been
        // started and before there is anything to reconcile.
        ticker.tick().await;

        let mut tick: u64 = 0;
        loop {
            ticker.tick().await;
            tick = tick.wrapping_add(1);
            self.reconcile(tick);
            // `prune_expired` documents itself as being driven from here; until
            // this loop existed nothing called it at all.
            self.prune_expired();
        }
    }

    /// Re-dial every trusted peer we can see but are not connected to.
    ///
    /// [`SyncManager::maybe_connect`] applies the dial tie-break, which on its own
    /// is not enough to get back online. The tie-break assumes the other end will
    /// notice us, and it may not: a peer that restarts while we are already
    /// running does not necessarily produce a fresh `ServiceResolved` here,
    /// because when its advertised record is unchanged mdns-sd has nothing new to
    /// report. The device then sat there offline until the engine was restarted by
    /// hand - that builds a new daemon, which unregisters and re-registers, and
    /// *that* finally looks like news to the other side.
    ///
    /// So the smaller id keeps dialling every tick, and the larger id dials every
    /// [`BACKUP_DIAL_EVERY`] ticks. The offset is what keeps this from breaking
    /// the tie-break: by the time the larger id acts, the smaller one has had
    /// several chances, and a successful dial shows up as `connected` here.
    fn reconcile(self: &Arc<Self>, tick: u64) {
        if !self.is_running() {
            return;
        }

        // Peers we are supposed to be talking to. Lock order matches `peers()`:
        // the trust store first, then the registry.
        let candidates: Vec<DeviceId> = {
            let trust = self.trust.read();
            self.registry
                .lock()
                .views(|device| trust.is_trusted(device))
                .into_iter()
                .filter(|view| view.trusted && !view.connected)
                .map(|view| view.device_id)
                .collect()
        };

        let backup_tick = tick % BACKUP_DIAL_EVERY == 0;

        for device in candidates {
            if self.identity.device_id() >= device && !backup_tick {
                continue;
            }

            let peer = { self.registry.lock().peer_of(device) };
            if let Some(peer) = peer {
                self.dial(&peer);
            }
        }
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

    /// Drop the stored error and tell the UI about it.
    ///
    /// `report` only ever writes an error; without this there is no way to
    /// acknowledge one, so a banner driven by [`StatusView::last_error`] could
    /// never be dismissed - every fresh snapshot carried the message straight
    /// back.
    pub fn clear_error(&self) -> StatusView {
        *self.last_error.lock() = None;
        let status = self.status();
        self.emit_status();
        status
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
                // A rebuilt list inherits nothing, so every cached preview
                // belongs to an entry that no longer exists.
                let dropped = {
                    let mut history = self.history.lock();
                    let dropped = history.clear();
                    *history = History::new(capacity);
                    dropped
                };
                self.forget_images(dropped);
                self.persist_history();
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

        self.send_explicit(content).await
    }

    /// Broadcast content that was already read from the clipboard.
    ///
    /// Split out of [`SyncManager::send_clipboard`] for the Android
    /// notification action: the read happens inside a transparent activity that
    /// holds focus for well under a second, and the result is handed to the
    /// engine; re-reading it here would fail, and doing it on the activity's
    /// behalf is the whole point of that activity.
    ///
    /// This is deliberately **not** `AndroidClipboardProvider::push`. A push
    /// emits a clipboard *change*, and `handle_local_change` drops changes while
    /// `autoSync` is off - so a notification button wired to
    /// `push` would silently do nothing for exactly the users who turned
    /// automatic sync off and therefore press it by hand. An explicit send is
    /// what the user asked for and goes out either way. What still applies is
    /// the content-kind policy: text and images can each be turned off.
    ///
    /// # Errors
    /// Returns [`CoreError::Clipboard`] when the user has turned this kind of
    /// content off in settings.
    pub async fn send_explicit(self: &Arc<Self>, content: ClipboardContent) -> Result<SendOutcome> {
        if !self.settings.read().sync_policy().accepts(&content) {
            return Err(CoreError::Clipboard(
                "this kind of content is turned off in settings".to_owned(),
            ));
        }

        self.dedup.lock().insert(content.id());
        self.record_history(&content);
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
        self.record_history(&content);
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
    /// Text is re-sent from the entry itself and images from the stored copy of
    /// their pixels, so both work long after the clipboard has moved on.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the entry is unknown or its pixels are gone.
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
                let content = self.restored_image(meta)?;
                self.dedup.lock().insert(content.id());
                self.publish(&content).await
            }
        }
    }

    /// Put a history entry back on the local clipboard without sending it.
    ///
    /// # Errors
    /// Returns [`CoreError`] when the entry is unknown, its pixels are gone or
    /// the clipboard is unavailable.
    pub async fn copy_history_item(&self, id: &str) -> Result<()> {
        let item = self
            .history
            .lock()
            .to_vec()
            .into_iter()
            .find(|item| item.id() == id)
            .ok_or_else(|| CoreError::Other(format!("no history entry with id {id}")))?;

        let content = match item {
            ClipboardItem::Text(payload) => ClipboardContent::Text(payload),
            ClipboardItem::Image(meta) => self.restored_image(meta)?,
        };

        // Writing to the clipboard makes the platform watcher fire as if the
        // user had copied it; remember the write so we do not broadcast a
        // restore as if it were a fresh copy.
        self.echo.lock().record_write(&content);
        self.clipboard.write(&content).await
    }

    /// Rebuild a history image from the pixels the store kept.
    ///
    /// The digest is recomputed from the bytes rather than read back: it is
    /// deliberately not written to `history.json` (32 numbers nobody displays,
    /// and meaningless in the UI), while a peer verifies the chunks it receives
    /// against it and would reject a restored image that carried zeroes.
    fn restored_image(&self, meta: ImageMeta) -> Result<ClipboardContent> {
        let data = self.stored_image(&meta)?;
        Ok(ClipboardContent::Image(ImagePayload::from_stored(
            meta, data,
        )))
    }

    /// Empty the history.
    ///
    /// The stored pixels of every entry go with it: leaving them behind would
    /// keep clipboard images on disk after the user asked for them to be
    /// forgotten.
    pub fn clear_history(&self) {
        let dropped = self.history.lock().clear();
        self.forget_images(dropped);
        self.persist_history();
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
        // The tie-break: on first sight only the smaller id dials, so two devices
        // that notice each other at the same moment do not open two connections.
        // The larger id is not stranded by this - see `reconcile`.
        if self.identity.device_id() >= peer.device_id {
            return;
        }

        self.dial(peer);
    }

    /// Dial a trusted peer unless we are already talking to it.
    ///
    /// Unlike [`SyncManager::maybe_connect`] this does not apply the dial
    /// tie-break, which is what lets [`SyncManager::reconcile`] act as a backstop
    /// once the smaller id has had its chances.
    fn dial(self: &Arc<Self>, peer: &PeerAddress) {
        if !self.is_running()
            || !self.is_trusted(peer.device_id)
            || self.network.is_connected(peer.device_id)
        {
            return;
        }

        let this = Arc::clone(self);
        let peer = peer.clone();
        tokio::spawn(async move {
            if let Err(error) = this.network.connect(&peer).await {
                // A peer that just went away is normal; anything else will be
                // retried on its next mDNS announcement or on the next
                // reconcile tick.
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

        // An untrusted peer may ask to pair, and may answer a request we made
        // ourselves. See `may_handle_from_untrusted` for why the second case has
        // to be let through.
        if !trusted
            && !may_handle_from_untrusted(kind, self.outgoing.lock().contains_key(&device_id))
        {
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
        self.record_history(&content);
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
        self.record_history(&content);

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

    /// Add content to the history, storing its pixels and releasing whatever it
    /// pushes out.
    ///
    /// The bytes are only available here: an image reaches the history at the
    /// one moment the engine holds its pixels, and the clipboard they came from
    /// may be overwritten a second later. Storing them before the history event
    /// is emitted also means the UI, which fetches a preview as soon as it sees
    /// a row, never races the write.
    fn record_history(&self, content: &ClipboardContent) {
        let image_path = match content {
            ClipboardContent::Image(payload) => self.store_image(&payload.meta.id, &payload.data),
            ClipboardContent::Text(_) => None,
        };

        let outcome = self.history.lock().push(content.to_item(), image_path);
        if !outcome.changed {
            return;
        }

        if let Some(evicted) = &outcome.evicted {
            self.forget_image(evicted.id());
        }

        self.persist_history();
        self.emit_history();
    }

    /// Hand the pixels of an image to the store, if there is one.
    ///
    /// Returns where they were put, relative to the state directory, which is
    /// what the history has to remember to find them again.
    fn store_image(&self, id: &str, png: &[u8]) -> Option<PathBuf> {
        let store = self.images.as_ref()?;
        store.put(id, png)
    }

    fn forget_image(&self, id: &str) {
        let Some(store) = &self.images else {
            return;
        };
        store.forget(id);
    }

    /// The stored pixels of a history image.
    ///
    /// # Errors
    /// Returns [`CoreError::Clipboard`] when this device has no image store or
    /// the picture is no longer in it - both are cases the user has to be told
    /// about, because the alternative is a silent no-op.
    fn stored_image(&self, meta: &ImageMeta) -> Result<Vec<u8>> {
        self.images
            .as_ref()
            .and_then(|store| store.get(&meta.id))
            .ok_or_else(|| {
                CoreError::Clipboard(
                    "the cached copy of that image is gone, so it cannot be used again".to_owned(),
                )
            })
    }

    /// Release the pixels of entries that are leaving the history.
    ///
    /// Only images ever had any, so text entries are skipped rather than asking
    /// the store to delete a file that was never written.
    fn forget_images(&self, dropped: Vec<ClipboardItem>) {
        for item in &dropped {
            if matches!(item, ClipboardItem::Image(_)) {
                self.forget_image(item.id());
            }
        }
    }

    /// Write the history out, if it is persisted at all.
    ///
    /// Synchronous, like the settings and trust writes: the file holds metadata
    /// for a few dozen entries, and handing it to a background task would let a
    /// save race the next mutation and persist a stale list.
    fn persist_history(&self) {
        let Some(path) = &self.history_path else {
            return;
        };

        if let Err(error) = self.history.lock().save(path) {
            tracing::warn!(
                %error,
                path = %path.display(),
                "could not persist the clipboard history"
            );
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
    /// Driven by [`SyncManager::reconcile_loop`]. It used to claim it was called
    /// from "the network loop's idle path", which does not exist - so this was
    /// dead code until the reconcile loop started calling it.
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
            // Tell the user. Without this the prompt simply disappears after two
            // minutes and nobody learns that the request was never answered.
            self.report(
                CoreError::Timeout("the other device to answer the pairing request").to_string(),
            );
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

/// Whether a message may be handled while the sender is still untrusted.
///
/// Exactly two things qualify:
///
/// * [`MessageKind::PairRequest`] - anyone may ask to pair; the user decides.
/// * [`MessageKind::PairAccept`] - but only while *we* have a request of our own
///   outstanding, because that is what it answers.
///
/// The second case is the subtle one. Pairing is mutually untrusted: when the
/// answer comes back, neither side has trusted the other yet. A plain
/// "the sender must be trusted" rule therefore drops the answer to our own
/// request, which is exactly what used to happen - a rejection looked like it had
/// never been sent, and the prompt sat there for the whole [`PAIRING_TIMEOUT`]
/// before vanishing without a word.
///
/// Letting one through is still safe, for two independent reasons:
/// [`SyncManager::handle_pair_accept`] ignores anything without a matching entry
/// in `outgoing`, and a positive answer carries an Ed25519 signature over the
/// nonce we generated, which a stranger cannot forge.
///
/// Everything else stays dropped, which is what stops a stranger on the network
/// from pushing content onto our clipboard.
#[must_use]
fn may_handle_from_untrusted(kind: MessageKind, has_outgoing_request: bool) -> bool {
    match kind {
        MessageKind::PairRequest => true,
        MessageKind::PairAccept => has_outgoing_request,
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use clipmesh_protocol::{ImageMeta, ImagePayload, TextPayload};
    use futures::stream::{self, BoxStream};

    /// A clipboard that holds whatever was last written to it.
    #[derive(Default)]
    struct FakeClipboard {
        content: Mutex<Option<ClipboardContent>>,
    }

    #[async_trait]
    impl ClipboardProvider for FakeClipboard {
        async fn read(&self) -> Result<Option<ClipboardContent>> {
            Ok(self.content.lock().clone())
        }

        async fn write(&self, content: &ClipboardContent) -> Result<()> {
            *self.content.lock() = Some(content.clone());
            Ok(())
        }

        fn watch(&self) -> BoxStream<'static, ClipboardEvent> {
            stream::empty().boxed()
        }
    }

    /// A network that goes nowhere.
    struct FakeNetwork;

    #[async_trait]
    impl NetworkProvider for FakeNetwork {
        async fn start(&self) -> Result<()> {
            Ok(())
        }

        async fn connect(&self, _peer: &PeerAddress) -> Result<()> {
            Ok(())
        }

        async fn send(&self, _device: DeviceId, _envelope: Envelope) -> Result<()> {
            Ok(())
        }

        async fn disconnect(&self, _device: DeviceId) -> Result<()> {
            Ok(())
        }

        async fn broadcast(&self, _envelope: Envelope) -> Result<Vec<DeviceId>> {
            Ok(Vec::new())
        }

        fn connected_peers(&self) -> Vec<DeviceId> {
            Vec::new()
        }

        fn events(&self) -> BoxStream<'static, NetworkEvent> {
            stream::empty().boxed()
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }
    }

    /// A store that writes real files under a state root, so that a "restart"
    /// in a test sees exactly what a restart on disk would see.
    struct FakeImageStore {
        root: PathBuf,
        forgets: Mutex<Vec<String>>,
    }

    impl FakeImageStore {
        fn new(root: impl Into<PathBuf>) -> Self {
            Self {
                root: root.into(),
                forgets: Mutex::new(Vec::new()),
            }
        }

        fn stored_path(&self, id: &str) -> PathBuf {
            self.root.join("images").join(format!("{id}.png"))
        }

        fn forgotten_ids(&self) -> Vec<String> {
            self.forgets.lock().clone()
        }
    }

    impl ImageStore for FakeImageStore {
        fn put(&self, id: &str, png: &[u8]) -> Option<PathBuf> {
            // `/` separated, like the real store: the path is written to
            // `history.json` and has to mean the same thing on every platform.
            let relative = PathBuf::from(format!("images/{id}.png"));
            let absolute = self.root.join(&relative);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(&absolute, png).unwrap();
            Some(relative)
        }

        fn get(&self, id: &str) -> Option<Vec<u8>> {
            std::fs::read(self.stored_path(id)).ok()
        }

        fn forget(&self, id: &str) {
            self.forgets.lock().push(id.to_owned());
            let _ = std::fs::remove_file(self.stored_path(id));
        }
    }

    /// An engine wired to fakes, with only the bits under test configured.
    fn build_engine(
        history_path: Option<PathBuf>,
        images: Option<Arc<dyn ImageStore>>,
        capacity: usize,
    ) -> Arc<SyncManager> {
        SyncManager::new(SyncManagerOptions {
            identity: Arc::new(DeviceIdentity::generate("Test device").unwrap()),
            trust: Arc::new(RwLock::new(TrustStore::in_memory())),
            clipboard: Arc::new(FakeClipboard::default()),
            network: Arc::new(FakeNetwork),
            settings: Settings {
                history_capacity: capacity,
                ..Settings::default()
            },
            settings_path: None,
            history_path,
            images,
        })
    }

    fn text_content(content: &str) -> ClipboardContent {
        ClipboardContent::Text(TextPayload::new_local(content, DeviceId::new()))
    }

    fn image_content(bytes: &[u8]) -> ClipboardContent {
        let meta = ImageMeta::new_local(DeviceId::new(), bytes, 8, 8);
        ClipboardContent::Image(ImagePayload::new(meta, bytes.to_vec()).unwrap())
    }

    fn contents(engine: &SyncManager) -> Vec<String> {
        engine
            .history()
            .into_iter()
            .map(|item| match item {
                ClipboardItem::Text(payload) => payload.content,
                ClipboardItem::Image(_) => panic!("only text entries were expected"),
            })
            .collect()
    }

    #[test]
    fn an_untrusted_peer_may_only_ask_to_pair_and_answer_our_own_request() {
        // Anyone may ask to pair; the user is the one who decides.
        assert!(may_handle_from_untrusted(MessageKind::PairRequest, false));
        assert!(may_handle_from_untrusted(MessageKind::PairRequest, true));

        // The answer to our own request has to get through even though neither
        // side trusts the other yet. Dropping it is what left a declined
        // pairing with no feedback on the requesting device.
        assert!(may_handle_from_untrusted(MessageKind::PairAccept, true));

        // ...but an unsolicited one is still ignored.
        assert!(!may_handle_from_untrusted(MessageKind::PairAccept, false));
    }

    #[test]
    fn nothing_else_crosses_the_trust_boundary() {
        // `has_outgoing_request` must not open the door for anything else: an
        // outstanding pairing request is not a reason to accept clipboard
        // traffic from a device the user never trusted.
        for kind in [
            MessageKind::Hello,
            MessageKind::HelloAck,
            MessageKind::ClipboardText,
            MessageKind::ClipboardImage,
            MessageKind::ImageChunk,
            MessageKind::Ack,
            MessageKind::Error,
            MessageKind::Ping,
            MessageKind::Pong,
        ] {
            assert!(!may_handle_from_untrusted(kind, false), "{kind:?}");
            assert!(!may_handle_from_untrusted(kind, true), "{kind:?}");
        }
    }

    #[test]
    fn a_none_history_path_stays_in_memory() {
        let directory = tempfile::tempdir().unwrap();
        let engine = build_engine(None, None, 50);

        engine.record_history(&text_content("only in memory"));

        assert_eq!(contents(&engine), vec!["only in memory"]);
        assert!(
            std::fs::read_dir(directory.path()).unwrap().next().is_none(),
            "nothing may be written when no path is configured"
        );
    }

    #[test]
    fn history_survives_a_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        {
            let first = build_engine(Some(path.clone()), None, 50);
            first.record_history(&text_content("first"));
            first.record_history(&text_content("second"));
            assert_eq!(contents(&first), vec!["second", "first"]);
        }

        let restarted = build_engine(Some(path), None, 50);
        assert_eq!(contents(&restarted), vec!["second", "first"]);
    }

    #[test]
    fn a_corrupt_history_file_is_replaced_by_the_next_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        std::fs::write(&path, "{ truncated").unwrap();

        let first = build_engine(Some(path.clone()), None, 50);
        assert!(
            first.history().is_empty(),
            "a damaged file must not stop the engine or invent entries"
        );

        first.record_history(&text_content("after the damage"));
        assert_eq!(contents(&first), vec!["after the damage"]);
        assert_eq!(
            contents(&build_engine(Some(path), None, 50)),
            vec!["after the damage"]
        );
    }

    #[test]
    fn clearing_the_history_empties_the_persisted_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");

        let engine = build_engine(Some(path.clone()), None, 50);
        engine.record_history(&text_content("gone"));
        engine.clear_history();

        assert!(engine.history().is_empty());
        assert!(build_engine(Some(path), None, 50).history().is_empty());
    }

    #[test]
    fn only_images_reach_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        engine.record_history(&text_content("no pixels"));
        assert!(std::fs::read_dir(directory.path()).unwrap().next().is_none());

        let image = image_content(&[1, 2, 3, 4]);
        engine.record_history(&image);

        assert_eq!(
            store.get(image.id()).as_deref(),
            Some(&[1u8, 2, 3, 4][..]),
            "the raw PNG bytes are handed over untouched"
        );
    }

    #[test]
    fn an_evicted_entry_takes_its_pixels_with_it() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 2);

        let first = image_content(&[1]);
        let first_id = first.id().to_owned();
        let second = image_content(&[2]);
        let second_id = second.id().to_owned();

        engine.record_history(&first);
        engine.record_history(&second);
        assert!(
            store.forgotten_ids().is_empty(),
            "a list that is not full yet evicts nothing"
        );

        engine.record_history(&text_content("pushes the first image out"));

        assert_eq!(store.forgotten_ids(), vec![first_id.clone()]);
        assert!(store.get(&first_id).is_none(), "the file is gone too");
        assert_eq!(engine.history().len(), 2);
        assert_eq!(
            store.get(&second_id).as_deref(),
            Some(&[2u8][..]),
            "the surviving image keeps its pixels"
        );
    }

    #[test]
    fn clearing_the_history_drops_every_stored_image_and_leaves_text_alone() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        let first = image_content(&[1]);
        let second = image_content(&[2]);
        engine.record_history(&first);
        engine.record_history(&text_content("no pixels to drop"));
        engine.record_history(&second);

        engine.clear_history();

        let mut forgotten = store.forgotten_ids();
        forgotten.sort();
        let mut expected = vec![first.id().to_owned(), second.id().to_owned()];
        expected.sort();
        assert_eq!(forgotten, expected);
        assert!(store.get(first.id()).is_none());
        assert!(store.get(second.id()).is_none());
    }

    #[test]
    fn an_image_survives_a_restart_with_its_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let store = Arc::new(FakeImageStore::new(directory.path()));

        let image = image_content(&[9, 8, 7]);
        let id = image.id().to_owned();
        {
            let engine = build_engine(
                Some(path.clone()),
                Some(Arc::clone(&store) as Arc<dyn ImageStore>),
                50,
            );
            engine.record_history(&image);
            engine.record_history(&text_content("after the image"));
        }

        let restarted = build_engine(
            Some(path),
            Some(Arc::clone(&store) as Arc<dyn ImageStore>),
            50,
        );
        let ids: Vec<String> = restarted
            .history()
            .into_iter()
            .map(|item| item.id().to_owned())
            .collect();
        assert_eq!(ids.len(), 2, "both entries come back");
        assert_eq!(ids[1], id, "the image is still listed, older than the text");
        assert_eq!(store.get(&id).as_deref(), Some(&[9u8, 8, 7][..]));
    }

    #[tokio::test]
    async fn an_image_in_the_history_can_be_put_back_on_the_clipboard() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        let image = image_content(&[1, 2, 3]);
        let id = image.id().to_owned();
        engine.record_history(&image);

        engine.copy_history_item(&id).await.unwrap();

        match engine.read_clipboard().await.unwrap() {
            Some(ClipboardContent::Image(payload)) => {
                assert_eq!(payload.meta.id, id);
                assert_eq!(payload.data, vec![1, 2, 3]);
                assert!(
                    payload.meta.verify(&payload.data).is_ok(),
                    "a restored image must carry a digest a peer can check"
                );
            }
            other => panic!("expected the image back on the clipboard, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_image_in_the_history_can_be_sent_again() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        let image = image_content(&[4, 5]);
        let id = image.id().to_owned();
        engine.record_history(&image);

        let outcome = engine.resend_history_item(&id).await.unwrap();
        assert_eq!(outcome.id, id);
    }

    #[tokio::test]
    async fn an_image_whose_pixels_are_gone_cannot_be_restored() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        let image = image_content(&[1]);
        let id = image.id().to_owned();
        engine.record_history(&image);
        std::fs::remove_file(store.stored_path(&id)).unwrap();

        assert!(engine.copy_history_item(&id).await.is_err());
        assert!(engine.resend_history_item(&id).await.is_err());
    }

    #[tokio::test]
    async fn without_a_store_an_image_cannot_be_restored() {
        // The engine stays usable with no store at all; it just cannot bring
        // pixels back, and says so instead of pretending.
        let engine = build_engine(None, None, 50);
        let image = image_content(&[1]);
        let id = image.id().to_owned();
        engine.record_history(&image);

        assert_eq!(engine.history().len(), 1);
        assert!(engine.copy_history_item(&id).await.is_err());
    }

    #[tokio::test]
    async fn changing_the_sync_policy_rebuilds_the_history_without_its_images() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(FakeImageStore::new(directory.path()));
        let engine = build_engine(None, Some(Arc::clone(&store) as Arc<dyn ImageStore>), 50);

        let image = image_content(&[1]);
        let image_id = image.id().to_owned();
        engine.record_history(&image);

        engine
            .update_settings(SettingsPatch {
                sync_images: Some(false),
                ..SettingsPatch::default()
            })
            .await
            .unwrap();

        assert!(engine.history().is_empty());
        assert_eq!(store.forgotten_ids(), vec![image_id.clone()]);
        assert!(store.get(&image_id).is_none());
    }

    #[tokio::test]
    async fn an_explicit_send_ignores_auto_sync() {
        // What the notification action does: the user pressed a button, so the
        // content goes out even though automatic sync is off.
        let engine = build_engine(None, None, 50);
        engine
            .update_settings(SettingsPatch {
                auto_sync: Some(false),
                ..SettingsPatch::default()
            })
            .await
            .unwrap();

        let outcome = engine
            .send_explicit(text_content("pressed by hand"))
            .await
            .unwrap();

        assert_eq!(outcome.delivered, 0, "the fake network goes nowhere");
        assert_eq!(contents(&engine), ["pressed by hand"]);
    }

    #[tokio::test]
    async fn an_explicit_send_still_honours_the_content_kinds() {
        let engine = build_engine(None, None, 50);
        engine
            .update_settings(SettingsPatch {
                sync_text: Some(false),
                ..SettingsPatch::default()
            })
            .await
            .unwrap();

        assert!(engine.send_explicit(text_content("nope")).await.is_err());
        assert!(engine.history().is_empty());
    }

    #[tokio::test]
    async fn a_pushed_change_is_dropped_when_auto_sync_is_off() {
        // The reason `send_explicit` exists rather than reusing the provider's
        // `push`: a change event is what the sync policy filters, and the
        // notification button must not be filtered.
        let engine = build_engine(None, None, 50);
        engine
            .update_settings(SettingsPatch {
                auto_sync: Some(false),
                ..SettingsPatch::default()
            })
            .await
            .unwrap();
        engine.start().await.unwrap();

        engine.handle_local_change(text_content("ignored")).await;

        assert!(engine.history().is_empty());
    }
}
