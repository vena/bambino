//! # Printer Discovery (SSDP)
//!
//! Find Bambu Lab printers on the local network using SSDP (Simple Service Discovery Protocol).
//!
//! [`DiscoveryEngine`] sends M-SEARCH queries on UDP port 2021 (and the alternate port 1990)
//! and parses incoming NOTIFY/response packets into [`SsdpDevice`] records.
//! The [`discover_devices()`] convenience function runs a timed broadcast-and-listen sweep
//! and returns all unique printers found. Works across std, ESP-IDF, and Embassy via the
//! [`AsyncUdpSocket`] trait.

pub mod parser;

use crate::error::BambuError;
use crate::io::AsyncUdpSocket;
#[cfg(feature = "std")]
use crate::io::{BindableUdpSocket, TimerProvider};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
pub use parser::{SsdpDevice, parse_ssdp_payload};

#[cfg(feature = "std")]
use std::collections::BTreeSet;

/// Standard Bambu Lab multicast group target for SSDP operations.
pub const MULTICAST_IP: &str = "239.255.255.250";

/// Typed form of [`MULTICAST_IP`], used for socket operations.
const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);

/// IPv4 limited broadcast address, used as a fallback when multicast is filtered locally.
const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);

/// Unspecified bind address ("all interfaces"), used when binding the discovery sockets.
#[cfg(feature = "std")]
const UNSPECIFIED_ADDR: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

/// Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].
pub const SSDP_PORT: u16 = 2021;

/// Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS].
pub const SSDP_PORT_ALT: u16 = 1990;

/// Interval between periodic M-SEARCH re-broadcasts during discovery sweeps (milliseconds).
#[cfg(feature = "std")]
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

        let multicast_target = SocketAddr::from((IpAddr::V4(MULTICAST_ADDR), self.port));
        log::debug!(
            "Transmitting multicast M-SEARCH request to: {}",
            multicast_target
        );
        let mcast_result = self.socket.send_to(query, multicast_target).await;

        let broadcast_target = SocketAddr::from((IpAddr::V4(BROADCAST_ADDR), self.port));
        log::debug!(
            "Transmitting fallback broadcast M-SEARCH request to: {}",
            broadcast_target
        );
        let bcast_result = self.socket.send_to(query, broadcast_target).await;

        match (mcast_result, bcast_result) {
            (Err(_), Err(e)) => Err(BambuError::NetworkError(e)),
            _ => Ok(()),
        }
    }

    /// Listens on the bound socket interface and processes the next incoming SSDP packet.
    ///
    /// Returns `Ok(Some(SsdpDevice))` if a valid printer signature was parsed,
    /// `Ok(None)` if a timeout occurred or a non-printer packet was received,
    /// and `Err(BambuError)` for terminal network socket faults.
    pub async fn poll_next_device(&self, buf: &mut [u8]) -> Result<Option<SsdpDevice>, BambuError> {
        match self.socket.recv_from(buf).await {
            Ok((len, from_addr)) => {
                log::trace!(
                    "UDP socket received datagram of size {} from: {}",
                    len,
                    from_addr
                );

                let parsed = parse_ssdp_payload(&buf[..len]);
                match &parsed {
                    Some(device) => {
                        log::debug!(
                            "Parsed Bambu Lab printer record: serial='{}', model={:?}, ip={}, name='{}', version='{}'",
                            device.serial,
                            device.model,
                            device.ip,
                            device.name,
                            device.version
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

/// Broadcasts SSDP search queries and listens for printer responses for the given duration.
///
/// Returns a deduplicated list of all printers found. The timer parameter drives sleep
/// timing, making this work across std, ESP-IDF, and Embassy.
///
/// # Example
///
/// ```ignore
/// use bambino::discovery::discover_devices;
/// use bambino::io::tokio::{TokioUdpSocket, TokioTimer};
///
/// let timer = TokioTimer::new();
/// let printers = discover_devices::<TokioUdpSocket, _>(
///     std::time::Duration::from_secs(5),
///     &timer,
/// ).await?;
///
/// for printer in &printers {
///     println!("{} ({:?}) at {}", printer.name, printer.model, printer.ip);
/// }
/// ```
#[cfg(feature = "std")]
pub async fn discover_devices<U, T>(
    timeout: core::time::Duration,
    timer: &T,
) -> Result<Vec<SsdpDevice>, BambuError>
where
    U: BindableUdpSocket,
    T: TimerProvider,
{
    // Bind sockets on both SSDP ports. Using the specific port is required because
    // printers send NOTIFY advertisements to 239.255.255.250:<port>, and the OS only
    // delivers multicast packets when the socket's bound port matches the destination port.
    let ports: &[u16] = &[SSDP_PORT, SSDP_PORT_ALT];

    let mut engines: Vec<(DiscoveryEngine<U>, u16)> = Vec::new();
    for &port in ports {
        let bind_addr = SocketAddr::from((IpAddr::V4(UNSPECIFIED_ADDR), port));
        log::debug!("Binding UDP socket on '{}'", bind_addr);
        match U::bind(bind_addr).await {
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
        timer.sleep(core::time::Duration::from_millis(50)).await?;
    }

    let mut devices: Vec<SsdpDevice> = Vec::new();
    let mut seen_serials: BTreeSet<String> = BTreeSet::new();
    let mut buf = [0u8; 1500];

    let total_millis = timeout.as_millis() as u64;
    let start = timer.now_millis();
    let mut last_search = start;

    log::debug!(
        "Commencing poll listener sequence ({}ms limit, {} port(s))",
        total_millis,
        engines.len()
    );

    while timer.now_millis().saturating_sub(start) < total_millis {
        let now = timer.now_millis();
        if now.saturating_sub(last_search) >= SSDP_REBROADCAST_INTERVAL_MS as u64 {
            log::trace!("Re-broadcasting periodic M-SEARCH queries");
            for (engine, _) in &engines {
                let _ = engine.broadcast_search().await;
            }
            last_search = timer.now_millis();
        }

        for (engine, port) in &engines {
            if let Ok(Some(mut device)) = engine.poll_next_device(&mut buf).await {
                device.discovery_port = *port;
                if seen_serials.insert(device.serial.clone()) {
                    log::debug!("Discovered '{}' via port {}", device.serial, port);
                    devices.push(device);
                }
            }
        }
    }

    log::debug!(
        "Completed discovery sweep. Total found unique machines: {}",
        devices.len()
    );

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{AsyncUdpSocket, BindableUdpSocket, SocketError};
    use crate::models::BambuModel;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock socket to test DiscoveryEngine search broadcasts and response parsing.
    struct MockDiscoverySocket {
        sent_payloads: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
        recv_counter: AtomicUsize,
    }

    impl BindableUdpSocket for MockDiscoverySocket {
        async fn bind(_addr: SocketAddr) -> Result<Self, SocketError> {
            Ok(Self {
                sent_payloads: Arc::new(std::sync::Mutex::new(Vec::new())),
                recv_counter: AtomicUsize::new(0),
            })
        }
    }

    impl AsyncUdpSocket for MockDiscoverySocket {
        async fn send_to(&self, buf: &[u8], _target: SocketAddr) -> Result<usize, SocketError> {
            self.sent_payloads.lock().unwrap().push(buf.to_vec());
            Ok(buf.len())
        }

        async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
            let count = self.recv_counter.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                let response = b"HTTP/1.1 200 OK\r\n\
                                 LOCATION: http://192.168.1.150:80/\r\n\
                                 USN: 01P06A521703222\r\n\
                                 DevModel.bambu.com: C12\r\n\r\n";
                let len = response.len();
                buf[..len].copy_from_slice(response);
                Ok((len, SocketAddr::from((IpAddr::V4(Ipv4Addr::new(192, 168, 1, 150)), 2021))))
            } else {
                Err(SocketError::TimedOut)
            }
        }
    }

    #[tokio::test]
    async fn test_discovery_engine_broadcast_and_poll() {
        let socket = MockDiscoverySocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))).await.unwrap();
        let sent_ref = socket.sent_payloads.clone();
        let engine = DiscoveryEngine::new(socket, SSDP_PORT);

        engine.broadcast_search().await.unwrap();
        {
            let payloads = sent_ref.lock().unwrap();
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

        let empty_device = engine.poll_next_device(&mut buf).await.unwrap();
        assert!(empty_device.is_none());
    }

    struct FailSocket;

    impl BindableUdpSocket for FailSocket {
        async fn bind(_addr: SocketAddr) -> Result<Self, SocketError> {
            Ok(Self)
        }
    }

    impl AsyncUdpSocket for FailSocket {
        async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize, SocketError> {
            Err(SocketError::ConnectionRefused)
        }
        async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
            Err(SocketError::TimedOut)
        }
    }

    #[tokio::test]
    async fn test_broadcast_search_returns_error_when_both_sends_fail() {
        let socket = FailSocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))).await.unwrap();
        let engine = DiscoveryEngine::new(socket, SSDP_PORT);

        let result = engine.broadcast_search().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_broadcast_search_succeeds_when_one_send_works() {
        use std::sync::atomic::AtomicBool;

        struct HalfFailSocket {
            first_call: AtomicBool,
        }

        impl BindableUdpSocket for HalfFailSocket {
            async fn bind(_addr: SocketAddr) -> Result<Self, SocketError> {
                Ok(Self {
                    first_call: AtomicBool::new(true),
                })
            }
        }

        impl AsyncUdpSocket for HalfFailSocket {
            async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize, SocketError> {
                if self.first_call.swap(false, Ordering::SeqCst) {
                    Err(SocketError::ConnectionRefused)
                } else {
                    Ok(100)
                }
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        let socket = HalfFailSocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))).await.unwrap();
        let engine = DiscoveryEngine::new(socket, SSDP_PORT);

        let result = engine.broadcast_search().await;
        assert!(result.is_ok());
    }

    struct MockTimer {
        clock: std::sync::Mutex<u64>,
    }

    impl MockTimer {
        fn new() -> Self {
            Self {
                clock: std::sync::Mutex::new(0),
            }
        }

        fn advance(&self, ms: u64) {
            *self.clock.lock().unwrap() += ms;
        }
    }

    impl TimerProvider for MockTimer {
        async fn sleep(
            &self,
            _duration: core::time::Duration,
        ) -> Result<(), crate::io::TimerError> {
            Ok(())
        }

        fn now_millis(&self) -> u64 {
            *self.clock.lock().unwrap()
        }
    }

    #[tokio::test]
    async fn test_tokio_timer_now_millis_advances() {
        use crate::io::tokio::TokioTimer;

        let timer = TokioTimer::new();
        let t0 = timer.now_millis();
        timer
            .sleep(core::time::Duration::from_millis(10))
            .await
            .unwrap();
        let t1 = timer.now_millis();
        assert!(t1 > t0, "now_millis must advance with real time");
    }

    #[tokio::test]
    async fn test_mock_timer_now_millis_controllable() {
        let timer = MockTimer::new();
        assert_eq!(timer.now_millis(), 0);
        timer.advance(250);
        assert_eq!(timer.now_millis(), 250);
        timer.advance(750);
        assert_eq!(timer.now_millis(), 1000);
    }

    #[tokio::test]
    async fn test_discover_devices_wall_clock_timeout() {
        use crate::io::tokio::TokioTimer;

        struct QuickExitSocket;

        impl BindableUdpSocket for QuickExitSocket {
            async fn bind(_addr: SocketAddr) -> Result<Self, SocketError> {
                Ok(Self)
            }
        }

        impl AsyncUdpSocket for QuickExitSocket {
            async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize, SocketError> {
                Ok(100)
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        // Use a real TokioTimer with a very short timeout to verify wall-clock
        // termination. The old poll-counting approach would have run for
        // (timeout_ms / 100ms) iterations regardless of actual elapsed time.
        let timer = TokioTimer::new();
        let before = std::time::Instant::now();
        let devices = discover_devices::<QuickExitSocket, TokioTimer>(
            core::time::Duration::from_millis(300),
            &timer,
        )
        .await
        .unwrap();
        let elapsed = before.elapsed();

        assert!(devices.is_empty());
        // Wall-clock should be close to 300ms (not 0ms or much longer)
        assert!(
            elapsed.as_millis() >= 200,
            "discovery should run for approximately the timeout duration"
        );
    }
}
