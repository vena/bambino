//! # Simple Service Discovery Protocol (SSDP) Network Discovery
//!
//! Coordinates active printer searches (M-SEARCH queries) and passive NOTIFY
//! listener loops on Port 2021 utilizing the abstract `AsyncUdpSocket` layer.
//! Enables multi-platform discovery across std, ESP-IDF, and Embassy environments.

pub mod parser;

use crate::error::BambuError;
use crate::io::{AsyncUdpSocket, TimerProvider};
pub use parser::{parse_ssdp_payload, resolve_model, BambuModel, SsdpDevice};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Standard Bambu Lab multicast group target for SSDP operations.
pub const MULTICAST_IP: &str = "239.255.255.250";

/// Standard UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].
pub const SSDP_PORT: u16 = 2021;

/// The strict HTTP/1.1 search query string mandated by physical printer firmware parsers [REF-NET-DISC].
///
/// **Safety Constraint:** The payload must be strictly terminated with a double CRLF (`\r\n\r\n`)
/// sequence. Omitting this signature will cause embedded hardware lines to discard the query packet.
const M_SEARCH_QUERY: &[u8] = b"M-SEARCH * HTTP/1.1\r\n\
                                HOST: 239.255.255.250:2021\r\n\
                                MAN: \"ssdp:discover\"\r\n\
                                MX: 3\r\n\
                                ST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

/// Asynchronous Discovery Engine providing search orchestration and passive monitoring.
pub struct DiscoveryEngine<U: AsyncUdpSocket> {
    socket: U,
}

impl<U: AsyncUdpSocket> DiscoveryEngine<U> {
    /// Creates a new Discovery Engine using a pre-allocated UDP socket.
    pub fn new(socket: U) -> Self {
        Self { socket }
    }

    /// Dispatches a standard multicast active discovery query to trigger local printer reports.
    ///
    /// Transmits the request over UDP directly to the multicast cluster target `239.255.255.250:2021`.
    pub async fn broadcast_search(&self) -> Result<(), BambuError> {
        let target = format!("{}:{}", MULTICAST_IP, SSDP_PORT);
        self.socket
            .send_to(M_SEARCH_QUERY, &target)
            .await
            .map_err(BambuError::NetworkError)?;
        Ok(())
    }

    /// Listens on the bound socket interface and processes the next incoming SSDP packet.
    ///
    /// Returns `Ok(Some(SsdpDevice))` if a valid printer signature was parsed,
    /// `Ok(None)` if a timeout occurred or a non-printer packet was received,
    /// and `Err(BambuError)` for terminal network socket faults.
    pub async fn poll_next_device(&self, buf: &mut [u8]) -> Result<Option<SsdpDevice>, BambuError> {
        match self.socket.recv_from(buf).await {
            Ok((len, _from_addr)) => {
                let parsed = parse_ssdp_payload(&buf[..len]);
                Ok(parsed)
            }
            // Catch-all transient socket timeout. Returns None to allow retry loop cycles.
            Err(crate::io::SocketError::TimedOut) => Ok(None),
            Err(e) => Err(BambuError::NetworkError(e)),
        }
    }
}

/// Helper executing an active multi-second broadcast and scanning sweep for nearby printers.
///
/// Combines the SSDP search request and polling loop within a unified, allocation-friendly API.
/// Runs platform-agnostically by driving delay timings through the parameterized `TimerProvider`.
#[cfg(any(feature = "std", feature = "alloc"))]
pub async fn discover_devices<U, T>(
    timeout: core::time::Duration,
    timer: &T,
) -> Result<Vec<SsdpDevice>, BambuError>
where
    U: AsyncUdpSocket,
    T: TimerProvider,
{
    // Bind to the wildcard address on an ephemeral port to transmit and collect responses
    let socket = U::bind("0.0.0.0:0")
        .await
        .map_err(BambuError::NetworkError)?;
    let engine = DiscoveryEngine::new(socket);

    // Send search query multiple times to insulate against standard wireless packet drops
    for _ in 0..2 {
        engine.broadcast_search().await?;
        T::sleep(core::time::Duration::from_millis(50)).await;
    }

    let mut devices: Vec<SsdpDevice> = Vec::new();
    let mut buf = [0u8; 1500];

    // Compute the polling bounds. Loops incrementally sleeping 100ms per iteration.
    let total_millis = timeout.as_millis();
    let interval_millis = 100;
    let iterations = total_millis / interval_millis;

    for _ in 0..iterations {
        // Poll for any incoming packets with non-blocking properties
        if let Ok(Some(device)) = engine.poll_next_device(&mut buf).await {
            // Deduplicate devices based on unique serial number records
            if !devices.iter().any(|d| d.serial == device.serial) {
                devices.push(device);
            }
        }
        T::sleep(core::time::Duration::from_millis(interval_millis as u64)).await;
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{AsyncUdpSocket, SocketError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock socket to test DiscoveryEngine search broadcasts and response parsing.
    struct MockDiscoverySocket {
        sent_payloads: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
        recv_counter: AtomicUsize,
    }

    impl AsyncUdpSocket for MockDiscoverySocket {
        async fn bind(_addr: &str) -> Result<Self, SocketError> {
            Ok(Self {
                sent_payloads: Arc::new(std::sync::Mutex::new(Vec::new())),
                recv_counter: AtomicUsize::new(0),
            })
        }

        async fn send_to(&self, buf: &[u8], _target: &str) -> Result<usize, SocketError> {
            self.sent_payloads.lock().unwrap().push(buf.to_vec());
            Ok(buf.len())
        }

        async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, String), SocketError> {
            let count = self.recv_counter.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                let response = b"HTTP/1.1 200 OK\r\n\
                                 LOCATION: http://192.168.1.150:80/\r\n\
                                 USN: 01P06A521703222\r\n\
                                 DevModel.bambu.com: C12\r\n\r\n";
                let len = response.len();
                buf[..len].copy_from_slice(response);
                Ok((len, "192.168.1.150:2021".to_string()))
            } else {
                Err(SocketError::TimedOut)
            }
        }
    }

    #[tokio::test]
    async fn test_discovery_engine_broadcast_and_poll() {
        let socket = MockDiscoverySocket::bind("0.0.0.0:0").await.unwrap();
        let sent_ref = socket.sent_payloads.clone();
        let engine = DiscoveryEngine::new(socket);

        engine.broadcast_search().await.unwrap();
        {
            let payloads = sent_ref.lock().unwrap();
            assert_eq!(payloads.len(), 1);
            assert!(payloads[0].starts_with(b"M-SEARCH"));
            assert!(payloads[0].ends_with(b"\r\n\r\n"));
        }

        let mut buf = [0u8; 1500];
        let device = engine.poll_next_device(&mut buf).await.unwrap().unwrap();
        assert_eq!(device.serial, "01P06A521703222");
        assert_eq!(device.model, BambuModel::P1S);

        // Next poll should encounter Mock timeout returning None
        let empty_device = engine.poll_next_device(&mut buf).await.unwrap();
        assert!(empty_device.is_none());
    }
}
