//! # Shared In-Memory I/O Mock Primitives
//!
//! Provides dummy TLS connectors and dynamic stream factories to allow
//! integration tests to run entirely in-memory over `tokio::io::duplex` pipes.
//!
//! **Why this is necessary:**
//! Establishing actual TLS sessions in tests requires generating and trusting
//! self-signed certificates, which makes tests flaky and dependent on the host OS's
//! networking stack. These dummies strip away the cryptographic and physical transport
//! layers, allowing us to verify the pure state-machine logic of our protocol clients.

use std::sync::Arc;
use tokio::sync::Mutex;

use bambino::ftps::FtpDataStreamFactory;
use bambino::io::{AsyncIo, SocketError, TlsConnector, TlsVersion, TokioIo};

/// A pass-through TLS connector for testing.
///
/// Immediately returns the raw stream unchanged without attempting any cryptographic
/// handshake. This allows tests to evaluate plaintext protocol interactions while
/// satisfying the client's strict `TlsConnector` trait bounds.
pub struct DummyTlsConnector;

impl<RawIO: AsyncIo> TlsConnector<RawIO> for DummyTlsConnector {
    type Stream = RawIO;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawIO,
    ) -> Result<Self::Stream, SocketError> {
        Ok(raw_stream)
    }
}

/// A pass-through TLS connector that reports a specific negotiated TLS version.
pub struct VersionReportingTlsConnector(pub Option<TlsVersion>);

impl<RawIO: AsyncIo> TlsConnector<RawIO> for VersionReportingTlsConnector {
    type Stream = RawIO;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawIO,
    ) -> Result<Self::Stream, SocketError> {
        Ok(raw_stream)
    }

    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        self.0
    }
}

/// A TLS connector that succeeds on the FTPS implicit control-channel port (990) but fails on
/// any other port — i.e. it always fails a PASV data-channel connect attempt.
///
/// Used to exercise the `poisoned` flag regression (review/ftps.md Phase 2): simulates a
/// data-channel TLS handshake failure after the server has already sent its `150`/`125`
/// "opening data connection" reply, to verify the control channel doesn't get left desynced.
pub struct FailingDataTlsConnector;

impl<RawIO: AsyncIo> TlsConnector<RawIO> for FailingDataTlsConnector {
    type Stream = RawIO;

    async fn connect(
        &self,
        _host: &str,
        port: u16,
        raw_stream: RawIO,
    ) -> Result<Self::Stream, SocketError> {
        if port == 990 {
            Ok(raw_stream)
        } else {
            Err(SocketError::ConnectionAborted)
        }
    }
}

/// A dynamic, in-memory stream factory for passive FTP data channels.
///
/// **Why this is used:**
/// Under FTPS, passive transfers (`PASV`) require the client to establish a brand new
/// socket connection back to the server. Since we are testing in-memory using duplex
/// streams, we cannot bind to real TCP ports. This factory holds a pre-allocated
/// loopback stream and yields it when the FTPS client attempts to dial the passive port.
pub struct MockDataStreamFactory {
    /// Container holding the pre-allocated duplex stream representing the passive channel.
    pub active_stream: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
}

impl FtpDataStreamFactory<TokioIo<tokio::io::DuplexStream>> for MockDataStreamFactory {
    async fn create_data_stream(
        &self,
        _host: &str,
        _port: u16,
    ) -> Result<TokioIo<tokio::io::DuplexStream>, SocketError> {
        let mut guard = self.active_stream.lock().await;
        // Yield the stream if available, otherwise simulate a standard TCP connection refusal
        guard.take().ok_or(SocketError::ConnectionRefused)
    }
}
