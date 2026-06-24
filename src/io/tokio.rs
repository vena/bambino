//! # Tokio Host Runtime Implementation
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for standard operating systems using the Tokio runtime
//! and the Rustls TLS stack.

use crate::io::{AsyncUdpSocket, SocketError, TimerProvider, TlsConnector, TokioIo};

pub(crate) const UDP_RECV_TIMEOUT_MS: u64 = 100;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerifier};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use std::sync::Arc;
use tokio_rustls::rustls;

#[cfg(feature = "std")]
use std::string::String;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::string::String;

/// Timer implementation utilizing Tokio's non-blocking system clock registry.
pub struct TokioTimer;

impl TimerProvider for TokioTimer {
    async fn sleep(duration: core::time::Duration) {
        ::tokio::time::sleep(duration).await;
    }
}

/// UDP socket interface wrapping a native Tokio UdpSocket.
pub struct TokioUdpSocket {
    inner: ::tokio::net::UdpSocket,
}

impl AsyncUdpSocket for TokioUdpSocket {
    async fn bind(addr: &str) -> Result<Self, SocketError> {
        // We bind a standard library `std::net::UdpSocket` first and configure standard properties
        // before converting it cleanly into an asynchronous Tokio UdpSocket.
        let std_socket = std::net::UdpSocket::bind(addr).map_err(to_socket_error)?;

        // Enable local broadcast capabilities safely.
        let _ = std_socket.set_broadcast(true);

        // Join the standard Bambu multicast group (239.255.255.250) to register an active IGMP membership.
        // On macOS and Windows, local firewalls and kernel routing stacks frequently drop incoming UDP
        // replies from SSDP targets on ephemeral ports unless the receiving socket has registered a
        // multicast group membership first.
        let multiaddr = std::net::Ipv4Addr::new(239, 255, 255, 250);
        let interface = std::net::Ipv4Addr::new(0, 0, 0, 0);
        let _ = std_socket.join_multicast_v4(&multiaddr, &interface);

        // **Non-blocking Mode Requirement [REF-NET-DISC]:**
        // Putting the standard library socket in non-blocking mode is strictly required before wrapping
        // it in the Tokio asynchronous engine. Failing to toggle this flag causes recent versions of Tokio
        // (v1.40+) to panic immediately on thread-local registration.
        std_socket.set_nonblocking(true).map_err(to_socket_error)?;

        // Convert the configured standard socket into an asynchronous Tokio UdpSocket.
        let inner = ::tokio::net::UdpSocket::from_std(std_socket).map_err(to_socket_error)?;

        Ok(Self { inner })
    }

    async fn send_to(&self, buf: &[u8], target: &str) -> Result<usize, SocketError> {
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
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, String), SocketError> {
        match ::tokio::time::timeout(
            core::time::Duration::from_millis(UDP_RECV_TIMEOUT_MS),
            self.inner.recv_from(buf),
        )
        .await
        {
            Ok(Ok((len, addr))) => Ok((len, addr.to_string())),
            Ok(Err(e)) => Err(to_socket_error(e)),
            Err(_) => Err(SocketError::TimedOut),
        }
    }
}

/// Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum.
pub fn to_socket_error(err: std::io::Error) -> SocketError {
    match err.kind() {
        std::io::ErrorKind::ConnectionRefused => SocketError::ConnectionRefused,
        std::io::ErrorKind::ConnectionAborted => SocketError::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset => SocketError::ConnectionReset,
        std::io::ErrorKind::NotConnected => SocketError::NotConnected,
        std::io::ErrorKind::TimedOut => SocketError::TimedOut,
        std::io::ErrorKind::AddrInUse => SocketError::AddressInUse,
        std::io::ErrorKind::AddrNotAvailable => SocketError::AddressNotAvailable,
        std::io::ErrorKind::InvalidInput => SocketError::InvalidInput,
        _ => SocketError::Other("Native OS platform IO error occurred"),
    }
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

/// Builds an unsafe `ClientConfig` containing the `NoCertificateVerification` verifier.
///
/// This provides standard support for local LAN mode printer handshakes
/// without needing a pre-installed root trust anchor.
pub fn build_unsafe_client_config() -> Arc<rustls::ClientConfig> {
    let verifier = Arc::new(NoCertificateVerification);

    // Configures client with Ring cryptographic provider, allowing self-signed handshakes.
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .expect("Protocols must be initialized successfully")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    Arc::new(config)
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
        _port: u16,
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
}
