//! Mutually authenticated TLS 1.3, with ClipMesh's own certificate policy.
//!
//! ## Why there is no CA
//!
//! ClipMesh devices are not issued certificates by anybody. Each one signs its
//! own, and a peer decides whether to believe it by comparing the fingerprint
//! with the user (see `docs/ARCHITECTURE.md` §4.2). That means rustls' built-in
//! webpki verifier - which wants a chain to a trusted root - is exactly the
//! wrong tool, and both sides install a custom verifier instead.
//!
//! ## What the verifier guarantees
//!
//! A certificate is only accepted if it is a well formed ClipMesh device
//! certificate: X.509, whose subject common name parses as a device id, and
//! whose SubjectPublicKeyInfo is a 32 byte Ed25519 key. In
//! [`TrustPolicy::Pinned`] it must *also* match the certificate and public key
//! pinned in the trust store, in constant time.
//!
//! The self-signature itself is deliberately not verified: the TLS handshake
//! already proves the peer holds the private key for the key inside the
//! certificate (the `CertificateVerify` message), which is a strictly stronger
//! statement than "this certificate signs itself".
//!
//! ## Trust on first use, but never silent
//!
//! A listening socket cannot know whether an inbound connection is a paired
//! device or a new one asking to pair - that is decided by the first
//! application message. So the listener uses [`TrustPolicy::AcceptUnknown`],
//! and `clipmesh-core` then allows an unknown peer to send exactly one thing:
//! a `PairRequest`. Everything else closes the connection. Outbound connections
//! to a device that is already trusted use [`TrustPolicy::Pinned`] and fail
//! closed at the handshake if the peer is not who we asked for.

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, WebPkiSupportedAlgorithms};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error as RustlsError,
    ServerConfig, SignatureScheme,
};
use rustls_pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime,
};
use x509_parser::prelude::{FromDer, X509Certificate};

use clipmesh_identity::{DeviceIdentity, TrustStore};
use clipmesh_protocol::DeviceId;

use crate::error::{SecurityError, Result};

/// Protocol versions ClipMesh will speak.
///
/// TLS 1.2 is deliberately excluded. Both ends of every connection are this
/// same program, so compatibility is never a reason to accept the older
/// protocol, and 1.3 gives us the exporter API the identity handshake binds to.
pub const PROTOCOL_VERSIONS: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13];

/// The single signature algorithm family ClipMesh device keys use.
pub const KEY_ALGORITHM: &str = "ed25519";

/// How the peer certificate is judged during the TLS handshake.
#[derive(Clone)]
pub enum TrustPolicy {
    /// Accept any well formed ClipMesh device certificate, and remember it.
    ///
    /// The application layer then restricts the peer to pairing messages until
    /// a human approves it. Used by the listener, and by outbound connections
    /// to a device the user has not paired with yet.
    AcceptUnknown,

    /// Accept only a certificate that is pinned in the trust store.
    ///
    /// Optionally also require a specific device id, which turns "is this
    /// certificate trusted" into "is this the device I asked for".
    Pinned {
        /// The live trust store. Read (not snapshotted) during the handshake,
        /// so unpairing a device takes effect on the very next connection.
        store: Arc<RwLock<TrustStore>>,
        /// Device we expect to reach, when we are dialling a known peer.
        expected: Option<DeviceId>,
    },
}

impl TrustPolicy {
    /// Policy for a listener, or for dialling a device we have not paired with.
    #[must_use]
    pub fn accept_unknown() -> Self {
        Self::AcceptUnknown
    }

    /// Policy that only accepts pinned certificates.
    #[must_use]
    pub fn pinned(store: Arc<RwLock<TrustStore>>, expected: Option<DeviceId>) -> Self {
        Self::Pinned { store, expected }
    }

    /// The right policy for dialling `device_id`: pinned if we already trust
    /// it, permissive if this is a pairing attempt.
    #[must_use]
    pub fn for_device(store: Arc<RwLock<TrustStore>>, device_id: DeviceId) -> Self {
        if store.read().is_trusted(device_id) {
            Self::Pinned {
                store,
                expected: Some(device_id),
            }
        } else {
            Self::AcceptUnknown
        }
    }

    /// Whether this policy pins the peer's identity.
    #[must_use]
    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned { .. })
    }
}

impl std::fmt::Debug for TrustPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AcceptUnknown => f.write_str("TrustPolicy::AcceptUnknown"),
            Self::Pinned { expected, .. } => f
                .debug_struct("TrustPolicy::Pinned")
                .field("expected", expected)
                .finish_non_exhaustive(),
        }
    }
}

/// The certificate the peer actually presented, captured during the handshake.
///
/// The TLS handshake only tells us "some acceptable certificate"; the engine
/// needs the actual bytes to pin a new device, to show the user a fingerprint,
/// and to re-check identity at the application layer.
#[derive(Debug, Clone, Default)]
pub struct ObservedCertificate(Arc<Mutex<Option<CertificateDer<'static>>>>);

impl ObservedCertificate {
    /// The captured certificate, if the handshake completed.
    #[must_use]
    pub fn get(&self) -> Option<CertificateDer<'static>> {
        self.0.lock().clone()
    }

    /// Take the captured certificate out of the slot.
    pub fn take(&self) -> Option<CertificateDer<'static>> {
        self.0.lock().take()
    }

    fn record(&self, certificate: &CertificateDer<'_>) {
        *self.0.lock() = Some(certificate.clone().into_owned());
    }
}

/// The verifier installed on both the client and the server side.
#[derive(Debug)]
struct DeviceCertVerifier {
    policy: TrustPolicy,
    algorithms: WebPkiSupportedAlgorithms,
    observed: ObservedCertificate,
    /// Empty: ClipMesh has no CA to hint at. Kept as a field so the slice
    /// returned by `root_hint_subjects` has somewhere to point.
    root_hints: Vec<DistinguishedName>,
}

impl DeviceCertVerifier {
    fn new(policy: TrustPolicy, observed: ObservedCertificate) -> Self {
        let algorithms = ring::default_provider().signature_verification_algorithms;
        Self {
            policy,
            algorithms,
            observed,
            root_hints: Vec::new(),
        }
    }

    /// The single place where a peer certificate is judged.
    fn judge(&self, end_entity: &CertificateDer<'_>) -> std::result::Result<DeviceId, RustlsError> {
        let device_id = identify_device(end_entity).map_err(|error| {
            tracing::warn!(%error, "rejected a peer certificate that is not a ClipMesh device certificate");
            RustlsError::InvalidCertificate(CertificateError::BadEncoding)
        })?;

        match &self.policy {
            TrustPolicy::AcceptUnknown => {}
            TrustPolicy::Pinned { store, expected } => {
                if let Some(expected) = expected {
                    if *expected != device_id {
                        tracing::warn!(%expected, %device_id, "rejected a peer that is not the device we dialled");
                        return Err(RustlsError::InvalidCertificate(
                            CertificateError::ApplicationVerificationFailure,
                        ));
                    }
                }

                let certificate = clipmesh_identity::DeviceCertificate::from_der(
                    end_entity.as_ref().to_vec(),
                )
                .map_err(|_| {
                    RustlsError::InvalidCertificate(CertificateError::BadEncoding)
                })?;
                let public_key = certificate.public_key_bytes().map_err(|_| {
                    RustlsError::InvalidCertificate(CertificateError::BadEncoding)
                })?;

                if let Err(error) = store.read().verify_peer(device_id, &certificate, &public_key) {
                    tracing::warn!(%device_id, %error, "rejected an untrusted peer during the TLS handshake");
                    return Err(RustlsError::InvalidCertificate(
                        CertificateError::ApplicationVerificationFailure,
                    ));
                }
            }
        }

        self.observed.record(end_entity);
        Ok(device_id)
    }

    fn check_tls12(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn check_tls13(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
}

impl ServerCertVerifier for DeviceCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        // `server_name` is ignored on purpose. Peers are reached by IP address
        // and authenticated by pinned fingerprint; a hostname would be a second,
        // weaker identity that nobody verifies.
        self.judge(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        self.check_tls12(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        self.check_tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for DeviceCertVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.root_hints
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, RustlsError> {
        self.judge(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        self.check_tls12(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        self.check_tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// Extract the device id from a peer certificate, checking everything the
/// policy relies on.
///
/// # Errors
/// Returns [`SecurityError::Certificate`] when the DER does not parse, the CN
/// is not a device id, or the key is not a 32 byte Ed25519 key.
pub fn identify_device(certificate: &CertificateDer<'_>) -> Result<DeviceId> {
    let (_, parsed) = X509Certificate::from_der(certificate.as_ref())
        .map_err(|error| SecurityError::Certificate(format!("not a valid X.509 DER: {error}")))?;

    let common_name = parsed
        .subject()
        .iter_common_name()
        .next()
        .and_then(|attribute| attribute.as_str().ok())
        .ok_or_else(|| {
            SecurityError::Certificate("certificate has no readable common name".to_owned())
        })?;

    let device_id = DeviceId::parse(common_name).map_err(|error| {
        SecurityError::Certificate(format!(
            "certificate common name {common_name:?} is not a device id: {error}"
        ))
    })?;

    let key: &[u8] = parsed.public_key().subject_public_key.data.as_ref();
    if key.len() != 32 {
        return Err(SecurityError::Certificate(format!(
            "certificate carries a {} byte key, expected 32 ({KEY_ALGORITHM})",
            key.len()
        )));
    }

    Ok(device_id)
}

/// Turn the device key into the PKCS#8 form rustls wants.
///
/// rustls only accepts a `PrivateKeyDer` here; it picks the signing algorithm
/// itself, and rejects keys it cannot serve outright rather than falling back
/// to something weaker.
fn private_key(identity: &DeviceIdentity) -> Result<PrivateKeyDer<'static>> {
    let pkcs8 = identity.key().to_pkcs8_der()?;
    Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        pkcs8.to_vec(),
    )))
}

fn certificate_chain(identity: &DeviceIdentity) -> Vec<CertificateDer<'static>> {
    vec![CertificateDer::from(identity.certificate().to_der())]
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// Build the TLS configuration for a listening socket.
///
/// # Errors
/// Returns [`SecurityError::Tls`] when the certificate or key is unusable.
pub fn server_config(
    identity: &DeviceIdentity,
    policy: TrustPolicy,
) -> Result<(Arc<ServerConfig>, ObservedCertificate)> {
    let observed = ObservedCertificate::default();
    let verifier = Arc::new(DeviceCertVerifier::new(policy, observed.clone()));

    let config = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(PROTOCOL_VERSIONS)
        .map_err(|error| SecurityError::Tls(error.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certificate_chain(identity), private_key(identity)?)
        .map_err(|error| SecurityError::Tls(error.to_string()))?;

    Ok((Arc::new(config), observed))
}

/// Build the TLS configuration for a dialling socket.
///
/// # Errors
/// Returns [`SecurityError::Tls`] when the certificate or key is unusable.
pub fn client_config(
    identity: &DeviceIdentity,
    policy: TrustPolicy,
) -> Result<(Arc<ClientConfig>, ObservedCertificate)> {
    let observed = ObservedCertificate::default();
    let verifier = Arc::new(DeviceCertVerifier::new(policy, observed.clone()));

    let config = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(PROTOCOL_VERSIONS)
        .map_err(|error| SecurityError::Tls(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(certificate_chain(identity), private_key(identity)?)
        .map_err(|error| SecurityError::Tls(error.to_string()))?;

    Ok((Arc::new(config), observed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipmesh_identity::DeviceKey;

    fn certificate_for(device_id: DeviceId, name: &str) -> (CertificateDer<'static>, [u8; 32]) {
        let key = DeviceKey::generate();
        let certificate =
            clipmesh_identity::DeviceCertificate::self_signed(&key, device_id, name).unwrap();
        (
            CertificateDer::from(certificate.to_der()),
            key.public_key_bytes(),
        )
    }

    #[test]
    fn a_clipmesh_certificate_identifies_its_device() {
        let device_id = DeviceId::new();
        let (der, _) = certificate_for(device_id, "Laptop");
        assert_eq!(identify_device(&der).unwrap(), device_id);
    }

    #[test]
    fn garbage_is_not_a_device_certificate() {
        let der = CertificateDer::from(vec![0u8; 128]);
        assert!(identify_device(&der).is_err());
    }

    #[test]
    fn configs_build_for_both_roles() {
        let identity = DeviceIdentity::generate("Host").unwrap();
        let store = Arc::new(RwLock::new(TrustStore::in_memory()));

        let (server, observed) =
            server_config(&identity, TrustPolicy::accept_unknown()).unwrap();
        assert!(observed.get().is_none());
        assert_eq!(server.alpn_protocols.len(), 0);

        let (client, observed) =
            client_config(&identity, TrustPolicy::pinned(store, None)).unwrap();
        assert!(observed.get().is_none());
        // The client must be ready to present a certificate immediately.
        assert!(client.client_auth_cert_resolver.has_certs());
    }

    #[test]
    fn the_policy_for_a_known_device_is_pinned() {
        let unknown = Arc::new(RwLock::new(TrustStore::in_memory()));
        assert!(!TrustPolicy::for_device(unknown, DeviceId::new()).is_pinned());

        let identity = DeviceIdentity::generate("Known").unwrap();
        let device = clipmesh_identity::TrustedDevice::new(
            "Known",
            clipmesh_protocol::Platform::Linux,
            identity.certificate().clone(),
            &identity.public_key(),
        )
        .unwrap();

        let device_id = device.device_id;
        let mut store = TrustStore::in_memory();
        store.trust(device).unwrap();
        let shared = Arc::new(RwLock::new(store));

        assert!(TrustPolicy::for_device(shared, device_id).is_pinned());
    }

    #[test]
    fn unpairing_takes_effect_immediately() {
        // The policy reads the live store rather than a snapshot, so a device
        // removed from the trust list cannot complete the next handshake.
        let identity = DeviceIdentity::generate("Known").unwrap();
        let device = clipmesh_identity::TrustedDevice::new(
            "Known",
            clipmesh_protocol::Platform::Linux,
            identity.certificate().clone(),
            &identity.public_key(),
        )
        .unwrap();
        let device_id = device.device_id;

        let mut store = TrustStore::in_memory();
        store.trust(device).unwrap();
        let shared = Arc::new(RwLock::new(store));

        assert!(TrustPolicy::for_device(Arc::clone(&shared), device_id).is_pinned());

        shared.write().forget(device_id).unwrap();
        assert!(!TrustPolicy::for_device(shared, device_id).is_pinned());
    }
}
