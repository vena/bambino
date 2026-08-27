//! # Tokio Host Runtime Implementation
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for standard operating systems using the Tokio runtime
//! and the Rustls TLS stack.

use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, CertificateFailure, RawStreamFactory, SocketError,
    TimerError, TimerProvider, TlsConnector, TlsVersion,
};

pub(crate) const UDP_RECV_TIMEOUT_MS: u64 = 100;
use core::net::SocketAddr;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, ServerName};
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
        let std_socket = std::net::UdpSocket::bind(addr)?;

        // Broadcast/multicast setup, then non-blocking mode — required before wrapping in the
        // Tokio asynchronous engine (recent Tokio versions, v1.40+, panic immediately on
        // thread-local registration otherwise). See `configure_std_udp_socket`'s doc comment.
        crate::io::configure_std_udp_socket(&std_socket)?;

        // Convert the configured standard socket into an asynchronous Tokio UdpSocket.
        let inner = ::tokio::net::UdpSocket::from_std(std_socket)?;

        Ok(Self { inner })
    }
}

impl AsyncUdpSocket for TokioUdpSocket {
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError> {
        Ok(self.inner.send_to(buf, target).await?)
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
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(SocketError::TimedOut),
        }
    }
}

impl From<std::io::Error> for SocketError {
    fn from(err: std::io::Error) -> Self {
        crate::io::map_std_io_error(err, "Native OS platform IO error occurred")
    }
}

/// Maps a failed `tokio_rustls` handshake to a `SocketError`, keeping rustls's certificate
/// verdict instead of flattening it into an opaque IO error (GitHub issue #157).
///
/// `tokio_rustls` surfaces the handshake failure as a `std::io::Error` with the real
/// `rustls::Error` boxed inside it, so the typed verdict is there but only reachable by
/// downcast — without this, `CertificateError::UnknownIssuer` and
/// `CertificateError::NotValidForName` both arrived as `SocketError::Other`, distinguishable
/// only by matching on a display string. Anything that is not a certificate rejection falls
/// through to the existing `std::io::ErrorKind` mapping unchanged.
///
/// `NoCertificatesPresented` is matched alongside `InvalidCertificate` because rustls puts it
/// at the top level of `Error` rather than under `CertificateError` — an easy variant to miss,
/// and the one case where there is no certificate for a caller to capture at all.
fn map_tls_handshake_error(err: std::io::Error) -> SocketError {
    if let Some(rustls_err) = err
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        match rustls_err {
            rustls::Error::InvalidCertificate(cert_err) => {
                log::debug!("rustls rejected the peer certificate: {cert_err:?}");
                return SocketError::CertificateInvalid(map_rustls_certificate_error(cert_err));
            }
            rustls::Error::NoCertificatesPresented => {
                log::debug!("rustls reports the peer sent no certificates");
                return SocketError::CertificateInvalid(CertificateFailure::Missing);
            }
            _ => {}
        }
    }

    SocketError::from(err)
}

/// Collapses rustls's certificate errors onto the portable [`CertificateFailure`] set.
///
/// `rustls::CertificateError` is `#[non_exhaustive]` and pairs several variants with a
/// `*Context` twin carrying extra detail; both members of each pair mean the same thing to a
/// caller, so they map together. The wildcard arm covers the rest as `Unspecified` — losing
/// detail, never softening the verdict.
// `UnsupportedSignatureAlgorithm` is deprecated in favor of its `*Context` twin but still
// constructible and still reachable, so it stays matched explicitly — dropping it would send it
// to the `Unspecified` wildcard, silently coarsening a verdict that is available.
#[allow(deprecated)]
fn map_rustls_certificate_error(err: &rustls::CertificateError) -> CertificateFailure {
    use rustls::CertificateError as E;

    match err {
        E::UnknownIssuer => CertificateFailure::UntrustedAnchor,
        E::NotValidForName | E::NotValidForNameContext { .. } => CertificateFailure::NameMismatch,
        E::Expired | E::ExpiredContext { .. } => CertificateFailure::Expired,
        E::NotValidYet | E::NotValidYetContext { .. } => CertificateFailure::NotYetValid,
        E::Revoked => CertificateFailure::Revoked,
        E::UnsupportedSignatureAlgorithm
        | E::UnsupportedSignatureAlgorithmContext { .. }
        | E::UnsupportedSignatureAlgorithmForPublicKeyContext { .. } => {
            CertificateFailure::UnsupportedAlgorithm
        }
        E::InvalidPurpose | E::InvalidPurposeContext { .. } => CertificateFailure::InvalidPurpose,
        E::BadEncoding => CertificateFailure::Malformed,
        _ => CertificateFailure::Unspecified,
    }
}

mod cert_verify;
/// Re-exported so a consumer assembling a `rustls::ClientConfig` by hand — rather than through
/// [`build_verified_client_config_with_options`] — can name these verifiers. `.claude/rules/
/// tls-identity-sni.md` documents `CnFallbackServerVerifier` at this path, which a private
/// `use` made unreachable.
pub use cert_verify::{CnFallbackServerVerifier, NoCertificateVerification};

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
/// ```rust,ignore
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
            .map_err(map_tls_handshake_error)?;

        Ok(TokioIo(tls_stream))
    }

    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        match stream.0.get_ref().1.protocol_version()? {
            rustls::ProtocolVersion::TLSv1_2 => Some(TlsVersion::Tls12),
            rustls::ProtocolVersion::TLSv1_3 => Some(TlsVersion::Tls13),
            _ => None,
        }
    }

    /// rustls retains the peer chain on the connection after the handshake, so this is a
    /// straight copy of what the server sent, in wire order (leaf first).
    fn peer_chain_der(&self, stream: &Self::Stream) -> Option<Vec<Vec<u8>>> {
        let chain = stream.0.get_ref().1.peer_certificates()?;
        Some(chain.iter().map(|cert| cert.as_ref().to_vec()).collect())
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
            .map_err(SocketError::from)?;
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
        let n = self.0.write(buf).await.map_err(TokioIoError)?;
        // `embedded_io_async::Write::write_all` *panics* on `Ok(0)`
        // (`embedded-io-async-0.7.0/src/lib.rs:143`), and the trait forbids impls from
        // returning it for a non-empty buffer. `TokioIo<T>` is generic over any
        // `tokio::io::AsyncWrite`, so nothing here upholds that contract on the caller's
        // behalf — and `write_all` sits on live network paths (MQTT, FTPS, camera). Convert
        // it into the error the caller can actually handle instead of panicking the library.
        if n == 0 && !buf.is_empty() {
            return Err(TokioIoError(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "underlying AsyncWrite returned Ok(0) for a non-empty buffer",
            )));
        }
        Ok(n)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        use ::tokio::io::AsyncWriteExt;
        self.0.flush().await.map_err(TokioIoError)
    }
}

#[cfg(test)]
mod tests;
