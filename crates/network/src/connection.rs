//! One authenticated session: TLS, then the identity handshake, then frames.
//!
//! ## The handshake, step by step
//!
//! ```text
//!        initiator                                responder
//!            │  TCP + TLS 1.3 (mutual auth)           │
//!            ├──────────── Hello(challenge_A) ───────►│
//!            │◄─────────── Hello(challenge_B) ─────────┤
//!            │  verify against the TLS certificate     │
//!            ├──────── HelloAck(sig over B, binding) ─►│
//!            │◄─────── HelloAck(sig over A, binding) ──┤
//!            │            session is Active            │
//! ```
//!
//! Both sides send their `Hello` before reading, so neither can deadlock
//! waiting for the other to speak first.
//!
//! The signature covers `challenge ‖ TLS exporter secret`. The challenge makes
//! it fresh; the exporter makes it non-transferable, because only the two ends
//! of *this* TLS connection can derive that secret. A man in the middle who
//! forwards the byte stream therefore cannot get an honest `Hello` to verify on
//! their own connection.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncWriteExt as _, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_rustls::TlsStream;

use clipmesh_core::{NetworkEvent, PeerSession};
use clipmesh_identity::{DeviceCertificate, DeviceIdentity};
use clipmesh_protocol::{
    now_millis, read_envelope, write_envelope, DeviceId, Envelope, MessageKind, Payload,
};
use clipmesh_security::session::{
    build_hello, build_hello_ack, verify_hello, verify_hello_ack, HandshakeChallenge,
};
use clipmesh_security::tls::ObservedCertificate;

use crate::error::{NetworkError, Result};
use crate::packet::{classify, Incoming};
use crate::tls::channel_binding;

/// How long the identity handshake may take once TLS is up.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a session may be completely silent before we close it.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(180);

/// How often we ping a peer we have not heard from.
pub const PING_AFTER: Duration = Duration::from_secs(45);

/// Capacity of a session's outbound queue.
///
/// Big enough to hold a whole 4K image in flight (160 chunks) without the
/// sender blocking, small enough that a peer which stops reading cannot make us
/// buffer without bound.
pub const OUTBOUND_CAPACITY: usize = 256;

/// A TLS stream to a peer.
pub type SessionStream = TlsStream<TcpStream>;

/// Which side of the connection we are on. Only affects which helper reads the
/// channel binding; the handshake itself is symmetric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// We dialled.
    Initiator,
    /// We accepted.
    Responder,
}

/// Everything the handshake needs besides the socket.
pub struct HandshakeContext<'a> {
    /// Which side we are.
    pub role: Role,
    /// Our identity.
    pub identity: &'a Arc<DeviceIdentity>,
    /// The certificate slot the TLS verifier filled in.
    pub observed: &'a ObservedCertificate,
    /// The peer's address, for logs and for the session record.
    pub address: SocketAddr,
    /// Whether the trust store knows this device.
    pub is_trusted: &'a (dyn Fn(DeviceId) -> bool + Send + Sync),
}

/// A session that passed both handshakes.
pub struct EstablishedSession {
    /// Who the peer proved to be.
    pub peer: PeerSession,
    /// The ready stream.
    pub stream: SessionStream,
}

/// Run the TLS *and* identity handshakes.
///
/// TLS is assumed to have completed already; this function performs the
/// application level exchange and is where a peer that cannot prove its
/// identity is rejected.
///
/// # Errors
/// Returns [`NetworkError::Handshake`] on any identity failure, or
/// [`NetworkError::Security`] if the channel binding cannot be derived.
pub async fn handshake(
    stream: SessionStream,
    context: HandshakeContext<'_>,
) -> Result<EstablishedSession> {
    let HandshakeContext {
        role,
        identity,
        observed,
        address,
        is_trusted,
    } = context;

    tracing::debug!(?role, "starting the identity handshake");
    let channel_binding = channel_binding(&stream)?;

    let certificate_der = observed
        .get()
        .ok_or_else(|| NetworkError::Handshake("the TLS layer captured no peer certificate".into()))?;

    let certificate = DeviceCertificate::from_der(certificate_der.as_ref().to_vec())?;

    tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        exchange(stream, identity, certificate, channel_binding, address, is_trusted),
    )
    .await
    .map_err(|_| NetworkError::Handshake("the peer did not finish the identity handshake in time".into()))?
}

async fn exchange(
    mut stream: SessionStream,
    identity: &Arc<DeviceIdentity>,
    certificate: DeviceCertificate,
    channel_binding: [u8; clipmesh_security::CHALLENGE_LENGTH],
    address: SocketAddr,
    is_trusted: &(dyn Fn(DeviceId) -> bool + Send + Sync),
) -> Result<EstablishedSession> {
    // The handshake is strictly sequential on both sides, so one mutable
    // borrow of the stream is enough - no split, no reassembly.
    //
    // 1. Both sides speak first, so neither waits on the other.
    let our_challenge = HandshakeChallenge::generate();
    let hello = build_hello(identity, &our_challenge, &channel_binding)?;
    write_envelope(&mut stream, &Envelope::new(Payload::Hello(hello))).await?;

    // 2. Verify the peer against the certificate it presented over TLS.
    let peer_hello = expect(&mut stream, MessageKind::Hello).await?;
    let Some(Payload::Hello(peer_hello)) = peer_hello.payload else {
        return Err(NetworkError::Handshake("expected a Hello".into()));
    };
    let peer = verify_hello(&peer_hello, &channel_binding, &certificate)?;

    // 3. Answer the peer's challenge.
    let trusted = is_trusted(peer.device_id);
    let ack = build_hello_ack(identity, &peer.challenge, &channel_binding, trusted);
    write_envelope(&mut stream, &Envelope::new(Payload::HelloAck(ack))).await?;

    // 4. Check the peer's answer to ours.
    let peer_ack = expect(&mut stream, MessageKind::HelloAck).await?;
    let Some(Payload::HelloAck(peer_ack)) = peer_ack.payload else {
        return Err(NetworkError::Handshake("expected a HelloAck".into()));
    };
    verify_hello_ack(
        &peer_ack,
        &our_challenge,
        &channel_binding,
        &peer.public_key,
        peer.device_id,
    )?;

    tracing::info!(
        device = %peer.device_id,
        name = %peer.name,
        trusted,
        "identity handshake complete"
    );

    let session = PeerSession {
        device_id: peer.device_id,
        name: peer.name.clone(),
        platform: peer.platform,
        address,
        trusted,
        since_millis: now_millis(),
        public_key: peer.public_key,
        fingerprint: peer.fingerprint,
        certificate: peer.certificate,
    };

    Ok(EstablishedSession { peer: session, stream })
}

/// Read envelopes until one of `expected` arrives.
async fn expect<S>(stream: &mut S, expected: MessageKind) -> Result<Envelope>
where
    S: tokio::io::AsyncRead + Unpin,
{
    loop {
        let envelope = read_envelope(stream).await?.ok_or_else(|| {
            NetworkError::Handshake(format!(
                "the peer closed the connection before sending {expected}"
            ))
        })?;

        match classify(&envelope) {
            Ok(Incoming::KeepAlive) => continue,
            Ok(Incoming::Handshake(kind)) if kind == expected => return Ok(envelope),
            Ok(other) => {
                return Err(NetworkError::Handshake(format!(
                    "expected {expected}, got {}",
                    other.kind()
                )))
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// A running session: where to send, and who it is.
#[derive(Debug)]
pub struct SessionHandle {
    /// Who the peer is.
    pub peer: PeerSession,
    /// Queue an envelope for delivery. Dropping the handle closes the session.
    pub outbound: mpsc::Sender<Envelope>,
    /// The task running the write half.
    ///
    /// A handle can outlive its socket: a session that ends on its own - the peer
    /// was killed, or its process restarted - leaves the entry behind, because
    /// nothing but `disconnect` and `shutdown` ever removes one. Keeping the task
    /// around gives the service a way to tell "still talking" from "long gone",
    /// so `is_connected` stops claiming a peer that is not there.
    pub(crate) writer: JoinHandle<()>,
}

/// Spawn the read and write loops for a session.
///
/// The returned handle is how the service sends; the tasks end on their own
/// when the peer goes away or the handle is dropped.
#[must_use]
pub fn run(
    session: EstablishedSession,
    events: broadcast::Sender<NetworkEvent>,
) -> SessionHandle {
    let EstablishedSession { peer, stream } = session;
    let device_id = peer.device_id;
    let session_info = peer.clone();

    let (reader, writer) = tokio::io::split(stream);
    let (outbound, outbound_rx) = mpsc::channel(OUTBOUND_CAPACITY);

    let writer = tokio::spawn(write_loop(writer, outbound_rx, device_id, events.clone()));
    tokio::spawn(read_loop(
        reader,
        outbound.clone(),
        device_id,
        events.clone(),
    ));

    SessionHandle {
        peer: session_info,
        outbound,
        writer,
    }
}
async fn write_loop(
    mut writer: WriteHalf<SessionStream>,
    mut queue: mpsc::Receiver<Envelope>,
    device_id: DeviceId,
    events: broadcast::Sender<NetworkEvent>,
) {
    let mut ping_timer = tokio::time::interval(PING_AFTER);
    ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick fires immediately; skip it so a fresh session is not
    // pinged before it has said anything.
    ping_timer.tick().await;

    loop {
        let envelope = tokio::select! {
            maybe = queue.recv() => match maybe {
                Some(envelope) => envelope,
                None => break,
            },
            _ = ping_timer.tick() => Envelope::ping(),
        };

        if let Err(error) = write_envelope(&mut writer, &envelope).await {
            tracing::debug!(%device_id, %error, "write failed; ending the session");
            break;
        }
    }

    // Drain what is left so the peer sees a clean shutdown rather than a reset.
    let _ = writer.shutdown().await;
    let _ = events.send(NetworkEvent::Disconnected {
        device: device_id,
        reason: "the session closed".into(),
    });
}

async fn read_loop(
    mut reader: ReadHalf<SessionStream>,
    outbound: mpsc::Sender<Envelope>,
    device_id: DeviceId,
    events: broadcast::Sender<NetworkEvent>,
) {
    let mut reason = "the peer closed the connection".to_owned();

    loop {
        match tokio::time::timeout(IDLE_TIMEOUT, read_envelope(&mut reader)).await {
            Err(_) => {
                reason = format!("no traffic for {} seconds", IDLE_TIMEOUT.as_secs());
                break;
            }
            Ok(Ok(None)) => break,
            Ok(Err(error)) => {
                reason = format!("protocol error: {error}");
                break;
            }
            Ok(Ok(Some(envelope))) => {
                match classify(&envelope) {
                    Ok(Incoming::KeepAlive) => {
                        // Answer a ping; a pong needs no reply.
                        if let Some(Payload::Ping(ping)) = &envelope.payload {
                            let _ = outbound.send(Envelope::pong(ping.timestamp)).await;
                        }
                        continue;
                    }
                    Ok(Incoming::PeerError) => {
                        if let Some(Payload::Error(error)) = &envelope.payload {
                            tracing::warn!(%device_id, code = ?error.code(), message = %error.message, "peer reported an error");
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%device_id, %error, "dropping an unusable frame");
                        continue;
                    }
                }

                if events
                    .send(NetworkEvent::Message {
                        device: device_id,
                        envelope: Box::new(envelope),
                    })
                    .is_err()
                {
                    // Nobody is listening any more; the app is shutting down.
                    reason = "the engine stopped listening".to_owned();
                    break;
                }
            }
        }
    }

    tracing::debug!(%device_id, %reason, "session ended");
    let _ = events.send(NetworkEvent::Disconnected {
        device: device_id,
        reason,
    });
}

/// Answer a peer's ping outside a session loop. Used by the loopback test.
#[doc(hidden)]
#[must_use]
pub fn pong_for(envelope: &Envelope) -> Option<Envelope> {
    match &envelope.payload {
        Some(Payload::Ping(ping)) => Some(Envelope::pong(ping.timestamp)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pings_get_pongs() {
        let ping = Envelope::ping();
        let pong = pong_for(&ping).unwrap();
        assert_eq!(classify(&pong).unwrap(), Incoming::KeepAlive);
        assert!(pong_for(&Envelope::pong(1)).is_none());
    }

    #[test]
    fn the_queue_is_big_enough_for_a_4k_screenshot() {
        // 10 MiB / 64 KiB = 160 chunks, plus the header.
        assert!(OUTBOUND_CAPACITY > 161);
    }
}
