//! Plain TCP: binding, connecting and port selection.
//!
//! Nothing here knows about TLS or clipboard content. It is deliberately the
//! dumbest layer in the crate so that the interesting security logic lives in
//! exactly one place ([`crate::connection`]).

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};

use crate::error::{NetworkError, Result};

/// The port ClipMesh prefers.
///
/// Purely a convenience: it makes the service recognisable in `netstat` output
/// and gives a stable hint for firewall rules. The authoritative port is always
/// the one advertised over mDNS.
pub const DEFAULT_PORT: u16 = 47711;

/// How long a TCP connect may take before we give up.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Bind a listener, preferring `preferred`.
///
/// Falls back to an ephemeral port when the preferred one is taken, because a
/// second ClipMesh instance (or an unrelated program) holding 47711 must not
/// stop the app from working - mDNS advertises whatever port we actually got.
///
/// # Errors
/// Returns [`NetworkError::Io`] when no listener can be bound at all.
pub async fn bind(preferred: u16) -> Result<(TcpListener, u16)> {
    match TcpListener::bind((Ipv4Addr::UNSPECIFIED, preferred)).await {
        Ok(listener) => {
            let port = listener.local_addr()?.port();
            Ok((listener, port))
        }
        Err(error) if error.kind() == ErrorKind::AddrInUse && preferred != 0 => {
            let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
            let port = listener.local_addr()?.port();
            tracing::warn!(
                preferred,
                port,
                "the preferred port is in use; advertising an ephemeral one instead"
            );
            Ok((listener, port))
        }
        Err(error) => Err(error.into()),
    }
}

/// Connect to a peer with a timeout.
///
/// `TCP_NODELAY` is on: clipboard payloads are small and latency sensitive, and
/// Nagle would add tens of milliseconds to every text sync for no benefit.
///
/// # Errors
/// Returns [`NetworkError::Transport`] on timeout or refusal.
pub async fn connect(address: SocketAddr) -> Result<TcpStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| NetworkError::Transport(format!("timed out connecting to {address}")))??;

    stream.set_nodelay(true)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binding_prefers_the_requested_port() {
        let (listener, port) = bind(DEFAULT_PORT).await.unwrap();
        assert!(port > 0);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn a_taken_port_falls_back_instead_of_failing() {
        let (first, first_port) = bind(DEFAULT_PORT).await.unwrap();

        let (_second, second_port) = bind(first_port).await.unwrap();
        assert_ne!(second_port, first_port, "must not fight over the port");

        drop(first);
    }

    #[tokio::test]
    async fn a_bound_listener_is_reachable() {
        let (listener, port) = bind(0).await.unwrap();
        let accept = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            stream
        });

        let client = connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .await
            .unwrap();
        let server = accept.await.unwrap();

        assert!(client.peer_addr().is_ok());
        assert!(server.peer_addr().is_ok());
    }

    #[tokio::test]
    async fn connecting_to_nothing_fails_fast() {
        // Port 1 on loopback is never listening; expect an error, not a hang.
        let result = connect(SocketAddr::from((Ipv4Addr::LOCALHOST, 1))).await;
        assert!(result.is_err());
    }
}
