//! # Transport Abstraction Layer
//!
//! Defines the async I/O traits that let the rest of the crate work without knowing
//! which runtime it's running on. The key traits:
//!
//! - [`AsyncIo`] — Read + Write (blanket-implemented for anything satisfying `embedded-io-async`).
//! - [`TlsConnector`] — Wraps a raw stream in TLS (used by tokio/rustls and embassy/embedded-tls).
//! - [`SecureConnect`] — Creates its own TCP+TLS connection (used by ESP-IDF, where TLS manages transport).
//! - [`AsyncUdpSocket`] — UDP send/recv for SSDP discovery.
//! - [`BindableUdpSocket`] — construct-and-bind a new UDP socket by address (std/tokio, ESP-IDF only).
//! - [`TimerProvider`] — Async sleep and monotonic clock for platform-agnostic timeouts.
//!
//! Platform implementations live in the `tokio`, `esp_idf`, and `embassy` submodules
//! (each gated behind its respective feature flag).
//! The [`TokioIo`] adapter bridges Tokio's `AsyncRead`/`AsyncWrite` to `embedded-io-async`.

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "esp-idf")]
pub mod esp_idf;

#[cfg(feature = "embassy")]
pub mod embassy;

use core::net::SocketAddr;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::borrow::Cow;

/// Unified transport-level Socket Errors, agnostic of runtime implementations.
///
/// `Other` carries `Cow<'static, str>` (not `Copy`, hence the enum overall isn't either)
/// rather than a fixed `&'static str` so platform backends can attach dynamic content —
/// e.g. ESP-IDF's error mapping (`src/io/esp_idf.rs::map_esp_tls_connect_error`) formats
/// the actual numeric `EspError` code into the message instead of a fixed compile-time
/// string. Mirrors `BambuError::ProtocolViolation`'s existing use of the same type for the
/// same reason (dynamic message content in a `no_std`+`alloc`-compatible way).
#[derive(Debug, Clone, PartialEq, Eq)]
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
    Other(Cow<'static, str>),
}

/// Maps standard library IO error kinds to the runtime-agnostic `SocketError` enum.
///
/// Shared by every platform backend that surfaces `std::io::Error` (tokio, ESP-IDF).
/// `other_msg` fills `SocketError::Other` for kinds with no direct mapping — pass a
/// platform-specific message so the catch-all error stays attributable.
#[cfg(feature = "std")]
pub(crate) fn map_std_io_error(err: std::io::Error, other_msg: &'static str) -> SocketError {
    match err.kind() {
        std::io::ErrorKind::ConnectionRefused => SocketError::ConnectionRefused,
        std::io::ErrorKind::ConnectionAborted => SocketError::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset => SocketError::ConnectionReset,
        std::io::ErrorKind::NotConnected => SocketError::NotConnected,
        std::io::ErrorKind::TimedOut => SocketError::TimedOut,
        std::io::ErrorKind::AddrInUse => SocketError::AddressInUse,
        std::io::ErrorKind::AddrNotAvailable => SocketError::AddressNotAvailable,
        std::io::ErrorKind::InvalidInput => SocketError::InvalidInput,
        _ => {
            log::debug!("{other_msg}: {err}");
            SocketError::Other(Cow::Borrowed(other_msg))
        }
    }
}

/// Unified timer/sleep errors, agnostic of runtime implementations.
///
/// Mirrors [`SocketError`]'s shape. Tokio and Embassy sleeps are infallible, so only
/// ESP-IDF's `EspAsyncTimer` (which can fail on FreeRTOS timer/task resource exhaustion)
/// ever constructs this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    /// Catch-all for platform-specific timer scheduling failures.
    Other(&'static str),
}

/// TLS protocol version negotiated during a handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    Tls12,
    Tls13,
}

/// Consolidated Async Read + Write trait boundary.
///
/// Intermediates communication across all layers (MQTTS, FTPS, RTSPS, Port 6000).
/// Automatically implemented for any types satisfying the core `embedded-io-async` traits.
pub trait AsyncIo: embedded_io_async::Read + embedded_io_async::Write {}
impl<T: embedded_io_async::Read + embedded_io_async::Write> AsyncIo for T {}

/// Asynchronous UDP Socket trait for unicast and multicast printer discovery.
///
/// Interlaces with Port 2021 SSDP traffic defined in [REF-NET-DISC]. Implemented by every
/// platform on an already-existing socket. For constructing a *new* socket bound to an
/// address string, see [`BindableUdpSocket`] — kept separate because Embassy's network
/// stack cannot support it (see that trait's doc comment).
#[allow(async_fn_in_trait)]
pub trait AsyncUdpSocket {
    /// Dispatches a raw datagram payload to the given target address.
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>;

    /// Listens for incoming datagrams, populating the buffer and returning the source address.
    ///
    /// Implementations must not busy-spin: on a "no data yet" outcome, either genuinely
    /// block/wait for data, or internally yield for a bounded duration (e.g. via
    /// [`TimerProvider::sleep`]) before reporting `Err`. A synchronous non-blocking read
    /// that returns instantly on every "would block" call defeats the caller's own pacing
    /// — `discover_devices` (`src/discovery/mod.rs`) polls this in a tight loop relying on
    /// each call to provide some wait/yield, and an implementation that returns immediately
    /// turns that loop into a genuine busy-spin, burning 100% CPU and potentially starving
    /// other tasks on single-core/cooperative-scheduler platforms.
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError>;
}

/// Dynamically constructs a new UDP socket bound to a local address.
///
/// Only implementable on platforms with OS-level dynamic socket creation (std/tokio,
/// ESP-IDF's BSD sockets). Embassy-net sockets must be constructed from pre-allocated
/// buffer slices supplied by the caller and bound via a typed `IpListenEndpoint` on an
/// already-existing socket, not a `SocketAddr` — so `EmbassyUdpSocket` does not implement
/// this trait. Mirrors the existing `TlsConnector`/`SecureConnect` split, which draws the
/// same boundary for TLS connection setup.
#[allow(async_fn_in_trait)]
pub trait BindableUdpSocket: AsyncUdpSocket + Sized {
    /// Binds to the designated local address, constructing a new socket.
    async fn bind(addr: SocketAddr) -> Result<Self, SocketError>;
}

/// Abstract TLS secure stream connector trait.
///
/// Facilitates wrapping raw TCP transport interfaces inside secure SSL/TLS sessions
/// without enforcing a static library provider.
#[allow(async_fn_in_trait)]
pub trait TlsConnector<RawStream: AsyncIo> {
    /// The resulting encrypted socket stream type.
    type Stream: AsyncIo;

    /// Negotiates a secure TLS handshake with the targeted printer.
    async fn connect(&self, host: &str, raw_stream: RawStream)
    -> Result<Self::Stream, SocketError>;

    /// Returns the TLS protocol version negotiated on the given stream.
    ///
    /// Platforms that cannot inspect the negotiated version return `None`,
    /// which causes the FTPS client to skip TLS version validation (best-effort).
    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        None
    }
}

/// Platform-neutral asynchronous sleep controller.
///
/// Required to resolve post-boot handshakes, retry throttling, and camera frame pacing
/// without burning processor cycles on embedded platforms.
#[allow(async_fn_in_trait)]
pub trait TimerProvider {
    /// Suspends execution of the calling task for the specified duration.
    ///
    /// Returns an error on platforms where sleep scheduling can genuinely fail
    /// (e.g. ESP-IDF's FreeRTOS timer/task resources). Infallible platforms
    /// (tokio, embassy) always return `Ok(())`.
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>;

    /// Returns the current monotonic clock value in milliseconds.
    ///
    /// The epoch is platform-specific (process start, system boot, etc.) — only
    /// *differences* between two calls are meaningful.
    fn now_millis(&self) -> u64;

    /// Whether this timer provides genuine wall-clock timing. `true` (the default) for
    /// every real platform implementation (`TokioTimer`, `EmbassyTimer`, `EspIdfTimer`).
    /// Only `PrinterClient`'s `DummyTimer` default overrides this to `false`.
    ///
    /// Exists so code that races an I/O operation against
    /// [`sleep()`](Self::sleep) — e.g. `src/mqtt/client.rs`'s `poll_wire`/
    /// `read_exact_packet` per-read deadline — can tell whether doing so will actually
    /// bound anything. `DummyTimer::sleep()` intentionally completes instantly
    /// regardless of the requested duration (so it never blocks retry/backoff loops
    /// that happen to be generic over `TimerProvider`); racing against it would make
    /// such a race resolve to "timed out" on essentially every call that doesn't also
    /// complete synchronously, silently turning "no wall-clock timeout configured" into
    /// "everything times out immediately" instead of the intended "no wall-clock
    /// protection here, fall back to other safety valves" (the same tradeoff
    /// `PrinterClient::poll_until`'s elapsed-time check already documents for
    /// `DummyTimer`). Callers should check this before racing against `sleep()` and
    /// skip the race entirely (plain unbounded await) when it's `false`.
    fn has_real_clock(&self) -> bool {
        true
    }
}

/// Higher-level secure connection trait for platforms where TLS manages its own transport.
///
/// `TlsConnector` requires callers to supply a pre-established raw stream, which works
/// for rustls (tokio) and embedded-tls (embassy) where TLS wraps an existing socket.
/// ESP-IDF's `EspTls` creates its own TCP connection internally, so it cannot implement
/// `TlsConnector`. This trait abstracts over both models: callers provide host+port and
/// receive a ready-to-use secure stream.
#[allow(async_fn_in_trait)]
pub trait SecureConnect {
    /// The resulting encrypted stream type.
    type Stream: AsyncIo;

    /// Establishes a new secure connection to the specified host and port.
    async fn secure_connect(&self, host: &str, port: u16) -> Result<Self::Stream, SocketError>;

    /// Returns the TLS protocol version negotiated on the given stream.
    ///
    /// Mirrors [`TlsConnector::negotiated_version`] so callers built on `SecureConnect`
    /// have the same capability to enforce a minimum TLS version (e.g. the TLS 1.2
    /// requirement `BambuFtpsClient::connect()` enforces on P2S/X2D via `TlsConnector`)
    /// once a `SecureConnect`-based caller needs it. Platforms that cannot inspect the
    /// negotiated version return `None`, causing such a check to be skipped
    /// (best-effort) rather than failing to compile against this trait at all.
    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        None
    }
}

/// Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation
/// to satisfy `embedded-io-async` bounds.
#[cfg(feature = "tokio")]
pub struct TokioIo<T>(pub T);

/// Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.
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
