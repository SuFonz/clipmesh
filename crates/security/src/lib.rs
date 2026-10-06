//! # clipmesh-security
//!
//! Transport security for ClipMesh. Two jobs:
//!
//! 1. **Confidential, mutually authenticated transport.** [`tls::server_config`]
//!    and [`tls::client_config`] build rustls 1.3 configurations that accept
//!    only ClipMesh device certificates, optionally pinned to the trust store.
//! 2. **Proof of identity at the application layer.** [`session`] carries the
//!    `Hello` / `HelloAck` exchange whose signatures are bound to the TLS
//!    connection through the exporter secret, which is what makes a
//!    man-in-the-middle unable to relay an honest handshake.
//!
//! ```no_run
//! use std::sync::Arc;
//! use parking_lot::RwLock;
//! use clipmesh_identity::{DeviceIdentity, TrustStore};
//! use clipmesh_security::tls::{self, TrustPolicy};
//!
//! let identity = DeviceIdentity::generate("My Laptop").unwrap();
//! let store = Arc::new(RwLock::new(TrustStore::in_memory()));
//!
//! // Listener: accept unknown devices, they may only ask to pair.
//! let (_server, _observed) = tls::server_config(&identity, TrustPolicy::accept_unknown()).unwrap();
//!
//! // Dialler: fail the handshake unless the peer is the device we asked for.
//! let peer = clipmesh_protocol::DeviceId::new();
//! let policy = TrustPolicy::for_device(store, peer);
//! let (_client, _observed) = tls::client_config(&identity, policy).unwrap();
//! ```

#![warn(missing_docs)]

pub mod crypto;
pub mod error;
pub mod session;
pub mod tls;

pub use crypto::{
    pair_accept_signing_input, random_challenge, CHALLENGE_LENGTH, CHANNEL_BINDING_CONTEXT,
    CHANNEL_BINDING_LABEL,
};
pub use error::{Result, SecurityError};
pub use session::{
    build_hello, build_hello_ack, build_pair_accept, verify_hello, verify_hello_ack,
    verify_pair_accept, HandshakeChallenge, PeerIdentity, SessionState, SessionStateMachine,
};
pub use tls::{client_config, identify_device, server_config, ObservedCertificate, TrustPolicy};
