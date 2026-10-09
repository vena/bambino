//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, CertificateFailure, RawStreamFactory, SocketError,
    StdIoError, TimerError, TimerProvider, TlsConnector, TlsVersion, account_for_trust_store,
    esp_tls_read_cap, esp_tls_read_count, map_mbedtls_verify_flags,
    mbedtls_code_from_esp_tls_record, mbedtls_error_kind,
};

use core::net::SocketAddr;

/// Async timer utilizing the ESP-IDF high-resolution timer service.
///
/// Wraps `EspAsyncTimer` to provide non-blocking async sleep that integrates
/// with the FreeRTOS scheduler instead of blocking the task thread.
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

impl TimerProvider for EspIdfTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        // Take the cached timer out (borrow ends on this line, before the await) or allocate a
        // fresh one if a concurrent sleep already holds it — see the field's doc comment.
        let taken = self.timer.borrow_mut().take();
        let mut timer = match taken {
            Some(timer) => timer,
            // A future cancelled mid-await below leaves the slot empty, so this allocation runs
            // after every deadline race the I/O side won — not just at startup (#390).
            None => Self::new_async_timer().map_err(|e| {
                if e.code() == ::esp_idf_svc::sys::ESP_ERR_NO_MEM {
                    TimerError::ResourceExhausted
                } else {
                    TimerError::Other(format!("esp_timer allocation failed: {e}").into())
                }
            })?,
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

        result.map_err(|e| TimerError::Other(format!("esp_timer scheduling failed: {e}").into()))
    }

    fn now_millis(&self) -> u64 {
        // esp_timer_get_time() returns microseconds since boot as i64
        (unsafe { ::esp_idf_svc::sys::esp_timer_get_time() } as u64) / 1000
    }

    /// Reads ESP-IDF's newlib `SystemTime`, which counts from 1970 at boot until SNTP sets it.
    ///
    /// Unsynchronised it is only boot-relative, but it still serves the seed this method
    /// exists for; an SNTP-synced device gets a real per-boot distinction.
    fn unix_millis(&self) -> Option<u64> {
        crate::io::std_unix_millis()
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
const UDP_RECV_POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(15);

/// UDP Socket implementation designed for ESP-IDF's BSD Socket integration.
pub struct EspIdfUdpSocket {
    inner: std::net::UdpSocket,
    timer: EspIdfTimer,
}

impl BindableUdpSocket for EspIdfUdpSocket {
    async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let inner = std::net::UdpSocket::bind(addr).map_err(SocketError::from)?;

        crate::io::configure_std_udp_socket(&inner)?;

        let timer = EspIdfTimer::new().map_err(|e| {
            esp_setup_error(
                &e,
                "failed to create ESP-IDF async timer for UDP recv pacing",
            )
        })?;

        Ok(Self { inner, timer })
    }
}

impl AsyncUdpSocket for EspIdfUdpSocket {
    /// Non-blocking send that reports transient lwIP buffer exhaustion as `TimedOut` rather than a terminal fault.
    ///
    /// `bind` puts the socket in non-blocking mode, and under Wi-Fi load lwIP can momentarily
    /// have no pbuf to hand this datagram. That surfaces as `ERR_MEM`/`ERR_BUF`, which lwIP's
    /// `err_to_errno` table maps to `ENOMEM`/`ENOBUFS` (`lwip/src/api/err.c`) — *not* to
    /// `EWOULDBLOCK`, which that table reserves for `ERR_TIMEOUT`/`ERR_WOULDBLOCK`. Both land in
    /// `map_std_io_error` as `SocketError::ResourceExhausted` and `SocketError::Other`, and `DiscoveryEngine::broadcast_search`
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
            Err(e) => Err(SocketError::from(e)),
        }
    }

    /// Non-blocking read paced with a short sleep on the WouldBlock path so this never busy-spins a caller polling in a tight loop — see `UDP_RECV_POLL_INTERVAL`'s doc comment.
    ///
    /// Not the same mechanism as `TokioUdpSocket::recv_from`, and the numbers differ on purpose:
    /// tokio *waits* up to `UDP_RECV_TIMEOUT_MS` (100 ms) for a datagram and returns as soon as one
    /// arrives, while this returns `TimedOut` after one empty read plus a 15 ms sleep. This platform
    /// has no async socket-readiness primitive for an arbitrary fd (see `TLS_POLL_INTERVAL`'s doc
    /// comment), so it cannot wait the way tokio does. Discovery depends on neither value: it
    /// treats `TimedOut` as "nothing yet" and keeps polling until its own window closes.
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
        match self.inner.recv_from(buf) {
            Ok((len, addr)) => Ok((len, addr)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Err(e) = self.timer.sleep(UDP_RECV_POLL_INTERVAL).await {
                    log::debug!("EspIdfUdpSocket::recv_from: pacing sleep failed: {e:?}");
                }
                Err(SocketError::TimedOut)
            }
            Err(e) => Err(SocketError::from(e)),
        }
    }
}

/// True if `err` is lwIP momentarily running out of send buffers, rather than a real fault.
///
/// Matched on `raw_os_error` rather than `ErrorKind` because std has no kind for `ENOBUFS` (it
/// decodes to `Uncategorized`). `ENOMEM` does decode, to `OutOfMemory`, but is matched the same
/// way so both lwIP buffer shortages stay together. See `send_to`'s doc comment for where these
/// errnos come from.
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

/// Maps an `EspError` from an ESP-IDF allocation call to `ResourceExhausted` when it is `ESP_ERR_NO_MEM`, else to `Other(context)`.
///
/// Used for `EspIdfTimer::new` (`esp_timer_create` returns `ESP_ERR_NO_MEM`) and `EspTls::adopt`
/// (esp-idf-svc 0.53.0 turns `esp_tls_init` returning `NULL` into `ESP_ERR_NO_MEM`). GitHub
/// issue #385.
fn esp_setup_error(err: &::esp_idf_svc::sys::EspError, context: &'static str) -> SocketError {
    log::debug!("{context}: {err}");
    if err.code() == ::esp_idf_svc::sys::ESP_ERR_NO_MEM {
        SocketError::ResourceExhausted
    } else {
        SocketError::Other(context.into())
    }
}

/// True if `err` indicates a non-blocking `connect()` is still in progress rather than a genuine failure.
/// `WouldBlock` covers whatever errno std's generic Unix `ErrorKind` decoder maps to it
/// (`EAGAIN`/`EWOULDBLOCK`); `EINPROGRESS` — the errno `connect()` actually returns for a pending
/// non-blocking connection — is checked separately because std's decoder does not recognize it as
/// `WouldBlock` (confirmed against `socket2`'s own `Socket::connect_timeout()`, which checks both
/// independently for the same reason).
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
        // `select()` allocates; under memory pressure it fails `ENOMEM`, which reached callers
        // as `Other` until an ESP32-C6 out-of-memory sweep caught it (GitHub issue #385).
        if err.kind() == std::io::ErrorKind::OutOfMemory {
            log::debug!("poll() failed while polling ESP-IDF TCP connect: {err}");
            return Err(SocketError::ResourceExhausted);
        }
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
async fn poll_connect_until_complete(
    socket: &::socket2::Socket,
    timer: &EspIdfTimer,
) -> Result<(), SocketError> {
    use std::os::fd::AsRawFd;

    let fd = socket.as_raw_fd();

    loop {
        if let Some(err) = socket.take_error().map_err(SocketError::from)? {
            return Err(SocketError::from(err));
        }

        let revents = poll_connect_revents(fd)?;

        if revents != 0 {
            // Readiness only says lwIP reached a verdict; SO_ERROR says which one. A refused or
            // unreachable connect reports POLLERR here, not a `poll()` failure.
            if let Some(err) = socket.take_error().map_err(SocketError::from)? {
                return Err(SocketError::from(err));
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

        timer.sleep(TLS_POLL_INTERVAL).await.map_err(|e| {
            crate::io::timer_failure_error(e, "ESP-IDF timer failed while polling TCP connect")
        })?;
    }
}

/// Poll interval between non-blocking TLS retry attempts (handshake and read/write).
///
/// `esp-idf-svc`/`esp-idf-hal` expose no async socket-readiness primitive for an
/// arbitrary fd (confirmed by inspecting `esp-idf-svc` 0.53.0's source, not just its
/// docs — the only async wait building block available is `EspAsyncTimer`). Real
/// wake-on-ready is possible via `esp_idf_svc::tls::EspAsyncTls` combined with the
/// `async-io` crate and `MountedEventfs`, but that needs a new dependency and real
/// app-side setup (a sized eventfd mount, a dedicated thread with a bumped stack, and
/// working around an ESP-IDF main-task/async-io-thread priority inversion) — left as a
/// future upgrade. This fixed-interval poll works because `EspIdfTlsConnector::connect` puts the
/// adopted fd in `O_NONBLOCK` *and* pins `Config::timeout_ms = 1`, so every `EspTls` call returns
/// after ~1ms of handshake progress instead of blocking inside the FFI call for the whole
/// handshake, and an outer `TimerProvider`-based timeout can preempt the operation between poll
/// attempts. Both are required: `O_NONBLOCK` alone still left `esp_tls_conn_new_sync` spinning
/// internally for up to the default 4s per call, which made this interval dead time between spins
/// rather than pacing (GitHub issue #67). (`Config::non_block` is deliberately *off* on the
/// adopted-socket path — see the comment in `connect` and GitHub issue #61.)
const TLS_POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(20);

/// How far a single `esp_tls_conn_new_sync` call may carry the handshake before returning, in
/// milliseconds -- the bound that makes [`TLS_POLL_INTERVAL`] and `connect_timeout` mean anything.
///
/// `1`, not `0`, because ESP-IDF changed what `0` means inside the 5.5 series; the version table
/// and measurements are on `pin_handshake_step!`, the one place both cfg builders set it (GitHub
/// issue #294).
///
/// `u32` to match `esp_idf_svc::tls::Config::timeout_ms`, the narrower of the two field types;
/// the raw `esp_tls_cfg::timeout_ms` is a `c_int` and is cast in `pin_handshake_step!`.
const TLS_HANDSHAKE_STEP_BUDGET_MS: u32 = 1;

/// The connector's own handshake deadline until `.with_connect_timeout(d)` sets one: disabled.
///
/// `PrinterClient` already bounds the whole dial + handshake with `with_connect_timeout`, and a
/// non-zero inner default silently capped that outer budget (GitHub issue #537). A direct
/// consumer driving the connector without `PrinterClient` opts in instead.
const DEFAULT_CONNECT_TIMEOUT: core::time::Duration = core::time::Duration::ZERO;

/// True if `err` indicates the non-blocking TLS operation would have blocked and should be retried, rather than a real failure.
/// `EWOULDBLOCK` is included alongside the two `esp_tls`-specific codes because
/// `EspTls::connect`/`read`/`write` can surface it directly in non-blocking mode — documented
/// upstream as "a peculiarity/bug of the esp-tls C module".
fn is_would_block(err: &::esp_idf_svc::sys::EspError) -> bool {
    let code = err.code();
    code == ::esp_idf_svc::sys::EWOULDBLOCK as i32
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_READ
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_WRITE
}

/// Reads and clears the last error of `err_type` (an `esp_tls_error_type_t`) recorded on `tls`'s error handle, if any.
///
/// The handshake loop reads the ESP type to tell ESP-IDF v5.5.5's "this call's `timeout_ms` budget expired,
/// handshake still in progress" apart from a real handshake failure. v5.5.5 signals the former as
/// a `-1` return from `esp_tls_conn_new_sync` plus `ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT` here,
/// where v5.5.3/v5.5.4/v6.0.1 return `0` (which reaches Rust as `EWOULDBLOCK`, already retryable
/// via [`is_would_block`]). Both `-1` cases collapse to `ESP_FAIL` by the time `esp-idf-svc` is
/// done with them, so the error handle is the only discriminator left.
///
/// Clearing is what makes the answer trustworthy: every version records
/// `ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT` on expiry, so a value left behind by an earlier retryable
/// step would otherwise read as "retryable" on a later genuine failure that recorded no ESP-type
/// error of its own. It also reads the mbedTLS type, which carries the actual cause of a failed
/// step (`MBEDTLS_ERR_NET_CONN_RESET` and friends) — see [`map_esp_tls_connect_error`].
fn take_esp_tls_error<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
    err_type: ::esp_idf_svc::sys::esp_tls_error_type_t,
) -> Option<i32> {
    let mut handle: ::esp_idf_svc::sys::esp_tls_error_handle_t = core::ptr::null_mut();

    // SAFETY: `tls.context_handle()` is a live `esp_tls` handle owned by `tls`, which outlives
    // this call; `handle` is a valid out-pointer into this stack frame.
    let got =
        unsafe { ::esp_idf_svc::sys::esp_tls_get_error_handle(tls.context_handle(), &mut handle) };
    if got != ::esp_idf_svc::sys::ESP_OK || handle.is_null() {
        return None;
    }

    let mut code: ::core::ffi::c_int = 0;
    // SAFETY: `handle` is non-null and owned by `tls`; `code` is a valid out-pointer into this
    // stack frame. Returns `ESP_OK` only when it actually populated `code`.
    let ret = unsafe {
        ::esp_idf_svc::sys::esp_tls_get_and_clear_error_type(handle, err_type, &mut code)
    };

    (ret == ::esp_idf_svc::sys::ESP_OK).then_some(code)
}

/// Maps a non-retryable failed handshake step to a `SocketError`, from the error records `esp_tls` kept for it.
///
/// A failed step always reaches Rust as the same opaque `ESP_FAIL` (see
/// `negotiate_unverified_step`, mirroring esp-idf-svc 0.53.0's `internal_connect`), so the
/// `EspError` itself cannot route anything: the cause lives in the mbedTLS-type record, drained
/// by [`take_esp_tls_error`] before this runs. DNS/dial codes never apply here — the connector
/// adopts an already-connected socket. Certificate rejections are routed earlier, by
/// `query_verify_failure`. Anything unrecognized falls back to `SocketError::Other` carrying both
/// recorded codes (#302).
///
/// `mbedtls_err` must already be in mbedTLS's negative convention (see
/// [`mbedtls_code_from_esp_tls_record`]), and is printed that way in the `Other` message.
/// `ESP_ERR_NO_MEM` in the ESP-type record (e.g. `set_client_config` failing to copy the
/// hostname) is an allocation failure with no mbedTLS code, so it is checked separately.
fn map_esp_tls_connect_error(
    err: &::esp_idf_svc::sys::EspError,
    esp_err: Option<i32>,
    mbedtls_err: Option<i32>,
) -> SocketError {
    if let Some(kind) = mbedtls_err.and_then(mbedtls_error_kind) {
        return super::map_embedded_io_error_kind(kind);
    }
    if esp_err == Some(::esp_idf_svc::sys::ESP_ERR_NO_MEM) {
        return SocketError::ResourceExhausted;
    }
    log::debug!(
        "ESP-IDF TLS handshake failed: {err} (esp_tls {esp_err:?}, mbedtls {mbedtls_err:?})"
    );
    SocketError::Other(
        std::format!(
            "ESP-IDF TLS handshake failed: {err} (esp_tls {esp_err:?}, mbedtls {mbedtls_err:?})"
        )
        .into(),
    )
}

/// Cert bundle used by `EspIdfTlsConnector`'s `ca_pem`/`client_cert`/`client_key` fields and `unverified()`/`with_certs()` constructors.
/// Factored out so a future cert-related option (e.g. ALPN config) only needs to be added in
/// one place.
///
/// `ca_pem` holds the caller's trust anchors already converted to a NUL-terminated PEM bundle
/// (see `crate::io::der_certs_to_pem_bundle`) rather than raw DER, because DER can only ever
/// express *one* anchor to mbedTLS. The conversion happens once at construction, not per
/// connect, so `build_config` stays a cheap borrow.
struct EspIdfTlsCerts {
    ca_pem: Option<Vec<u8>>,
    /// How many DER anchors the caller supplied, i.e. how many `ca_pem` should load. The
    /// baseline a failed handshake's trust store is compared against (GitHub issue #384).
    anchor_count: usize,
    client_cert: Option<Vec<u8>>,
    client_key: Option<Vec<u8>>,
}

impl EspIdfTlsCerts {
    fn new() -> Self {
        Self {
            ca_pem: None,
            anchor_count: 0,
            client_cert: None,
            client_key: None,
        }
    }

    /// Fails on an empty `ca_certs` or one where no anchor parses: either would leave nothing to verify against.
    fn with_certs(
        ca_certs: impl IntoIterator<Item = Vec<u8>>,
        client_auth: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Result<Self, crate::Error> {
        let (client_cert, client_key) = match client_auth {
            Some((cert, key)) => (Some(cert), Some(key)),
            None => (None, None),
        };
        // Collected rather than streamed straight into the bundle builder so the anchor count
        // is known: `report_anchor_bundle_parse` needs the denominator to say "3 of 5 anchors
        // failed" instead of just "3 failed".
        let ca_certs: Vec<Vec<u8>> = ca_certs.into_iter().collect();
        let anchor_count = ca_certs.len();
        // `None` only for an empty iterator.
        let Some(ca_pem) = crate::io::der_certs_to_pem_bundle(ca_certs) else {
            return Err(crate::Error::InvalidArgument(
                "with_certs got no trust anchors; use EspIdfTlsConnector::unverified() to skip \
                 verification"
                    .into(),
            ));
        };
        if !report_anchor_bundle_parse(&ca_pem, anchor_count) {
            return Err(crate::Error::InvalidArgument(
                "none of the trust anchors given to with_certs parsed (they must be DER)".into(),
            ));
        }

        Ok(Self {
            ca_pem: Some(ca_pem),
            anchor_count,
            client_cert,
            client_key,
        })
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
/// this slice up to and including its single trailing NUL. Same input is **not** the same
/// result, though: `mbedtls_x509_crt_parse` allocates as it goes and skips an anchor whose PEM
/// decode cannot allocate, so a handshake under memory pressure can hold fewer anchors than this
/// parse reported (observed on an ESP32-P4: partial here, aborted at handshake). This report
/// therefore covers the anchors themselves, not every handshake; a handshake that ran short is
/// caught afterwards by [`count_handshake_anchors`] (GitHub issue #384).
///
/// Returns false only when no anchor parsed, which `with_certs` turns into an error. A partial
/// store is reported but accepted: mbedTLS's own policy is that it is still usable, and failing
/// the connector here would reject a configuration ESP-IDF accepts.
///
/// `expected` is the number of DER certificates that went into the bundle.
fn report_anchor_bundle_parse(ca_pem: &[u8], expected: usize) -> bool {
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
             -0x{:04X})",
            -err
        ),
    }
    ret >= 0
}

use crate::io::RedactedHost;

/// `esp-tls`'s log tag, as a NUL-terminated C string for `esp_log_level_set`/`_get`.
const ESP_TLS_LOG_TAG: &[u8] = b"esp-tls\0";

/// Nesting depth of live [`EspTlsLogQuiet`] guards and the `esp-tls` tag's log level as it was
/// before the outermost guard lowered it. Held behind one lock so "decide whether I am the
/// outermost guard, then read/write the saved level" is one atomic transaction — two separate
/// atomics for depth and saved level let an outer guard's drop interleave with an inner guard's
/// enter and clobber the saved level (see the enter/exit race this replaced).
static ESP_TLS_LOG_QUIET: std::sync::Mutex<(usize, u32)> = std::sync::Mutex::new((0, 0));

/// Lowers the `esp-tls` tag to `ESP_LOG_ERROR` for the length of a handshake, restoring it on drop.
///
/// A **successful** handshake emitted ~60 `W esp-tls: Failed to open new connection in
/// specified timeout` lines before this existed (measured: 62 lines on a 1.42s ESP32-P4
/// handshake that connected first try). Nothing had failed and no timeout had been exceeded —
/// `esp_tls_conn_new_sync` logs that line on every step that does not *complete* the
/// handshake, and `connect` pins `Config::timeout_ms = 1` so every step returns promptly
/// (GitHub issues #67 and #294). The count therefore scales with handshake duration, one line per
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
/// trust store. The construction-time report cannot see a parse that runs short only at
/// handshake time; what covers that case is `connect` counting the failed handshake's anchors
/// and reporting `IncompleteTrustStore` (GitHub issue #384), not this warning.
///
/// Nesting-counted because MQTT and FTPS can handshake concurrently on separate tasks: without
/// it the inner guard's drop would un-silence the tag while the outer handshake was still
/// stepping.
///
/// No-op where `CONFIG_LOG_DYNAMIC_LEVEL_CONTROL` is disabled — `esp_log_level_set` does
/// nothing there and the old noise comes back, which is a degraded log, not a broken
/// handshake.
struct EspTlsLogQuiet;

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

/// Sets the two handshake fields every `esp_tls_cfg` `EspIdfTlsConnector::connect` uses must carry, on either config type.
///
/// A macro because the anchored path's `esp_idf_svc::tls::Config` and the anchor-less path's raw
/// `esp_tls_cfg` are different types with the same two field names (and `timeout_ms` of different
/// integer types). Both cfg builders call it, so the two paths cannot drift again: the
/// anchor-less path starts from `esp_tls_cfg::default()`, kept a zeroed `timeout_ms` while the
/// anchored path was fixed for #67, and still reported `1 steps, 0us polling` on hardware.
///
/// **`non_block = false`** (GitHub issue #61). ESP-IDF's `esp_tls_low_level_conn` populates
/// `tls->rset`/`tls->wset` only in its `ESP_TLS_INIT` branch, but `EspTls::adopt` enters at
/// `ESP_TLS_CONNECTING`, so with `non_block = true` the `FD_SET` never ran and `select()` waits
/// out the full `timeout_ms` on zeroed fd sets, returns 0, and the handshake is never started —
/// every retry burns another timeout and `connect` can only end in `TimedOut`. `esp-idf-svc`'s
/// own `EspAsyncTls::negotiate` clears the flag for the same reason. The fd itself stays
/// `O_NONBLOCK` (see `EspIdfTcpStream`), so mbedTLS still returns `WANT_READ`/`WANT_WRITE` and
/// the poll loop works. `scripts/check-esp-idf.sh` cannot catch this class of bug — it compiles
/// clean either way; reproducing it needs a flashed board and a printer.
///
/// **`timeout_ms = TLS_HANDSHAKE_STEP_BUDGET_MS`** (GitHub issues #67, #294). With `non_block =
/// false` the call lands in `esp_tls_conn_new_sync`, a `while (1)` around
/// `esp_tls_low_level_conn` bounded only by `cfg->timeout_ms` — and `esp-idf-svc`'s `Config::new`
/// defaults that to 4000ms. The fd is `O_NONBLOCK`, so mbedTLS returns `WANT_READ` immediately and
/// that loop simply spins, unyielding, for up to 4s per call. Without a bound the poll loop is
/// not pacing anything: `TLS_POLL_INTERVAL` and the `connect_timeout` deadline are only evaluated
/// between spins, so a 10s budget has ~4s granularity and overshoots to ~12.06s (measured: five
/// boot-adjacent timeouts within 10ms of each other, 3 x ~4.02s). A successful handshake
/// finishes inside the first spin, so the loop never ran at all on the happy path.
///
/// `1`, not `0`, because ESP-IDF changed what `0` means mid-patch-series. The relevant
/// `conn_new_sync` gates its expiry check on `ret == 0 && cfg->timeout_ms <op> 0`, where `<op>`
/// and the value returned on expiry differ across the provisioned checkouts:
///
/// ```text
/// v5.5.3 / v5.5.4 / v6.0.1   `>= 0`, and the expiry returns `0`
/// v5.5.5                     `> 0`,  and the expiry returns `-1`
/// ```
///
/// So `timeout_ms = 0` bounded the call on 5.5.3/5.5.4/6.0.1 (`elapsed >= 0` is true on the first
/// pass → one step per call) but disabled the bound entirely on v5.5.5, where the test is never
/// reached and a single `negotiate()` runs the whole handshake inside `while (1)`. Measured on
/// ESP32-P4 against a P1S: 113 of 113 v5.5.5 handshakes reported `1 steps` with `0us polling`,
/// worst case 33.8s of uninterruptible block, tripping the Task Watchdog ~30 times per unattended
/// run. `1` satisfies both comparisons, so the bound holds on every version.
///
/// The cost is that a step is "as much handshake as fits in ~1ms" instead of exactly one — a
/// ~1ms busy-wait per call, against a 20ms sleep between calls. v5.5.5's `-1` on expiry is *not*
/// distinguishable from a real failure by return value (`esp-idf-svc` maps both to `ESP_FAIL`) —
/// see `take_esp_tls_error` and the handshake loop for how the retryable case is recovered.
///
/// The two pins only make sense together: the `ESP_TLS_CONNECTING` branch feeds `timeout_ms` to
/// `select()` when `non_block` is true, so a 1ms `timeout_ms` alone would be a 1ms readiness
/// poll instead of a handshake step.
///
/// Side effect: `conn_new_sync`'s expiry path logs `W esp-tls: Failed to open new connection in
/// specified timeout` on every step that does not complete the handshake, on every version.
/// `EspTlsLogQuiet` around the handshake loop handles that noise (GitHub issue #156).
macro_rules! pin_handshake_step {
    ($cfg:expr, $budget_ms:expr) => {{
        $cfg.non_block = false;
        $cfg.timeout_ms = $budget_ms as _;
    }};
}

/// Builds an `esp_idf_svc::tls::Config` from cert bytes, with the handshake pins set (`pin_handshake_step!`).
///
/// `ca_pem` is a NUL-terminated PEM bundle (`der_certs_to_pem_bundle`); the client cert/key
/// stay DER, since each is a single item and DER is this crate's public convention.
fn build_tls_config<'a>(
    ca_pem: &'a Option<Vec<u8>>,
    client_cert: &'a Option<Vec<u8>>,
    client_key: &'a Option<Vec<u8>>,
) -> ::esp_idf_svc::tls::Config<'a> {
    let mut cfg = ::esp_idf_svc::tls::Config::new();
    pin_handshake_step!(cfg, TLS_HANDSHAKE_STEP_BUDGET_MS);

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

/// `crt_bundle_attach` hook that disables mbedTLS server-certificate verification outright,
/// used by `build_unverified_tls_cfg` for `EspIdfTlsConnector::unverified()`.
///
/// ESP-IDF v5.5.5's only built-in way to skip verification is the build-time
/// `CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY`, which depends on `CONFIG_ESP_TLS_INSECURE` (off by
/// default) -- an app sdkconfig choice bambino cannot see or require, with no field for it in
/// `esp_idf_svc::tls::Config` (0.53.0) (GitHub issue #168). `crt_bundle_attach` has neither
/// limitation: confirmed against ESP-IDF v5.5.5's `esp_tls_mbedtls.c` `set_client_config`, it is
/// checked *before* `use_global_ca_store`/`cacert_buf` and the no-verification fallback, gated
/// only by `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` (on by ESP-IDF default), and `set_client_config`
/// sets `MBEDTLS_SSL_VERIFY_REQUIRED` immediately *before* invoking this hook -- so overriding
/// it back to `MBEDTLS_SSL_VERIFY_NONE` here is what actually takes effect. `conf` is the
/// `mbedtls_ssl_config*` `set_client_config` builds, passed through as `void*`; ESP-IDF's own
/// `esp_crt_bundle_attach` (the implementation this replaces) receives and casts the same
/// pointer. The C call site never inspects this function's return value, so `ESP_OK` is
/// returned unconditionally.
#[cfg(esp_idf_mbedtls_certificate_bundle)]
unsafe extern "C" fn accept_any_certificate(
    conf: *mut ::core::ffi::c_void,
) -> ::esp_idf_svc::sys::esp_err_t {
    // SAFETY: `set_client_config` only ever calls a configured `crt_bundle_attach` with a live,
    // correctly-typed `mbedtls_ssl_config*` for the connection currently being negotiated --
    // the same contract `esp_crt_bundle_attach` itself relies on.
    unsafe {
        ::esp_idf_svc::sys::mbedtls_ssl_conf_authmode(
            conf.cast(),
            ::esp_idf_svc::sys::MBEDTLS_SSL_VERIFY_NONE as ::core::ffi::c_int,
        );
    }
    0
}

/// Builds a raw, anchor-less `esp_tls_cfg` for `EspIdfTlsConnector::connect`'s no-anchor case.
///
/// Bypasses `esp_idf_svc::tls::Config` and its private `Config::try_into_raw` entirely -- see
/// `accept_any_certificate`'s doc comment for why `esp_idf_svc` leaves no other way to reach
/// this. Not `#[cfg(esp_idf_mbedtls_certificate_bundle)]` itself: `esp_tls_cfg::default()` and
/// the client-cert/key fields always exist, so this stays callable from every build: it just
/// comes back without a working `crt_bundle_attach` hook when the Kconfig is off, and
/// `EspIdfTlsConnector::connect` never reaches this function in that case (see its own
/// anchor-less-but-no-bundle early return).
///
/// Client cert/key field names mirror `esp_idf_svc::tls::Config::try_into_raw`'s mapping onto
/// the same bindgen anonymous unions, so the two cfg-building paths stay easy to compare.
fn build_unverified_tls_cfg(
    client_cert: &Option<Vec<u8>>,
    client_key: &Option<Vec<u8>>,
) -> ::esp_idf_svc::sys::esp_tls_cfg {
    let mut rcfg = ::esp_idf_svc::sys::esp_tls_cfg::default();
    pin_handshake_step!(rcfg, TLS_HANDSHAKE_STEP_BUDGET_MS);

    #[cfg(esp_idf_mbedtls_certificate_bundle)]
    {
        rcfg.crt_bundle_attach = Some(accept_any_certificate);
    }

    if let (Some(cert), Some(key)) = (client_cert, client_key) {
        rcfg.__bindgen_anon_3.clientcert_buf = cert.as_ptr();
        rcfg.__bindgen_anon_4.clientcert_bytes = cert.len() as u32;
        rcfg.__bindgen_anon_5.clientkey_buf = key.as_ptr();
        rcfg.__bindgen_anon_6.clientkey_bytes = key.len() as u32;
    }

    rcfg
}

/// One raw handshake step for `EspIdfTlsConnector::connect`'s no-anchor case -- the
/// raw-`esp_tls_cfg` equivalent of `EspTls::negotiate`, which can't be used here because it
/// always converts through the private `Config::try_into_raw` (see `build_unverified_tls_cfg`).
///
/// Mirrors `esp_idf_svc::tls::EspTls::internal_connect`'s exact return-code mapping --
/// `internal_connect` is private, so it can't be called directly, and `context_handle()` is the
/// one seam `esp_idf_svc` exposes to reach the same `*mut esp_tls` it uses internally. Keep this
/// in sync with `internal_connect` if `esp-idf-svc` is ever upgraded.
fn negotiate_unverified_step<S: ::esp_idf_svc::tls::Socket>(
    tls: &mut ::esp_idf_svc::tls::EspTls<S>,
    host: &str,
    rcfg: &::esp_idf_svc::sys::esp_tls_cfg,
) -> Result<(), ::esp_idf_svc::sys::EspError> {
    // SAFETY: `tls.context_handle()` is a live `esp_tls` handle owned by `tls`, which outlives
    // this call; `host` is a valid UTF-8 `&str` and `rcfg` a live `esp_tls_cfg` for the
    // duration of the call, both borrowed from the caller's stack frame.
    let ret = unsafe {
        ::esp_idf_svc::sys::esp_tls_conn_new_sync(
            host.as_bytes().as_ptr().cast(),
            host.len() as ::core::ffi::c_int,
            0,
            rcfg,
            tls.context_handle(),
        )
    };

    match ret {
        1 => Ok(()),
        ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_READ => {
            Err(::esp_idf_svc::sys::EspError::from_infallible::<
                { ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_READ },
            >())
        }
        ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_WRITE => {
            Err(::esp_idf_svc::sys::EspError::from_infallible::<
                { ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_WRITE },
            >())
        }
        0 => Err(::esp_idf_svc::sys::EspError::from_infallible::<
            { ::esp_idf_svc::sys::EWOULDBLOCK as i32 },
        >()),
        _ => Err(::esp_idf_svc::sys::EspError::from_infallible::<
            { ::esp_idf_svc::sys::ESP_FAIL },
        >()),
    }
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
pub struct EspIdfTlsStream<S>
where
    S: ::esp_idf_svc::tls::Socket,
{
    tls: ::esp_idf_svc::tls::EspTls<S>,
    timer: EspIdfTimer,
    /// Largest read passed to `esp_tls`, from `esp_tls_read_cap` at connect (GitHub issue #387).
    read_cap: usize,
}

impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::ErrorType for EspIdfTlsStream<S> {
    type Error = embedded_io_async::ErrorKind;
}

impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::Read for EspIdfTlsStream<S> {
    // Capped, and the count checked, because `esp_tls` can return an error code as a byte count:
    // see `ESP_TLS_READ_CAP`. A short read is normal, so the cap needs nothing from callers.
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let len = buf.len().min(self.read_cap);
        let buf = &mut buf[..len];
        let tls = &mut self.tls;
        let n = retry_on_would_block(&self.timer, "read", || tls.read(buf)).await?;
        esp_tls_read_count(n, len)
    }
}

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

/// Classifies a post-handshake `EspTls` read/write failure into the closest `embedded_io_async::ErrorKind`.
///
/// `esp_tls_conn_read`/`write` return `esp_mbedtls_read`/`write`'s result (ESP-IDF
/// `esp_tls_mbedtls.c`), so the code is an mbedTLS one, in mbedTLS's own negative convention
/// (unlike the handshake path's error record, nothing here is negated). A positive errno never
/// reaches here, which is why the old `ECONNRESET`/`ETIMEDOUT` arms were dead and a peer reset read as
/// "non-network I/O error" (#303, a regression of #46). Unrecognized codes fall back to
/// `Other`, with the real code preserved at `log::debug!`.
fn esp_tls_io_error_kind(err: &::esp_idf_svc::sys::EspError) -> embedded_io_async::ErrorKind {
    mbedtls_error_kind(err.code()).unwrap_or_else(|| {
        log::debug!("ESP-IDF TLS I/O failed: {err}");
        embedded_io_async::ErrorKind::Other
    })
}

/// Why [`poll_until_ready`] stopped without a result.
enum PollError<E> {
    /// `op` failed with something other than would-block.
    Op(E),
    /// The pacing sleep between attempts failed.
    Timer(TimerError),
}

/// Calls `op` until it stops reporting would-block, sleeping `TLS_POLL_INTERVAL` between attempts.
///
/// Shared by `EspIdfTlsStream` (`EspTls` calls) and `EspIdfTcpStream` (`std::io` calls). Takes
/// `timer`/`op` separately rather than `&mut self` so a caller can borrow the stream (via the
/// closure) and the timer as disjoint fields.
async fn poll_until_ready<T, E>(
    timer: &EspIdfTimer,
    mut op: impl FnMut() -> Result<T, E>,
    would_block: impl Fn(&E) -> bool,
) -> Result<T, PollError<E>> {
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(e) if would_block(&e) => {
                timer
                    .sleep(TLS_POLL_INTERVAL)
                    .await
                    .map_err(PollError::Timer)?;
            }
            Err(e) => return Err(PollError::Op(e)),
        }
    }
}

/// Runs one `EspTls` read or write through [`poll_until_ready`], classifying its failure.
async fn retry_on_would_block(
    timer: &EspIdfTimer,
    op_name: &str,
    op: impl FnMut() -> Result<usize, ::esp_idf_svc::sys::EspError>,
) -> Result<usize, embedded_io_async::ErrorKind> {
    poll_until_ready(timer, op, is_would_block)
        .await
        .map_err(|e| match e {
            PollError::Op(e) => {
                log::debug!("ESP-IDF TLS {op_name} failed: {e}");
                esp_tls_io_error_kind(&e)
            }
            PollError::Timer(e) => e.into(),
        })
}

/// Returns the raw mbedTLS context behind `tls`, or `None` when esp-tls has none (not yet set up, or torn down).
fn ssl_context<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<*mut ::esp_idf_svc::sys::mbedtls_ssl_context> {
    // SAFETY: `context_handle()` is the live `esp_tls` handle owned by `tls`, which outlives this
    // call, and `esp_tls_get_ssl_context` only reads it. The returned context is owned by that
    // same handle, so it stays valid for as long as the caller holds `&tls`.
    let ctx = unsafe { ::esp_idf_svc::sys::esp_tls_get_ssl_context(tls.context_handle()) }
        .cast::<::esp_idf_svc::sys::mbedtls_ssl_context>();
    (!ctx.is_null()).then_some(ctx)
}

/// Shared mbedTLS version query.
/// Generic over the adopted `Socket` impl so it isn't tied to one connector shape.
fn query_negotiated_tls_version<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<TlsVersion> {
    let ssl_ctx = ssl_context(tls)?;

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
fn query_peer_chain_der<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<Vec<Vec<u8>>> {
    let ssl_ctx = ssl_context(tls)?;

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
fn query_verify_failure<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> Option<CertificateFailure> {
    let ssl_ctx = ssl_context(tls)?;

    let flags = unsafe { ::esp_idf_svc::sys::mbedtls_ssl_get_verify_result(ssl_ctx) };

    map_mbedtls_verify_flags(flags)
}

/// Counts the trust anchors a failed handshake actually held.
///
/// A failed handshake leaves the SSL context and its config alive in ESP-IDF v5.5.5: on
/// failure `esp_mbedtls_handshake` only sets `conn_state = ESP_TLS_FAIL`, and
/// `esp_mbedtls_cleanup` runs on a setup failure or on delete, never here. The config's CA
/// chain is `tls->cacert`, installed by `set_ca_cert` through `mbedtls_ssl_conf_ca_chain`.
/// mbedTLS unlinks a certificate that fails to parse (`mbedtls_x509_crt_parse_der_internal`),
/// and an initialized but unused head node has `version == 0`, so the nodes with a nonzero
/// version are exactly the anchors loaded.
///
/// mbedTLS offers no getter for a config's CA chain, so this reads the `MBEDTLS_PRIVATE`
/// fields bindgen exposes as `private_conf`/`private_ca_chain`. Read-only, and bindgen
/// regenerates them from the headers being built against, so a layout change breaks the build
/// rather than this read; it is still not an mbedTLS API guarantee, so re-check on an
/// ESP-IDF/mbedTLS bump. A missing context or config counts as zero anchors: in a path where
/// `query_verify_failure` has already read a verdict off this same context that cannot happen,
/// and if it did, reporting a short store is the answer that does not over-claim.
fn count_handshake_anchors<S: ::esp_idf_svc::tls::Socket>(
    tls: &::esp_idf_svc::tls::EspTls<S>,
) -> usize {
    let Some(ssl_ctx) = ssl_context(tls) else {
        return 0;
    };

    // SAFETY: `ssl_ctx` is the live context owned by `tls`, which outlives this call; `conf` and
    // each chain node are owned by that context's `esp_tls_t` and are only read here.
    let conf = unsafe { (*ssl_ctx).private_conf };
    if conf.is_null() {
        return 0;
    }
    let mut node = unsafe { (*conf).private_ca_chain }.cast_const();

    let mut loaded = 0;
    while !node.is_null() {
        // SAFETY: as above; `next` is either null or another node of the same chain.
        let (version, next) = unsafe { ((*node).version, (*node).next) };
        if version != 0 {
            loaded += 1;
        }
        node = next.cast_const();
    }
    loaded
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
pub struct EspIdfTcpStream {
    stream: Option<std::net::TcpStream>,
    timer: EspIdfTimer,
}

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
    ///
    /// Iterates every resolved address, matching `TokioRawStreamFactory::dial`
    /// (`tokio::net::TcpStream::connect`) — a hostname resolving to multiple addresses
    /// (mDNS `.local`, A + AAAA) whose first entry is unreachable falls through to the next
    /// instead of failing the dial.
    ///
    /// **A hostname blocks the calling task while it resolves.** `to_socket_addrs` calls lwIP's
    /// synchronous `getaddrinfo`, which has no `.await` point, so the executor task — and every
    /// future on it, including the outer connect-timeout race — stalls until DNS answers or gives
    /// up. How long that can take on hardware hasn't been measured. An IP literal is only parsed
    /// and never blocks; resolve a hostname yourself beforehand if the task must stay responsive.
    pub async fn connect(host: &str, port: u16) -> Result<Self, SocketError> {
        use std::net::ToSocketAddrs;

        let addrs = (host, port).to_socket_addrs().map_err(SocketError::from)?;

        let timer = EspIdfTimer::new().map_err(|e| {
            esp_setup_error(&e, "failed to create ESP-IDF async timer for TCP connect")
        })?;

        let mut last_err = None;
        for addr in addrs {
            let socket = match ::socket2::Socket::new(
                ::socket2::Domain::for_address(addr),
                ::socket2::Type::STREAM,
                Some(::socket2::Protocol::TCP),
            ) {
                Ok(socket) => socket,
                Err(e) => {
                    last_err = Some(SocketError::from(e));
                    continue;
                }
            };

            if let Err(e) = socket.set_nonblocking(true) {
                last_err = Some(SocketError::from(e));
                continue;
            }

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
                crate::io::warn_nodelay_failed(&e);
            }

            match socket.connect(&addr.into()) {
                Ok(()) => {}
                Err(e) if is_connect_in_progress(&e) => {}
                Err(e) => {
                    last_err = Some(SocketError::from(e));
                    continue;
                }
            }

            match poll_connect_until_complete(&socket, &timer).await {
                Ok(()) => {
                    return Ok(Self {
                        stream: Some(socket.into()),
                        timer,
                    });
                }
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or(SocketError::AddressNotAvailable))
    }

    fn inner(&self) -> &std::net::TcpStream {
        live(self.stream.as_ref())
    }

    fn inner_mut(&mut self) -> &mut std::net::TcpStream {
        live(self.stream.as_mut())
    }
}

/// The socket inside an `EspIdfTcpStream`, which is gone once `EspIdfTlsConnector` adopted it.
fn live<S>(stream: Option<S>) -> S {
    stream.expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS")
}

impl embedded_io_async::ErrorType for EspIdfTcpStream {
    type Error = StdIoError;
}

/// Runs one plain-socket read or write through [`poll_until_ready`].
///
/// Without the polling loop the raw plaintext stream had no preempt point at all — a stuck peer
/// blocked the FreeRTOS task indefinitely with no `.await` yield point for an outer timeout to
/// preempt.
async fn retry_on_would_block_io(
    timer: &EspIdfTimer,
    op_name: &str,
    op: impl FnMut() -> std::io::Result<usize>,
) -> Result<usize, StdIoError> {
    poll_until_ready(timer, op, |e: &std::io::Error| {
        e.kind() == std::io::ErrorKind::WouldBlock
    })
    .await
    .map_err(|e| match e {
        PollError::Op(e) => {
            log::debug!("ESP-IDF TCP {op_name} failed: {e}");
            e.into()
        }
        PollError::Timer(e) => {
            let kind = match e {
                TimerError::ResourceExhausted => std::io::ErrorKind::OutOfMemory,
                TimerError::Other(_) => std::io::ErrorKind::Other,
            };
            std::io::Error::new(kind, e).into()
        }
    })
}

impl embedded_io_async::Read for EspIdfTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use std::io::Read;
        let Self { stream, timer, .. } = self;
        let stream = live(stream.as_mut());
        retry_on_would_block_io(timer, "read", || stream.read(buf)).await
    }
}

impl embedded_io_async::Write for EspIdfTcpStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        use std::io::Write as _;
        let Self { stream, timer, .. } = self;
        let stream = live(stream.as_mut());
        retry_on_would_block_io(timer, "write", || stream.write(buf)).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        use std::io::Write as _;
        self.inner_mut().flush().map_err(StdIoError::from)
    }
}

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
/// Built on `esp_idf_svc::tls::EspTls` via `EspTls::adopt()` (a spike when this backend was
/// written confirmed it needs no raw mbedTLS FFI to wrap an existing fd) instead of
/// `EspTls::new()` + `connect()`.
///
/// **No way to force TLS 1.2.** Unlike `io/tokio.rs`'s `TlsVersions::Tls12Only`, this connector
/// has no equivalent knob: `esp_idf_svc::tls::Config` (0.53.0, as vendored) exposes no min/max TLS
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
/// `model.quirks().requires_ftps_tls_1_2()` is true — the connection is safely rejected
/// rather than silently downgraded — but there is currently no way to make it succeed on
/// ESP-IDF for those models.
///
/// **Only `io/tokio.rs` (`tokio-rustls`) exposes a genuine max-protocol-version knob.**
/// `io/embassy.rs` shares this connector's inability to cap, for a different reason: its
/// backend is `mbedtls-rs` (not `embedded-tls`, which it replaced — see `Cargo.toml`'s
/// dependency comment) and `EmbassyTlsConnector::connect` sets only `min_version`, though
/// `mbedtls-rs` 0.3's `ClientSessionConfig::max_version` would allow it. Reporting is a
/// separate matter and both embedded backends can do it — embassy via
/// `Session::tls_version()`, this one via `query_negotiated_tls_version` — so
/// `require_tls_1_2_if_enforced` (`ftps/client.rs`) passes on both whenever the printer
/// negotiates 1.2 of its own accord, and `with_ftps_allow_unverified_tls_1_2(true)` is needed
/// only against a peer that insists on 1.3.
///
/// `Clone` shares one copy of the certificates, so `PrinterClient`'s MQTT, FTPS and camera
/// channels can take clones of one connector instead of each holding its own PEM bundle.
#[derive(Clone)]
pub struct EspIdfTlsConnector {
    certs: std::sync::Arc<EspIdfTlsCerts>,
    connect_timeout: core::time::Duration,
}

impl EspIdfTlsConnector {
    /// Creates a connector that skips server certificate verification.
    ///
    /// Reached via `build_unverified_tls_cfg`'s `crt_bundle_attach` hook
    /// (`accept_any_certificate`), which forces `MBEDTLS_SSL_VERIFY_NONE` directly on the
    /// mbedTLS config -- `esp_idf_svc::tls::Config` (0.53.0) has no field for ESP-IDF's own
    /// `skip_server_cert_verify` flag, and that flag only exists in `esp_tls_cfg` at all when
    /// the consuming app's sdkconfig sets `CONFIG_ESP_TLS_INSECURE` (off by default) -- a
    /// build-time condition bambino cannot see or require. See `accept_any_certificate`'s doc
    /// comment for the full mechanism and why this needed bypassing `esp_idf_svc::tls::Config`
    /// entirely (GitHub issue #168).
    ///
    /// **Requires `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE`** (on by ESP-IDF default) -- the Kconfig
    /// option that makes the `crt_bundle_attach` field exist on `esp_tls_cfg` in the first
    /// place. If a build has it disabled, `connect()` fails immediately with a `SocketError`
    /// explaining why, rather than reaching ESP-IDF's opaque
    /// `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED`.
    ///
    /// **Confirmed on a real ESP32-C6 against a live P1S** (`esp32-hw-probe`, GitHub issue
    /// #168): the unverified handshake completed over TLS 1.2 and returned the printer's chain.
    ///
    /// Prefer [`Self::with_certs`] wherever the caller can supply the
    /// printer's CA — it needs no sdkconfig change and actually verifies the peer.
    /// The handshake has no deadline of its own unless `.with_connect_timeout(d)` sets one;
    /// `PrinterClient::with_connect_timeout` bounds it from outside.
    pub fn unverified() -> Self {
        Self {
            certs: std::sync::Arc::new(EspIdfTlsCerts::new()),
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
    /// **Fails with [`Error::InvalidArgument`](crate::Error::InvalidArgument)** on an empty
    /// `ca_certs` or one where no anchor parses, rather than quietly falling back to an
    /// unverified connector: an anchor load that came back empty (missing file, wrong partition)
    /// must not turn verification off. Use [`Self::unverified`] to skip verification on purpose.
    /// A store where only some anchors parse is accepted and logged at error level.
    ///
    /// `ca_certs`: DER-encoded CA certificate bytes, one `Vec` per certificate.
    /// `client_auth`: Optional (cert, key), both DER-encoded, for mutual TLS.
    pub fn with_certs(
        ca_certs: impl IntoIterator<Item = Vec<u8>>,
        client_auth: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Result<Self, crate::Error> {
        Ok(Self {
            certs: std::sync::Arc::new(EspIdfTlsCerts::with_certs(ca_certs, client_auth)?),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        })
    }

    /// Sets a handshake deadline for direct use of this connector; disabled by default.
    ///
    /// Under `PrinterClient`, leave it unset: `PrinterClient::with_connect_timeout` already bounds
    /// the dial and handshake together, and a shorter inner deadline would silently cap it. The
    /// deadline bounds how long the poll loop keeps retrying rather than how long any single
    /// attempt may take.
    /// The deadline is checked *between* iterations, so it cannot preempt a stall *inside*
    /// one: the `EspTls::negotiate` FFI call is not interruptible from this task once entered.
    /// `connect` pins `Config::timeout_ms = 1` so each call advances the handshake for at most
    /// ~1ms, which keeps that window short and gives this deadline ~`TLS_POLL_INTERVAL`
    /// granularity (GitHub issues #67 and #294) — but a call that blocks internally is still
    /// unbounded regardless of
    /// what is passed here, and the calling task is then lost with nothing logged (observed
    /// once on ESP32-P4, GitHub issue #66). Consumers running printer I/O on a dedicated task
    /// should subscribe it to the ESP-IDF Task Watchdog, which is the only layer that can
    /// recover from that; no in-crate timeout can, and this one does not claim to.
    /// Passing `Duration::ZERO` disables the
    /// deadline entirely, matching `set_command_timeout`'s "0 disables" convention
    /// and `client::connect::with_connect_timeout`'s precedent — otherwise the very
    /// first would-block poll would immediately exceed a zero-length budget.
    /// Chain onto `unverified()`/`with_certs()`.
    #[must_use]
    pub fn with_connect_timeout(mut self, connect_timeout: core::time::Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self
    }
}

impl TlsConnector<EspIdfTcpStream> for EspIdfTlsConnector {
    type Stream = EspIdfTlsStream<EspIdfTcpStream>;

    /// Bounds the handshake loop by `self.connect_timeout`, tracked the same way `poll_until` does (`src/client/mod.rs`: capture `now_millis()` before the loop, compare `saturating_sub` against a budget each iteration).
    async fn connect(
        &self,
        host: &str,
        raw_stream: EspIdfTcpStream,
    ) -> Result<Self::Stream, SocketError> {
        // Fail before touching the socket at all when this connector has no trust anchor and
        // this build can't reach the one path that lets it skip verification either: see
        // `Self::unverified`'s doc comment for the `crt_bundle_attach` mechanism `connect` uses below,
        // and why `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is what makes that mechanism exist at all
        // (GitHub issue #168). `cfg!` (not `#[cfg]`) here: this is a runtime decision over a
        // compile-time constant, not a choice between two code paths that reference different
        // (possibly-absent) struct fields -- `build_unverified_tls_cfg` stays callable either
        // way, it just comes back without a working hook when the Kconfig is off.
        if self.certs.ca_pem.is_none() && !cfg!(esp_idf_mbedtls_certificate_bundle) {
            return Err(SocketError::Other(
                "EspIdfTlsConnector has no trust anchor and this build has \
                 CONFIG_MBEDTLS_CERTIFICATE_BUNDLE disabled, so there is no crt_bundle_attach \
                 seam left to skip server-certificate verification through (GitHub issue #168). \
                 Use with_certs() with a real CA, or enable CONFIG_MBEDTLS_CERTIFICATE_BUNDLE."
                    .into(),
            ));
        }

        // The adopted fd is already non-blocking: `EspIdfTcpStream::connect`, its only
        // constructor, sets it and it stays so for the stream's lifetime. That is what makes
        // mbedTLS's read/write calls inside `negotiate()` (and inside `EspIdfTlsStream`'s later
        // read/write) return `WANT_READ`/`WANT_WRITE` instead of blocking the FreeRTOS task.

        // Both configs carry the handshake pins (`pin_handshake_step!`). The raw one is only
        // used when this connector has no trust anchor -- see `build_unverified_tls_cfg` and
        // `Self::unverified`'s doc comment for why it bypasses `Config` entirely.
        let cfg = self.certs.build_config();
        let unverified_cfg = self
            .certs
            .ca_pem
            .is_none()
            .then(|| build_unverified_tls_cfg(&self.certs.client_cert, &self.certs.client_key));

        let timer = EspIdfTimer::new()
            .map_err(|e| esp_setup_error(&e, "failed to create ESP-IDF async timer for TLS"))?;

        let mut tls = ::esp_idf_svc::tls::EspTls::adopt(raw_stream)
            .map_err(|e| esp_setup_error(&e, "ESP-TLS adopt of raw socket failed"))?;

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
            let step = if let Some(rcfg) = unverified_cfg.as_ref() {
                negotiate_unverified_step(&mut tls, host, rcfg)
            } else {
                tls.negotiate(host, &cfg).map(|_| ())
            };
            negotiate_us += now_micros().saturating_sub(step_start);
            steps += 1;

            // `is_would_block` covers the v5.5.3/v5.5.4/v6.0.1 spelling of "budget expired,
            // handshake still in progress" (`conn_new_sync` returns 0, which surfaces as
            // `EWOULDBLOCK`). On v5.5.5 the same condition returns -1 and surfaces as the same
            // opaque `ESP_FAIL` a real failure does, so the error handle is consulted instead --
            // see `take_esp_tls_error` and `pin_handshake_step!`. Drained
            // unconditionally on every error, including the already-retryable ones, so the
            // record can never outlive the step that produced it.
            // The mbedTLS record is drained alongside it for the same reason, and kept: it is
            // the only place a non-retryable step's real cause survives (#302).
            let (retryable, esp_err, mbedtls_err) = match &step {
                Ok(_) => (false, None, None),
                Err(e) => {
                    let esp_err = take_esp_tls_error(
                        &tls,
                        ::esp_idf_svc::sys::esp_tls_error_type_t_ESP_TLS_ERR_TYPE_ESP,
                    );
                    // Stored negated by ESP-IDF; see `mbedtls_code_from_esp_tls_record`.
                    let mbedtls_err = take_esp_tls_error(
                        &tls,
                        ::esp_idf_svc::sys::esp_tls_error_type_t_ESP_TLS_ERR_TYPE_MBEDTLS,
                    )
                    .map(mbedtls_code_from_esp_tls_record);
                    let retryable = is_would_block(e)
                        || esp_err == Some(::esp_idf_svc::sys::ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT);
                    (retryable, esp_err, mbedtls_err)
                }
            };

            match step {
                Ok(_) => {
                    log::debug!(
                        "ESP-TLS handshake with {} completed in {}ms ({steps} steps, {negotiate_us}us in esp_tls, {sleep_us}us polling)",
                        RedactedHost(host),
                        timer.now_millis().saturating_sub(start)
                    );
                    break;
                }
                Err(_) if retryable => {
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
                    slept.map_err(|e| {
                        crate::io::timer_failure_error(
                            e,
                            "ESP-IDF timer failed while polling TLS handshake",
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
                        return Err(SocketError::CertificateInvalid(account_for_trust_store(
                            failure,
                            count_handshake_anchors(&tls),
                            self.certs.anchor_count,
                        )));
                    }
                    return Err(map_esp_tls_connect_error(&e, esp_err, mbedtls_err));
                }
            }
        }

        let read_cap = esp_tls_read_cap(
            cfg!(all(
                esp_idf_mbedtls_ssl_proto_tls1_3,
                esp_idf_esp_tls_client_session_tickets
            )),
            query_negotiated_tls_version(&tls),
        );
        Ok(EspIdfTlsStream {
            tls,
            timer,
            read_cap,
        })
    }

    // No `close()` override: `esp_idf_svc::tls::EspTls` exposes no shutdown method (only
    // `Drop`, which runs `esp_tls_conn_destroy`), so this backend keeps the trait's no-op
    // default rather than faking an orderly `close_notify` it cannot send. The
    // `context_handle()` raw pointer is not a way around this — destroying the context through
    // it would leave the safe wrapper holding a dangling handle to free again. Revisit if
    // esp-idf-svc grows a real close seam (GitHub issue #293).

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
pub struct EspIdfRawStreamFactory;

impl RawStreamFactory<EspIdfTcpStream> for EspIdfRawStreamFactory {
    async fn dial(&self, host: &str, port: u16) -> Result<EspIdfTcpStream, SocketError> {
        EspIdfTcpStream::connect(host, port).await
    }
}
