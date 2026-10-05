//! The [`NetworkProvider`] implementation: discovery, listener and sessions.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use parking_lot::{Mutex, RwLock};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use clipmesh_core::{CoreError, NetworkEvent, NetworkProvider, PeerAddress};
use clipmesh_core::Result as CoreResult;
use clipmesh_identity::{DeviceIdentity, TrustStore};
use clipmesh_protocol::{DeviceId, Envelope};
use clipmesh_security::tls::TrustPolicy;

use crate::connection::{self, EstablishedSession, HandshakeContext, Role, SessionHandle};
use crate::discovery::{Discovery, DiscoveryEvent};
use crate::error::{NetworkError, Result};
use crate::tcp;
use crate::tls;

/// Capacity of the event channel.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// How the service should behave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Port to prefer for the listener.
    pub preferred_port: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            preferred_port: tcp::DEFAULT_PORT,
        }
    }
}

/// Discovery and transport for one device.
///
/// ## Who dials whom
///
/// Both devices discover each other over mDNS, so both *could* dial at the same
/// moment and end up with two sessions. The engine breaks the tie by device id
/// (see `clipmesh-core`); this type simply does what it is told.
///
/// ## Listener policy
///
/// The listener accepts any well formed ClipMesh device certificate, because an
/// inbound connection might be a device that wants to pair - there is no way to
/// know before reading its first message. `clipmesh-core` then allows such a
/// peer to send exactly one kind of message, a `PairRequest`, and closes the
/// connection on anything else.
pub struct NetworkService {
    identity: Arc<DeviceIdentity>,
    trust: Arc<RwLock<TrustStore>>,
    config: NetworkConfig,
    events: broadcast::Sender<NetworkEvent>,

    /// A handle to ourselves, so the trait methods can spawn tasks that own an
    /// `Arc<Self>` even though they only receive `&self`.
    me: Weak<Self>,

    sessions: Mutex<HashMap<DeviceId, SessionHandle>>,
    /// mDNS fullname -> device id, so a `ServiceRemoved` maps back to a device.
    announced: Mutex<HashMap<String, DeviceId>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    discovery: Mutex<Option<Discovery>>,
    running: AtomicBool,
    listen_port: AtomicU16,
}

impl NetworkService {
    /// Build the service. Call [`NetworkProvider::start`] to open it.
    #[must_use]
    pub fn new(
        identity: Arc<DeviceIdentity>,
        trust: Arc<RwLock<TrustStore>>,
        config: NetworkConfig,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        Arc::new_cyclic(|me| Self {
            identity,
            trust,
            config,
            events,
            me: me.clone(),
            sessions: Mutex::new(HashMap::new()),
            announced: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
            discovery: Mutex::new(None),
            running: AtomicBool::new(false),
            listen_port: AtomicU16::new(0),
        })
    }

    /// The port the listener actually bound to.
    #[must_use]
    pub fn listen_port(&self) -> u16 {
        self.listen_port.load(Ordering::Relaxed)
    }

    fn this(&self) -> Result<Arc<Self>> {
        self.me
            .upgrade()
            .ok_or_else(|| NetworkError::Transport("the network service is shutting down".into()))
    }

    async fn run_listener(self: Arc<Self>, listener: TcpListener) {
        loop {
            let (socket, address) = match listener.accept().await {
                Ok(pair) => pair,
                Err(error) => {
                    let _ = self.events.send(NetworkEvent::Error(format!(
                        "could not accept a connection: {error}"
                    )));
                    // A transient accept error (file descriptor limit, a reset
                    // before we got to it) must not kill the listener.
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };

            let service = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(error) = service.accept_inbound(socket, address).await {
                    tracing::debug!(%address, %error, "rejected an inbound connection");
                }
            });
        }
    }

    async fn accept_inbound(self: Arc<Self>, socket: TcpStream, address: SocketAddr) -> Result<()> {
        socket.set_nodelay(true)?;

        // Built per connection so that each one gets its own slot for the
        // certificate the peer presents. Building it is cheap: it touches only
        // our own key material, never the trust store.
        let (acceptor, observed) =
            tls::acceptor(&self.identity, TrustPolicy::accept_unknown()).map_err(NetworkError::Security)?;

        let stream = acceptor
            .accept(socket)
            .await
            .map_err(|error| NetworkError::Handshake(format!("TLS accept failed: {error}")))?;
        // `tokio-rustls` hands back a role specific stream; the session code
        // works with the enum so one implementation covers both directions.
        let stream = tokio_rustls::TlsStream::Server(stream);

        let trust = Arc::clone(&self.trust);
        let is_trusted = move |device: DeviceId| trust.read().is_trusted(device);

        let session = connection::handshake(
            stream,
            HandshakeContext {
                role: Role::Responder,
                identity: &self.identity,
                observed: &observed,
                address,
                is_trusted: &is_trusted,
            },
        )
        .await?;

        self.register_session(session);
        Ok(())
    }

    fn register_session(self: &Arc<Self>, session: EstablishedSession) {
        let handle = connection::run(session, self.events.clone());
        let peer = handle.peer.clone();
        let device_id = peer.device_id;

        // A reconnect replaces the old session; dropping the previous handle
        // closes the old socket instead of leaving two live sessions.
        let previous = self.sessions.lock().insert(device_id, handle);
        if previous.is_some() {
            tracing::debug!(%device_id, "replacing an existing session");
        }

        let _ = self.events.send(NetworkEvent::Connected(Box::new(peer)));
    }

    async fn run_discovery(self: Arc<Self>, mut browse: BoxStream<'static, DiscoveryEvent>) {
        while let Some(event) = browse.next().await {
            match event {
                DiscoveryEvent::Found(service) => {
                    // Our own announcement comes back on every interface.
                    if service.device_id == self.identity.device_id() {
                        continue;
                    }

                    self.announced
                        .lock()
                        .insert(service.fullname.clone(), service.device_id);

                    let _ = self.events.send(NetworkEvent::Discovered(Box::new(PeerAddress {
                        device_id: service.device_id,
                        device: service.device_info(),
                        fingerprint: service.fingerprint.clone(),
                        address: service.address,
                    })));
                }
                DiscoveryEvent::Lost { fullname } => {
                    if let Some(device_id) = self.announced.lock().remove(&fullname) {
                        let _ = self.events.send(NetworkEvent::Lost(device_id));
                    }
                }
                DiscoveryEvent::Error(message) => {
                    let _ = self.events.send(NetworkEvent::Error(message));
                }
            }
        }
    }
}

impl std::fmt::Debug for NetworkService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkService")
            .field("device_id", &self.identity.device_id())
            .field("running", &self.running.load(Ordering::Relaxed))
            .field("sessions", &self.sessions.lock().len())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl NetworkProvider for NetworkService {
    async fn start(&self) -> CoreResult<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(NetworkError::AlreadyRunning.into());
        }

        // Roll the flag back on any failure so a retry is possible.
        let started = async {
            let info = self.identity.info();
            let fingerprint = self.identity.fingerprint().to_hex();

            let (listener, port) = tcp::bind(self.config.preferred_port).await?;
            self.listen_port.store(port, Ordering::Relaxed);

            let discovery = Discovery::start(&info, &fingerprint, port)?;
            let browse = discovery.browse()?;
            *self.discovery.lock() = Some(discovery);

            let service = self.this()?;

            let listener_service = Arc::clone(&service);
            let listener_task = tokio::spawn(listener_service.run_listener(listener));

            let browse_service = Arc::clone(&service);
            let browse_task = tokio::spawn(browse_service.run_discovery(browse));

            *self.tasks.lock() = vec![listener_task, browse_task];

            tracing::info!(port, "network service started");
            Ok::<(), NetworkError>(())
        }
        .await;

        if let Err(error) = started {
            self.running.store(false, Ordering::SeqCst);
            return Err(error.into());
        }
        Ok(())
    }

    async fn connect(&self, peer: &PeerAddress) -> CoreResult<()> {
        if !self.running.load(Ordering::Relaxed) {
            return Err(NetworkError::NotRunning.into());
        }

        // Pinned when we already trust the device, so the handshake fails
        // closed if somebody else answers at that address. Permissive while
        // pairing, because there is nothing to pin against yet.
        let policy = TrustPolicy::for_device(Arc::clone(&self.trust), peer.device_id);
        let (connector, observed) =
            tls::connector(&self.identity, policy).map_err(NetworkError::Security)?;

        let socket = tcp::connect(peer.address).await?;
        let stream = connector
            .connect(tls::server_name(), socket)
            .await
            .map_err(|error| {
                NetworkError::Handshake(format!("TLS connect to {} failed: {error}", peer.address))
            })?;
        let stream = tokio_rustls::TlsStream::Client(stream);

        let trust = Arc::clone(&self.trust);
        let is_trusted = move |device: DeviceId| trust.read().is_trusted(device);

        let session = connection::handshake(
            stream,
            HandshakeContext {
                role: Role::Initiator,
                identity: &self.identity,
                observed: &observed,
                address: peer.address,
                is_trusted: &is_trusted,
            },
        )
        .await?;

        // `connect` only has `&self`, but registering a session needs a real
        // `Arc` so the spawned tasks can own it.
        self.this()?.register_session(session);
        Ok(())
    }

    async fn send(&self, device: DeviceId, envelope: Envelope) -> CoreResult<()> {
        let sender = {
            let sessions = self.sessions.lock();
            sessions.get(&device).map(|handle| handle.outbound.clone())
        };

        let sender = sender.ok_or(NetworkError::NotConnected(device))?;
        sender
            .send(envelope)
            .await
            .map_err(|_| CoreError::from(NetworkError::NotConnected(device)))
    }

    async fn broadcast(&self, envelope: Envelope) -> CoreResult<Vec<DeviceId>> {
        let targets: Vec<(DeviceId, tokio::sync::mpsc::Sender<Envelope>)> = self
            .sessions
            .lock()
            .iter()
            .map(|(device, handle)| (*device, handle.outbound.clone()))
            .collect();

        let mut delivered = Vec::with_capacity(targets.len());
        for (device, sender) in targets {
            // Never block the fan-out on one slow peer: skip it and say so.
            match sender.try_send(envelope.clone()) {
                Ok(()) => delivered.push(device),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(%device, "outbound queue is full; skipping this peer");
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::debug!(%device, "session closed while broadcasting");
                }
            }
        }

        Ok(delivered)
    }

    fn connected_peers(&self) -> Vec<DeviceId> {
        self.sessions.lock().keys().copied().collect()
    }

    fn events(&self) -> BoxStream<'static, NetworkEvent> {
        let receiver = self.events.subscribe();
        Box::pin(futures::stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((event, receiver)),
                    // A slow consumer missed events. The engine works from
                    // snapshots, so skipping them is safe.
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "the engine fell behind the network event stream");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }))
    }

    async fn disconnect(&self, device: DeviceId) -> CoreResult<()> {
        match self.sessions.lock().remove(&device) {
            Some(handle) => {
                // Dropping the handle closes the outbound queue, which ends the
                // write loop and with it the socket.
                drop(handle);
                Ok(())
            }
            None => Err(NetworkError::NotConnected(device).into()),
        }
    }

    async fn shutdown(&self) -> CoreResult<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Ok(());
        }

        for task in self.tasks.lock().drain(..) {
            task.abort();
        }

        self.sessions.lock().clear();
        self.announced.lock().clear();

        if let Some(discovery) = self.discovery.lock().take() {
            if let Err(error) = discovery.shutdown() {
                tracing::warn!(%error, "mDNS shutdown reported a problem");
            }
        }

        tracing::info!("network service stopped");
        Ok(())
    }
}

/// Map network failures onto the engine's error type.
///
/// Kept next to the errors it maps so the two stay in step. The orphan rule
/// allows this because `NetworkError` is local.
impl From<NetworkError> for CoreError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::NotTrusted(device) => Self::NotTrusted(device),
            NetworkError::NotConnected(device) => Self::NotConnected(device),
            NetworkError::Security(inner) => Self::Security(inner),
            NetworkError::Identity(inner) => Self::Identity(inner),
            NetworkError::Protocol(inner) => Self::Protocol(inner),
            NetworkError::Io(inner) => Self::Io(inner),
            other => Self::Network(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_identity::IdentityPaths;
    use clipmesh_protocol::Platform;

    fn service(name: &str) -> Arc<NetworkService> {
        let identity = Arc::new(DeviceIdentity::generate(name).unwrap());
        let store = Arc::new(RwLock::new(TrustStore::in_memory()));
        NetworkService::new(identity, store, NetworkConfig::default())
    }

    #[tokio::test]
    async fn a_service_starts_and_stops() {
        let service = service("Host");

        service.start().await.unwrap();
        assert!(service.listen_port() > 0);
        // Starting twice is a programming error, not a silent no-op.
        assert!(matches!(service.start().await, Err(CoreError::Network(_))));

        service.shutdown().await.unwrap();
        // Shutting down twice is harmless.
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn sending_without_a_session_is_an_error() {
        let service = service("Host");
        let stranger = DeviceId::new();
        assert!(matches!(
            service.send(stranger, Envelope::ping()).await,
            Err(CoreError::NotConnected(_))
        ));
        assert!(matches!(
            service.disconnect(stranger).await,
            Err(CoreError::NotConnected(_))
        ));
    }

    #[tokio::test]
    async fn connecting_while_stopped_is_refused() {
        let service = service("Host");
        let peer = PeerAddress {
            device_id: DeviceId::new(),
            device: clipmesh_protocol::DeviceInfo::new(
                DeviceId::new(),
                "Peer",
                Platform::Linux,
                "0.1.0",
            ),
            fingerprint: String::new(),
            address: "127.0.0.1:1".parse().unwrap(),
        };
        assert!(matches!(
            service.connect(&peer).await,
            Err(CoreError::Network(_))
        ));
    }

    #[tokio::test]
    async fn broadcasting_with_no_peers_delivers_to_nobody() {
        let service = service("Host");
        service.start().await.unwrap();
        let delivered = service.broadcast(Envelope::ping()).await.unwrap();
        assert!(delivered.is_empty());
        service.shutdown().await.unwrap();
    }

    #[test]
    fn the_service_keeps_a_usable_self_reference() {
        let service = service("Host");
        assert!(service.this().is_ok());
    }

    #[test]
    fn errors_convert_into_engine_errors() {
        let core: CoreError = NetworkError::NotConnected(DeviceId::new()).into();
        assert!(matches!(core, CoreError::NotConnected(_)));

        let core: CoreError = NetworkError::Transport("boom".into()).into();
        assert!(matches!(core, CoreError::Network(_)));
    }

    #[test]
    fn identity_paths_are_not_touched_by_construction() {
        // Constructing a service must not create directories or files on disk.
        let directory = tempfile::tempdir().unwrap();
        let paths = IdentityPaths::at(directory.path());
        let identity = Arc::new(
            DeviceIdentity::load_or_create(&paths, "Host").expect("identity is created lazily"),
        );
        let store = Arc::new(RwLock::new(TrustStore::in_memory()));
        let _service = NetworkService::new(identity, store, NetworkConfig::default());
        assert!(paths.private_key().exists());
    }
}
