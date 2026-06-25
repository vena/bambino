//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{AsyncIo, AsyncUdpSocket, SocketError, TimerProvider, TlsConnector};

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
