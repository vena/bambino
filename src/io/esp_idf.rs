//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{
    AsyncUdpSocket, BindableUdpSocket, SecureConnect, SocketError, TimerError, TimerProvider,
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
        let mut cfg = ::esp_idf_svc::tls::Config::new();
        cfg.non_block = true;

        if let Some(ca) = &self.ca_cert {
            cfg.ca_cert = Some(::esp_idf_svc::tls::X509::der(ca));
        } else {
            cfg.skip_common_name = true;
        }

        if let (Some(cert), Some(key)) = (&self.client_cert, &self.client_key) {
            cfg.client_cert = Some(::esp_idf_svc::tls::X509::der(cert));
            cfg.client_key = Some(::esp_idf_svc::tls::X509::der(key));
        }

        cfg
    }
}

/// Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`.
///
/// `EspTls`'s own `read`/`write` are synchronous calls, but the underlying socket runs
/// in non-blocking mode (`Config::non_block = true`, set by `EspIdfSecureConnector`), so
/// each call returns immediately instead of blocking the FreeRTOS task. Retries happen
/// by yielding to the async executor via `EspIdfTimer::sleep` — see `TLS_POLL_INTERVAL`.
#[cfg(feature = "esp-idf")]
pub struct EspTlsStream {
    tls: ::esp_idf_svc::tls::EspTls<::esp_idf_svc::tls::InternalSocket>,
    timer: EspIdfTimer,
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::ErrorType for EspTlsStream {
    type Error = embedded_io_async::ErrorKind;
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Read for EspTlsStream {
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
                Err(_) => return Err(embedded_io_async::ErrorKind::Other),
            }
        }
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Write for EspTlsStream {
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
                Err(_) => return Err(embedded_io_async::ErrorKind::Other),
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
                Err(_) => return Err(SocketError::ConnectionRefused),
            }
        }

        Ok(EspTlsStream { tls, timer })
    }
}
