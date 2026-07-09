//! # Tokio Host Runtime Implementation
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for standard operating systems using the Tokio runtime
//! and the Rustls TLS stack.

use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, RawStreamFactory, SocketError, TimerError, TimerProvider,
    TlsConnector, TlsVersion,
};

pub(crate) const UDP_RECV_TIMEOUT_MS: u64 = 100;
use core::net::SocketAddr;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{
    CertificateDer, PrivateKeyDer, ServerName, SignatureVerificationAlgorithm, UnixTime,
};
use x509_parser::prelude::FromDer;
use std::sync::Arc;
use tokio_rustls::rustls;

/// Timer implementation utilizing Tokio's non-blocking system clock registry.
pub struct TokioTimer {
    epoch: std::time::Instant,
}

impl TokioTimer {
    /// Creates a timer, capturing the current instant as its monotonic epoch.
    pub fn new() -> Self {
        Self {
            epoch: std::time::Instant::now(),
        }
    }
}

impl Default for TokioTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerProvider for TokioTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        ::tokio::time::sleep(duration).await;
        Ok(())
    }

    fn now_millis(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

/// UDP socket interface wrapping a native Tokio UdpSocket.
pub struct TokioUdpSocket {
    inner: ::tokio::net::UdpSocket,
}

impl BindableUdpSocket for TokioUdpSocket {
    async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        // We bind a standard library `std::net::UdpSocket` first and configure standard properties
        // before converting it cleanly into an asynchronous Tokio UdpSocket.
        let std_socket = std::net::UdpSocket::bind(addr).map_err(to_socket_error)?;

        // Broadcast/multicast setup, then non-blocking mode — required before wrapping in the
        // Tokio asynchronous engine (recent Tokio versions, v1.40+, panic immediately on
        // thread-local registration otherwise). See `configure_std_udp_socket`'s doc comment.
        crate::io::configure_std_udp_socket(&std_socket)?;

        // Convert the configured standard socket into an asynchronous Tokio UdpSocket.
        let inner = ::tokio::net::UdpSocket::from_std(std_socket).map_err(to_socket_error)?;

        Ok(Self { inner })
    }
}

impl AsyncUdpSocket for TokioUdpSocket {
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError> {
        self.inner
            .send_to(buf, target)
            .await
            .map_err(to_socket_error)
    }

    /// Asynchronously reads an incoming datagram, bounding the wait block with a timeout.
    ///
    /// **Why this is critical:**
    /// By default, `tokio::net::UdpSocket::recv_from` blocks indefinitely if no packet is available.
    /// During sweeps where some network environments drop unicast discovery replies, this blocks
    /// execution threads forever. Wrapping the call in a 100ms timeout enables standard polling
    /// loops to proceed and exit gracefully.
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
        match ::tokio::time::timeout(
            core::time::Duration::from_millis(UDP_RECV_TIMEOUT_MS),
            self.inner.recv_from(buf),
        )
        .await
        {
            Ok(Ok((len, addr))) => Ok((len, addr)),
            Ok(Err(e)) => Err(to_socket_error(e)),
            Err(_) => Err(SocketError::TimedOut),
        }
    }
}

/// Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum.
pub fn to_socket_error(err: std::io::Error) -> SocketError {
    crate::io::map_std_io_error(err, "Native OS platform IO error occurred")
}

/// Custom certificate verifier that bypasses standard CA chain authority validation.
///
/// **Why this is required:**
/// Physical Bambu Lab printers (all models) host an onboard local MQTTS/FTPS broker
/// utilizing self-signed certificates with the printer's serial number in the CN field.
/// Because these do not trace back to any root authority in OS certificate stores,
/// standard verifiers reject the connections immediately.
#[derive(Debug)]
pub struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, RustlsError> {
        // Asserting validation is OK. The printer certificate uses the serial number as its CN.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Expose support for all typical legacy and modern signing configurations to avoid handshake failure.
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Certificate verifier for the "verified" (CA-checked) connection path that validates real
/// chain-of-trust against caller-supplied trusted roots, but — unlike rustls's default
/// `WebPkiServerVerifier` — works against real Bambu printer certs at all.
///
/// **Why this can't use `rustls-webpki`:** real Bambu printer certs are X.509 **v1** (confirmed
/// against a live P1S — no version tag, implicit v1 encoding per RFC 5280 §4.1.2.1).
/// `rustls-webpki` (confirmed against the pinned `0.103.13`, `src/cert.rs::version3`) rejects
/// *any* cert that isn't v3, unconditionally — this is deliberate mozilla::pkix policy
/// ("We allow only v3"), not a bug, and it applies to `EndEntityCert`/`ParsedCertificate`
/// parsing used by chain validation *and* to the free functions `verify_tls12_signature`/
/// `verify_tls13_signature` (which independently re-parse the leaf via
/// `EndEntityCert::try_from` during the handshake's signature check). So neither chain
/// validation nor the handshake-signature check can be delegated to anything in
/// `rustls-webpki` for a real Bambu cert — confirmed as a known limitation other real-world
/// self-signed device certs have hit too (see `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md` for the
/// GitHub issue citations, including the LND project's identical problem).
///
/// This verifier uses `x509-parser` instead (a general ASN.1/X.509 parser, not a
/// policy-enforcing validator — confirmed via its own test suite that it treats the version
/// field as optional, defaulting to v1 when absent, exactly per the DER grammar) for all
/// parsing, and does two independent things no other code in this crate does:
/// - **Chain-of-trust**: is the leaf's signature valid under one of the caller-supplied
///   trusted roots' public keys, with a matching issuer/subject and unexpired validity period?
///   (`verify_server_cert`, via `X509Certificate::verify_signature` — real `ring`-backed
///   verification, not hand-rolled crypto.)
/// - **Handshake-signature check**: does the live TLS handshake signature verify under the
///   leaf's own public key? (`verify_tls12_signature`/`verify_tls13_signature`, via
///   `rustls_pki_types::SignatureVerificationAlgorithm::verify_signature` directly — this is
///   the check that actually proves the peer holds the private key matching the presented
///   cert; per the LND issue's own reasoning, this is what prevents MITM here, not the chain
///   check alone.)
///
/// Identity (SAN-then-CN, mirroring mbedtls's `x509_crt_verify_name` algorithm) is still
/// checked last, same logic as before — only its data source changed, from a hand-rolled DER
/// walker to `x509-parser`'s parsed fields.
pub struct CnFallbackServerVerifier {
    trusted_roots: Vec<CertificateDer<'static>>,
    algs_mapping:
        &'static [(SignatureScheme, &'static [&'static dyn SignatureVerificationAlgorithm])],
}

impl CnFallbackServerVerifier {
    /// Builds the verifier from a set of trusted root certs. Fails if `ca_certs` is empty or
    /// any supplied cert fails to parse — there is nothing to validate a chain against
    /// otherwise, so failing fast at config-build time (rather than silently succeeding and
    /// only failing later at handshake time) is deliberate.
    pub fn new(
        ca_certs: impl IntoIterator<Item = CertificateDer<'static>>,
    ) -> Result<Self, RustlsError> {
        let trusted_roots: Vec<_> = ca_certs.into_iter().collect();
        if trusted_roots.is_empty() {
            return Err(RustlsError::General(
                "CnFallbackServerVerifier requires at least one trusted root cert".into(),
            ));
        }
        for root in &trusted_roots {
            x509_parser::certificate::X509Certificate::from_der(root.as_ref()).map_err(|e| {
                RustlsError::General(format!("failed to parse trusted root cert: {e}"))
            })?;
        }

        let provider = rustls::crypto::ring::default_provider();
        Ok(Self {
            trusted_roots,
            algs_mapping: provider.signature_verification_algorithms.mapping,
        })
    }
}

impl core::fmt::Debug for CnFallbackServerVerifier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CnFallbackServerVerifier").finish()
    }
}

impl ServerCertVerifier for CnFallbackServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        use x509_parser::certificate::X509Certificate;
        use x509_parser::time::ASN1Time;

        let (_, leaf) = X509Certificate::from_der(end_entity.as_ref())
            .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;

        let now_asn1 = ASN1Time::from_timestamp(now.as_secs() as i64)
            .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if !leaf.validity().is_valid_at(now_asn1) {
            let err = if now_asn1.timestamp() < leaf.validity().not_before.timestamp() {
                CertificateError::NotValidYet
            } else {
                CertificateError::Expired
            };
            return Err(RustlsError::InvalidCertificate(err));
        }

        let mut signed_by_trusted_root = false;
        for root_der in &self.trusted_roots {
            let Ok((_, root)) = X509Certificate::from_der(root_der.as_ref()) else {
                continue;
            };
            if leaf.issuer().as_raw() != root.subject().as_raw() {
                continue;
            }
            if leaf.verify_signature(Some(root.public_key())).is_ok() {
                signed_by_trusted_root = true;
                break;
            }
        }
        if !signed_by_trusted_root {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ));
        }

        verify_name_matches_leaf_cert(&leaf, server_name)?;

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_handshake_signature(cert, message, dss, self.algs_mapping, true)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_handshake_signature(cert, message, dss, self.algs_mapping, false)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs_mapping.iter().map(|(scheme, _)| *scheme).collect()
    }
}

/// Verifies a live TLS handshake signature against the leaf cert's own public key, extracted
/// via `x509-parser` (bypassing `rustls-webpki`'s `EndEntityCert`, which would reject a v1
/// cert again here just as it does during chain validation). `try_all`: TLS 1.2 tries every
/// candidate algorithm mapped to `dss.scheme` in turn (multiple `SignatureVerificationAlgorithm`s
/// can share one `SignatureScheme`); TLS 1.3 only tries the first, matching the documented
/// behavior of the `rustls-webpki` free functions of the same name this replaces.
fn verify_handshake_signature(
    cert: &CertificateDer<'_>,
    message: &[u8],
    dss: &DigitallySignedStruct,
    algs_mapping: &'static [(SignatureScheme, &'static [&'static dyn SignatureVerificationAlgorithm])],
    try_all: bool,
) -> Result<HandshakeSignatureValid, RustlsError> {
    let (_, leaf) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
        .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;
    let public_key = &leaf.public_key().subject_public_key.data;

    let candidates = algs_mapping
        .iter()
        .find(|(scheme, _)| *scheme == dss.scheme)
        .map(|(_, algs)| *algs)
        .ok_or(RustlsError::PeerIncompatible(
            rustls::PeerIncompatible::SignatureAlgorithmsExtensionRequired,
        ))?;

    let candidates = if try_all {
        candidates
    } else {
        candidates.get(..1).unwrap_or(candidates)
    };

    for alg in candidates {
        if alg
            .verify_signature(public_key, message, dss.signature())
            .is_ok()
        {
            return Ok(HandshakeSignatureValid::assertion());
        }
    }
    Err(RustlsError::InvalidCertificate(
        CertificateError::BadSignature,
    ))
}

/// Verifies a parsed leaf cert's identity against `server_name`: SAN first if present, else
/// Subject CN — mirroring mbedtls's `x509_crt_verify_name` algorithm. Only
/// `ServerName::DnsName` is supported (Bambu serials are never IP addresses).
fn verify_name_matches_leaf_cert(
    leaf: &x509_parser::certificate::X509Certificate<'_>,
    server_name: &ServerName<'_>,
) -> Result<(), RustlsError> {
    let expected = match server_name {
        ServerName::DnsName(dns) => dns.as_ref(),
        _ => return Err(RustlsError::UnsupportedNameType),
    };

    let san = leaf
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|ext| &ext.value.general_names);

    if let Some(general_names) = san {
        let dns_names = general_names.iter().filter_map(|gn| match gn {
            x509_parser::extensions::GeneralName::DNSName(name) => Some(*name),
            _ => None,
        });
        return if dns_names.clone().next().is_some() {
            if dns_names.clone().any(|n| n.eq_ignore_ascii_case(expected)) {
                Ok(())
            } else {
                Err(RustlsError::InvalidCertificate(
                    CertificateError::NotValidForName,
                ))
            }
        } else {
            match_subject_cn(leaf, expected)
        };
    }

    match_subject_cn(leaf, expected)
}

fn match_subject_cn(
    leaf: &x509_parser::certificate::X509Certificate<'_>,
    expected: &str,
) -> Result<(), RustlsError> {
    let matches = leaf
        .subject()
        .iter_common_name()
        .filter_map(|cn| cn.as_str().ok())
        .any(|cn| cn.eq_ignore_ascii_case(expected));

    if matches {
        Ok(())
    } else {
        Err(RustlsError::InvalidCertificate(
            CertificateError::NotValidForName,
        ))
    }
}

/// Builds an unsafe `ClientConfig` with configurable TLS version constraints.
///
/// When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only. This is
/// required for P2S and X2D models whose embedded vsFTPd servers fail on TLS 1.3
/// session tickets [REF-FTPS-CONN].
pub fn build_unsafe_client_config_with_options(force_tls_1_2: bool) -> Arc<rustls::ClientConfig> {
    let verifier = Arc::new(NoCertificateVerification);

    let provider = rustls::crypto::ring::default_provider();
    let versions: &[&rustls::SupportedProtocolVersion] = if force_tls_1_2 {
        &[&rustls::version::TLS12]
    } else {
        rustls::DEFAULT_VERSIONS
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(versions)
        .expect("Protocols must be initialized successfully")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    Arc::new(config)
}

/// Builds an unsafe `ClientConfig` with default TLS version negotiation (TLS 1.2 + 1.3).
pub fn build_unsafe_client_config() -> Arc<rustls::ClientConfig> {
    build_unsafe_client_config_with_options(false)
}

/// Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs.
///
/// Use `rustls_pki_types::pem::PemObject` to load PEM files:
/// ```ignore
/// use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
/// let ca = CertificateDer::from_pem_file("ca.pem")?;
/// ```
///
/// `client_auth`: pass `Some((cert_chain, key))` for mutual TLS, `None` for server-only verification.
pub fn build_verified_client_config(
    ca_certs: impl IntoIterator<Item = CertificateDer<'static>>,
    client_auth: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> Result<Arc<rustls::ClientConfig>, rustls::Error> {
    build_verified_client_config_with_options(ca_certs, client_auth, false)
}

/// Builds a verified `ClientConfig` with configurable TLS version constraints.
///
/// When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only (required
/// for FTPS data channels on P2S/X2D models [REF-FTPS-CONN]).
pub fn build_verified_client_config_with_options(
    ca_certs: impl IntoIterator<Item = CertificateDer<'static>>,
    client_auth: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
    force_tls_1_2: bool,
) -> Result<Arc<rustls::ClientConfig>, rustls::Error> {
    let provider = rustls::crypto::ring::default_provider();
    let versions: &[&rustls::SupportedProtocolVersion] = if force_tls_1_2 {
        &[&rustls::version::TLS12]
    } else {
        rustls::DEFAULT_VERSIONS
    };

    let verifier = Arc::new(CnFallbackServerVerifier::new(ca_certs)?);

    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(versions)
        .expect("Protocols must be initialized successfully")
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let config = match client_auth {
        Some((cert_chain, key)) => builder.with_client_auth_cert(cert_chain, key)?,
        None => builder.with_no_client_auth(),
    };

    Ok(Arc::new(config))
}

/// TLS Secure connector wrapping Tokio-Rustls.
pub struct TokioTlsConnector {
    connector: tokio_rustls::TlsConnector,
}

impl TokioTlsConnector {
    /// Creates a connector given a pre-configured tokio-rustls connector instance.
    pub fn new(connector: tokio_rustls::TlsConnector) -> Self {
        Self { connector }
    }
}

impl TlsConnector<TokioIo<::tokio::net::TcpStream>> for TokioTlsConnector {
    type Stream = TokioIo<tokio_rustls::client::TlsStream<::tokio::net::TcpStream>>;

    async fn connect(
        &self,
        host: &str,
        raw_stream: TokioIo<::tokio::net::TcpStream>,
    ) -> Result<Self::Stream, SocketError> {
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| SocketError::InvalidInput)?
            .to_owned();

        let tls_stream = self
            .connector
            .connect(server_name, raw_stream.0)
            .await
            .map_err(to_socket_error)?;

        Ok(TokioIo(tls_stream))
    }

    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        match stream.0.get_ref().1.protocol_version()? {
            rustls::ProtocolVersion::TLSv1_2 => Some(TlsVersion::Tls12),
            rustls::ProtocolVersion::TLSv1_3 => Some(TlsVersion::Tls13),
            _ => None,
        }
    }
}

/// Raw (pre-TLS) connection factory for the Tokio runtime.
///
/// Creates raw TCP connections wrapped in [`TokioIo`] — used for MQTT's lazy connect and
/// FTPS passive-mode data transfers alike (the Tokio counterpart to
/// [`DummyFactory`](crate::client::dummy::DummyFactory)).
pub struct TokioRawStreamFactory;

impl RawStreamFactory<TokioIo<::tokio::net::TcpStream>> for TokioRawStreamFactory {
    async fn dial(
        &self,
        host: &str,
        port: u16,
    ) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError> {
        let stream = ::tokio::net::TcpStream::connect(format!("{}:{}", host, port))
            .await
            .map_err(to_socket_error)?;
        Ok(TokioIo(stream))
    }
}

/// Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.
pub struct TokioIo<T>(pub T);

/// Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.
#[derive(Debug)]
pub struct TokioIoError(pub std::io::Error);

// In embedded-io version 0.7+, the `embedded_io::Error` trait has a supertrait bound on `core::error::Error`.
// Therefore, we must implement both `core::fmt::Display` and `std::error::Error` for `TokioIoError`.

impl core::fmt::Display for TokioIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Tokio IO Error: {}", self.0)
    }
}

impl std::error::Error for TokioIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl embedded_io_async::Error for TokioIoError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        crate::io::map_io_error_kind(self.0.kind())
    }
}

/// Implement ErrorType for TokioIo as specified by the embedded-io-async 0.7 spec.
///
/// This separates error declaration from read/write trait implementations.
impl<T> embedded_io_async::ErrorType for TokioIo<T> {
    type Error = TokioIoError;
}

impl<T: ::tokio::io::AsyncRead + Unpin> embedded_io_async::Read for TokioIo<T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use ::tokio::io::AsyncReadExt;
        self.0.read(buf).await.map_err(TokioIoError)
    }
}

impl<T: ::tokio::io::AsyncWrite + Unpin> embedded_io_async::Write for TokioIo<T> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        use ::tokio::io::AsyncWriteExt;
        self.0.write(buf).await.map_err(TokioIoError)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        use ::tokio::io::AsyncWriteExt;
        self.0.flush().await.map_err(TokioIoError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_unsafe_client_config() {
        let config = build_unsafe_client_config();
        assert!(config.alpn_protocols.is_empty());
    }

    #[test]
    fn test_build_verified_client_config_empty_roots_fails_fast() {
        // An empty root store can never validate a chain — CnFallbackServerVerifier::new fails
        // immediately (NoRootAnchors) rather than deferring to a confusing handshake-time error.
        let config = build_verified_client_config(std::iter::empty(), None);
        assert!(
            config.is_err(),
            "empty root store should fail fast at config-build time"
        );
    }

    #[test]
    fn test_build_verified_client_config_with_options_tls12() {
        let (ca_der, ..) = test_support::generate_test_ca();
        let config =
            build_verified_client_config_with_options([ca_der], None, true);
        assert!(config.is_ok());
    }

    /// Shared fixtures for `CnFallbackServerVerifier` tests: real DER-encoded certs rather than
    /// hand-crafted byte arrays, so the parser is exercised against genuine X.509 encoding.
    mod test_support {
        use rcgen::{BasicConstraints, CertificateParams, DnType, Issuer, IsCa, KeyPair, SigningKey};

        use super::*;

        /// Returns the CA cert, an `Issuer` for signing v3 test leaves, and the CA key's own
        /// DER + algorithm — the latter two let `strip_to_v1` reconstruct an independent
        /// `KeyPair` instance for the same key (needed since `Issuer::new` consumes its key).
        pub(super) fn generate_test_ca() -> (
            CertificateDer<'static>,
            Issuer<'static, KeyPair>,
            Vec<u8>,
            &'static rcgen::SignatureAlgorithm,
        ) {
            let mut params = CertificateParams::new(Vec::new()).unwrap();
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            params
                .distinguished_name
                .push(DnType::CommonName, "bambino test CA");
            let key = KeyPair::generate().unwrap();
            let key_der = key.serialize_der();
            let algo = key.algorithm();
            let cert = params.clone().self_signed(&key).unwrap();
            let der = cert.der().clone();
            (der, Issuer::new(params, key), key_der, algo)
        }

        /// Builds a v3 leaf cert signed by `issuer`. `common_name` is always set; `san_dns_names`
        /// controls whether a SAN extension is present at all.
        pub(super) fn generate_test_leaf(
            issuer: &Issuer<'_, KeyPair>,
            common_name: &str,
            san_dns_names: Vec<String>,
        ) -> CertificateDer<'static> {
            let mut params = CertificateParams::new(san_dns_names).unwrap();
            params
                .distinguished_name
                .push(DnType::CommonName, common_name);
            let key = KeyPair::generate().unwrap();
            params.signed_by(&key, issuer).unwrap().der().clone()
        }

        /// Builds a v3 leaf cert signed by `issuer` with an explicit (possibly expired)
        /// validity window, for the expiry-rejection test.
        pub(super) fn generate_test_leaf_with_validity(
            issuer: &Issuer<'_, KeyPair>,
            common_name: &str,
            not_before: time::OffsetDateTime,
            not_after: time::OffsetDateTime,
        ) -> CertificateDer<'static> {
            let mut params = CertificateParams::new(Vec::new()).unwrap();
            params
                .distinguished_name
                .push(DnType::CommonName, common_name);
            params.not_before = not_before;
            params.not_after = not_after;
            let key = KeyPair::generate().unwrap();
            params.signed_by(&key, issuer).unwrap().der().clone()
        }

        /// Minimal DER (tag, content, rest) reader — test-fixture scaffolding only, deliberately
        /// kept separate from any production parsing code.
        fn read_tlv(input: &[u8]) -> (u8, &[u8], &[u8]) {
            let tag = input[0];
            let len_byte = input[1] as usize;
            let (len, header_len) = if len_byte & 0x80 == 0 {
                (len_byte, 2)
            } else {
                let n = len_byte & 0x7F;
                let mut len = 0usize;
                for b in &input[2..2 + n] {
                    len = (len << 8) | *b as usize;
                }
                (len, 2 + n)
            };
            (
                tag,
                &input[header_len..header_len + len],
                &input[header_len + len..],
            )
        }

        fn write_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
            let mut out = vec![tag];
            let len = content.len();
            if len < 0x80 {
                out.push(len as u8);
            } else {
                let bytes = len.to_be_bytes();
                let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
                let significant = &bytes[first_nonzero..];
                out.push(0x80 | significant.len() as u8);
                out.extend_from_slice(significant);
            }
            out.extend_from_slice(content);
            out
        }

        /// Strips an rcgen-generated v3 leaf's `[0]` version tag and `[3]` extensions block,
        /// then re-signs the resulting v1-shaped TBSCertificate under `issuer_key` and
        /// re-wraps it — producing a cert with the exact shape real Bambu printer certs have
        /// (confirmed against a live P1S): no version tag (implicit v1), no extensions at all.
        /// This is the fixture that actually proves the verifier handles real printer certs —
        /// the rcgen-default v3 fixtures above do not, and did not catch this session's first,
        /// broken attempt at this verifier.
        pub(super) fn strip_to_v1(
            v3_der: &CertificateDer<'static>,
            issuer_key: &KeyPair,
        ) -> CertificateDer<'static> {
            let (cert_tag, cert_content, _) = read_tlv(v3_der.as_ref());
            assert_eq!(cert_tag, 0x30, "expected outer Certificate SEQUENCE");
            let (tbs_tag, tbs_content, after_tbs) = read_tlv(cert_content);
            assert_eq!(tbs_tag, 0x30, "expected TBSCertificate SEQUENCE");
            let (sig_alg_tag, sig_alg_content, _) = read_tlv(after_tbs);
            assert_eq!(sig_alg_tag, 0x30, "expected signatureAlgorithm SEQUENCE");

            let mut remaining = tbs_content;
            let mut kept = Vec::new();
            while !remaining.is_empty() {
                let (tag, content, rest) = read_tlv(remaining);
                // Drop [0] EXPLICIT version and [3] EXPLICIT extensions — everything else
                // (serialNumber, signature AlgorithmIdentifier, issuer, validity, subject,
                // subjectPublicKeyInfo) is kept verbatim from the real, correctly-encoded v3
                // cert.
                if tag != 0xA0 && tag != 0xA3 {
                    kept.extend_from_slice(&write_tlv(tag, content));
                }
                remaining = rest;
            }

            let new_tbs = write_tlv(0x30, &kept);
            let signature = issuer_key
                .sign(&new_tbs)
                .expect("test fixture signing must succeed");
            let mut bit_string_content = vec![0x00]; // BIT STRING "unused bits" prefix
            bit_string_content.extend_from_slice(&signature);

            let mut cert_body = Vec::new();
            cert_body.extend_from_slice(&new_tbs);
            cert_body.extend_from_slice(&write_tlv(0x30, sig_alg_content));
            cert_body.extend_from_slice(&write_tlv(0x03, &bit_string_content));

            CertificateDer::from(write_tlv(0x30, &cert_body))
        }
    }

    #[test]
    fn test_cn_fallback_verifier_accepts_cn_match_when_no_san() {
        let (ca_der, issuer, ..) = test_support::generate_test_ca();
        let leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

        let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
        assert!(result.is_ok(), "expected CN fallback match, got {result:?}");
    }

    #[test]
    fn test_cn_fallback_verifier_rejects_cn_mismatch_when_no_san() {
        let (ca_der, issuer, ..) = test_support::generate_test_ca();
        let leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("SOMEOTHERSERIAL").unwrap();

        let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
        assert!(result.is_err(), "CN mismatch must be rejected");
    }

    #[test]
    fn test_cn_fallback_verifier_prefers_san_over_mismatched_cn() {
        let (ca_der, issuer, ..) = test_support::generate_test_ca();
        // CN deliberately wrong; SAN carries the real expected identity. A cert with any SAN
        // extension present must be matched via SAN only, per mbedtls's own algorithm.
        let leaf = test_support::generate_test_leaf(
            &issuer,
            "WRONG-CN",
            vec!["TESTSERIAL0001".to_string()],
        );
        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

        let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
        assert!(result.is_ok(), "expected SAN match, got {result:?}");
    }

    #[test]
    fn test_cn_fallback_verifier_rejects_untrusted_chain() {
        let (ca_der, ..) = test_support::generate_test_ca();
        // A second, unrelated CA signs this leaf — it must never validate against the first
        // CA's trusted roots, regardless of whether the name matches.
        let (_other_ca_der, other_issuer, ..) = test_support::generate_test_ca();
        let leaf = test_support::generate_test_leaf(&other_issuer, "TESTSERIAL0001", Vec::new());
        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

        let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
        assert!(
            result.is_err(),
            "cert signed by an untrusted CA must be rejected"
        );
    }

    #[test]
    fn test_cn_fallback_verifier_rejects_expired_cert() {
        let (ca_der, issuer, ..) = test_support::generate_test_ca();
        let not_before = time::OffsetDateTime::UNIX_EPOCH;
        let not_after = not_before + time::Duration::days(1); // expired long ago
        let leaf = test_support::generate_test_leaf_with_validity(
            &issuer,
            "TESTSERIAL0001",
            not_before,
            not_after,
        );
        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

        let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
        assert!(result.is_err(), "an expired cert must be rejected");
    }

    /// The test that would have caught this session's first, broken attempt at this verifier:
    /// a genuinely v1-shaped cert (no version tag, no extensions — the real Bambu printer cert
    /// shape, confirmed against a live P1S), not just rcgen's default v3 output.
    #[test]
    fn test_cn_fallback_verifier_accepts_real_v1_shaped_cert_with_cn_match() {
        use rcgen::KeyPair;

        let (ca_der, issuer, ca_key_der, ca_algo) = test_support::generate_test_ca();
        let v3_leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
        let ca_key_der = PrivateKeyDer::try_from(ca_key_der).unwrap();
        let ca_key_copy = KeyPair::from_der_and_sign_algo(&ca_key_der, ca_algo).unwrap();
        let v1_leaf = test_support::strip_to_v1(&v3_leaf, &ca_key_copy);

        // Confirm the fixture really is v1-shaped before trusting the test result below: a v3
        // cert always has an extensions block (even if empty of entries); a true v1 cert has
        // none of the extensions TLV at all.
        let (_, parsed) = x509_parser::certificate::X509Certificate::from_der(v1_leaf.as_ref())
            .expect("v1 fixture must still be valid DER");
        assert!(
            parsed.extensions().is_empty(),
            "v1 fixture must have no extensions block at all"
        );

        let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
        let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

        let result =
            verifier.verify_server_cert(&v1_leaf, &[], &server_name, &[], UnixTime::now());
        assert!(
            result.is_ok(),
            "expected a real v1-shaped, SAN-less cert to validate via CN fallback, got {result:?}"
        );
    }

    #[test]
    fn test_build_verified_client_config_bad_key_returns_error() {
        let _bogus_cert = CertificateDer::from(vec![0u8; 10]);
        let bogus_key = PrivateKeyDer::try_from(vec![0u8; 10]);
        // Invalid DER key bytes should fail to parse
        assert!(bogus_key.is_err());
    }
}
