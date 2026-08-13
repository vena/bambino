//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, RawStreamFactory, SocketError, TimerError, TimerProvider,
    TlsConnector, TlsVersion,
};

#[cfg(feature = "esp-idf")]
use core::net::SocketAddr;

/// Async timer utilizing the ESP-IDF high-resolution timer service.
///
/// Wraps `EspAsyncTimer` to provide non-blocking async sleep that integrates
/// with the FreeRTOS scheduler instead of blocking the task thread.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTimer {
    timer: core::cell::RefCell<::esp_idf_svc::timer::EspAsyncTimer>,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTimer {
    /// Constructs a new timer backed by a dedicated ESP-IDF high-resolution timer service.
    ///
    /// `EspIdfTcpStream::connect` and `EspIdfTlsConnector::connect` each allocate their own
    /// instance rather than sharing one — verified that 10,000 sequential
    /// allocate/drop cycles on both ESP32-C6 and ESP32-C3 hit zero failures, so the
    /// `esp_timer` slot cap isn't a practical concern here; see `esp32-hw-probe/`.
    pub fn new() -> Result<Self, ::esp_idf_svc::sys::EspError> {
        let service = ::esp_idf_svc::timer::EspTimerService::<::esp_idf_svc::timer::Task>::new()?;
        let timer = service.timer_async()?;
        Ok(Self {
            timer: core::cell::RefCell::new(timer),
        })
    }
}

#[cfg(feature = "esp-idf")]
impl TimerProvider for EspIdfTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        self.timer
            .borrow_mut()
            .after(duration)
            .await
            .map_err(|_| TimerError::Other("ESP-IDF hardware timer scheduling failed"))
    }

    fn now_millis(&self) -> u64 {
        // esp_timer_get_time() returns microseconds since boot as i64
        (unsafe { ::esp_idf_svc::sys::esp_timer_get_time() } as u64) / 1000
    }
}

/// Pacing sleep for `EspIdfUdpSocket::recv_from`'s WouldBlock path.
///
/// `EspIdfUdpSocket::recv_from` wraps a synchronous, non-blocking socket read with no
/// `.await` yield point of its own — without this sleep, a caller polling in a tight loop
/// (e.g. `discover_devices`, `src/discovery/mod.rs`) turns into a genuine busy-spin for the
/// full discovery window, burning 100% of whatever core/task runs it and risking FreeRTOS
/// idle-task watchdog trips on affected configs. SSDP discovery is not latency-sensitive,
/// so 10-20ms of added per-empty-read latency is a good trade; mirrors `TLS_POLL_INTERVAL`'s
/// pacing pattern.
#[cfg(feature = "esp-idf")]
const UDP_RECV_POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(15);

/// UDP Socket implementation designed for ESP-IDF's BSD Socket integration.
#[cfg(feature = "esp-idf")]
pub struct EspIdfUdpSocket {
    inner: std::net::UdpSocket,
    timer: EspIdfTimer,
}

#[cfg(feature = "esp-idf")]
impl BindableUdpSocket for EspIdfUdpSocket {
    async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let inner = std::net::UdpSocket::bind(addr).map_err(to_esp_socket_error)?;

        crate::io::configure_std_udp_socket(&inner)?;

        let timer = EspIdfTimer::new().map_err(|e| {
            log::debug!("failed to create ESP-IDF async timer for UDP recv pacing: {e}");
            SocketError::Other("failed to create ESP-IDF async timer for UDP recv pacing".into())
        })?;

        Ok(Self { inner, timer })
    }
}

#[cfg(feature = "esp-idf")]
impl AsyncUdpSocket for EspIdfUdpSocket {
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError> {
        self.inner.send_to(buf, target).map_err(to_esp_socket_error)
    }

    /// Non-blocking read paced with a short sleep on the WouldBlock path so this never busy-spins a caller polling in a tight loop — see `UDP_RECV_POLL_INTERVAL`'s doc comment.
    /// `TokioUdpSocket::recv_from` achieves the same pacing via a 100ms timeout wrapping a
    /// genuinely-blocking OS call; this platform has no async socket-readiness primitive for an
    /// arbitrary fd (see `TLS_POLL_INTERVAL`'s doc comment for why), so pacing is applied explicitly
    /// here instead.
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
        match self.inner.recv_from(buf) {
            Ok((len, addr)) => Ok((len, addr)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Err(e) = self.timer.sleep(UDP_RECV_POLL_INTERVAL).await {
                    log::debug!("EspIdfUdpSocket::recv_from: pacing sleep failed: {e:?}");
                }
                Err(SocketError::TimedOut)
            }
            Err(e) => Err(to_esp_socket_error(e)),
        }
    }
}

/// Helper mapping standard Rust IO errors to our ESP-IDF socket errors.
#[cfg(feature = "esp-idf")]
fn to_esp_socket_error(err: std::io::Error) -> SocketError {
    crate::io::map_std_io_error(err, "ESP-IDF platform BSD network error")
}

/// True if `err` indicates a non-blocking `connect()` is still in progress rather than a genuine failure.
/// `WouldBlock` covers whatever errno std's generic Unix `ErrorKind` decoder maps to it
/// (`EAGAIN`/`EWOULDBLOCK`); `EINPROGRESS` — the errno `connect()` actually returns for a pending
/// non-blocking connection — is checked separately because std's decoder does not recognize it as
/// `WouldBlock` (confirmed against `socket2`'s own `Socket::connect_timeout()`, which checks both
/// independently for the same reason).
#[cfg(feature = "esp-idf")]
fn is_connect_in_progress(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::WouldBlock
        || err.raw_os_error() == Some(::esp_idf_svc::sys::EINPROGRESS as i32)
}

/// Polls a non-blocking `connect()` to completion by alternately checking `SO_ERROR` (via `take_error()`) and connectedness (via `peer_addr()`, which fails with `NotConnected` until the three-way handshake finishes) — `take_error()` alone can't distinguish "still connecting, no error yet" from "connected successfully," both of which return `Ok(None)`.
/// Sleeps `TLS_POLL_INTERVAL` between attempts so the caller's outer `race_against_connect_timeout`
/// can preempt this loop; does not bound itself (see `EspIdfTcpStream::connect`'s doc comment for
/// why).
#[cfg(feature = "esp-idf")]
async fn poll_connect_until_complete(
    socket: &::socket2::Socket,
    timer: &EspIdfTimer,
) -> Result<(), SocketError> {
    loop {
        if let Some(err) = socket.take_error().map_err(to_esp_socket_error)? {
            return Err(to_esp_socket_error(err));
        }
        match socket.peer_addr() {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotConnected => {}
            Err(e) => return Err(to_esp_socket_error(e)),
        }
        timer.sleep(TLS_POLL_INTERVAL).await.map_err(|_| {
            SocketError::Other("ESP-IDF timer failed while polling TCP connect".into())
        })?;
    }
}

/// Poll interval between non-blocking TLS retry attempts (handshake and read/write).
///
/// `esp-idf-svc`/`esp-idf-hal` expose no async socket-readiness primitive for an
/// arbitrary fd (confirmed by inspecting `esp-idf-svc` 0.52.1's source, not just its
/// docs — the only async wait building block available is `EspAsyncTimer`). Real
/// wake-on-ready is possible via `esp_idf_svc::tls::EspAsyncTls` combined with the
/// `async-io` crate and `MountedEventfs`, but that needs a new dependency and real
/// app-side setup (a sized eventfd mount, a dedicated thread with a bumped stack, and
/// working around an ESP-IDF main-task/async-io-thread priority inversion) — left as a
/// future upgrade. This fixed-interval poll works because `Config::non_block = true` makes
/// every `EspTls` call return immediately instead of blocking inside the FFI call, so an outer
/// `TimerProvider`-based timeout can preempt the operation between poll attempts.
#[cfg(feature = "esp-idf")]
const TLS_POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(20);

/// Default upper bound on the handshake loop in `EspIdfTlsConnector::connect`, used when the caller doesn't supply one via `.with_connect_timeout(d)`.
/// Chosen generously — printers on a healthy LAN handshake in well under a second, but a 10s budget
/// avoids false timeouts on congested Wi-Fi.
#[cfg(feature = "esp-idf")]
const DEFAULT_CONNECT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(10);

/// True if `err` indicates the non-blocking TLS operation would have blocked and should be retried, rather than a real failure.
/// `EWOULDBLOCK` is included alongside the two `esp_tls`-specific codes because
/// `EspTls::connect`/`read`/`write` can surface it directly in non-blocking mode — documented
/// upstream as "a peculiarity/bug of the esp-tls C module".
#[cfg(feature = "esp-idf")]
fn is_would_block(err: &::esp_idf_svc::sys::EspError) -> bool {
    let code = err.code();
    code == ::esp_idf_svc::sys::EWOULDBLOCK as i32
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_READ
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_WRITE
}

/// Maps a non-WouldBlock `EspError` from a failed TLS connect/negotiate attempt to a
/// `SocketError`, distinguishing DNS/address failures and genuine connection refusals from
/// opaque/other failures instead of collapsing everything to `ConnectionRefused` (the
/// previous behavior — actively misleading for e.g. a bad CA cert or an out-of-memory
/// condition inside mbedTLS, both of which used to read as "refused").
///
/// Checks two families of codes, both surfaced by `EspTls::connect`/`negotiate` in practice:
/// `esp_tls`'s own `ESP_ERR_ESP_TLS_*` codes (returned when `esp_tls` fails before or during
/// the TCP dial itself, e.g. DNS resolution) and raw BSD errno codes (`ECONNREFUSED` etc.,
/// the same family `is_would_block` above already inspects for `EWOULDBLOCK`) that can also
/// surface directly. Anything not recognized falls back to `SocketError::Other`, with the
/// real code preserved at `log::debug!` — still an improvement over silently mapping to
/// `ConnectionRefused`, since `Other` doesn't claim a specific (and possibly wrong) cause.
#[cfg(feature = "esp-idf")]
fn map_esp_tls_connect_error(err: &::esp_idf_svc::sys::EspError) -> SocketError {
    let code = err.code();

    if code == ::esp_idf_svc::sys::ESP_ERR_ESP_TLS_CANNOT_RESOLVE_HOSTNAME
        || code == ::esp_idf_svc::sys::EHOSTUNREACH as i32
        || code == ::esp_idf_svc::sys::ENETUNREACH as i32
        || code == ::esp_idf_svc::sys::EADDRNOTAVAIL as i32
    {
        return SocketError::AddressNotAvailable;
    }

    if code == ::esp_idf_svc::sys::ESP_ERR_ESP_TLS_FAILED_CONNECT_TO_HOST
        || code == ::esp_idf_svc::sys::ECONNREFUSED as i32
    {
        return SocketError::ConnectionRefused;
    }

    if code == ::esp_idf_svc::sys::ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT
        || code == ::esp_idf_svc::sys::ETIMEDOUT as i32
    {
        return SocketError::TimedOut;
    }

    log::debug!("ESP-IDF TLS handshake failed: {err}");
    SocketError::Other(std::format!("ESP-IDF TLS handshake failed: {err}").into())
}

/// Cert bundle used by `EspIdfTlsConnector`'s `ca_cert`/`client_cert`/`client_key` fields and `new()`/`with_certs()` constructors.
/// Factored out so a future cert-related option (e.g. ALPN config) only needs to be added in
/// one place.
#[cfg(feature = "esp-idf")]
struct EspIdfTlsCerts {
    ca_cert: Option<Vec<u8>>,
    client_cert: Option<Vec<u8>>,
    client_key: Option<Vec<u8>>,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTlsCerts {
    fn new() -> Self {
        Self {
            ca_cert: None,
            client_cert: None,
            client_key: None,
        }
    }

    fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self {
        let (client_cert, client_key) = match client_auth {
            Some((cert, key)) => (Some(cert), Some(key)),
            None => (None, None),
        };
        Self {
            ca_cert: Some(ca_cert),
            client_cert,
            client_key,
        }
    }

    fn build_config(&self) -> ::esp_idf_svc::tls::Config<'_> {
        build_tls_config(&self.ca_cert, &self.client_cert, &self.client_key)
    }
}

/// Builds an `esp_idf_svc::tls::Config` from cert bytes.
#[cfg(feature = "esp-idf")]
fn build_tls_config<'a>(
    ca_cert: &'a Option<Vec<u8>>,
    client_cert: &'a Option<Vec<u8>>,
    client_key: &'a Option<Vec<u8>>,
) -> ::esp_idf_svc::tls::Config<'a> {
    let mut cfg = ::esp_idf_svc::tls::Config::new();
    cfg.non_block = true;

    if let Some(ca) = ca_cert {
        cfg.ca_cert = Some(::esp_idf_svc::tls::X509::der(ca));
    } else {
        cfg.skip_common_name = true;
    }

    if let (Some(cert), Some(key)) = (client_cert, client_key) {
        cfg.client_cert = Some(::esp_idf_svc::tls::X509::der(cert));
        cfg.client_key = Some(::esp_idf_svc::tls::X509::der(key));
    }

    cfg
}

/// Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`.
///
/// `EspTls`'s own `read`/`write` are synchronous calls, but the underlying socket runs
/// in non-blocking mode (`Config::non_block = true`, set by `EspIdfTlsConnector`), so
/// each call returns immediately instead of blocking the FreeRTOS task. Retries happen
/// by yielding to the async executor via `EspIdfTimer::sleep` — see `TLS_POLL_INTERVAL`.
///
/// Generic over the adopted socket type `S`: `EspIdfTlsConnector` (wrap-an-existing-stream,
/// below) produces `EspIdfTlsStream<EspIdfTcpStream>`.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTlsStream<S>
where
    S: ::esp_idf_svc::tls::Socket,
{
    tls: ::esp_idf_svc::tls::EspTls<S>,
    timer: EspIdfTimer,
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::ErrorType for EspIdfTlsStream<S> {
    type Error = embedded_io_async::ErrorKind;
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::Read for EspIdfTlsStream<S> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let tls = &mut self.tls;
        retry_on_would_block(&self.timer, "read", || tls.read(buf)).await
    }
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::Write for EspIdfTlsStream<S> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let tls = &mut self.tls;
        retry_on_would_block(&self.timer, "write", || tls.write(buf)).await
    }

    // esp_tls writes go straight to the socket with no internal buffering (confirmed via
    // esp-idf-svc source: `EspTls::write_raw` calls `esp_tls_conn_write` directly, and
    // esp-idf-svc's own `embedded_io::Write for EspTls` impl treats `flush()` as a no-op too)
    // — nothing to flush.
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Classifies a post-handshake `EspTls` read/write failure into the closest
/// `embedded_io_async::ErrorKind`, mirroring `map_esp_tls_connect_error`'s connect-phase
/// classification so `map_embedded_io_error_kind` (`src/io/mod.rs`) doesn't have to fall back
/// to its `ConnectionReset` catch-all for every real cause (see #46 — that catch-all previously
/// received nothing but `Other` here, defeating its own point). Unrecognized codes still fall
/// back to `Other`, with the real code preserved at `log::debug!`.
#[cfg(feature = "esp-idf")]
fn esp_tls_io_error_kind(err: &::esp_idf_svc::sys::EspError) -> embedded_io_async::ErrorKind {
    let code = err.code();

    if code == ::esp_idf_svc::sys::ECONNRESET as i32 {
        return embedded_io_async::ErrorKind::ConnectionReset;
    }
    if code == ::esp_idf_svc::sys::ECONNREFUSED as i32 {
        return embedded_io_async::ErrorKind::ConnectionRefused;
    }
    if code == ::esp_idf_svc::sys::ETIMEDOUT as i32 {
        return embedded_io_async::ErrorKind::TimedOut;
    }

    log::debug!("ESP-IDF TLS I/O failed: {err}");
    embedded_io_async::ErrorKind::Other
}

/// Shared `WouldBlock` retry loop for `EspIdfTlsStream::read`/`write` — both wrap a single `EspTls` call (`op`) in a loop that sleeps `TLS_POLL_INTERVAL` and retries on `is_would_block`, differing only in which `EspTls` method is invoked and the log message text.
/// Takes `timer`/`op` separately rather than `&mut self` so the caller can borrow `self.tls` (via
/// the closure) and `self.timer` (via this argument) as disjoint fields — see call sites in
/// `EspIdfTlsStream::read`/`write` above.
#[cfg(feature = "esp-idf")]
async fn retry_on_would_block<F>(
    timer: &EspIdfTimer,
    op_name: &str,
    mut op: F,
) -> Result<usize, embedded_io_async::ErrorKind>
where
    F: FnMut() -> Result<usize, ::esp_idf_svc::sys::EspError>,
{
    loop {
        match op() {
            Ok(n) => return Ok(n),
            Err(e) if is_would_block(&e) => {
                timer
                    .sleep(TLS_POLL_INTERVAL)
                    .await
                    .map_err(|_| embedded_io_async::ErrorKind::Other)?;
            }
            Err(e) => {
                log::debug!("ESP-IDF TLS {op_name} failed: {e}");
                return Err(esp_tls_io_error_kind(&e));
            }
        }
    }
}

/// Shared mbedTLS version query.
/// Generic over the adopted `Socket` impl so it isn't tied to one connector shape.
#[cfg(feature = "esp-idf")]
fn query_negotiated_tls_version<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<TlsVersion> {
    let ssl_ctx = unsafe { ::esp_idf_svc::sys::esp_tls_get_ssl_context(tls.context_handle()) }
        .cast::<::esp_idf_svc::sys::mbedtls_ssl_context>();

    if ssl_ctx.is_null() {
        return None;
    }

    let version_ptr = unsafe { ::esp_idf_svc::sys::mbedtls_ssl_get_version(ssl_ctx) };
    if version_ptr.is_null() {
        return None;
    }

    let version_str = unsafe { core::ffi::CStr::from_ptr(version_ptr) }
        .to_str()
        .ok()?;

    match version_str {
        "TLSv1.2" => Some(TlsVersion::Tls12),
        "TLSv1.3" => Some(TlsVersion::Tls13),
        _ => None,
    }
}

/// Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring `TokioIoError` (`io/tokio.rs`) — needed because `embedded-io-async` has no blanket impl for `std::io::Error` itself, only for types that opt in explicitly.
#[cfg(feature = "esp-idf")]
#[derive(Debug)]
pub struct EspIdfIoError(std::io::Error);

#[cfg(feature = "esp-idf")]
impl core::fmt::Display for EspIdfIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ESP-IDF TCP IO error: {}", self.0)
    }
}

#[cfg(feature = "esp-idf")]
impl std::error::Error for EspIdfIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Error for EspIdfIoError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        crate::io::map_io_error_kind(self.0.kind())
    }
}

/// Raw (unencrypted) TCP stream, used both as the seed for `EspIdfTlsConnector::connect`'s `EspTls::adopt()` call and directly as `RawIO` for models whose `model.quirks().uses_plaintext_ftps_data_channel()` is true (the FTPS data channel is then never TLS-wrapped, so its `embedded_io_async::Read`/`Write` impls below are exercised for real, not just to satisfy the `AsyncIo` trait bound).
///
/// The underlying socket stays non-blocking for the stream's entire lifetime (not
/// just during `connect()`'s own polling loop) — `read()`/`write()` below retry on
/// `WouldBlock` by yielding to the async executor via `EspIdfTimer::sleep(TLS_POLL_INTERVAL)`,
/// the same pattern `EspIdfTlsStream` already uses. A genuinely blocking socket here would give a
/// stalled peer (network partition, printer reboot) no `.await` yield point for any outer
/// timeout/cancellation to preempt, indefinitely parking the FreeRTOS task — exactly the hazard
/// `connect()`'s own non-blocking dial already fixes one layer up.
///
/// Wraps `Option<TcpStream>` rather than `TcpStream` directly so `Socket::release()` can
/// `.take()` the stream and hand its fd to `IntoRawFd::into_raw_fd()` — `esp_tls_conn_destroy`
/// closes an adopted fd itself once `release()` returns, so the Rust-side `TcpStream` must
/// give up ownership of the fd first or the fd would be double-closed.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTcpStream {
    stream: Option<std::net::TcpStream>,
    timer: EspIdfTimer,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTcpStream {
    /// Dials a raw TCP connection to `host:port`.
    ///
    /// Uses a non-blocking `connect()`, polled to completion by `.await`ing
    /// `EspIdfTimer::sleep(TLS_POLL_INTERVAL)` between attempts, rather than a single
    /// blocking `std::net::TcpStream::connect()` call. A blocking connect has no `.await`
    /// yield point, so `race()` (`src/io/mod.rs`) can never preempt it — a printer that's
    /// off, on another subnet, or behind a silent packet-dropping firewall used to hang the
    /// whole task for however long the underlying OS/lwIP connect took, silently breaking
    /// the `connect_timeout_secs` guarantee `race_against_connect_timeout`
    /// (`src/client/connect.rs`) documents. This mirrors `EspIdfTlsConnector::connect`'s
    /// existing non-blocking-handshake pattern, applied one layer earlier, to the TCP dial
    /// itself. `std::net::TcpStream::connect()`'s all-in-one API can't be used here since
    /// the non-blocking flag must be set *before* `connect()` is called on a not-yet-connected
    /// socket — hence going through `socket2::Socket` instead.
    ///
    /// The socket stays non-blocking after the connection completes — see
    /// `EspIdfTcpStream`'s doc comment for why `read()`/`write()` need that.
    ///
    /// Does not bound its own retry loop by a timeout — bounding is the responsibility of
    /// the *outer* `race_against_connect_timeout` in `ensure_mqtt()`/`ensure_ftps()`/
    /// `ensure_camera()`, which can now actually preempt this future because it has real
    /// `.await` points, matching the plain (non-connector-owned) design
    /// `RawStreamFactory::dial` has on every other platform.
    pub async fn connect(host: &str, port: u16) -> Result<Self, SocketError> {
        use std::net::ToSocketAddrs;

        let addr = (host, port)
            .to_socket_addrs()
            .map_err(to_esp_socket_error)?
            .next()
            .ok_or(SocketError::AddressNotAvailable)?;

        let socket = ::socket2::Socket::new(
            ::socket2::Domain::for_address(addr),
            ::socket2::Type::STREAM,
            Some(::socket2::Protocol::TCP),
        )
        .map_err(to_esp_socket_error)?;

        socket.set_nonblocking(true).map_err(to_esp_socket_error)?;

        match socket.connect(&addr.into()) {
            Ok(()) => {}
            Err(e) if is_connect_in_progress(&e) => {}
            Err(e) => return Err(to_esp_socket_error(e)),
        }

        let timer = EspIdfTimer::new().map_err(|e| {
            log::debug!("failed to create ESP-IDF async timer for TCP connect: {e}");
            SocketError::Other("failed to create ESP-IDF async timer for TCP connect".into())
        })?;

        poll_connect_until_complete(&socket, &timer).await?;

        Ok(Self {
            stream: Some(socket.into()),
            timer,
        })
    }

    fn inner(&self) -> &std::net::TcpStream {
        self.stream
            .as_ref()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS")
    }

    fn inner_mut(&mut self) -> &mut std::net::TcpStream {
        self.stream
            .as_mut()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS")
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::ErrorType for EspIdfTcpStream {
    type Error = EspIdfIoError;
}

/// Shared `WouldBlock` retry loop for `EspIdfTcpStream::read`/`write`, mirroring
/// `retry_on_would_block` above but for plain `std::io` calls instead of `EspTls` ones.
/// Without this, the raw plaintext stream had no preempt point at all — a stuck
/// peer blocked the FreeRTOS task indefinitely with no `.await` yield point for an outer
/// timeout to preempt.
#[cfg(feature = "esp-idf")]
async fn retry_on_would_block_io<F>(
    timer: &EspIdfTimer,
    op_name: &str,
    mut op: F,
) -> Result<usize, EspIdfIoError>
where
    F: FnMut() -> std::io::Result<usize>,
{
    loop {
        match op() {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                timer.sleep(TLS_POLL_INTERVAL).await.map_err(|_| {
                    EspIdfIoError(std::io::Error::other(
                        "ESP-IDF timer failed while polling TCP I/O",
                    ))
                })?;
            }
            Err(e) => {
                log::debug!("ESP-IDF TCP {op_name} failed: {e}");
                return Err(EspIdfIoError(e));
            }
        }
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Read for EspIdfTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use std::io::Read;
        let timer = &self.timer;
        let stream = self
            .stream
            .as_mut()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS");
        retry_on_would_block_io(timer, "read", || stream.read(buf)).await
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Write for EspIdfTcpStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        use std::io::Write as _;
        let timer = &self.timer;
        let stream = self
            .stream
            .as_mut()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS");
        retry_on_would_block_io(timer, "write", || stream.write(buf)).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        use std::io::Write as _;
        self.inner_mut().flush().map_err(EspIdfIoError)
    }
}

#[cfg(feature = "esp-idf")]
impl ::esp_idf_svc::tls::Socket for EspIdfTcpStream {
    fn handle(&self) -> i32 {
        use std::os::fd::AsRawFd;
        self.inner().as_raw_fd()
    }

    fn release(&mut self) -> Result<(), ::esp_idf_svc::sys::EspError> {
        use std::os::fd::IntoRawFd;
        if let Some(stream) = self.stream.take() {
            // esp_tls_conn_destroy() closes the adopted fd itself; abandon the Rust-side
            // owner without running its Drop (which would close the same fd again).
            let _ = stream.into_raw_fd();
        }
        Ok(())
    }
}

/// TLS connector for ESP-IDF that wraps an already-connected raw stream (FTPS's data and control channels, and MQTT's lazy connect via `RawStreamFactory`+`TlsConnector`).
/// Built on `esp_idf_svc::tls::EspTls` via `EspTls::adopt()` (confirmed by Phase 3's spike: no raw
/// mbedTLS FFI needed to wrap an existing fd) instead of `EspTls::new()` + `connect()`.
///
/// **No way to force TLS 1.2.** Unlike `io/tokio.rs`'s
/// `build_verified_client_config_with_options(..., force_tls_1_2: bool)` /
/// `build_unsafe_client_config_with_options(force_tls_1_2: bool)`, this connector has no
/// equivalent knob: `esp_idf_svc::tls::Config` (0.52.1, as vendored) exposes no min/max TLS
/// version field, and the mbedTLS accessor functions that would set it
/// (`mbedtls_ssl_conf_min_tls_version`/`mbedtls_ssl_conf_max_tls_version`) are absent from
/// this ESP-IDF build's actual bindgen output (confirmed by inspecting the generated
/// `esp-idf-sys` bindings directly, not just the safe wrapper's public API) — the
/// corresponding `mbedtls_ssl_config` struct fields are present but named
/// `private_max_tls_version`/`private_min_tls_version` per mbedTLS's own field-privacy
/// convention, so writing them directly would bypass that library's documented API contract
/// with no ABI stability guarantee across ESP-IDF/mbedTLS version bumps. Practical impact:
/// if a printer's vsFTPd offers/prefers TLS 1.3, `require_tls_1_2_if_enforced`
/// (`ftps/client.rs`) still fails closed for models where
/// `model.quirks().enforces_ftps_tls_1_2()` is true — the connection is safely rejected
/// rather than silently downgraded — but there is currently no way to make it succeed on
/// ESP-IDF for those models. `io/tokio.rs` (`tokio-rustls`) and `io/embassy.rs`
/// (`embedded-tls`) have no equivalent gap; both expose a genuine max-protocol-version knob.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTlsConnector {
    certs: EspIdfTlsCerts,
    connect_timeout: core::time::Duration,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTlsConnector {
    /// Creates a connector that skips server certificate verification.
    /// The handshake (this connector wraps an already-connected raw stream, so there's no TCP dial to
    /// bound — only the handshake itself) defaults to `DEFAULT_CONNECT_TIMEOUT`; override via
    /// `.with_connect_timeout(d)`.
    pub fn new() -> Self {
        Self {
            certs: EspIdfTlsCerts::new(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Creates a connector that verifies the server certificate against a CA cert.
    ///
    /// `ca_cert_pem`: PEM or DER-encoded CA certificate bytes.
    /// `client_auth`: Optional (cert_pem, key_pem) for mutual TLS.
    #[must_use]
    pub fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self {
        Self {
            certs: EspIdfTlsCerts::with_certs(ca_cert, client_auth),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Overrides the default handshake deadline. Passing `Duration::ZERO` disables the
    /// deadline entirely, matching `set_command_timeout`'s "0 disables" convention
    /// and `client::connect::with_connect_timeout`'s precedent — otherwise the very
    /// first would-block poll would immediately exceed a zero-length budget.
    /// Non-consuming — chain onto `new()`/`with_certs()`.
    #[must_use]
    pub fn with_connect_timeout(mut self, connect_timeout: core::time::Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self
    }
}

#[cfg(feature = "esp-idf")]
impl Default for EspIdfTlsConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "esp-idf")]
impl TlsConnector<EspIdfTcpStream> for EspIdfTlsConnector {
    type Stream = EspIdfTlsStream<EspIdfTcpStream>;

    /// Bounds the handshake loop by `self.connect_timeout`, tracked the same way `poll_until` does (`src/client/mod.rs`: capture `now_millis()` before the loop, compare `saturating_sub` against a budget each iteration).
    async fn connect(
        &self,
        host: &str,
        raw_stream: EspIdfTcpStream,
    ) -> Result<Self::Stream, SocketError> {
        // The adopted fd must be non-blocking for `Config::non_block = true` (set by
        // `build_tls_config`) to actually produce non-blocking handshake polling below —
        // otherwise mbedTLS's read/write calls inside `negotiate()` would block on the
        // fd itself despite the config flag. Plaintext callers of `EspIdfTcpStream` never
        // reach this function, so flipping the fd here doesn't affect them.
        raw_stream
            .inner()
            .set_nonblocking(true)
            .map_err(to_esp_socket_error)?;

        let cfg = self.certs.build_config();

        let timer = EspIdfTimer::new().map_err(|e| {
            log::debug!("failed to create ESP-IDF async timer for TLS: {e}");
            SocketError::Other("failed to create ESP-IDF async timer for TLS".into())
        })?;

        let mut tls = ::esp_idf_svc::tls::EspTls::adopt(raw_stream)
            .map_err(|e| {
                log::debug!("ESP-TLS adopt of raw socket failed: {e}");
                SocketError::Other("ESP-TLS adopt of raw socket failed".into())
            })?;

        let start = timer.now_millis();

        loop {
            match tls.negotiate(host, &cfg) {
                Ok(_) => break,
                Err(e) if is_would_block(&e) => {
                    // connect_timeout == 0 means "disabled" (matching
                    // with_connect_timeout's doc comment and its precedent elsewhere in
                    // this crate), not "expire on the very first would-block poll" — skip the
                    // deadline check entirely in that case.
                    if !self.connect_timeout.is_zero()
                        && timer.now_millis().saturating_sub(start)
                            >= self.connect_timeout.as_millis() as u64
                    {
                        return Err(SocketError::TimedOut);
                    }
                    timer.sleep(TLS_POLL_INTERVAL).await.map_err(|_| {
                        SocketError::Other(
                            "ESP-IDF timer failed while polling TLS handshake".into(),
                        )
                    })?;
                }
                Err(e) => return Err(map_esp_tls_connect_error(&e)),
            }
        }

        Ok(EspIdfTlsStream { tls, timer })
    }

    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        query_negotiated_tls_version(&stream.tls)
    }
}

/// Raw (pre-TLS) connection factory for ESP-IDF, using raw `std::net::TcpStream` — the ESP-IDF counterpart to `TokioRawStreamFactory` (`io/tokio.rs`), used for both MQTT's lazy connect and FTPS's passive data channel.
/// Whether the returned stream ends up TLS-wrapped (via `EspIdfTlsConnector`) or used directly
/// (plaintext FTPS data-channel models) is decided by the caller, not this factory.
#[cfg(feature = "esp-idf")]
pub struct EspIdfRawStreamFactory;

#[cfg(feature = "esp-idf")]
impl RawStreamFactory<EspIdfTcpStream> for EspIdfRawStreamFactory {
    async fn dial(&self, host: &str, port: u16) -> Result<EspIdfTcpStream, SocketError> {
        EspIdfTcpStream::connect(host, port).await
    }
}
