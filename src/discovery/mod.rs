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
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Standard Bambu Lab multicast group target for SSDP operations.
pub const MULTICAST_IP: &str = "239.255.255.250";

/// Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].
pub const SSDP_PORT: u16 = 2021;

/// Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS].
pub const SSDP_PORT_ALT: u16 = 1990;

/// Interval between periodic M-SEARCH re-broadcasts during discovery sweeps (milliseconds).
pub(crate) const SSDP_REBROADCAST_INTERVAL_MS: u128 = 3000;

const M_SEARCH_QUERY_2021: &[u8] = b"M-SEARCH * HTTP/1.1\r\n\
                                     HOST: 239.255.255.250:2021\r\n\
                                     MAN: \"ssdp:discover\"\r\n\
                                     MX: 3\r\n\
                                     ST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

const M_SEARCH_QUERY_1990: &[u8] = b"M-SEARCH * HTTP/1.1\r\n\
                                     HOST: 239.255.255.250:1990\r\n\
                                     MAN: \"ssdp:discover\"\r\n\
                                     MX: 3\r\n\
                                     ST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

/// Detects the active physical interface IP used to route external traffic.
///
/// **Why this is critical on macOS (and uses local multicast connect):**
/// macOS routing tables prioritize loopback (`lo0`) and virtual bridge interfaces (such as
/// Docker, VPN, or WSL networks) for wildcard (`0.0.0.0`) multicast requests. Connecting a dummy
/// socket to the standard multicast address forces the host OS to select and return the active
/// physical adapter IP without transmitting a single byte on the wire.
#[cfg(feature = "std")]
fn get_local_routing_ip() -> Option<std::net::IpAddr> {
    let dummy = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    dummy.connect("239.255.255.250:2021").ok()?;
    Some(dummy.local_addr().ok()?.ip())
}

/// Asynchronous Discovery Engine providing search orchestration and passive monitoring.
pub struct DiscoveryEngine<U: AsyncUdpSocket> {
    socket: U,
    port: u16,
}

impl<U: AsyncUdpSocket> DiscoveryEngine<U> {
    /// Creates a new Discovery Engine bound to a specific SSDP port.
    pub fn new(socket: U, port: u16) -> Self {
        Self { socket, port }
    }

    fn m_search_query(&self) -> &'static [u8] {
        if self.port == SSDP_PORT_ALT {
            M_SEARCH_QUERY_1990
        } else {
            M_SEARCH_QUERY_2021
        }
    }

    /// Dispatches active search queries to trigger local printer reports.
    ///
    /// **Dual Multicast/Broadcast Fallback:**
    /// Sends the query to both standard SSDP multicast (`239.255.255.250`) and global subnet
    /// broadcast (`255.255.255.255`). This ensures that even if local routers filter IGMP multicast,
    /// or if OS network interface routing routes multicast to inactive adapters, the query is
    /// successfully broadcast across all active local interfaces.
    pub async fn broadcast_search(&self) -> Result<(), BambuError> {
        let query = self.m_search_query();

        let multicast_target = format!("{}:{}", MULTICAST_IP, self.port);
        log::debug!("Transmitting multicast M-SEARCH request to: {}", multicast_target);
        let _ = self.socket.send_to(query, &multicast_target).await;

        let broadcast_target = format!("255.255.255.255:{}", self.port);
        log::debug!("Transmitting fallback broadcast M-SEARCH request to: {}", broadcast_target);
        let _ = self.socket.send_to(query, &broadcast_target).await;

        Ok(())
    }

    /// Listens on the bound socket interface and processes the next incoming SSDP packet.
    ///
    /// Returns `Ok(Some(SsdpDevice))` if a valid printer signature was parsed,
    /// `Ok(None)` if a timeout occurred or a non-printer packet was received,
    /// and `Err(BambuError)` for terminal network socket faults.
    pub async fn poll_next_device(&self, buf: &mut [u8]) -> Result<Option<SsdpDevice>, BambuError> {
        match self.socket.recv_from(buf).await {
            Ok((len, from_addr)) => {
                log::trace!("UDP socket received datagram of size {} from: {}", len, from_addr);

                let parsed = parse_ssdp_payload(&buf[..len]);
                match &parsed {
                    Some(device) => {
                        log::debug!(
                            "Parsed Bambu Lab printer record: serial='{}', model={:?}, ip={}, name='{}', version='{}'",
                            device.serial, device.model, device.ip, device.name, device.version
                        );
                    }
                    None => {
                        log::trace!("Datagram discarded (not a Bambu printer format)");
                    }
                }
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
///
/// **Why `_timer` is prefixed with an underscore:** Sleep timings are invoked directly via the
/// associated type function `T::sleep` to preserve clean generic signatures. The variable is retained
/// as `_timer` to document the provider dependency while cleanly bypassing compiler warnings.
#[cfg(any(feature = "std", feature = "alloc"))]
pub async fn discover_devices<U, T>(
    timeout: core::time::Duration,
    _timer: &T,
) -> Result<Vec<SsdpDevice>, BambuError>
where
    U: AsyncUdpSocket,
    T: TimerProvider,
{
    // Bind sockets on both SSDP ports. Using the specific port is required because
    // printers send NOTIFY advertisements to 239.255.255.250:<port>, and the OS only
    // delivers multicast packets when the socket's bound port matches the destination port.
    let ports: &[u16] = &[SSDP_PORT, SSDP_PORT_ALT];

    #[cfg(feature = "std")]
    if let Some(local_ip) = get_local_routing_ip() {
        log::debug!("Detected active routing interface IP: {}", local_ip);
    }

    let mut engines: Vec<(DiscoveryEngine<U>, u16)> = Vec::new();
    for &port in ports {
        let bind_addr = format!("0.0.0.0:{}", port);
        log::debug!("Binding UDP socket on '{}'", bind_addr);
        match U::bind(&bind_addr).await {
            Ok(socket) => engines.push((DiscoveryEngine::new(socket, port), port)),
            Err(e) => {
                log::debug!("Failed to bind port {}: {:?} (skipping)", port, e);
                if engines.is_empty() {
                    return Err(BambuError::NetworkError(e));
                }
            }
        }
    }

    for i in 0..2 {
        log::debug!("Initializing active query scan block #{}", i + 1);
        for (engine, _) in &engines {
            engine.broadcast_search().await?;
        }
        T::sleep(core::time::Duration::from_millis(50)).await;
    }

    let mut devices: Vec<SsdpDevice> = Vec::new();
    let mut buf = [0u8; 1500];

    let total_millis = timeout.as_millis();
    let mut elapsed_millis: u128 = 0;
    let mut millis_since_last_search: u128 = 0;

    log::debug!("Commencing poll listener sequence ({}ms limit, {} port(s))", total_millis, engines.len());

    // Alternate polling across all bound sockets. Each recv_from times out after ~100ms,
    // so we cycle through engines round-robin.
    while elapsed_millis < total_millis {
        if millis_since_last_search >= SSDP_REBROADCAST_INTERVAL_MS {
            log::trace!("Re-broadcasting periodic M-SEARCH queries");
            for (engine, _) in &engines {
                let _ = engine.broadcast_search().await;
            }
            millis_since_last_search = 0;
        }

        for (engine, port) in &engines {
            match engine.poll_next_device(&mut buf).await {
                Ok(Some(mut device)) => {
                    device.discovery_port = *port;
                    if !devices.iter().any(|d| d.serial == device.serial) {
                        log::debug!("Discovered '{}' via port {}", device.serial, port);
                        devices.push(device);
                    }
                }
                Ok(None) => {
                    elapsed_millis += 100;
                    millis_since_last_search += 100;
                }
                Err(_) => {
                    elapsed_millis += 100;
                    millis_since_last_search += 100;
                }
            }
        }
    }

    log::debug!("Completed discovery sweep. Total found unique machines: {}", devices.len());

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
        let engine = DiscoveryEngine::new(socket, SSDP_PORT);

        engine.broadcast_search().await.unwrap();
        {
            let payloads = sent_ref.lock().unwrap();
            // Expected 2 broadcasts (Multicast + Global Broadcast fallback)
            assert_eq!(payloads.len(), 2);
            assert!(payloads[0].starts_with(b"M-SEARCH"));
            assert!(payloads[0].ends_with(b"\r\n\r\n"));
            assert!(payloads[1].starts_with(b"M-SEARCH"));
            assert!(payloads[1].ends_with(b"\r\n\r\n"));
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
