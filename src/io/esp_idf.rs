//! # ESP-IDF (ESP32 standard library) Platform Support
//!
//! Bridges native ESP-IDF services and standard BSD socket structures to
//! our transport-agnostic client traits under Espressif's Rust standard library.

#[cfg(feature = "esp-idf")]
use crate::io::{AsyncIo, AsyncUdpSocket, SocketError, TimerProvider, TlsConnector};

/// Timer implementation designed for the ESP-IDF RTOS tick count scheduler.
///
/// Under ESP-IDF, standard system sleeps map directly to the FreeRTOS `vTaskDelay` scheduler loop.
#[cfg(feature = "esp-idf")]
pub struct EspIdfTimer;

#[cfg(feature = "esp-idf")]
impl TimerProvider for EspIdfTimer {
    async fn sleep(duration: core::time::Duration) {
        // Under ESP-IDF, thread sleep pauses the execution of the active FreeRTOS task.
        // For asynchronous tasks, standard executor delay hooks should be preferred in real applications.
        std::thread::sleep(duration);
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
        // Set socket to non-blocking to comply with async scheduling loops
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
        // Under ESP-IDF standard non-blocking mode, if no packet is queued, the socket returns WouldBlock.
        // Since standard AsyncUdpSocket requires async waiting, true implementations should poll or yield.
        match self.inner.recv_from(buf) {
            Ok((len, addr)) => Ok((len, addr.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Return non-fatal temporary error to allow runtime polling integration
                Err(SocketError::TimedOut)
            }
            Err(e) => Err(to_esp_socket_error(e)),
        }
    }
}

/// Helper mapping standard standard Rust IO errors to our ESP-IDF socket errors.
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
