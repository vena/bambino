//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::ftps::FtpDataStreamFactory;
#[cfg(feature = "esp-idf")]
use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, SecureConnect, SocketError, TimerError, TimerProvider,
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

/// UDP Socket implementation designed for ESP-IDF's BSD Socket integration.
#[cfg(feature = "esp-idf")]
pub struct EspIdfUdpSocket {
    inner: std::net::UdpSocket,
}

#[cfg(feature = "esp-idf")]
impl BindableUdpSocket for EspIdfUdpSocket {
    async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let inner = std::net::UdpSocket::bind(addr).map_err(|e| to_esp_socket_error(e))?;

        let _ = inner.set_broadcast(true);

        let multiaddr = std::net::Ipv4Addr::new(239, 255, 255, 250);
        let interface = std::net::Ipv4Addr::new(0, 0, 0, 0);
        let _ = inner.join_multicast_v4(&multiaddr, &interface);

        inner
            .set_nonblocking(true)
            .map_err(|e| to_esp_socket_error(e))?;
        Ok(Self { inner })
    }
}

#[cfg(feature = "esp-idf")]
impl AsyncUdpSocket for EspIdfUdpSocket {
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError> {
        self.inner
            .send_to(buf, target)
            .map_err(|e| to_esp_socket_error(e))
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
        match self.inner.recv_from(buf) {
            Ok((len, addr)) => Ok((len, addr)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(SocketError::TimedOut),
            Err(e) => Err(to_esp_socket_error(e)),
        }
    }
}

/// Helper mapping standard Rust IO errors to our ESP-IDF socket errors.
#[cfg(feature = "esp-idf")]
fn to_esp_socket_error(err: std::io::Error) -> SocketError {
    crate::io::map_std_io_error(err, "ESP-IDF platform BSD network error")
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
/// future upgrade. This fixed-interval poll already fixes the actual problem this phase
/// targets: since `Config::non_block = true` makes every `EspTls` call return immediately
/// instead of blocking inside the FFI call, an outer `TimerProvider`-based timeout can
/// preempt the operation between poll attempts, which it could not do before.
#[cfg(feature = "esp-idf")]
const TLS_POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(20);

/// True if `err` indicates the non-blocking TLS operation would have blocked and should
/// be retried, rather than a real failure. `EWOULDBLOCK` is included alongside the two
/// `esp_tls`-specific codes because `EspTls::connect`/`read`/`write` can surface it
/// directly in non-blocking mode — documented upstream as "a peculiarity/bug of the
/// esp-tls C module".
#[cfg(feature = "esp-idf")]
fn is_would_block(err: &::esp_idf_svc::sys::EspError) -> bool {
    let code = err.code();
    code == ::esp_idf_svc::sys::EWOULDBLOCK as i32
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_READ
        || code == ::esp_idf_svc::sys::ESP_TLS_ERR_SSL_WANT_WRITE
}

/// Secure connector for ESP-IDF using the platform's native `EspTls` stack.
///
/// Unlike tokio/embassy where TLS wraps a caller-supplied TCP stream, ESP-IDF's
/// `EspTls` manages TCP connection establishment internally. This implements
/// `SecureConnect` directly — callers provide host+port and receive a ready stream.
///
/// Built on `esp_idf_svc::tls::{EspTls, Config}` — the safe wrapper `esp-idf-svc` ships
/// around `esp_tls` — rather than raw `esp-idf-sys` FFI, so the bindgen-union cert-field
/// wiring (`cfg.__bindgen_anon_N.field`) is delegated to code `esp-idf-svc` maintains
/// instead of reimplemented here. The handshake runs with `Config::non_block = true`,
/// so `EspTls::connect` never blocks inside the FFI call — see `TLS_POLL_INTERVAL` for
/// how the resulting `ESP_TLS_ERR_SSL_WANT_READ`/`_WRITE`/`EWOULDBLOCK` outcomes are
/// retried.
#[cfg(feature = "esp-idf")]
pub struct EspIdfSecureConnector {
    ca_cert: Option<Vec<u8>>,
    client_cert: Option<Vec<u8>>,
    client_key: Option<Vec<u8>>,
}

#[cfg(feature = "esp-idf")]
impl EspIdfSecureConnector {
    /// Creates a connector that skips server certificate verification.
    pub fn new() -> Self {
        Self {
            ca_cert: None,
            client_cert: None,
            client_key: None,
        }
    }

    /// Creates a connector that verifies the server certificate against a CA cert.
    ///
    /// `ca_cert_pem`: PEM or DER-encoded CA certificate bytes.
    /// `client_auth`: Optional (cert_pem, key_pem) for mutual TLS.
    pub fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self {
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

/// Builds an `esp_idf_svc::tls::Config` from cert bytes, shared by every ESP-IDF TLS
/// connection path (`EspIdfSecureConnector`'s dial-own-connection `SecureConnect` impl,
/// and `EspIdfTlsConnector`'s wrap-existing-stream `TlsConnector` impl below) so the
/// cert-field wiring is written once rather than duplicated per connector type.
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
/// in non-blocking mode (`Config::non_block = true`, set by both `EspIdfSecureConnector`
/// and `EspIdfTlsConnector`), so each call returns immediately instead of blocking the
/// FreeRTOS task. Retries happen by yielding to the async executor via
/// `EspIdfTimer::sleep` — see `TLS_POLL_INTERVAL`.
///
/// Generic over the adopted socket type `S`: `EspIdfSecureConnector` (dial-own-connection)
/// produces `EspTlsStream<InternalSocket>` (the default), while `EspIdfTlsConnector`
/// (wrap-an-existing-stream, below) produces `EspTlsStream<EspIdfTcpStream>`.
#[cfg(feature = "esp-idf")]
pub struct EspTlsStream<S = ::esp_idf_svc::tls::InternalSocket>
where
    S: ::esp_idf_svc::tls::Socket,
{
    tls: ::esp_idf_svc::tls::EspTls<S>,
    timer: EspIdfTimer,
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::ErrorType for EspTlsStream<S> {
    type Error = embedded_io_async::ErrorKind;
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::Read for EspTlsStream<S> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        loop {
            match self.tls.read(buf) {
                Ok(n) => return Ok(n),
                Err(e) if is_would_block(&e) => {
                    self.timer
                        .sleep(TLS_POLL_INTERVAL)
                        .await
                        .map_err(|_| embedded_io_async::ErrorKind::Other)?;
                }
                Err(e) => {
                    log::debug!("ESP-IDF TLS read failed: {e}");
                    return Err(embedded_io_async::ErrorKind::Other);
                }
            }
        }
    }
}

#[cfg(feature = "esp-idf")]
impl<S: ::esp_idf_svc::tls::Socket> embedded_io_async::Write for EspTlsStream<S> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        loop {
            match self.tls.write(buf) {
                Ok(n) => return Ok(n),
                Err(e) if is_would_block(&e) => {
                    self.timer
                        .sleep(TLS_POLL_INTERVAL)
                        .await
                        .map_err(|_| embedded_io_async::ErrorKind::Other)?;
                }
                Err(e) => {
                    log::debug!("ESP-IDF TLS write failed: {e}");
                    return Err(embedded_io_async::ErrorKind::Other);
                }
            }
        }
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(feature = "esp-idf")]
impl SecureConnect for EspIdfSecureConnector {
    type Stream = EspTlsStream;

    async fn secure_connect(&self, host: &str, port: u16) -> Result<Self::Stream, SocketError> {
        let cfg = self.build_config();

        let timer = EspIdfTimer::new()
            .map_err(|_| SocketError::Other("failed to create ESP-IDF async timer for TLS"))?;

        let mut tls = ::esp_idf_svc::tls::EspTls::new()
            .map_err(|_| SocketError::Other("ESP-TLS initialization failed"))?;

        loop {
            match tls.connect(host, port, &cfg) {
                Ok(_) => break,
                Err(e) if is_would_block(&e) => {
                    timer.sleep(TLS_POLL_INTERVAL).await.map_err(|_| {
                        SocketError::Other("ESP-IDF timer failed while polling TLS handshake")
                    })?;
                }
                Err(e) => {
                    log::debug!("ESP-IDF TLS handshake failed: {e}");
                    return Err(SocketError::ConnectionRefused);
                }
            }
        }

        Ok(EspTlsStream { tls, timer })
    }

    /// Reads the negotiated TLS version via `esp_tls_get_ssl_context()` +
    /// mbedTLS's `mbedtls_ssl_get_version()` — both confirmed present in
    /// `esp-idf-svc` 0.52.1's bindgen output (`esp_tls_get_ssl_context` is a public,
    /// stable ESP-TLS API already used the same way by `esp-idf-svc` itself, in
    /// `EspTls`'s internal ALPN accessor — this isn't new unsafe surface, it's the
    /// same accessor pattern applied to a different mbedTLS query).
    ///
    /// **Assumes the default mbedTLS backend** (`CONFIG_ESP_TLS_USING_MBEDTLS=y`,
    /// ESP-IDF's default — the rest of this file makes the same assumption
    /// implicitly, e.g. `build_config`'s use of `X509::der`). A wolfSSL-configured
    /// build (`CONFIG_ESP_TLS_USING_WOLFSSL=y`) would need a different accessor and
    /// a compile-time way to detect which backend is active; this crate has no
    /// `build.rs` forwarding `esp_idf_esp_tls_using_wolfssl`-style cfgs the way
    /// `esp-idf-svc`'s own build script does for itself, so that detection isn't
    /// possible here today — out of scope until one is added.
    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        query_negotiated_tls_version(&stream.tls)
    }
}

/// Shared mbedTLS version query, used by both `EspIdfSecureConnector::negotiated_version`
/// (above) and `EspIdfTlsConnector::negotiated_version` (below) — the accessor chain from
/// `*mut esp_tls` down to a parsed `TlsVersion` doesn't depend on which `Socket` impl the
/// `EspTls` was constructed with (`InternalSocket` for dial-own-connection,
/// `EspIdfTcpStream` for wrap-existing-stream), so it's written once here.
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

/// Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring
/// `TokioIoError` (`io/mod.rs`) — needed because `embedded-io-async` has no blanket impl
/// for `std::io::Error` itself, only for types that opt in explicitly.
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

/// Raw (unencrypted) blocking TCP stream, used both as the seed for
/// `EspIdfTlsConnector::connect`'s `EspTls::adopt()` call and directly as `RawIO` for
/// models whose `model.quirks().uses_plaintext_ftps_data_channel()` is true (the FTPS
/// data channel is then never TLS-wrapped, so its `embedded_io_async::Read`/`Write`
/// impls below are exercised for real, not just to satisfy the `AsyncIo` trait bound).
///
/// Blocking is deliberate and matches `EspIdfUdpSocket`'s approach of using
/// `std::net::*` directly rather than inventing async socket polling for every raw
/// transport — `esp-idf-svc`/`esp-idf-hal` expose no async readiness primitive for an
/// arbitrary fd (see `TLS_POLL_INTERVAL`'s doc comment), so a genuine non-blocking wait
/// isn't available here either way. The socket stays in ESP-IDF's default blocking mode
/// unless `EspIdfTlsConnector::connect` flips it to non-blocking right before handing it
/// to `EspTls::adopt()` (see that function) — plaintext callers never trigger that path,
/// so their reads/writes block the calling task/thread until data is available, same as
/// any other blocking `std::net::TcpStream` use.
///
/// Wraps `Option<TcpStream>` rather than `TcpStream` directly so `Socket::release()` can
/// `.take()` the stream and hand its fd to `IntoRawFd::into_raw_fd()` — `esp_tls_conn_destroy`
/// closes an adopted fd itself once `release()` returns, so the Rust-side `TcpStream` must
/// give up ownership of the fd first or the fd would be double-closed.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTcpStream(Option<std::net::TcpStream>);

#[cfg(feature = "esp-idf")]
impl EspIdfTcpStream {
    /// Dials a raw TCP connection to `host:port`. Stays in ESP-IDF's default blocking
    /// socket mode — see the type's doc comment for why.
    pub fn connect(host: &str, port: u16) -> Result<Self, SocketError> {
        let stream = std::net::TcpStream::connect((host, port)).map_err(to_esp_socket_error)?;
        Ok(Self(Some(stream)))
    }

    fn inner(&self) -> &std::net::TcpStream {
        self.0
            .as_ref()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS")
    }

    fn inner_mut(&mut self) -> &mut std::net::TcpStream {
        self.0
            .as_mut()
            .expect("EspIdfTcpStream used after socket ownership was released to ESP-TLS")
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::ErrorType for EspIdfTcpStream {
    type Error = EspIdfIoError;
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Read for EspIdfTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use std::io::Read;
        self.inner_mut().read(buf).map_err(EspIdfIoError)
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Write for EspIdfTcpStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        use std::io::Write as _;
        self.inner_mut().write(buf).map_err(EspIdfIoError)
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
        if let Some(stream) = self.0.take() {
            // esp_tls_conn_destroy() closes the adopted fd itself; abandon the Rust-side
            // owner without running its Drop (which would close the same fd again).
            let _ = stream.into_raw_fd();
        }
        Ok(())
    }
}

/// TLS connector for ESP-IDF that wraps an already-connected raw stream, for platforms
/// (FTPS's data and control channels) that need `TlsConnector` rather than
/// `SecureConnect`'s dial-your-own-connection model. Built on the same
/// `esp_idf_svc::tls::EspTls` safe wrapper as `EspIdfSecureConnector`, via
/// `EspTls::adopt()` (confirmed by Phase 3's spike: no raw mbedTLS FFI needed to wrap an
/// existing fd) instead of `EspTls::new()` + `connect()`.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTlsConnector {
    ca_cert: Option<Vec<u8>>,
    client_cert: Option<Vec<u8>>,
    client_key: Option<Vec<u8>>,
}

#[cfg(feature = "esp-idf")]
impl EspIdfTlsConnector {
    /// Creates a connector that skips server certificate verification.
    pub fn new() -> Self {
        Self {
            ca_cert: None,
            client_cert: None,
            client_key: None,
        }
    }

    /// Creates a connector that verifies the server certificate against a CA cert.
    ///
    /// `ca_cert_pem`: PEM or DER-encoded CA certificate bytes.
    /// `client_auth`: Optional (cert_pem, key_pem) for mutual TLS.
    pub fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self {
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
}

#[cfg(feature = "esp-idf")]
impl Default for EspIdfTlsConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "esp-idf")]
impl TlsConnector<EspIdfTcpStream> for EspIdfTlsConnector {
    type Stream = EspTlsStream<EspIdfTcpStream>;

    async fn connect(
        &self,
        host: &str,
        _port: u16,
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

        let cfg = build_tls_config(&self.ca_cert, &self.client_cert, &self.client_key);

        let timer = EspIdfTimer::new()
            .map_err(|_| SocketError::Other("failed to create ESP-IDF async timer for TLS"))?;

        let mut tls = ::esp_idf_svc::tls::EspTls::adopt(raw_stream)
            .map_err(|_| SocketError::Other("ESP-TLS adopt of raw socket failed"))?;

        loop {
            match tls.negotiate(host, &cfg) {
                Ok(_) => break,
                Err(e) if is_would_block(&e) => {
                    timer.sleep(TLS_POLL_INTERVAL).await.map_err(|_| {
                        SocketError::Other("ESP-IDF timer failed while polling TLS handshake")
                    })?;
                }
                Err(e) => {
                    log::debug!("ESP-IDF TLS handshake failed: {e}");
                    return Err(SocketError::ConnectionRefused);
                }
            }
        }

        Ok(EspTlsStream { tls, timer })
    }

    fn negotiated_version(&self, stream: &Self::Stream) -> Option<TlsVersion> {
        query_negotiated_tls_version(&stream.tls)
    }
}

/// Passive/data-channel connection factory for ESP-IDF FTPS, using raw
/// `std::net::TcpStream` — the ESP-IDF counterpart to `TokioFtpDataStreamFactory`
/// (`io/tokio.rs`). Whether the returned stream ends up TLS-wrapped (via
/// `EspIdfTlsConnector`) or used directly (plaintext data-channel models) is decided by
/// `BambuFtpsClient`, not this factory.
#[cfg(feature = "esp-idf")]
pub struct EspIdfFtpDataStreamFactory;

#[cfg(feature = "esp-idf")]
impl FtpDataStreamFactory<EspIdfTcpStream> for EspIdfFtpDataStreamFactory {
    async fn create_data_stream(
        &self,
        host: &str,
        port: u16,
    ) -> Result<EspIdfTcpStream, SocketError> {
        EspIdfTcpStream::connect(host, port)
    }
}
