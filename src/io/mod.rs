//! # Transport Abstraction Layer
//!
//! Defines the async I/O traits that let the rest of the crate work without knowing
//! which runtime it's running on. The key traits:
//!
//! - [`AsyncIo`] — Read + Write (blanket-implemented for anything satisfying `embedded-io-async`).
//! - [`TlsConnector`] — Wraps a raw stream in TLS (used by tokio/rustls and embassy/mbedtls-rs).
//! - [`RawStreamFactory`] — Dials a fresh raw (pre-TLS) stream to a host:port. Used for MQTT's
//!   lazy connect and FTPS's per-transfer data channel.
//! - [`AsyncUdpSocket`] — UDP send/recv for SSDP discovery.
//! - [`BindableUdpSocket`] — construct-and-bind a new UDP socket by address (std/tokio, ESP-IDF only).
//! - [`TimerProvider`] — Async sleep and monotonic clock for platform-agnostic timeouts.
//!
//! Platform implementations live in the `tokio`, `esp_idf`, and `embassy` submodules
//! (each gated behind its respective feature flag).
//! The `TokioIo` adapter (only present when the `tokio` feature is enabled) bridges Tokio's `AsyncRead`/`AsyncWrite` to `embedded-io-async`.

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

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::vec::Vec;

/// Unified transport-level Socket Errors, agnostic of runtime implementations.
///
/// `Other` carries `Cow<'static, str>` (not `Copy`, hence the enum overall isn't either)
/// rather than a fixed `&'static str` so platform backends can attach dynamic content —
/// e.g. ESP-IDF's error mapping (`src/io/esp_idf.rs::map_esp_tls_connect_error`) formats
/// the actual numeric `EspError` code into the message instead of a fixed compile-time
/// string. Mirrors `Error::ProtocolViolation`'s existing use of the same type for the
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
    /// Peer's certificate was rejected during the TLS handshake, with the reason it failed.
    ///
    /// Separate from `Other` so a consumer can *act* on the distinction — offering
    /// trust-on-first-use certificate capture is only correct for
    /// [`CertificateFailure::UntrustedAnchor`], and prompting on any other cause would be a
    /// security-relevant misfire, not just poor UX. Every backend that can name the cause
    /// populates this; a backend that only knows "the handshake failed" still returns the
    /// error it always did rather than guessing a cause (GitHub issue #157).
    CertificateInvalid(CertificateFailure),
    /// Catch-all variant for atypical OS-specific networking errors.
    Other(Cow<'static, str>),
}

/// Why a peer's certificate was rejected, in terms every backend can express.
///
/// Deliberately coarser than any one backend's native error type: rustls names ~20 distinct
/// certificate errors and mbedTLS reports a bitmask of ~14 flags, but a caller acts on far
/// fewer distinctions than that. What matters is that "the chain reached no trusted anchor"
/// and "the chain verified and the name did not match" never collapse into each other — they
/// are different problems with different remedies.
///
/// `Unspecified` means the backend rejected the certificate for a reason with no portable
/// counterpart here (rustls's `UnhandledCriticalExtension`, mbedTLS's `BADCERT_OTHER`, …). It
/// is a *lack of detail*, never "probably fine" — the handshake failed either way.
///
/// Revocation *status* has no variant: [`Revoked`](Self::Revoked) means a certificate was
/// positively revoked, and nothing here reports "revocation could not be checked". That is
/// deliberate — this crate performs no CRL or OCSP lookup and Bambu certificates carry no CRL
/// distribution points, so a variant for it could never be produced. Add one only alongside a
/// backend that can actually reach that state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateFailure {
    /// Chain terminated at no anchor the caller supplied. The one cause for which offering
    /// to capture and pin the presented certificate is a sensible remedy.
    ///
    /// Broader than "the peer is self-signed": the tokio backend's `CnFallbackServerVerifier`
    /// also reports a chain hop rejected for a *non-CA-capable* intermediate this way (see
    /// `check_ca_capable` in `io/tokio/cert_verify.rs`, which chooses `UnknownIssuer`
    /// deliberately). Trust-on-first-use is unauthenticated at first contact by construction,
    /// so this doesn't weaken it — but don't read this variant as "a benign self-signed
    /// printer certificate".
    UntrustedAnchor,
    /// Chain verified, but no name in the certificate matched the host that was dialed.
    NameMismatch,
    /// A certificate in the chain is past its `notAfter`.
    Expired,
    /// A certificate in the chain is before its `notBefore` — on an embedded target this is
    /// far more often an unset system clock than a genuinely premature certificate.
    NotYetValid,
    /// A certificate in the chain was revoked by its issuer.
    Revoked,
    /// The chain uses a signature algorithm, key type, or key size the backend refuses.
    UnsupportedAlgorithm,
    /// Chain and name are fine, but a certificate is not authorized for this use — key usage,
    /// extended key usage, or Netscape cert type forbids TLS server authentication.
    ///
    /// Explicitly *not* a trust-on-first-use candidate, unlike
    /// [`UntrustedAnchor`](Self::UntrustedAnchor): capturing and pinning the certificate
    /// changes nothing, because it will be rejected for the same reason on every later
    /// connection. Usually the wrong certificate installed, or one mis-issued by a proxy.
    InvalidPurpose,
    /// Peer presented no certificate at all — there is nothing to evaluate, and nothing a
    /// caller could capture or pin.
    Missing,
    /// A certificate could not be parsed at all.
    Malformed,
    /// Rejected for a reason with no portable counterpart above.
    Unspecified,
}

/// mbedTLS `MBEDTLS_X509_BADCERT_*` verification flags, as returned by
/// `mbedtls_ssl_get_verify_result`.
///
/// Redeclared here rather than imported from `esp_idf_svc::sys` or `mbedtls_rs_sys` so one
/// mapping serves both mbedTLS-backed platforms (ESP-IDF and Embassy) and stays unit-testable
/// on the host, where neither `-sys` crate is even built. These are stable public mbedTLS API
/// constants (`x509.h`), not internal values.
#[cfg(any(feature = "esp-idf", feature = "embassy", test))]
pub(crate) mod mbedtls_badcert {
    pub(crate) const EXPIRED: u32 = 0x01;
    pub(crate) const REVOKED: u32 = 0x02;
    pub(crate) const CN_MISMATCH: u32 = 0x04;
    pub(crate) const NOT_TRUSTED: u32 = 0x08;
    pub(crate) const MISSING: u32 = 0x40;
    pub(crate) const SKIP_VERIFY: u32 = 0x80;
    pub(crate) const FUTURE: u32 = 0x200;
    pub(crate) const KEY_USAGE: u32 = 0x800;
    pub(crate) const EXT_KEY_USAGE: u32 = 0x1000;
    pub(crate) const NS_CERT_TYPE: u32 = 0x2000;
    pub(crate) const BAD_MD: u32 = 0x4000;
    pub(crate) const BAD_PK: u32 = 0x8000;
    pub(crate) const BAD_KEY: u32 = 0x10000;
}

/// Reduces an mbedTLS verification bitmask to a single [`CertificateFailure`].
///
/// Returns `None` when the mask carries no verdict, so the caller keeps whatever error it
/// already had rather than being handed an invented certificate cause. Three masks mean that:
/// `0` (verification passed, so the handshake failed for some other reason), `u32::MAX`
/// (mbedTLS's "verification was never performed"), and — less obviously — a mask whose only
/// bit is `BADCERT_SKIP_VERIFY`, which mbedTLS sets when it *deliberately skipped* the check.
/// Reporting that as a rejection would assert a verdict about a check that never ran, the same
/// false statement the other two guards exist to prevent. `SKIP_VERIFY` is masked off rather
/// than short-circuited, so if real flags accompany it those were still measured and are still
/// reported.
///
/// mbedTLS sets *every* flag that applies, so the order below is a precedence, not a match:
/// the most fundamental defect wins, where "fundamental" means how early it makes the
/// certificate unusable regardless of everything else.
///
/// - `Missing` first — nothing was presented, so no other flag can mean anything.
/// - Then the defects no remediation reaches: an unusable key or digest, then a certificate
///   not authorized for this use at all.
/// - Then lifecycle: revoked, expired, not yet valid.
/// - Then `NOT_TRUSTED` above `CN_MISMATCH`. A self-signed proxy presenting the wrong name
///   sets both, and reporting the anchor problem is what lets a caller route it to
///   certificate capture. An *expired* untrusted certificate reports `Expired` for the mirror
///   reason: capturing it would pin something already invalid.
///
/// The full mask is logged at debug so nothing is lost.
#[cfg(any(feature = "esp-idf", feature = "embassy", test))]
pub(crate) fn map_mbedtls_verify_flags(flags: u32) -> Option<CertificateFailure> {
    use mbedtls_badcert as f;

    if flags == 0 || flags == u32::MAX {
        return None;
    }

    log::debug!("mbedTLS certificate verification flags: {flags:#x}");

    let measured = flags & !f::SKIP_VERIFY;
    if measured == 0 {
        log::debug!("mbedTLS skipped certificate verification; reporting no certificate verdict");
        return None;
    }

    let failure = if measured & f::MISSING != 0 {
        CertificateFailure::Missing
    } else if measured & (f::BAD_MD | f::BAD_PK | f::BAD_KEY) != 0 {
        CertificateFailure::UnsupportedAlgorithm
    } else if measured & (f::KEY_USAGE | f::EXT_KEY_USAGE | f::NS_CERT_TYPE) != 0 {
        CertificateFailure::InvalidPurpose
    } else if measured & f::REVOKED != 0 {
        CertificateFailure::Revoked
    } else if measured & f::EXPIRED != 0 {
        CertificateFailure::Expired
    } else if measured & f::FUTURE != 0 {
        CertificateFailure::NotYetValid
    } else if measured & f::NOT_TRUSTED != 0 {
        CertificateFailure::UntrustedAnchor
    } else if measured & f::CN_MISMATCH != 0 {
        CertificateFailure::NameMismatch
    } else {
        CertificateFailure::Unspecified
    };

    Some(failure)
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

/// Configures a `std::net::UdpSocket` for SSDP discovery: enables broadcast, joins the standard Bambu multicast group (239.255.255.250) — on macOS and Windows, local firewalls and kernel routing stacks frequently drop incoming UDP replies from SSDP targets on ephemeral ports unless the receiving socket has registered a multicast group membership first — and puts the socket into non-blocking mode.
/// Shared by every std-based platform backend that binds its own UDP socket
/// (`TokioUdpSocket::bind`, `EspIdfUdpSocket::bind`); `set_broadcast`/`join_multicast_v4` failures
/// are logged and otherwise ignored (best-effort, not fatal to discovery), while a
/// `set_nonblocking` failure is returned since every caller requires it (Tokio panics on
/// thread-local registration otherwise; ESP-IDF's recv pacing assumes it).
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

/// Maps a `std::io::ErrorKind` to the closest `embedded_io_async::ErrorKind`.
/// Shared by every std-based platform's `embedded_io_async::Error::kind()` impl (`TokioIoError`,
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
    /// TLS 1.2 negotiated.
    Tls12,
    /// TLS 1.3 negotiated.
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
    /// Platforms that cannot inspect the negotiated version return `None`. This does **not**
    /// mean "skip validation" — a caller enforcing a specific version (e.g.
    /// `FtpsClient::require_tls_1_2_if_enforced`, which P2S/X2D need) treats an
    /// undetermined `None` as a failure to confirm the required version and rejects the
    /// connection, the same as a confirmed wrong version. `None` only means "this platform
    /// has nothing useful to report" — whether that's fail-open or fail-closed is entirely
    /// up to the caller.
    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        None
    }

    /// Returns the peer's certificate chain exactly as presented during the handshake,
    /// DER-encoded, leaf first.
    ///
    /// Exists so a consumer can pin a printer's certificate — this crate ships no CA material
    /// and deliberately treats certificates as runtime input, so trust-on-first-use has to be
    /// built on top of it. Storage and policy (where a pin lives, what happens on a mismatch)
    /// belong to the caller; all this provides is read access to what the peer actually sent.
    ///
    /// Returns `None` where the platform cannot report it — a *lack of information*, never
    /// "the peer sent nothing" and never "skip validation". A caller enforcing a pin must treat
    /// `None` as a failure to confirm, exactly as `negotiated_version` documents above.
    ///
    /// Whether the chain contains the issuing CA or only the leaf is up to the peer, and that
    /// decides what pinning is possible. Confirmed on a P1S: two certificates, the `CN=<serial>`
    /// leaf followed by the self-signed `CN=BBL CA` root (`CA:TRUE`) — so an anchor *can* be
    /// captured at first contact and fed back through `with_certs(..)` for genuine chain
    /// verification, rather than being limited to a leaf-fingerprint comparison. Only the P1S has
    /// been checked; use `bambino-cli inspect-cert` to confirm any other model rather than
    /// assuming it generalizes.
    ///
    /// The returned DER is copied out of the live session: on ESP-IDF the chain is owned by the
    /// SSL context and is freed on drop or renegotiation, so borrowing it would dangle.
    fn peer_chain_der(&self, _stream: &Self::Stream) -> Option<Vec<Vec<u8>>> {
        None
    }
}

/// Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port.
///
/// Protocol-neutral by design: MQTT's lazy connect (`PrinterClient::ensure_mqtt`) and FTPS's
/// per-transfer passive data channel (`FtpsClient::list_directory`/`upload_file`/
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

    /// Whether this timer provides genuine wall-clock timing.
    /// `true` (the default) for every real platform implementation (`TokioTimer`, `EmbassyTimer`,
    /// `EspIdfTimer`). Only `PrinterClient`'s `DummyTimer` default overrides this to `false`.
    ///
    /// Exists so code that races an I/O operation against
    /// [`sleep()`](Self::sleep) — e.g. `src/mqtt/client/mod.rs`'s `poll_wire`/
    /// `src/mqtt/client/frame.rs`'s `read_exact_packet` per-read deadline — can tell whether
    /// doing so will actually bound anything. `DummyTimer::sleep()` intentionally completes
    /// instantly regardless of the requested duration (so it never blocks retry/backoff loops
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
/// `BinaryCameraStream::read_next_frame_with_timer` (`src/camera/binary.rs`) for the same
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

/// Maps an `embedded_io_async::ErrorKind` (the only info a generic `AsyncIo::read()` error
/// exposes across every platform, std or no_std) to the closest `SocketError` variant.
///
/// Unconditional (not `#[cfg(feature = "std")]`) — unlike `map_std_io_error`, this only needs
/// `embedded_io_async::ErrorKind`, which every `AsyncIo` implementor (tokio, ESP-IDF, Embassy)
/// already produces via `embedded_io_async::Error::kind()`. Used by `read_chunk` so a genuine
/// `ConnectionRefused`/`TimedOut`/etc. isn't collapsed to a generic `ConnectionReset` the way it
/// was before this mapping existed.
pub(crate) fn map_embedded_io_error_kind(kind: embedded_io_async::ErrorKind) -> SocketError {
    match kind {
        embedded_io_async::ErrorKind::ConnectionRefused => SocketError::ConnectionRefused,
        embedded_io_async::ErrorKind::ConnectionAborted => SocketError::ConnectionAborted,
        embedded_io_async::ErrorKind::ConnectionReset => SocketError::ConnectionReset,
        embedded_io_async::ErrorKind::NotConnected => SocketError::NotConnected,
        embedded_io_async::ErrorKind::TimedOut => SocketError::TimedOut,
        embedded_io_async::ErrorKind::AddrInUse => SocketError::AddressInUse,
        embedded_io_async::ErrorKind::AddrNotAvailable => SocketError::AddressNotAvailable,
        embedded_io_async::ErrorKind::InvalidInput => SocketError::InvalidInput,
        _ => SocketError::ConnectionReset,
    }
}

/// Reads up to `buf.len()` bytes via a single underlying `read()` call, optionally raced against a wall-clock deadline.
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
                Err(map_embedded_io_error_kind(embedded_io_async::Error::kind(&e)))
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
            Err(map_embedded_io_error_kind(embedded_io_async::Error::kind(&e)))
        }
        Raced::Right(Ok(())) => Err(SocketError::TimedOut),
        // A failing timer is not a timeout. `EspIdfTimer::sleep` returns `Err(TimerError)`
        // *before awaiting anything* when `new_async_timer()` fails (esp_timer slot
        // exhaustion), so `sleep_fut` is instantly ready and wins every race: reporting
        // TimedOut there made every read look like a zero-elapsed timeout and drove the
        // reconnect loop at full speed — the exact failure `has_real_clock()` exists to
        // prevent, through a door it cannot detect. Surface it as a distinct error so callers
        // stop retrying instead of spinning.
        Raced::Right(Err(e)) => {
            log::warn!("read deadline timer failed: {:?}", e);
            Err(SocketError::Other(Cow::Borrowed(
                "read deadline timer failed",
            )))
        }
    }
}

/// PEM line width, in base64 characters, per RFC 7468 §2 ("generators MUST wrap at 64").
#[cfg(any(feature = "esp-idf", test))]
const PEM_LINE_WIDTH: usize = 64;

/// Standard base64 alphabet (RFC 4648 §4) — the encoding PEM bodies use.
#[cfg(any(feature = "esp-idf", test))]
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Appends `data` to `out` as padded standard base64, wrapped to `PEM_LINE_WIDTH` columns.
///
/// Hand-rolled rather than pulling in a base64 crate: this is the only base64 encoder the
/// crate needs, it runs once per connector construction (not per packet), and every candidate
/// dependency would be a new transitive edge on the `esp-idf` target for ~20 lines of code.
#[cfg(any(feature = "esp-idf", test))]
fn append_base64_wrapped(data: &[u8], out: &mut Vec<u8>) {
    let mut column = 0;
    for chunk in data.chunks(3) {
        // Zero-extend a short final chunk; the padding count below discards the invented bits.
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        // 3 input bytes -> 4 output chars, minus one char per missing input byte, replaced by '='.
        let significant = chunk.len() + 1;
        for i in 0..4 {
            let byte = if i < significant {
                BASE64_ALPHABET[((triple >> (18 - 6 * i)) & 0x3F) as usize]
            } else {
                b'='
            };
            out.push(byte);
            column += 1;
            if column == PEM_LINE_WIDTH {
                out.push(b'\n');
                column = 0;
            }
        }
    }
    if column != 0 {
        out.push(b'\n');
    }
}

/// Encodes DER certificates as one NUL-terminated PEM bundle, or `None` if `certs` is empty.
///
/// **Why this exists at all.** mbedTLS picks its parse strategy from the buffer itself:
/// `mbedtls_x509_crt_parse` only takes the multi-certificate PEM loop when the buffer is
/// NUL-terminated *and* contains a `BEGIN CERTIFICATE` header, and otherwise calls
/// `mbedtls_x509_crt_parse_der`, which parses exactly one certificate and returns
/// (`components/mbedtls/mbedtls/library/x509_crt.c`, ESP-IDF v5.5.4). So concatenated DER
/// silently loads only the first anchor — no error, just a confusing verification failure
/// later. PEM is the only encoding that carries more than one trust anchor through
/// `esp_tls`'s single `cacert_buf`. See GitHub issue #145.
///
/// The trailing NUL is part of the contract: `esp_idf_svc::tls::X509::pem_until_nul` panics
/// without one, and mbedTLS's own format sniff requires it.
#[cfg(any(feature = "esp-idf", test))]
pub(crate) fn der_certs_to_pem_bundle(
    certs: impl IntoIterator<Item = Vec<u8>>,
) -> Option<Vec<u8>> {
    const HEADER: &[u8] = b"-----BEGIN CERTIFICATE-----\n";
    const FOOTER: &[u8] = b"-----END CERTIFICATE-----\n";

    let mut out: Vec<u8> = Vec::new();
    for der in certs {
        out.extend_from_slice(HEADER);
        append_base64_wrapped(&der, &mut out);
        out.extend_from_slice(FOOTER);
    }

    if out.is_empty() {
        return None;
    }

    out.push(0);
    Some(out)
}

/// Wraps a TLS peer name so `Display` renders a short prefix and masks the rest.
///
/// The `host` handed to a `TlsConnector::connect` is the printer's **serial number**, not its
/// IP — MQTT connects by serial for SNI and FTPS follows. A serial is treated as a credential
/// in this crate (root `CLAUDE.md`: never write one into a file here), so a log line naming
/// the peer must not print it whole: consumer logs get pasted into bug reports, and on
/// ESP-IDF they also go straight out the UART to whoever is watching.
///
/// Keeps the first three characters because that prefix is the model code
/// (`crate::models::resolve_model` keys off it) — useful when triaging, and not a secret. The
/// mask is fixed-width rather than one character per elided byte, so the serial's length
/// doesn't leak either. An empty name renders `<empty>` rather than a bare mask, keeping
/// "nothing was passed" distinguishable from "it was hidden".
///
/// Lives here rather than in `io/esp_idf.rs` (its only caller today) for the same reason
/// `der_certs_to_pem_bundle` does: this module compiles on the host, so the boundary cases
/// below are actually covered by `cargo test`, while nothing inside `io/esp_idf.rs` can be.
#[cfg(any(feature = "esp-idf", test))]
pub(crate) struct RedactedHost<'a>(pub(crate) &'a str);

#[cfg(any(feature = "esp-idf", test))]
impl core::fmt::Display for RedactedHost<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_empty() {
            return f.write_str("<empty>");
        }
        // `char_indices`, not byte slicing: a non-ASCII name would panic on a split landing
        // mid-codepoint, and one caller is a handshake-failure path where a panic is worst.
        let split = self.0.char_indices().nth(3).map_or(self.0.len(), |(i, _)| i);
        write!(f, "{}***", &self.0[..split])
    }
}

#[cfg(test)]
mod redacted_host_tests {
    use super::RedactedHost;
    #[cfg(not(feature = "std"))]
    use alloc::string::ToString as _;

    #[test]
    fn keeps_the_model_prefix_and_masks_the_rest() {
        // Shaped like a serial, not a real one.
        assert_eq!(RedactedHost("03WABCDEFGHIJ").to_string(), "03W***");
    }

    #[test]
    fn mask_width_does_not_track_the_hidden_length() {
        assert_eq!(
            RedactedHost("03WA").to_string(),
            RedactedHost("03WABCDEFGHIJKLMNOP").to_string()
        );
    }

    #[test]
    fn names_at_or_below_the_prefix_length_are_still_masked() {
        // No early return for a short name: the mask must not signal "nothing was elided",
        // or a 3-character name would be readable in full and marked as if it weren't.
        assert_eq!(RedactedHost("03W").to_string(), "03W***");
        assert_eq!(RedactedHost("0").to_string(), "0***");
    }

    #[test]
    fn empty_name_is_distinguishable_from_a_hidden_one() {
        assert_eq!(RedactedHost("").to_string(), "<empty>");
    }

    #[test]
    fn multibyte_name_splits_on_a_char_boundary_without_panicking() {
        // Each 'e' here is 2 bytes, so a byte-indexed split at 3 would panic.
        assert_eq!(RedactedHost("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}").to_string(), "\u{e9}\u{e9}\u{e9}***");
    }
}

#[cfg(test)]
mod verify_flag_tests {
    use super::mbedtls_badcert as f;
    use super::*;

    #[test]
    fn test_no_verdict_masks_return_none() {
        // 0 = verification passed (so the handshake failed for some other reason);
        // u32::MAX = mbedTLS never performed verification. Neither may be reported as a
        // certificate failure, or the caller would be told a cause that wasn't measured.
        assert_eq!(map_mbedtls_verify_flags(0), None);
        assert_eq!(map_mbedtls_verify_flags(u32::MAX), None);
    }

    #[test]
    fn test_skip_verify_alone_is_not_a_rejection() {
        // mbedTLS sets SKIP_VERIFY when it deliberately did not check. Reporting that as a
        // certificate rejection asserts a verdict about a check that never ran.
        assert_eq!(map_mbedtls_verify_flags(f::SKIP_VERIFY), None);
    }

    #[test]
    fn test_skip_verify_does_not_suppress_real_flags() {
        // Masked off, not short-circuited: anything accompanying it was still measured.
        assert_eq!(
            map_mbedtls_verify_flags(f::SKIP_VERIFY | f::NOT_TRUSTED),
            Some(CertificateFailure::UntrustedAnchor)
        );
    }

    #[test]
    fn test_missing_outranks_every_other_flag() {
        // Nothing was presented, so no other flag can mean anything.
        assert_eq!(
            map_mbedtls_verify_flags(f::MISSING),
            Some(CertificateFailure::Missing)
        );
        assert_eq!(
            map_mbedtls_verify_flags(f::MISSING | f::BAD_KEY | f::NOT_TRUSTED),
            Some(CertificateFailure::Missing)
        );
    }

    #[test]
    fn test_purpose_flags_do_not_reach_untrusted_anchor() {
        // The distinction that matters here is against `UntrustedAnchor`: a certificate barred
        // from server auth is not a trust-on-first-use candidate, so it must never arrive as
        // the one verdict that opens a capture prompt.
        for purpose_flag in [f::KEY_USAGE, f::EXT_KEY_USAGE, f::NS_CERT_TYPE] {
            assert_eq!(
                map_mbedtls_verify_flags(purpose_flag),
                Some(CertificateFailure::InvalidPurpose)
            );
            assert_eq!(
                map_mbedtls_verify_flags(purpose_flag | f::NOT_TRUSTED),
                Some(CertificateFailure::InvalidPurpose)
            );
        }
    }

    #[test]
    fn test_trust_and_name_never_collapse() {
        // The distinction GitHub issue #157 exists for.
        assert_eq!(
            map_mbedtls_verify_flags(f::NOT_TRUSTED),
            Some(CertificateFailure::UntrustedAnchor)
        );
        assert_eq!(
            map_mbedtls_verify_flags(f::CN_MISMATCH),
            Some(CertificateFailure::NameMismatch)
        );
    }

    #[test]
    fn test_combined_flags_follow_documented_precedence() {
        // A self-signed proxy with the wrong name sets both; the anchor problem is the one a
        // caller can act on with certificate capture, so it wins.
        assert_eq!(
            map_mbedtls_verify_flags(f::NOT_TRUSTED | f::CN_MISMATCH),
            Some(CertificateFailure::UntrustedAnchor)
        );
        // Expiry outranks the anchor problem: capturing an already-expired certificate would
        // pin something invalid.
        assert_eq!(
            map_mbedtls_verify_flags(f::NOT_TRUSTED | f::EXPIRED),
            Some(CertificateFailure::Expired)
        );
        // A bad key/digest outranks everything else.
        assert_eq!(
            map_mbedtls_verify_flags(f::BAD_KEY | f::EXPIRED | f::NOT_TRUSTED),
            Some(CertificateFailure::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn test_remaining_single_flags() {
        assert_eq!(
            map_mbedtls_verify_flags(f::EXPIRED),
            Some(CertificateFailure::Expired)
        );
        assert_eq!(
            map_mbedtls_verify_flags(f::FUTURE),
            Some(CertificateFailure::NotYetValid)
        );
        assert_eq!(
            map_mbedtls_verify_flags(f::REVOKED),
            Some(CertificateFailure::Revoked)
        );
        for algo_flag in [f::BAD_MD, f::BAD_PK, f::BAD_KEY] {
            assert_eq!(
                map_mbedtls_verify_flags(algo_flag),
                Some(CertificateFailure::UnsupportedAlgorithm)
            );
        }
    }

    #[test]
    fn test_unmapped_flag_is_unspecified_not_none() {
        // MBEDTLS_X509_BADCERT_OTHER (0x100) has no portable counterpart, but the certificate
        // was still rejected — degrading it to `None` would let a caller read the failure as
        // something other than a certificate problem.
        assert_eq!(
            map_mbedtls_verify_flags(0x100),
            Some(CertificateFailure::Unspecified)
        );
    }

    #[test]
    fn test_crl_flags_do_not_masquerade_as_revoked() {
        // The BADCRL_* family means "revocation status could not be established", which is not
        // the same claim as BADCERT_REVOKED. This crate does no CRL lookup, so these are
        // unreachable today — the assertion is that if one ever does arrive it degrades to
        // `Unspecified` rather than asserting a certificate was positively revoked.
        const BADCRL_NOT_TRUSTED: u32 = 0x10;
        const BADCRL_EXPIRED: u32 = 0x20;
        assert_eq!(
            map_mbedtls_verify_flags(BADCRL_NOT_TRUSTED | BADCRL_EXPIRED),
            Some(CertificateFailure::Unspecified)
        );
    }
}

#[cfg(test)]
mod pem_bundle_tests {
    use super::*;

    /// Decodes a PEM bundle back to the DER payloads it carries, so round-trip tests don't
    /// have to hand-check base64.
    fn decode_bundle(pem: &[u8]) -> Vec<Vec<u8>> {
        let text = core::str::from_utf8(pem).expect("bundle is ASCII");
        let mut out = Vec::new();
        let mut body = String::new();
        let mut inside = false;
        for line in text.lines() {
            match line {
                "-----BEGIN CERTIFICATE-----" => {
                    inside = true;
                    body.clear();
                }
                "-----END CERTIFICATE-----" => {
                    inside = false;
                    out.push(decode_base64(&body));
                }
                _ if inside => {
                    assert!(
                        line.len() <= PEM_LINE_WIDTH,
                        "PEM body line exceeds {PEM_LINE_WIDTH} columns: {}",
                        line.len()
                    );
                    body.push_str(line);
                }
                _ => {}
            }
        }
        out
    }

    fn decode_base64(s: &str) -> Vec<u8> {
        let mut bits: u32 = 0;
        let mut nbits = 0;
        let mut out = Vec::new();
        for c in s.bytes() {
            if c == b'=' {
                break;
            }
            let v = BASE64_ALPHABET
                .iter()
                .position(|&a| a == c)
                .unwrap_or_else(|| panic!("non-base64 byte {c:#x} in PEM body")) as u32;
            bits = (bits << 6) | v;
            nbits += 6;
            if nbits >= 8 {
                nbits -= 8;
                out.push((bits >> nbits) as u8);
            }
        }
        out
    }

    #[test]
    fn empty_input_yields_no_bundle() {
        assert_eq!(der_certs_to_pem_bundle(Vec::<Vec<u8>>::new()), None);
    }

    #[test]
    fn bundle_is_nul_terminated_and_pem_framed() {
        let bundle = der_certs_to_pem_bundle([vec![1u8, 2, 3]]).expect("one cert");
        assert_eq!(bundle.last(), Some(&0), "X509::pem_until_nul requires a NUL");
        let text = core::str::from_utf8(&bundle[..bundle.len() - 1]).unwrap();
        assert!(text.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(text.ends_with("-----END CERTIFICATE-----\n"));
    }

    /// The whole point of #145: every anchor must survive, not just the first.
    #[test]
    fn all_certs_round_trip_in_order() {
        // Lengths chosen to hit each base64 padding case (len % 3 == 0, 1, 2) and to exceed
        // one wrapped line, since mis-wrapping is the classic hand-rolled-PEM bug.
        let certs: Vec<Vec<u8>> = vec![
            (0u8..=200).collect(),
            (0u8..=100).collect(),
            (0u8..=101).collect(),
            vec![0xFF],
        ];
        let bundle = der_certs_to_pem_bundle(certs.clone()).expect("non-empty");
        assert_eq!(decode_bundle(&bundle[..bundle.len() - 1]), certs);
    }

    /// Matches a known-good vector so the encoder can't be self-consistently wrong.
    #[test]
    fn base64_matches_reference_vectors() {
        for (input, expected) in [
            (&b""[..], ""),
            (&b"f"[..], "Zg==\n"),
            (&b"fo"[..], "Zm8=\n"),
            (&b"foo"[..], "Zm9v\n"),
            (&b"foob"[..], "Zm9vYg==\n"),
            (&b"fooba"[..], "Zm9vYmE=\n"),
            (&b"foobar"[..], "Zm9vYmFy\n"),
        ] {
            let mut out = Vec::new();
            append_base64_wrapped(input, &mut out);
            assert_eq!(core::str::from_utf8(&out).unwrap(), expected);
        }
    }

    /// A 48-byte input encodes to exactly 64 base64 chars — the boundary where an off-by-one
    /// would emit either a 65-column line or a stray blank line.
    #[test]
    fn exact_line_width_boundary_wraps_once() {
        let mut out = Vec::new();
        append_base64_wrapped(&vec![0u8; 48], &mut out);
        assert_eq!(out.len(), PEM_LINE_WIDTH + 1);
        assert_eq!(out.last(), Some(&b'\n'));
        assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 1);
    }
}
