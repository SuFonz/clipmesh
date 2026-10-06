//! # clipmesh-network
//!
//! Everything that touches a socket: mDNS discovery, TCP, mutually
//! authenticated TLS 1.3, and the framing on top.
//!
//! ## The layers
//!
//! | module | responsibility |
//! | --- | --- |
//! | [`discovery`] | advertise this device, watch for others. Untrusted by design. |
//! | [`tcp`] | bind, connect, pick a port. |
//! | [`tls`] | turn the device identity into rustls configs; read the channel binding. |
//! | [`connection`] | one session: TLS, then the identity handshake, then frames. |
//! | [`packet`] | classify an envelope as handshake / keepalive / payload. |
//! | [`service`] | the [`clipmesh_core::NetworkProvider`] implementation. |
//!
//! ## Trust boundaries
//!
//! Nothing mDNS says is trusted. The fingerprint in the TXT record is a hint;
//! the real one comes from the certificate the peer presents during the TLS
//! handshake, and the device id is taken from that certificate rather than from
//! the broadcast. A peer that lies in its TXT record simply fails to connect.
//!
//! ```no_run
//! use std::sync::Arc;
//! use parking_lot::RwLock;
//! use clipmesh_core::NetworkProvider;
//! use clipmesh_identity::{DeviceIdentity, TrustStore};
//! use clipmesh_network::{NetworkConfig, NetworkService};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let identity = Arc::new(DeviceIdentity::generate("My Laptop")?);
//! let trust = Arc::new(RwLock::new(TrustStore::in_memory()));
//!
//! let network = NetworkService::new(identity, trust, NetworkConfig::default());
//! network.start().await?;
//! println!("listening on {}", network.listen_port());
//! # network.shutdown().await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod connection;
pub mod discovery;
pub mod error;
pub mod packet;
pub mod service;
pub mod tcp;
pub mod tls;

pub use connection::{
    EstablishedSession, HandshakeContext, Role, SessionHandle, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT,
};
pub use discovery::{DiscoveredService, Discovery, DiscoveryEvent, SERVICE_TYPE};
pub use error::{NetworkError, Result};
pub use packet::{classify, Incoming};
pub use service::{NetworkConfig, NetworkService};
pub use tcp::{bind, connect, DEFAULT_PORT};
pub use tls::{acceptor, channel_binding, connector, server_name};
