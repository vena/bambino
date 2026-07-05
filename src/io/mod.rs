//! # Transport Abstraction Layer
//!
//! Defines the async I/O traits that let the rest of the crate work without knowing
//! which runtime it's running on. The key traits:
//!
//! - [`AsyncIo`] — Read + Write (blanket-implemented for anything satisfying `embedded-io-async`).
//! - [`TlsConnector`] — Wraps a raw stream in TLS (used by tokio/rustls and embassy/embedded-tls).
//! - [`RawStreamFactory`] — Dials a fresh raw (pre-TLS) stream to a host:port. Used for MQTT's
//!   lazy connect and FTPS's per-transfer data channel.
//! - [`AsyncUdpSocket`] — UDP send/recv for SSDP discovery.
//! - [`BindableUdpSocket`] — construct-and-bind a new UDP socket by address (std/tokio, ESP-IDF only).
//! - [`TimerProvider`] — Async sleep and monotonic clock for platform-agnostic timeouts.
//!
//! Platform implementations live in the `tokio`, `esp_idf`, and `embassy` submodules
//! (each gated behind its respective feature flag).
//! The [`TokioIo`] adapter bridges Tokio's `AsyncRead`/`AsyncWrite` to `embedded-io-async`.

#[cfg(feature = "tokio")]
pub mod tokio;
#[cfg(feature = "tokio")]
pub use tokio::{TokioIo, TokioIoError};

#[cfg(feature = "esp-idf")]
pub mod esp_idf;

#[cfg(feature = "embassy")]
pub mod embassy;

use core::future::{Future, poll_fn};
use core::net::SocketAddr;
use core::pin::pin;
use core::task::Poll;

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

/// Configures a `std::net::UdpSocket` for SSDP discovery: enables broadcast, joins the
/// standard Bambu multicast group (239.255.255.250) — on macOS and Windows, local firewalls
/// and kernel routing stacks frequently drop incoming UDP replies from SSDP targets on
/// ephemeral ports unless the receiving socket has registered a multicast group membership
/// first — and puts the socket into non-blocking mode. Shared by every std-based platform
/// backend that binds its own UDP socket (`TokioUdpSocket::bind`, `EspIdfUdpSocket::bind`);
/// `set_broadcast`/`join_multicast_v4` failures are logged and otherwise ignored (best-effort,
/// not fatal to discovery), while a `set_nonblocking` failure is returned since every caller
/// requires it (Tokio panics on thread-local registration otherwise; ESP-IDF's recv pacing
/// assumes it).
#[cfg(feature = "std")]
pub(crate) fn configure_std_udp_socket(socket: &std::net::UdpSocket) -> Result<(), SocketError> {
    if let Err(e) = socket.set_broadcast(true) {
        log::debug!("configure_std_udp_socket: set_broadcast failed: {e}");
    }
    let multiaddr = std::net::Ipv4Addr::new(239, 255, 255, 250);
    let interface = std::net::Ipv4Addr::new(0, 0, 0, 0);
    if let Err(e) = socket.join_multicast_v4(&multiaddr, &interface) {
        log::debug!("configure_std_udp_socket: join_multicast_v4 failed: {e}");
    }
    socket
        .set_nonblocking(true)
        .map_err(|e| map_std_io_error(e, "failed to set UDP socket non-blocking"))
}

/// Maps a `std::io::ErrorKind` to the closest `embedded_io_async::ErrorKind`. Shared by every
/// std-based platform's `embedded_io_async::Error::kind()` impl (`TokioIoError`,
/// `EspIdfIoError`) — both previously duplicated this exact match.
#[cfg(feature = "std")]
pub(crate) fn map_io_error_kind(kind: std::io::ErrorKind) -> embedded_io_async::ErrorKind {
    match kind {
        std::io::ErrorKind::ConnectionRefused => embedded_io_async::ErrorKind::ConnectionRefused,
        std::io::ErrorKind::ConnectionAborted => embedded_io_async::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset => embedded_io_async::ErrorKind::ConnectionReset,
        std::io::ErrorKind::NotConnected => embedded_io_async::ErrorKind::NotConnected,
        std::io::ErrorKind::TimedOut => embedded_io_async::ErrorKind::TimedOut,
        std::io::ErrorKind::AddrInUse => embedded_io_async::ErrorKind::AddrInUse,
        std::io::ErrorKind::AddrNotAvailable => embedded_io_async::ErrorKind::AddrNotAvailable,
        _ => embedded_io_async::ErrorKind::Other,
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
/// this trait. Mirrors the existing `TlsConnector`/`RawStreamFactory` split, which draws the
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

/// Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port.
///
/// Protocol-neutral by design: MQTT's lazy connect (`PrinterClient::ensure_mqtt`) and FTPS's
/// per-transfer passive data channel (`BambuFtpsClient::list_directory`/`upload_file`/
/// `download_file`) both just need "give me a raw stream to host:port" with no
/// protocol-specific semantics in the trait itself — confirmed by every implementor
/// (`TokioRawStreamFactory`, `EspIdfRawStreamFactory`, `EmbassyRawStreamFactory`) having zero
/// FTP- or MQTT-specific logic. Non-consuming (`&self`) so a `PrinterClient` can hold one
/// persistently and call it on every lazy (re)connect, mirroring `TlsConnector::connect`.
#[allow(async_fn_in_trait)]
pub trait RawStreamFactory<RawIO: AsyncIo> {
    /// Connects a raw, un-encrypted socket to the designated host and port.
    async fn dial(&self, host: &str, port: u16) -> Result<RawIO, SocketError>;
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

/// Outcome of [`race`] — which of the two raced futures completed first.
pub(crate) enum Raced<A, B> {
    Left(A),
    Right(B),
}

/// Polls two futures concurrently, resolving to whichever completes first.
///
/// Built entirely on `core::future`/`core::task` primitives (`poll_fn` + `pin!`) rather
/// than a runtime-specific macro (`tokio::select!`) or an external crate
/// (`embassy_futures::select`), so the exact same code compiles and behaves identically
/// on tokio (host), ESP-IDF (std), and bare-metal Embassy (no_std) — no per-platform
/// `#[cfg]` branching needed. If both futures happen to be ready on the same poll, `a`
/// wins arbitrarily (checked first).
///
/// `pub(crate)` — reused by `PrinterClient::ensure_mqtt`/`ensure_ftps` (`src/client/mod.rs`) to
/// race their two-step dial+connect sequences against a connect-timeout deadline, and by
/// `BambuBinaryCameraStream::read_next_frame_with_timer` (`src/camera/binary.rs`) for the same
/// per-read deadline purpose `mqtt::client`'s own `read_chunk`/`read_exact_packet` is built on.
pub(crate) async fn race<A, B>(a: A, b: B) -> Raced<A::Output, B::Output>
where
    A: Future,
    B: Future,
{
    let mut a = pin!(a);
    let mut b = pin!(b);
    poll_fn(move |cx| {
        if let Poll::Ready(v) = a.as_mut().poll(cx) {
            return Poll::Ready(Raced::Left(v));
        }
        if let Poll::Ready(v) = b.as_mut().poll(cx) {
            return Poll::Ready(Raced::Right(v));
        }
        Poll::Pending
    })
    .await
}

/// Reads up to `buf.len()` bytes via a single underlying `read()` call, optionally raced
/// against a wall-clock deadline.
///
/// **Why a single `read()` step (not `read_exact`, and not the whole multi-byte target
/// in one shot):** `embedded_io_async::Read::read_exact`'s default implementation writes
/// directly into the caller's buffer across a loop of multiple internal `read()` calls.
/// If a future built on it is dropped mid-loop — exactly what happens when the timeout
/// side of a race wins — there is no way to learn how many of those internal calls had
/// already landed bytes, so the caller can't know how much of the buffer is valid.
/// Racing one `read()` step at a time instead means a "timeout wins" outcome only ever
/// discards *zero-progress* state: either this step's `read()` had not yet returned
/// (nothing written to `buf`, safe to retry) or it already returned and we recorded
/// exactly how many bytes landed via its `Ok(n)` before the timeout was even considered.
/// The residual risk of the underlying transport silently consuming bytes during a
/// cancelled `read()` without reporting them back is inherent to any cancellable I/O
/// primitive (platform-dependent, unavoidable at this layer) — a single small `read`
/// step minimizes that exposure relative to racing one large atomic multi-byte read.
///
/// `deadline_ms` is `None` when `timer` has no real wall-clock (see
/// [`TimerProvider::has_real_clock`] — notably the default `DummyTimer`), in which case
/// this degrades to a plain unbounded `read()`, identical to this crate's behavior
/// before per-read deadlines existed.
pub(crate) async fn read_chunk<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    buf: &mut [u8],
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<usize, SocketError> {
    let Some(deadline_ms) = deadline_ms else {
        return match stream.read(buf).await {
            Ok(0) => Err(SocketError::ConnectionReset), // peer closed the stream
            Ok(n) => Ok(n),
            Err(e) => {
                log::trace!("read failed: {:?}", e);
                Err(SocketError::ConnectionReset)
            }
        };
    };

    let remaining_ms = deadline_ms.saturating_sub(timer.now_millis());
    if remaining_ms == 0 {
        return Err(SocketError::TimedOut);
    }

    let read_fut = stream.read(buf);
    let sleep_fut = timer.sleep(core::time::Duration::from_millis(remaining_ms));

    match race(read_fut, sleep_fut).await {
        Raced::Left(Ok(0)) => Err(SocketError::ConnectionReset), // peer closed the stream
        Raced::Left(Ok(n)) => Ok(n),
        Raced::Left(Err(e)) => {
            log::trace!("read failed: {:?}", e);
            Err(SocketError::ConnectionReset)
        }
        Raced::Right(_) => Err(SocketError::TimedOut),
    }
}
