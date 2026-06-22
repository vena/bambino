//! # Transport Abstraction Layer
//!
//! Abstracts standard IO, secure TLS streams, UDP datagrams, and sleep timers
//! across standard, embedded, and bare-metal environments.
//!
//! Provides the `TokioIo` adaptor to translate Tokio's standard `AsyncRead` and
//! `AsyncWrite` trait bounds to the `embedded-io-async` boundaries used inside this crate.

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "esp-idf")]
pub mod esp_idf;

#[cfg(feature = "embassy")]
pub mod embassy;

#[cfg(feature = "std")]
use std::string::String;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::string::String;

/// Unified transport-level Socket Errors, agnostic of runtime implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketError {
    /// Remote socket explicitly refused connection.
    ConnectionRefused,
    /// Connection was terminated by the local or remote software stack.
    ConnectionAborted,
    /// Connection was abruptly terminated by the remote host.
    ConnectionReset,
    /// Socket is in an un-established state.
    NotConnected,
    /// Handshake, read, or write boundaries timed out.
    TimedOut,
    /// Local port or address is already bound by another process.
    AddressInUse,
    /// Bound interface address is no longer valid or accessible.
    AddressNotAvailable,
    /// Supplied connection or routing parameters are malformed.
    InvalidInput,
    /// Catch-all variant for atypical OS-specific networking errors.
    Other(&'static str),
}

/// Consolidated Async Read + Write trait boundary.
///
/// Intermediates communication across all layers (MQTTS, FTPS, RTSPS, Port 6000).
/// Automatically implemented for any types satisfying the core `embedded-io-async` traits.
pub trait AsyncIo: embedded_io_async::Read + embedded_io_async::Write {}
impl<T: embedded_io_async::Read + embedded_io_async::Write> AsyncIo for T {}

/// Asynchronous UDP Socket trait for unicast and multicast printer discovery.
///
/// Interlaces with Port 2021 SSDP traffic defined in [REF-NET-DISC].
pub trait AsyncUdpSocket: Sized {
    /// Binds to the designated local address.
    async fn bind(addr: &str) -> Result<Self, SocketError>;

    /// Dispatches a raw datagram payload to a specific IPv4 target.
    async fn send_to(&self, buf: &[u8], target: &str) -> Result<usize, SocketError>;

    /// Listens for incoming datagrams, populating the buffer and returning the source string.
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, String), SocketError>;
}

/// Abstract TLS secure stream connector trait.
///
/// Facilitates wrapping raw TCP transport interfaces inside secure SSL/TLS sessions
/// without enforcing a static library provider.
pub trait TlsConnector<RawStream: AsyncIo> {
    /// The resulting encrypted socket stream type.
    type Stream: AsyncIo;

    /// Negotiates a secure TLS handshake with the targeted printer.
    async fn connect(
        &self,
        host: &str,
        port: u16,
        raw_stream: RawStream,
    ) -> Result<Self::Stream, SocketError>;
}

/// Platform-neutral asynchronous sleep controller.
///
/// Required to resolve post-boot handshakes, retry throttling, and camera frame pacing
/// without burning processor cycles on embedded platforms.
pub trait TimerProvider {
    /// Suspends execution of the calling task for the specified duration.
    async fn sleep(duration: core::time::Duration);
}

/// Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation
/// to satisfy `embedded-io-async` bounds.
#[cfg(feature = "tokio")]
pub struct TokioIo<T>(pub T);

#[cfg(feature = "tokio")]
#[derive(Debug)]
pub struct TokioIoError(pub std::io::Error);

// In embedded-io version 0.7+, the `embedded_io::Error` trait has a supertrait bound on `core::error::Error`.
// Therefore, we must implement both `core::fmt::Display` and `std::error::Error` for `TokioIoError`.

#[cfg(feature = "tokio")]
impl core::fmt::Display for TokioIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Tokio IO Error: {}", self.0)
    }
}

#[cfg(feature = "tokio")]
impl std::error::Error for TokioIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[cfg(feature = "tokio")]
impl embedded_io_async::Error for TokioIoError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        match self.0.kind() {
            std::io::ErrorKind::ConnectionRefused => {
                embedded_io_async::ErrorKind::ConnectionRefused
            }
            std::io::ErrorKind::ConnectionAborted => {
                embedded_io_async::ErrorKind::ConnectionAborted
            }
            std::io::ErrorKind::ConnectionReset => embedded_io_async::ErrorKind::ConnectionReset,
            std::io::ErrorKind::NotConnected => embedded_io_async::ErrorKind::NotConnected,
            std::io::ErrorKind::TimedOut => embedded_io_async::ErrorKind::TimedOut,
            std::io::ErrorKind::AddrInUse => embedded_io_async::ErrorKind::AddrInUse,
            std::io::ErrorKind::AddrNotAvailable => embedded_io_async::ErrorKind::AddrNotAvailable,
            _ => embedded_io_async::ErrorKind::Other,
        }
    }
}

/// Implement ErrorType for TokioIo as specified by the embedded-io-async 0.7 spec.
///
/// This separates error declaration from read/write trait implementations.
#[cfg(feature = "tokio")]
impl<T> embedded_io_async::ErrorType for TokioIo<T> {
    type Error = TokioIoError;
}

#[cfg(feature = "tokio")]
impl<T: ::tokio::io::AsyncRead + Unpin> embedded_io_async::Read for TokioIo<T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use ::tokio::io::AsyncReadExt;
        self.0.read(buf).await.map_err(TokioIoError)
    }
}

#[cfg(feature = "tokio")]
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
