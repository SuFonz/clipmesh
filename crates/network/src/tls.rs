//! Turning this device's identity into rustls configurations, and reading the
//! channel binding out of an established stream.
//!
//! The policy decisions live in `clipmesh-security`; this module only wires
//! them to `tokio-rustls` and knows how to get the exporter secret, which is
//! what ties the application level handshake to *this* connection.

use clipmesh_identity::DeviceIdentity;
use clipmesh_security::tls::{self, ObservedCertificate, TrustPolicy};
use clipmesh_security::{Result as SecurityResult, SecurityError};
use rustls_pki_types::ServerName;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::{NetworkError, Result};

/// The name presented in the SNI field.
///
/// It is fixed and meaningless: both verifiers authenticate by pinned
/// fingerprint and ignore the hostname entirely (see
/// `clipmesh_security::tls`). rustls still requires *a* name, so we send a
/// constant rather than leaking the peer's device id in cleartext SNI.
pub const SERVER_NAME: &str = "clipmesh.local";

/// The server name both sides expect.
///
/// # Panics
/// Never: the constant above is a valid DNS name, and this is asserted by a
/// unit test.
#[must_use]
pub fn server_name() -> ServerName<'static> {
    ServerName::try_from(SERVER_NAME).expect("SERVER_NAME is a valid DNS name")
}

/// Build a TLS acceptor for inbound connections.
///
/// # Errors
/// Returns [`SecurityError`] when the device certificate or key is unusable.
pub fn acceptor(
    identity: &DeviceIdentity,
    policy: TrustPolicy,
) -> SecurityResult<(TlsAcceptor, ObservedCertificate)> {
    let (config, observed) = tls::server_config(identity, policy)?;
    Ok((TlsAcceptor::from(config), observed))
}

/// Build a TLS connector for outbound connections.
///
/// # Errors
/// Returns [`SecurityError`] when the device certificate or key is unusable.
pub fn connector(
    identity: &DeviceIdentity,
    policy: TrustPolicy,
) -> SecurityResult<(TlsConnector, ObservedCertificate)> {
    let (config, observed) = tls::client_config(identity, policy)?;
    Ok((TlsConnector::from(config), observed))
}

/// Read the channel binding value from an established stream.
///
/// `tokio_rustls::TlsStream` is an enum over the client and server halves, and
/// both expose the exporter through the same call, so one function covers both
/// roles - which is exactly what a symmetric handshake wants.
///
/// # Errors
/// Returns [`SecurityError::ChannelBinding`] if the handshake has not finished.
pub fn channel_binding<S>(
    stream: &tokio_rustls::TlsStream<S>,
) -> Result<[u8; clipmesh_security::CHALLENGE_LENGTH]> {
    let mut output = [0u8; clipmesh_security::CHALLENGE_LENGTH];
    let label = clipmesh_security::CHANNEL_BINDING_LABEL;
    let context = Some(clipmesh_security::CHANNEL_BINDING_CONTEXT);

    let exported = match stream {
        tokio_rustls::TlsStream::Client(inner) => {
            inner.get_ref().1.export_keying_material(&mut output[..], label, context)
        }
        tokio_rustls::TlsStream::Server(inner) => {
            inner.get_ref().1.export_keying_material(&mut output[..], label, context)
        }
    };

    exported.map_err(|error| {
        NetworkError::Security(SecurityError::ChannelBinding(error.to_string()))
    })?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_name_is_valid() {
        assert_eq!(server_name().to_str(), SERVER_NAME);
    }

    #[test]
    fn both_roles_build_configs_from_one_identity() {
        let identity = DeviceIdentity::generate("TLS Test").unwrap();

        assert!(acceptor(&identity, TrustPolicy::accept_unknown()).is_ok());
        assert!(connector(&identity, TrustPolicy::accept_unknown()).is_ok());
    }
}
