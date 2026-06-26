//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{AsyncIo, AsyncUdpSocket, SecureConnect, SocketError, TimerProvider, TlsConnector};

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
    async fn sleep(&self, duration: core::time::Duration) {
        self.timer
            .borrow_mut()
            .after(duration)
            .await
            .expect("ESP-IDF hardware timer scheduling failed");
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
impl AsyncUdpSocket for EspIdfUdpSocket {
    async fn bind(addr: &str) -> Result<Self, SocketError> {
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

    async fn send_to(&self, buf: &[u8], target: &str) -> Result<usize, SocketError> {
        self.inner
            .send_to(buf, target)
            .map_err(|e| to_esp_socket_error(e))
    }

    async fn recv_from(
        &self,
        buf: &mut [u8],
    ) -> Result<(usize, alloc::string::String), SocketError> {
        match self.inner.recv_from(buf) {
            Ok((len, addr)) => Ok((len, addr.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(SocketError::TimedOut),
            Err(e) => Err(to_esp_socket_error(e)),
        }
    }
}

/// Helper mapping standard Rust IO errors to our ESP-IDF socket errors.
#[cfg(feature = "esp-idf")]
fn to_esp_socket_error(err: std::io::Error) -> SocketError {
    match err.kind() {
        std::io::ErrorKind::ConnectionRefused => SocketError::ConnectionRefused,
        std::io::ErrorKind::ConnectionAborted => SocketError::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset => SocketError::ConnectionReset,
        std::io::ErrorKind::NotConnected => SocketError::NotConnected,
        std::io::ErrorKind::TimedOut => SocketError::TimedOut,
        std::io::ErrorKind::AddrInUse => SocketError::AddressInUse,
        std::io::ErrorKind::AddrNotAvailable => SocketError::AddressNotAvailable,
        std::io::ErrorKind::InvalidInput => SocketError::InvalidInput,
        _ => SocketError::Other("ESP-IDF platform BSD network error"),
    }
}

/// Secure connector for ESP-IDF using the platform's native `EspTls` stack.
///
/// Unlike tokio/embassy where TLS wraps a caller-supplied TCP stream, ESP-IDF's
/// `EspTls` manages TCP connection establishment internally. This implements
/// `SecureConnect` directly — callers provide host+port and receive a ready stream.
///
/// The resulting stream wraps a raw POSIX file descriptor obtained from `EspTls`,
/// adapted to `embedded-io-async` via blocking-mode reads/writes on the FreeRTOS
/// task thread. Full async integration requires `esp-idf-svc` socket-async support
/// which is not yet stable.
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
}

/// Wrapper adapting an ESP-IDF TLS socket fd to `embedded-io-async` traits.
#[cfg(feature = "esp-idf")]
pub struct EspTlsStream {
    // Raw POSIX fd from EspTls — reads/writes use lwIP BSD socket calls.
    fd: i32,
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::ErrorType for EspTlsStream {
    type Error = embedded_io_async::ErrorKind;
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Read for EspTlsStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let ret = unsafe {
            ::esp_idf_svc::sys::read(self.fd, buf.as_mut_ptr() as *mut _, buf.len() as u32)
        };
        if ret < 0 {
            Err(embedded_io_async::ErrorKind::Other)
        } else {
            Ok(ret as usize)
        }
    }
}

#[cfg(feature = "esp-idf")]
impl embedded_io_async::Write for EspTlsStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let ret = unsafe {
            ::esp_idf_svc::sys::write(self.fd, buf.as_ptr() as *const _, buf.len() as u32)
        };
        if ret < 0 {
            Err(embedded_io_async::ErrorKind::Other)
        } else {
            Ok(ret as usize)
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
        let host_cstr = std::ffi::CString::new(host).map_err(|_| SocketError::InvalidInput)?;

        let mut cfg: ::esp_idf_svc::sys::esp_tls_cfg = unsafe { core::mem::zeroed() };

        if let Some(ca) = &self.ca_cert {
            cfg.cacert_buf = ca.as_ptr();
            cfg.cacert_bytes = ca.len() as u32;
        } else {
            cfg.skip_common_name = true;
        }

        if let (Some(cert), Some(key)) = (&self.client_cert, &self.client_key) {
            cfg.clientcert_buf = cert.as_ptr();
            cfg.clientcert_bytes = cert.len() as u32;
            cfg.clientkey_buf = key.as_ptr();
            cfg.clientkey_bytes = key.len() as u32;
        }

        let tls = unsafe { ::esp_idf_svc::sys::esp_tls_init() };
        if tls.is_null() {
            return Err(SocketError::Other("ESP-TLS initialization failed"));
        }

        let ret = unsafe {
            ::esp_idf_svc::sys::esp_tls_conn_new_sync(
                host_cstr.as_ptr(),
                host_cstr.as_bytes().len() as i32,
                port as i32,
                &cfg,
                tls,
            )
        };
        if ret != 0 {
            unsafe { ::esp_idf_svc::sys::esp_tls_conn_destroy(tls) };
            return Err(SocketError::ConnectionRefused);
        }

        let mut fd: i32 = -1;
        let get_ret =
            unsafe { ::esp_idf_svc::sys::esp_tls_get_conn_sockfd(tls, &mut fd as *mut i32) };
        if get_ret != 0 || fd < 0 {
            unsafe { ::esp_idf_svc::sys::esp_tls_conn_destroy(tls) };
            return Err(SocketError::Other(
                "Failed to extract socket fd from ESP-TLS",
            ));
        }

        Ok(EspTlsStream { fd })
    }
}
