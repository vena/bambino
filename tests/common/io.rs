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

use bambino::io::{AsyncIo, RawStreamFactory, SocketError, TlsConnector, TlsVersion, TokioIo};

/// A pass-through TLS connector for testing.
///
/// Immediately returns the raw stream unchanged without attempting any cryptographic
/// handshake. This allows tests to evaluate plaintext protocol interactions while
/// satisfying the client's strict `TlsConnector` trait bounds.
pub struct DummyTlsConnector;

impl<RawIO: AsyncIo> TlsConnector<RawIO> for DummyTlsConnector {
    type Stream = RawIO;

    async fn connect(&self, _host: &str, raw_stream: RawIO) -> Result<Self::Stream, SocketError> {
        Ok(raw_stream)
    }
}

/// A pass-through TLS connector that reports a specific negotiated TLS version.
pub struct VersionReportingTlsConnector(pub Option<TlsVersion>);

impl<RawIO: AsyncIo> TlsConnector<RawIO> for VersionReportingTlsConnector {
    type Stream = RawIO;

    async fn connect(&self, _host: &str, raw_stream: RawIO) -> Result<Self::Stream, SocketError> {
        Ok(raw_stream)
    }

    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        self.0
    }
}

/// A TLS connector that succeeds on the first `connect()` call (the FTPS implicit
/// control channel) but fails on every subsequent call — i.e. it always fails a PASV
/// data-channel connect attempt.
///
/// Used to exercise the `poisoned` flag regression (review/ftps.md Phase 2): simulates a
/// data-channel TLS handshake failure after the server has already sent its `150`/`125`
/// "opening data connection" reply, to verify the control channel doesn't get left desynced.
///
/// Tracks connection order via an `AtomicBool` rather than the target port — `TlsConnector`'s
/// `connect()` no longer takes a `port` parameter (`review/io.md` Phase 5.4: the raw stream is
/// already connected to its target port by the time `connect()` is called, so no implementer
/// needs it) — control-vs-data-channel is instead exactly "was this the first `connect()` call
/// on this instance," matching how `BambuFtpsClient` actually sequences connects (control
/// channel once in `connect()`, then one data-channel connect per transfer).
pub struct FailingDataTlsConnector {
    control_channel_connected: std::sync::atomic::AtomicBool,
}

impl FailingDataTlsConnector {
    pub fn new() -> Self {
        Self {
            control_channel_connected: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl<RawIO: AsyncIo> TlsConnector<RawIO> for FailingDataTlsConnector {
    type Stream = RawIO;

    async fn connect(&self, _host: &str, raw_stream: RawIO) -> Result<Self::Stream, SocketError> {
        let was_already_connected = self
            .control_channel_connected
            .swap(true, std::sync::atomic::Ordering::SeqCst);
        if was_already_connected {
            Err(SocketError::ConnectionAborted)
        } else {
            Ok(raw_stream)
        }
    }
}

/// A pass-through TLS connector that records the `host` string it was given, so tests can
/// assert *which* identity value (serial vs. IP) a connect call site actually sent — see
/// `.claude/rules/tls-identity-sni.md`.
pub struct HostCapturingTlsConnector {
    pub captured_host: Arc<Mutex<Option<String>>>,
}

impl HostCapturingTlsConnector {
    /// Returns the connector plus a cloned handle to its capture cell — grab the handle before
    /// handing the connector's ownership off to `connect()`, which consumes it.
    pub fn new() -> (Self, Arc<Mutex<Option<String>>>) {
        let captured_host = Arc::new(Mutex::new(None));
        (
            Self {
                captured_host: captured_host.clone(),
            },
            captured_host,
        )
    }
}

impl<RawIO: AsyncIo> TlsConnector<RawIO> for HostCapturingTlsConnector {
    type Stream = RawIO;

    async fn connect(&self, host: &str, raw_stream: RawIO) -> Result<Self::Stream, SocketError> {
        *self.captured_host.lock().await = Some(host.to_string());
        Ok(raw_stream)
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

impl RawStreamFactory<TokioIo<tokio::io::DuplexStream>> for MockDataStreamFactory {
    async fn dial(
        &self,
        _host: &str,
        _port: u16,
    ) -> Result<TokioIo<tokio::io::DuplexStream>, SocketError> {
        let mut guard = self.active_stream.lock().await;
        // Yield the stream if available, otherwise simulate a standard TCP connection refusal
        guard.take().ok_or(SocketError::ConnectionRefused)
    }
}
