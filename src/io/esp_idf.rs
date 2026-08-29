//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, CertificateFailure, RawStreamFactory, SocketError,
    TimerError, TimerProvider, TlsConnector, TlsVersion, map_mbedtls_verify_flags,
};

#[cfg(feature = "esp-idf")]
use core::net::SocketAddr;

/// Async timer utilizing the ESP-IDF high-resolution timer service.
///
/// Wraps `EspAsyncTimer` to provide non-blocking async sleep that integrates
/// with the FreeRTOS scheduler instead of blocking the task thread.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTimer {
    /// `Option` so `sleep` can *move* the timer out for the duration of the await rather than
    /// hold a `RefCell` borrow across it (`clippy::await_holding_refcell_ref`). A borrow held
    /// across an await panics with `BorrowMutError` if a second caller awaits `sleep` on the
    /// same timer, which is reachable: `TimerProvider::sleep` takes `&self`, and a single
    /// timer is shared by a whole client (see `README.md`'s `with_ftps` example), so any
    /// `select!`-style race between two sleeps on it would abort. With the timer taken out, a
    /// concurrent caller sees `None` and allocates its own instead of panicking.
    timer: core::cell::RefCell<Option<::esp_idf_svc::timer::EspAsyncTimer>>,
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
        Ok(Self {
            timer: core::cell::RefCell::new(Some(Self::new_async_timer()?)),
        })
    }

    /// Allocates one `esp_timer` slot backed by its own timer service.
    fn new_async_timer() -> Result<::esp_idf_svc::timer::EspAsyncTimer, ::esp_idf_svc::sys::EspError>
    {
        let service = ::esp_idf_svc::timer::EspTimerService::<::esp_idf_svc::timer::Task>::new()?;
        service.timer_async()
    }
}

#[cfg(feature = "esp-idf")]
impl TimerProvider for EspIdfTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        // Take the cached timer out (borrow ends on this line, before the await) or allocate a
        // fresh one if a concurrent sleep already holds it — see the field's doc comment.
        let taken = self.timer.borrow_mut().take();
        let mut timer = match taken {
            Some(timer) => timer,
            None => Self::new_async_timer()
                .map_err(|_| TimerError::Other("ESP-IDF hardware timer allocation failed"))?,
        };

        let result = timer.after(duration).await;

        // Restore for the next call, unless a concurrent caller already put one back — keep
        // exactly one cached and drop the extra, so repeated racing sleeps don't accumulate
        // `esp_timer` slots. Dropping this future mid-await instead (cancellation) simply
        // leaves the slot empty and the next `sleep` reallocates.
        let mut slot = self.timer.borrow_mut();
        if slot.is_none() {
            *slot = Some(timer);
        }
        drop(slot);

        result.map_err(|_| TimerError::Other("ESP-IDF hardware timer scheduling failed"))
    }

    fn now_millis(&self) -> u64 {
        // esp_timer_get_time() returns microseconds since boot as i64
        (unsafe { ::esp_idf_svc::sys::esp_timer_get_time() } as u64) / 1000
    }
}

/// Microseconds since boot, for the handshake-loop instrumentation in `EspIdfTlsConnector::connect`.
///
/// `TimerProvider::now_millis` is too coarse for that particular measurement: a single
/// `esp_tls_low_level_conn` step can cost well under a millisecond, so summing per-step
/// millisecond deltas across ~60 steps truncates the compute half of the handshake to zero and
/// cannot distinguish "compute is negligible" from "compute was rounded away" (GitHub issue
/// #160). Nothing else should need this — use `TimerProvider::now_millis` for timeouts and
/// pacing.
#[cfg(feature = "esp-idf")]
fn now_micros() -> u64 {
    unsafe { ::esp_idf_svc::sys::esp_timer_get_time() as u64 }
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
    /// Non-blocking send that reports transient lwIP buffer exhaustion as `TimedOut` rather than a terminal fault.
    ///
    /// `bind` puts the socket in non-blocking mode, and under Wi-Fi load lwIP can momentarily
    /// have no pbuf to hand this datagram. That surfaces as `ERR_MEM`/`ERR_BUF`, which lwIP's
    /// `err_to_errno` table maps to `ENOMEM`/`ENOBUFS` (`lwip/src/api/err.c`) — *not* to
    /// `EWOULDBLOCK`, which that table reserves for `ERR_TIMEOUT`/`ERR_WOULDBLOCK`. Both land in
    /// `map_std_io_error`'s `_` arm as `SocketError::Other`, and `DiscoveryEngine::broadcast_search`
    /// errors out when its multicast and broadcast sends both fail — which a single pbuf shortage
    /// makes likely, since they go back to back. Discovery then aborted on a condition that would
    /// have cleared on its own milliseconds later.
    ///
    /// `TimedOut` is the right signal because `poll_next_device` already treats it as benign, so
    /// the caller retries instead of giving up. `WouldBlock` is folded in for completeness: lwIP
    /// is not expected to produce it for a UDP `sendto`, but a non-blocking socket returning it
    /// means exactly the same "try again" as the buffer-exhaustion case.
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError> {
        match self.inner.send_to(buf, target) {
            Ok(len) => Ok(len),
            Err(e) if is_transient_send_shortage(&e) => {
                log::debug!(
                    "EspIdfUdpSocket::send_to: transient lwIP buffer shortage, reporting as TimedOut: {e}"
                );
                if let Err(e) = self.timer.sleep(UDP_RECV_POLL_INTERVAL).await {
                    log::debug!("EspIdfUdpSocket::send_to: pacing sleep failed: {e:?}");
                }
                Err(SocketError::TimedOut)
            }
            Err(e) => Err(to_esp_socket_error(e)),
        }
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

/// True if `err` is lwIP momentarily running out of send buffers, rather than a real fault.
///
/// Matched on `raw_os_error` rather than `ErrorKind` because std maps neither `ENOMEM` nor
/// `ENOBUFS` to a kind that distinguishes "retry me" from "give up": both fall through to the
/// catch-all. See `send_to`'s doc comment for where these errnos come from.
#[cfg(feature = "esp-idf")]
fn is_transient_send_shortage(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    matches!(
        err.raw_os_error(),
        Some(e) if e == ::esp_idf_svc::sys::ENOMEM as i32
            || e == ::esp_idf_svc::sys::ENOBUFS as i32
    )
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

/// True once lwIP has resolved a pending non-blocking `connect()`, either by completing the handshake or by failing it.
///
/// Uses a zero-timeout `poll()` for `POLLOUT`, the standard non-blocking-connect completion
/// test, because lwIP offers no cheaper one that is actually correct — `getpeername()` is not
/// a completion test here (see `poll_connect_until_complete`), and a zero-length `send()` is
/// not either, since `netconn_write_vectors_partly` returns `ERR_OK` for `size == 0` before
/// ever reaching the state check that would report `ERR_INPROGRESS`.
///
/// `POLLOUT` is exactly the right signal: `lwip_pollscan` raises it only when the socket's
/// `sendevent` flag is set, a client-dialled TCP socket starts at `sendevent = 0`
/// (`alloc_socket`), and the flag is set by the `NETCONN_EVT_SENDPLUS` that
/// `lwip_netconn_do_connected` fires on SYN/ACK — the same callback that clears
/// `NETCONN_CONNECT`, which is what unblocks writes. So this becomes true precisely when a
/// write would stop failing with `ERR_INPROGRESS`.
///
/// A zero timeout keeps the call non-blocking, so the caller's sleep/`.await` pacing (and the
/// outer timeout that depends on it) still works. Returns the raw `revents` — 0 while the
/// connect is still pending — leaving the caller to separate a completed connection from a
/// failed one, which `POLLOUT` alone cannot express.
#[cfg(feature = "esp-idf")]
fn poll_connect_revents(fd: core::ffi::c_int) -> Result<i16, SocketError> {
    let mut poll_fd = ::esp_idf_svc::sys::pollfd {
        fd,
        events: ::esp_idf_svc::sys::POLLOUT as i16,
        revents: 0,
    };

    // SAFETY: `poll_fd` is a single initialized `pollfd` owned by this frame, and the count (1)
    // matches. A zero timeout means the call cannot block. lwIP writes only `revents`.
    let rc = unsafe { ::esp_idf_svc::sys::poll(&mut poll_fd, 1, 0) };

    // Deliberately no EINTR retry here. On this platform `poll()` is a newlib shim over
    // `select()` (`components/newlib/src/poll.c`, which keeps select's errno verbatim), and
    // lwIP's socket layer never sets `EINTR` at all — the whole lwIP component references it
    // only in its `errno.h` definition, the unused Unix port, and PPP file I/O. The one place
    // `EINTR` is set is `esp_vfs_select` (`components/vfs/vfs.c`), and there it means a VFS
    // driver's `start_select` *failed*, not that a signal interrupted a call still making
    // progress. Retrying that would spin on a persistent failure, so the errno is reported
    // rather than swallowed: the fixed string this used to return discarded the one piece of
    // information that tells a driver refusal apart from a genuine socket fault.
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        return Err(SocketError::Other(
            std::format!("poll() failed while polling ESP-IDF TCP connect: {err}").into(),
        ));
    }

    if rc == 0 {
        return Ok(0);
    }

    Ok(poll_fd.revents)
}

/// `revents` bits that mean the connect failed rather than completed.
#[cfg(feature = "esp-idf")]
const POLL_CONNECT_FAILED: i16 = (::esp_idf_svc::sys::POLLERR
    | ::esp_idf_svc::sys::POLLHUP
    | ::esp_idf_svc::sys::POLLNVAL) as i16;

/// Polls a non-blocking `connect()` to completion by waiting for the fd to become writable, then reading `SO_ERROR` (via `take_error()`) to tell a completed connection from a refused one.
///
/// Deliberately does *not* use `peer_addr()` as the completion test. On lwIP `getpeername()`
/// answers as soon as `connect()` is initiated — `lwip_netconn_do_getaddr` returns `ERR_CONN`
/// for a remote-name request only when the pcb is `CLOSED` or `LISTEN`, and a pcb in
/// `SYN_SENT` is neither — so it returned `Ok` on the first poll iteration and this function
/// handed back a socket still mid-handshake. The first write then failed outright rather than
/// reporting would-block (`lwip_netconn_do_write` returns `ERR_INPROGRESS` → `EINPROGRESS`,
/// which mbedTLS's `net_would_block` does not treat as retryable), killing the TLS handshake
/// on its first record with `MBEDTLS_ERR_NET_SEND_FAILED`. See GitHub issue #64.
///
/// `take_error()` alone cannot carry this either: "still connecting, no error yet" and
/// "connected successfully" both return `Ok(None)`. Hence readiness first, `SO_ERROR` second.
///
/// Sleeps `TLS_POLL_INTERVAL` between attempts so the caller's outer `race_against_connect_timeout`
/// can preempt this loop; does not bound itself (see `EspIdfTcpStream::connect`'s doc comment for
/// why).
#[cfg(feature = "esp-idf")]
async fn poll_connect_until_complete(
    socket: &::socket2::Socket,
    timer: &EspIdfTimer,
) -> Result<(), SocketError> {
    use std::os::fd::AsRawFd;

    let fd = socket.as_raw_fd();

    loop {
        if let Some(err) = socket.take_error().map_err(to_esp_socket_error)? {
            return Err(to_esp_socket_error(err));
        }

        let revents = poll_connect_revents(fd)?;

        if revents != 0 {
            // Readiness only says lwIP reached a verdict; SO_ERROR says which one. A refused or
            // unreachable connect reports POLLERR here, not a `poll()` failure.
            if let Some(err) = socket.take_error().map_err(to_esp_socket_error)? {
                return Err(to_esp_socket_error(err));
            }
            // An error bit with no SO_ERROR to explain it still means the socket is unusable.
            // Returning Ok here would hand back a dead socket and fail on the first write
            // instead — the same shape of bug as #64 itself.
            if revents & POLL_CONNECT_FAILED != 0 {
                return Err(SocketError::Other(
                    "ESP-IDF TCP connect failed: poll reported an error with no SO_ERROR".into(),
                ));
            }
            return Ok(());
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
/// future upgrade. This fixed-interval poll works because `EspIdfTlsConnector::connect` puts the
/// adopted fd in `O_NONBLOCK` *and* pins `Config::timeout_ms = 0`, so every `EspTls` call takes a
/// single handshake step and returns immediately instead of blocking inside the FFI call, and an
/// outer `TimerProvider`-based timeout can preempt the operation between poll attempts. Both are
/// required: `O_NONBLOCK` alone still left `esp_tls_conn_new_sync` spinning internally for up to
/// the default 4s per call, which made this interval dead time between spins rather than pacing
/// (GitHub issue #67). (`Config::non_block` is deliberately *off* on the adopted-socket path —
/// see the comment in `connect` and GitHub issue #61.)
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

/// Cert bundle used by `EspIdfTlsConnector`'s `ca_pem`/`client_cert`/`client_key` fields and `new()`/`with_certs()` constructors.
/// Factored out so a future cert-related option (e.g. ALPN config) only needs to be added in
/// one place.
///
/// `ca_pem` holds the caller's trust anchors already converted to a NUL-terminated PEM bundle
/// (see `crate::io::der_certs_to_pem_bundle`) rather than raw DER, because DER can only ever
/// express *one* anchor to mbedTLS. The conversion happens once at construction, not per
/// connect, so `build_config` stays a cheap borrow.
#[cfg(feature = "esp-idf")]
struct EspIdfTlsCerts {
    ca_pem: Option<Vec<u8>>,
    client_cert: Option<Vec<u8>>,
    client_key: Option<Vec<u8>>,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTlsCerts {
    fn new() -> Self {
        Self {
            ca_pem: None,
            client_cert: None,
            client_key: None,
        }
    }

    fn with_certs(
        ca_certs: impl IntoIterator<Item = Vec<u8>>,
        client_auth: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Self {
        let (client_cert, client_key) = match client_auth {
            Some((cert, key)) => (Some(cert), Some(key)),
            None => (None, None),
        };
        // Collected rather than streamed straight into the bundle builder so the anchor count
        // is known: `report_anchor_bundle_parse` needs the denominator to say "3 of 5 anchors
        // failed" instead of just "3 failed".
        let ca_certs: Vec<Vec<u8>> = ca_certs.into_iter().collect();
        let anchor_count = ca_certs.len();
        // `None` for an empty iterator, which `build_tls_config` treats exactly like
        // `new()` — an anchor-less connector, not a connector with an empty anchor set
        // that mbedTLS would reject at handshake time with nothing pointing at the cause.
        let ca_pem = crate::io::der_certs_to_pem_bundle(ca_certs);

        if let Some(pem) = ca_pem.as_deref() {
            report_anchor_bundle_parse(pem, anchor_count);
        }

        Self {
            ca_pem,
            client_cert,
            client_key,
        }
    }

    fn build_config(&self) -> ::esp_idf_svc::tls::Config<'_> {
        build_tls_config(&self.ca_pem, &self.client_cert, &self.client_key)
    }
}

/// Parses the trust-anchor bundle exactly as `esp-tls` will, and logs how many anchors loaded.
///
/// **This is what makes silencing the `esp-tls` tag during the handshake safe** (see
/// `EspTlsLogQuiet`). ESP-IDF's `set_ca_cert` (`components/esp-tls/esp_tls_mbedtls.c`)
/// tolerates a partial trust-store parse: when `mbedtls_x509_crt_parse` returns a positive
/// count of failed certificates it logs `mbedtls_x509_crt_parse was partly successful` at
/// **warning** level on the `esp-tls` tag and continues, so the handshake can succeed against
/// a partially-loaded store. The absence of that warning is otherwise the only signal that
/// every anchor loaded — load-bearing for a multi-anchor bundle (GitHub issue #145: five BBL
/// anchors spanning two CA generations, where a P1S chains to one and newer models chain to
/// another), because holding four of five silently verifies some models and fails others while
/// the successful handshake looks identical either way.
///
/// Running the same parse here, at construction time, moves that signal *outside* the
/// suppressed window and reports it more precisely than `esp-tls` does (`n of m`, failures at
/// `error` level rather than `warn`, and the all-parsed confirmation at `info` so a consumer
/// running at the default level can tell a complete trust store from an unreported one). Same
/// call, same buffer, same length — `set_ca_cert`
/// passes `cacert_buf`/`cacert_bytes` straight through, and `X509::pem_until_nul` sets those to
/// this slice up to and including its single trailing NUL — so the result is the same integer
/// `set_ca_cert` will get. Reported, not returned as an error: mbedTLS's own policy is that a
/// partial store is still usable, and failing the connector here would reject a configuration
/// ESP-IDF accepts.
///
/// `expected` is the number of DER certificates that went into the bundle.
#[cfg(feature = "esp-idf")]
fn report_anchor_bundle_parse(ca_pem: &[u8], expected: usize) {
    // SAFETY: `chain` is zeroed and then initialized by `mbedtls_x509_crt_init` before any
    // other call touches it, `ca_pem` is a live NUL-terminated buffer for the whole call, and
    // `mbedtls_x509_crt_free` runs on every path before the storage is dropped.
    let ret = unsafe {
        let mut chain = core::mem::MaybeUninit::<::esp_idf_svc::sys::mbedtls_x509_crt>::zeroed();
        ::esp_idf_svc::sys::mbedtls_x509_crt_init(chain.as_mut_ptr());
        let ret = ::esp_idf_svc::sys::mbedtls_x509_crt_parse(
            chain.as_mut_ptr(),
            ca_pem.as_ptr(),
            ca_pem.len(),
        );
        ::esp_idf_svc::sys::mbedtls_x509_crt_free(chain.as_mut_ptr());
        ret
    };

    match ret {
        0 => log::info!("TLS trust store: all {expected} anchor(s) parsed"),
        failed if failed > 0 => log::error!(
            "TLS trust store: {failed} of {expected} anchor(s) failed to parse; handshakes will \
             verify only the printer models that chain to a surviving anchor"
        ),
        // Negative: nothing parsed. mbedTLS returns the first error it hit, negated by
        // convention when printed (matching `set_ca_cert`'s own `-0x%04X`).
        err => log::error!(
            "TLS trust store: none of {expected} anchor(s) parsed (mbedtls_x509_crt_parse \
             -0x{:04X}); every handshake will fail verification",
            -err
        ),
    }
}

#[cfg(feature = "esp-idf")]
use crate::io::RedactedHost;

/// `esp-tls`'s log tag, as a NUL-terminated C string for `esp_log_level_set`/`_get`.
#[cfg(feature = "esp-idf")]
const ESP_TLS_LOG_TAG: &[u8] = b"esp-tls\0";

/// Nesting depth of live [`EspTlsLogQuiet`] guards and the `esp-tls` tag's log level as it was
/// before the outermost guard lowered it. Held behind one lock so "decide whether I am the
/// outermost guard, then read/write the saved level" is one atomic transaction — two separate
/// atomics for depth and saved level let an outer guard's drop interleave with an inner guard's
/// enter and clobber the saved level (see the enter/exit race this replaced).
#[cfg(feature = "esp-idf")]
static ESP_TLS_LOG_QUIET: std::sync::Mutex<(usize, u32)> = std::sync::Mutex::new((0, 0));

/// Lowers the `esp-tls` tag to `ESP_LOG_ERROR` for the length of a handshake, restoring it on drop.
///
/// A **successful** handshake emitted ~60 `W esp-tls: Failed to open new connection in
/// specified timeout` lines before this existed (measured: 62 lines on a 1.42s ESP32-P4
/// handshake that connected first try). Nothing had failed and no timeout had been exceeded —
/// `esp_tls_conn_new_sync` logs that line on every step that does not *complete* the
/// handshake, and `connect` pins `Config::timeout_ms = 0` so every step returns immediately
/// (GitHub issue #67). The count therefore scales with handshake duration, one line per
/// `TLS_POLL_INTERVAL`. bambino is the only layer that knows those warnings are expected; a
/// consumer reading the log cannot tell them from real ones (GitHub issue #156).
///
/// `ESP_LOG_ERROR`, not `ESP_LOG_NONE`: `esp_tls_handshake`'s real failures
/// (`mbedtls_ssl_handshake returned -0x%04X`) and `conn_new_sync`'s own
/// `Failed to open new connection` are `ESP_LOGE`, and those must still reach the consumer.
/// The only `esp-tls` warnings that survive being dropped here are the two emitted from
/// `create_ssl_handle` — the partial-anchor-parse one, which
/// [`report_anchor_bundle_parse`] re-reports at construction time and at higher severity, and
/// a "TLS 1.3 is not enabled in config" notice about the peer's offered protocol, not the
/// trust store.
///
/// Nesting-counted because MQTT and FTPS can handshake concurrently on separate tasks: without
/// it the inner guard's drop would un-silence the tag while the outer handshake was still
/// stepping.
///
/// No-op where `CONFIG_LOG_DYNAMIC_LEVEL_CONTROL` is disabled — `esp_log_level_set` does
/// nothing there and the old noise comes back, which is a degraded log, not a broken
/// handshake.
#[cfg(feature = "esp-idf")]
struct EspTlsLogQuiet;

#[cfg(feature = "esp-idf")]
impl EspTlsLogQuiet {
    fn enter() -> Self {
        // Lock scope covers the "am I outermost, then read/write saved level" decision as one
        // step — see the static's doc comment for why splitting it into two atomics was wrong.
        let mut state = ESP_TLS_LOG_QUIET.lock().unwrap_or_else(|e| e.into_inner());
        state.0 += 1;
        if state.0 == 1 {
            // SAFETY: `ESP_TLS_LOG_TAG` is a `'static` NUL-terminated byte string; both calls
            // take it as a read-only C string and copy what they need.
            unsafe {
                let previous =
                    ::esp_idf_svc::sys::esp_log_level_get(ESP_TLS_LOG_TAG.as_ptr().cast());
                state.1 = previous;
                ::esp_idf_svc::sys::esp_log_level_set(
                    ESP_TLS_LOG_TAG.as_ptr().cast(),
                    ::esp_idf_svc::sys::esp_log_level_t_ESP_LOG_ERROR,
                );
            }
        }
        Self
    }
}

#[cfg(feature = "esp-idf")]
impl Drop for EspTlsLogQuiet {
    fn drop(&mut self) {
        let mut state = ESP_TLS_LOG_QUIET.lock().unwrap_or_else(|e| e.into_inner());
        state.0 -= 1;
        if state.0 == 0 {
            let previous = state.1;
            // SAFETY: as in `enter`.
            unsafe {
                ::esp_idf_svc::sys::esp_log_level_set(ESP_TLS_LOG_TAG.as_ptr().cast(), previous);
            }
        }
    }
}

/// Builds an `esp_idf_svc::tls::Config` from cert bytes.
///
/// `ca_pem` is a NUL-terminated PEM bundle (`der_certs_to_pem_bundle`); the client cert/key
/// stay DER, since each is a single item and DER is this crate's public convention.
#[cfg(feature = "esp-idf")]
fn build_tls_config<'a>(
    ca_pem: &'a Option<Vec<u8>>,
    client_cert: &'a Option<Vec<u8>>,
    client_key: &'a Option<Vec<u8>>,
) -> ::esp_idf_svc::tls::Config<'a> {
    let mut cfg = ::esp_idf_svc::tls::Config::new();
    cfg.non_block = true;

    // Turn off ESP-IDF's bundled public root CAs (GitHub issue #62). `esp-idf-svc`'s
    // `Config::new` defaults this to `true` wherever `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is
    // enabled, and ESP-IDF's `set_client_config` checks `crt_bundle_attach` *first* with
    // mutually exclusive branches — so leaving the default on would verify against the public
    // roots and silently ignore the caller's `ca_pem` below. Bambu printer certs chain to a
    // private BBL CA, never to a public root, so the bundle is never the anchor this crate wants.
    // Cfg gate mirrors the field's own gate in `esp-idf-svc`; it is `build.rs` that makes the
    // gate evaluate at all (see that file — without it this is silently dead code).
    #[cfg(esp_idf_mbedtls_certificate_bundle)]
    {
        cfg.use_crt_bundle_attach = false;
    }

    if let Some(ca) = ca_pem {
        // `pem_until_nul`, not `der`: `X509::der` stores the slice without a trailing NUL,
        // which is precisely the condition mbedTLS reads as "this is a single DER cert" —
        // it would then parse only the first anchor of the bundle and drop the rest
        // silently. `der_certs_to_pem_bundle` guarantees the NUL this requires.
        cfg.ca_cert = Some(::esp_idf_svc::tls::X509::pem_until_nul(ca));
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
/// `EspTls`'s own `read`/`write` are synchronous calls, but the underlying fd runs
/// in non-blocking mode (`O_NONBLOCK`, set by `EspIdfTlsConnector::connect`), so
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

/// Upper bound on certificates walked out of a peer chain, guarding the `next`-pointer walk in
/// `query_peer_chain_der` against looping forever on a corrupt list. A real printer chain is a
/// leaf plus at most a couple of CAs; anything past this is a bug in mbedTLS or in memory, not a
/// chain worth reporting.
#[cfg(feature = "esp-idf")]
const MAX_PEER_CHAIN_CERTS: usize = 8;

/// Shared mbedTLS peer-certificate-chain query, returning DER, leaf first.
///
/// Generic over the adopted `Socket` impl for the same reason as `query_negotiated_tls_version`,
/// and reaches the raw `mbedtls_ssl_context` by the same `esp_tls_get_ssl_context` route.
///
/// **Requires `CONFIG_MBEDTLS_SSL_KEEP_PEER_CERTIFICATE`.** It is on by ESP-IDF default, but a
/// consumer that depends on this accessor should pin it explicitly in `sdkconfig` rather than
/// inherit the default: with it off, mbedTLS frees the peer certificate at the end of the
/// handshake, `mbedtls_ssl_get_peer_cert` returns `NULL`, and this degrades to `None`.
///
/// `mbedtls_ssl_get_peer_cert` returns the whole chain the peer sent, not just the leaf
/// (`ssl->session_negotiate->peer_cert = chain` in mbedTLS's `ssl_tls.c`); the leaf-only
/// re-parse elsewhere in that file is the session export/resumption path, which this is not.
/// The chain is owned by the live SSL context and freed on drop or renegotiation, so every
/// certificate is copied out here rather than borrowed.
#[cfg(feature = "esp-idf")]
fn query_peer_chain_der<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<Vec<Vec<u8>>> {
    let ssl_ctx = unsafe { ::esp_idf_svc::sys::esp_tls_get_ssl_context(tls.context_handle()) }
        .cast::<::esp_idf_svc::sys::mbedtls_ssl_context>();

    if ssl_ctx.is_null() {
        return None;
    }

    let mut cert = unsafe { ::esp_idf_svc::sys::mbedtls_ssl_get_peer_cert(ssl_ctx) };
    if cert.is_null() {
        log::debug!(
            "mbedTLS reported no peer certificate; \
             CONFIG_MBEDTLS_SSL_KEEP_PEER_CERTIFICATE is likely disabled"
        );
        return None;
    }

    let mut chain: Vec<Vec<u8>> = Vec::new();

    while !cert.is_null() && chain.len() < MAX_PEER_CHAIN_CERTS {
        // Read the two `raw` fields individually rather than copying the `mbedtls_x509_buf`
        // out: that keeps this from depending on whether bindgen derived `Copy` for it.
        let (der_ptr, der_len) = unsafe { ((*cert).raw.p, (*cert).raw.len) };
        if !der_ptr.is_null() && der_len > 0 {
            chain.push(unsafe { core::slice::from_raw_parts(der_ptr, der_len) }.to_vec());
        }
        cert = unsafe { (*cert).next };
    }

    if chain.is_empty() { None } else { Some(chain) }
}

/// Reads mbedTLS's certificate-verification verdict off a failed handshake, if it has one.
///
/// mbedTLS reports *which* check failed out of band rather than in the return value: every
/// verification failure comes back as `MBEDTLS_ERR_X509_CERT_VERIFY_FAILED` (`-0x2700`), which
/// esp-tls in turn flattens to `ESP_FAIL`, so "no trusted anchor" and "name mismatch" were
/// indistinguishable to a caller (GitHub issue #157). The detail lives in
/// `mbedtls_ssl_get_verify_result`, reached through the same `esp_tls_get_ssl_context` route
/// `query_peer_chain_der` already uses.
///
/// Returns `None` — leaving the caller's existing error untouched — whenever the context is
/// gone or the mask carries no verdict, so a missing answer degrades to the pre-existing
/// opaque error rather than to a fabricated cause.
///
/// **Confirmed on an ESP32-C6 against a P1-series printer:** the verify result *does* survive
/// to this point. Withholding the anchor the printer chains to reported `UntrustedAnchor`, and
/// a correct anchor set with a deliberately wrong TLS name reported `NameMismatch` — the two
/// cases that were byte-identical `ESP_FAIL` before GitHub issue #157. This was worth measuring
/// rather than assuming: `mbedtls_ssl_get_peer_cert` is documented to return `NULL` after a
/// *failed* handshake, so a failed context demonstrably does not retain everything, and the
/// verify result surviving does not follow from the peer certificate surviving. A chain that
/// was both untrusted *and* wrongly named reported `UntrustedAnchor`, confirming that mbedTLS
/// really does set both flags and that `map_mbedtls_verify_flags`' precedence — not just its
/// unit tests — decides the answer.
#[cfg(feature = "esp-idf")]
fn query_verify_failure<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<CertificateFailure> {
    let ssl_ctx = unsafe { ::esp_idf_svc::sys::esp_tls_get_ssl_context(tls.context_handle()) }
        .cast::<::esp_idf_svc::sys::mbedtls_ssl_context>();

    if ssl_ctx.is_null() {
        return None;
    }

    let flags = unsafe { ::esp_idf_svc::sys::mbedtls_ssl_get_verify_result(ssl_ctx) };

    map_mbedtls_verify_flags(flags)
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

        // Nagle off: every protocol this crate dials is small-request/response over TLS, which
        // is the exact shape Nagle penalises. A TLS handshake writes several small records, and
        // Nagle holds a second small write until the peer ACKs the first — pairing with the
        // peer's delayed-ACK timer for a stall of up to ~200ms per occurrence, on a link whose
        // real RTT is single-digit milliseconds (GitHub issue #160). MQTT command traffic has
        // the same shape afterwards, so this is a property of the socket, not of the handshake.
        //
        // Not fatal on failure, unlike `set_nonblocking` above: non-blocking is a correctness
        // requirement here (the poll loops retry on WouldBlock and would otherwise hang the
        // task), whereas Nagle-off is a latency optimisation. A platform that refuses it should
        // still connect, just more slowly — so this warns and continues rather than failing a
        // connection that would otherwise work.
        // `set_tcp_nodelay`, not `set_nodelay`: that is socket2's spelling. Tokio's
        // `TcpStream` calls the same option `set_nodelay`, so the two backends read slightly
        // differently on purpose.
        if let Err(e) = socket.set_tcp_nodelay(true) {
            log::warn!("could not disable Nagle on the TCP socket, latency may suffer: {e}");
        }

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
/// ESP-IDF for those models.
///
/// **Only `io/tokio.rs` (`tokio-rustls`) exposes a genuine max-protocol-version knob.**
/// `io/embassy.rs` has the same gap for a different reason: its backend is `mbedtls-rs` (not
/// `embedded-tls`, which it replaced — see `Cargo.toml`'s dependency comment), and
/// `EmbassyTlsConnector::connect` sets only `min_version` while `negotiated_version` returns
/// `None` unconditionally, so `require_tls_1_2_if_enforced` (`ftps/client.rs`) fails closed
/// there too for every `enforces_ftps_tls_1_2()` model. On both embedded backends the only way
/// through today is `with_ftps_allow_unverified_tls_1_2(true)`, which bypasses the check
/// rather than satisfying it.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTlsConnector {
    certs: EspIdfTlsCerts,
    connect_timeout: core::time::Duration,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTlsConnector {
    /// Creates a connector that skips server certificate verification.
    /// Requires `CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY=y` in the consuming app's sdkconfig
    /// (a sub-option of `CONFIG_ESP_TLS_INSECURE`; both are off by default). No library call
    /// can enable it — ESP-IDF compiles the no-verification branch out otherwise, and
    /// `set_client_config` then fails the connection with `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED`.
    /// Failing loudly there is deliberate: this crate no longer falls back to ESP-IDF's
    /// public-root CA bundle, which could never validate a self-signed printer certificate
    /// anyway (GitHub issue #62). Prefer [`Self::with_certs`] wherever the caller can supply
    /// the printer's CA — it needs no sdkconfig change and actually verifies the peer.
    /// The handshake (this connector wraps an already-connected raw stream, so there's no TCP dial to
    /// bound — only the handshake itself) defaults to `DEFAULT_CONNECT_TIMEOUT`; override via
    /// `.with_connect_timeout(d)`.
    pub fn new() -> Self {
        Self {
            certs: EspIdfTlsCerts::new(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Creates a connector that verifies the server certificate against one or more CA certs.
    /// The supplied CAs are the sole trust anchors: ESP-IDF's bundled public root CAs are
    /// explicitly disabled, so these bytes reach mbedTLS as `cacert_buf` rather than being
    /// silently overridden by the bundle (GitHub issue #62). Certificates are a runtime
    /// input — nothing is embedded in this crate.
    ///
    /// **Takes many anchors, mirroring the tokio backend** (`build_verified_client_config`'s
    /// `ca_certs: impl IntoIterator<..>`). Bambu is mid-PKI-rollover: a P1S chains to the
    /// legacy `BBL CA` root while newer models chain through `BBL Device CA <model>-V2` to
    /// `BBL CA2 RSA`/`BBL CA2 ECC`, so a caller covering the model range needs several
    /// anchors at once and cannot pick one. See GitHub issue #145.
    ///
    /// **Still DER in, despite the PEM bundle used internally.** DER is this crate's public
    /// convention throughout; the certs are re-encoded once here into the NUL-terminated PEM
    /// bundle that is the only form mbedTLS will parse as more than one certificate — see
    /// `crate::io::der_certs_to_pem_bundle`. Passing PEM bytes in is still wrong and will
    /// fail the handshake, now with the extra confusion of being base64'd a second time.
    ///
    /// An empty `ca_certs` yields an anchor-less connector, behaving exactly like
    /// [`Self::new`] rather than failing later inside the handshake.
    ///
    /// `ca_certs`: DER-encoded CA certificate bytes, one `Vec` per certificate.
    /// `client_auth`: Optional (cert, key), both DER-encoded, for mutual TLS.
    #[must_use]
    pub fn with_certs(
        ca_certs: impl IntoIterator<Item = Vec<u8>>,
        client_auth: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Self {
        Self {
            certs: EspIdfTlsCerts::with_certs(ca_certs, client_auth),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Overrides the default handshake deadline, which bounds how long the poll loop keeps
    /// retrying rather than how long any single attempt may take.
    /// The deadline is checked *between* iterations, so it cannot preempt a stall *inside*
    /// one: the `EspTls::negotiate` FFI call is not interruptible from this task once entered.
    /// `connect` pins `Config::timeout_ms = 0` so each call is a single handshake step, which
    /// keeps that window near-instant and gives this deadline ~`TLS_POLL_INTERVAL` granularity
    /// (GitHub issue #67) — but a call that blocks internally is still unbounded regardless of
    /// what is passed here, and the calling task is then lost with nothing logged (observed
    /// once on ESP32-P4, GitHub issue #66). Consumers running printer I/O on a dedicated task
    /// should subscribe it to the ESP-IDF Task Watchdog, which is the only layer that can
    /// recover from that; no in-crate timeout can, and this one does not claim to.
    /// Passing `Duration::ZERO` disables the
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
        // The adopted fd must be non-blocking: it is what makes mbedTLS's read/write calls
        // inside `negotiate()` (and inside `EspIdfTlsStream`'s later read/write) return
        // `WANT_READ`/`WANT_WRITE` instead of blocking the FreeRTOS task, which is what the
        // poll loops retry on. `Config::non_block` plays no part in that on this path — see
        // the override below. Plaintext callers of `EspIdfTcpStream` never reach this
        // function, so flipping the fd here doesn't affect them.
        raw_stream
            .inner()
            .set_nonblocking(true)
            .map_err(to_esp_socket_error)?;

        let mut cfg = self.certs.build_config();

        // Force `non_block` off for the adopted-socket path (GitHub issue #61). ESP-IDF's
        // `esp_tls_low_level_conn` populates `tls->rset`/`tls->wset` only in its
        // `ESP_TLS_INIT` branch, but `EspTls::adopt` enters at `ESP_TLS_CONNECTING`, so with
        // `non_block = true` the `FD_SET` never ran and `select()` waits out the full
        // `Config::timeout_ms` on zeroed fd sets, returns 0, and the handshake is never
        // started — every retry burns another timeout and `connect` can only end in
        // `TimedOut`. `esp-idf-svc`'s own `EspAsyncTls::negotiate` clears the flag for the
        // same reason. The fd itself stays `O_NONBLOCK` (set above), so mbedTLS still
        // returns `WANT_READ`/`WANT_WRITE` and the poll loop below works as intended.
        // `Config::non_block = true` remains correct for the plain `EspTls::connect` path,
        // so this override is local to `connect` rather than a change to
        // `build_tls_config`. `scripts/check-esp-idf.sh` cannot catch this class of bug —
        // it compiles clean either way; reproducing it needs a flashed board and a printer.
        cfg.non_block = false;

        // Make each `negotiate()` perform exactly one handshake step (GitHub issue #67).
        // With `non_block = false` the call lands in `esp_tls_conn_new_sync`, which is a
        // `while (1)` around `esp_tls_low_level_conn` bounded only by `cfg->timeout_ms` — and
        // `esp-idf-svc`'s `Config::new` defaults that to 4000ms. The fd is `O_NONBLOCK`, so
        // mbedTLS returns `WANT_READ` immediately and that loop simply spins, unyielding, for
        // up to 4s per call. `timeout_ms = 0` makes its `elapsed / 1000 >= timeout_ms` test
        // true on the first pass, so it runs one step and returns 0, which `esp-idf-svc` maps
        // to `EWOULDBLOCK` and `is_would_block` already treats as retryable.
        //
        // Without this the poll loop below is not pacing anything: `TLS_POLL_INTERVAL` and the
        // `connect_timeout` deadline are only evaluated between 4s spins, so a 10s budget has
        // ~4s granularity and overshoots to ~12.06s (measured: five boot-adjacent timeouts
        // within 10ms of each other, 3 x ~4.02s). A successful handshake finishes inside the
        // first spin, so the loop never ran at all on the happy path.
        //
        // Set here rather than in `build_tls_config` because 0 is only safe while `non_block`
        // is false: the `ESP_TLS_CONNECTING` branch passes `cfg->timeout_ms > 0 ? &tv : NULL`
        // to `select()`, so a `non_block = true` caller would get an indefinite block instead
        // of a single step. That branch is unreachable from here, but a shared default would
        // reach it.
        //
        // Side effect: `conn_new_sync`'s early-return path logs
        // `W esp-tls: Failed to open new connection in specified timeout` on every step that
        // does not complete the handshake, so a normal ~1.3s connect emits ~55 `W` lines on a
        // handshake that is going perfectly. That is not worth trading the pacing away for —
        // `timeout_ms = 1` would spin ~1ms per call before logging the identical warning,
        // reintroducing the busy-wait #67 removed without silencing anything — so the noise is
        // handled where it belongs instead, by `EspTlsLogQuiet` around the loop below
        // (GitHub issue #156).
        cfg.timeout_ms = 0;

        let timer = EspIdfTimer::new().map_err(|e| {
            log::debug!("failed to create ESP-IDF async timer for TLS: {e}");
            SocketError::Other("failed to create ESP-IDF async timer for TLS".into())
        })?;

        let mut tls = ::esp_idf_svc::tls::EspTls::adopt(raw_stream).map_err(|e| {
            log::debug!("ESP-TLS adopt of raw socket failed: {e}");
            SocketError::Other("ESP-TLS adopt of raw socket failed".into())
        })?;

        let start = timer.now_millis();

        // Silences the ~60 false `esp-tls` warnings a *successful* handshake would otherwise
        // emit, leaving this function's own summary log as the only account of the handshake.
        // Real `esp-tls` errors still get through — see the type's doc comment for why
        // `ESP_LOG_ERROR` rather than `ESP_LOG_NONE`, and why the one warning that matters is
        // re-reported by `report_anchor_bundle_parse` at construction time instead.
        let _quiet = EspTlsLogQuiet::enter();

        // Splits the handshake into compute (`negotiate_us`) and waiting on the peer
        // (`sleep_us`); the two should add to roughly the elapsed time. Kept because the timeout
        // error below is far more actionable with them than without -- "timed out after 10s, 3
        // steps, 40us in esp_tls" says the peer never answered, which a bare timeout does not.
        // Microseconds rather than milliseconds because per-step compute is often sub-millisecond;
        // see `now_micros`. Two clock reads per step on a path that already sleeps 20ms per step.
        //
        // GitHub issue #160 additionally bucketed per-step costs and reported the slowest and
        // first steps at info level. That answered its question -- the handshake is compute plus
        // peer wait, not poll pacing -- and was removed once it had; recover it from the history
        // of this file rather than rebuilding it if another timing question comes up.
        let mut steps: u32 = 0;
        let mut negotiate_us: u64 = 0;
        let mut sleep_us: u64 = 0;

        loop {
            let step_start = now_micros();
            let step = tls.negotiate(host, &cfg);
            negotiate_us += now_micros().saturating_sub(step_start);
            steps += 1;

            match step {
                Ok(_) => {
                    log::debug!(
                        "ESP-TLS handshake with {} completed in {}ms ({steps} steps, {negotiate_us}us in esp_tls, {sleep_us}us polling)",
                        RedactedHost(host),
                        timer.now_millis().saturating_sub(start)
                    );
                    break;
                }
                Err(e) if is_would_block(&e) => {
                    // connect_timeout == 0 means "disabled" (matching
                    // with_connect_timeout's doc comment and its precedent elsewhere in
                    // this crate), not "expire on the very first would-block poll" — skip the
                    // deadline check entirely in that case.
                    // Saturate rather than truncate: as_millis() is u128, and `as u64` wraps
                    // modulo 2^64. The is_zero() guard above is evaluated on the original
                    // Duration, so a huge-but-nonzero "effectively no timeout" value passed
                    // the guard and then wrapped down to an arbitrarily small deadline — the
                    // opposite of what the caller asked for.
                    let timeout_ms =
                        u64::try_from(self.connect_timeout.as_millis()).unwrap_or(u64::MAX);
                    if !self.connect_timeout.is_zero()
                        && timer.now_millis().saturating_sub(start) >= timeout_ms
                    {
                        log::error!(
                            "ESP-TLS handshake with {} timed out after {timeout_ms}ms ({steps} steps, {negotiate_us}us in esp_tls, {sleep_us}us polling)",
                            RedactedHost(host)
                        );
                        return Err(SocketError::TimedOut);
                    }
                    let sleep_start = now_micros();
                    let slept = timer.sleep(TLS_POLL_INTERVAL).await;
                    sleep_us += now_micros().saturating_sub(sleep_start);
                    slept.map_err(|_| {
                        SocketError::Other(
                            "ESP-IDF timer failed while polling TLS handshake".into(),
                        )
                    })?;
                }
                Err(e) => {
                    log::error!("ESP-TLS handshake with {} failed: {e}", RedactedHost(host));
                    // Checked before the code-based mapping: every certificate rejection
                    // reaches this point as the same opaque `ESP_FAIL`, so the error code
                    // cannot route it. `None` means mbedTLS has no verdict to give and the
                    // code-based mapping stands.
                    if let Some(failure) = query_verify_failure(&tls) {
                        return Err(SocketError::CertificateInvalid(failure));
                    }
                    return Err(map_esp_tls_connect_error(&e));
                }
            }
        }

        Ok(EspIdfTlsStream { tls, timer })
    }

    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        query_negotiated_tls_version(&stream.tls)
    }

    fn peer_chain_der(&self, stream: &Self::Stream) -> Option<Vec<Vec<u8>>> {
        query_peer_chain_der(&stream.tls)
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
