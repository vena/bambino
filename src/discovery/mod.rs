//! # Printer Discovery (SSDP)
//!
//! Find Bambu Lab printers on the local network using SSDP (Simple Service Discovery Protocol).
//!
//! [`DiscoveryEngine`] sends M-SEARCH queries on UDP port 2021 (and the alternate port 1990)
//! and parses incoming NOTIFY/response packets into [`SsdpDevice`] records.
//! [`DiscoveryEngine`] itself works across std, ESP-IDF, and Embassy via the
//! [`AsyncUdpSocket`] trait. The [`discover_devices()`] convenience function runs a timed
//! broadcast-and-listen sweep and returns all unique printers found, but is std-only
//! (`BindableUdpSocket` isn't implemented on Embassy — see
//! `.claude/rules/udp-socket-binding.md`); Embassy callers must drive `DiscoveryEngine`
//! directly instead.

pub mod parser;

use crate::error::Error;
use crate::io::AsyncUdpSocket;
#[cfg(feature = "std")]
use crate::io::{BindableUdpSocket, TimerProvider};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
pub use parser::{SsdpDevice, parse_ssdp_payload};

#[cfg(feature = "std")]
use std::collections::BTreeSet;

/// Standard Bambu Lab multicast group target for SSDP operations.
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);

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

/// Pacing sleep inside `discover_devices()`'s listen loop, applied on two paths that would
/// otherwise busy-spin for the rest of the discovery window:
///
/// 1. After a `poll_next_device` error — a persistently-erroring socket (e.g. a
///    platform-specific UDP ICMP-unreachable quirk). `poll_next_device` returns `Err` (not
///    `Ok(None)`) only for genuine socket faults, which have no `.await` yield point of their
///    own on the error path.
/// 2. After a full pass over every engine that consumed no measurable wall-clock time — the
///    signature of an `AsyncUdpSocket` impl that returns instantly on "no data" instead of
///    waiting or yielding as the trait requires.
#[cfg(feature = "std")]
const SSDP_POLL_BACKOFF_MS: u64 = 50;

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
    pub async fn broadcast_search(&self) -> Result<(), Error> {
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
            (Err(_), Err(e)) => Err(Error::Network(e)),
            _ => Ok(()),
        }
    }

    /// Listens on the bound socket interface and processes the next incoming SSDP packet.
    ///
    /// Returns `Ok(Some(SsdpDevice))` if a valid printer signature was parsed,
    /// `Ok(None)` if a timeout occurred or a non-printer packet was received,
    /// and `Err(Error)` for terminal network socket faults.
    pub async fn poll_next_device(&self, buf: &mut [u8]) -> Result<Option<SsdpDevice>, Error> {
        match self.socket.recv_from(buf).await {
            Ok((len, from_addr)) => {
                log::trace!(
                    "UDP socket received datagram of size {} from: {}",
                    len,
                    from_addr
                );

                // `AsyncUdpSocket` is public and unsealed, and its impls sit on top of raw
                // syscalls (EspIdfUdpSocket) or third-party FFI, so an out-of-range `len` is
                // untrusted input at a network boundary, not an internal invariant. Drop the
                // datagram instead of panicking the whole discovery loop on the slice.
                let Some(datagram) = buf.get(..len) else {
                    log::warn!(
                        "AsyncUdpSocket reported a datagram length of {} into a {}-byte buffer; dropping it",
                        len,
                        buf.len()
                    );
                    return Ok(None);
                };

                let mut parsed = parse_ssdp_payload(datagram);
                match &mut parsed {
                    Some(device) => {
                        // Stamp discovery_port here, not just in the discover_devices()
                        // convenience wrapper, so callers driving DiscoveryEngine directly (the
                        // required pattern on Embassy, since discover_devices() is std-only) also
                        // get a correctly populated field instead of the zero-value default.
                        device.discovery_port = self.port;
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
            Err(crate::io::SocketError::TimedOut) => Ok(None),
            Err(e) => Err(Error::Network(e)),
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
/// ```rust,ignore
/// use bambino::discovery::discover_devices;
/// use bambino::io::tokio::{TokioUdpSocket, TokioTimer};
///
/// let timer = TokioTimer::new();
/// // Allow at least 20s. Models that never answer M-SEARCH on port 2021 (notably the P1S)
/// // are found only through their ~10.1s NOTIFY advertisements, so a shorter window
/// // intermittently returns nothing at all — see `reference/01_network_discovery.md`.
/// let printers = discover_devices::<TokioUdpSocket, _>(
///     std::time::Duration::from_secs(20),
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
) -> Result<Vec<SsdpDevice>, Error>
where
    U: BindableUdpSocket,
    T: TimerProvider,
{
    // Bind sockets on both SSDP ports. Using the specific port is required because
    // printers send NOTIFY advertisements to 239.255.255.250:<port>, and the OS only
    // delivers multicast packets when the socket's bound port matches the destination port.
    let ports: &[u16] = &[SSDP_PORT, SSDP_PORT_ALT];

    let mut engines: Vec<(DiscoveryEngine<U>, u16)> = Vec::new();
    // Track the last bind failure and keep trying every port, instead of returning
    // as soon as the *first* port fails to bind. Returning early made degraded mode only work
    // when the second port failed after the first succeeded — if the first port failed (e.g.
    // another process already holds it), the second, free port was never even attempted.
    let mut last_bind_err: Option<crate::io::SocketError> = None;
    for &port in ports {
        let bind_addr = SocketAddr::from((IpAddr::V4(UNSPECIFIED_ADDR), port));
        log::debug!("Binding UDP socket on '{}'", bind_addr);
        match U::bind(bind_addr).await {
            Ok(socket) => engines.push((DiscoveryEngine::new(socket, port), port)),
            Err(e) => {
                log::debug!("Failed to bind port {}: {:?} (skipping)", port, e);
                last_bind_err = Some(e);
            }
        }
    }

    if engines.is_empty() {
        return Err(Error::Network(last_bind_err.expect(
            "ports is non-empty, so an empty engines list means every bind attempt recorded an error",
        )));
    }

    if engines.len() < ports.len() {
        log::warn!(
            "SSDP discovery running in degraded mode: only {} of {} ports bound",
            engines.len(),
            ports.len()
        );
    }

    for i in 0..2 {
        log::debug!("Initializing active query scan block #{}", i + 1);
        for (engine, _) in &engines {
            // Tolerate a per-engine send failure here too, matching the degraded-mode
            // bind loop above and the periodic re-broadcast loop below — propagating the error
            // with `?` aborted the whole sweep even when a healthy port could still have found
            // printers.
            let _ = engine.broadcast_search().await;
        }
        // Non-fatal for the same reason as the broadcast above and the backoff sleep below: a
        // TimerError on this 50ms inter-burst pause used to abort the entire sweep with both
        // sockets already bound and the listen loop never entered.
        if let Err(e) = timer.sleep(core::time::Duration::from_millis(50)).await {
            log::debug!("Inter-burst pacing sleep failed: {:?} (continuing sweep)", e);
        }
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
        let pass_start = timer.now_millis();
        let now = pass_start;
        if now.saturating_sub(last_search) >= SSDP_REBROADCAST_INTERVAL_MS as u64 {
            log::trace!("Re-broadcasting periodic M-SEARCH queries");
            for (engine, _) in &engines {
                let _ = engine.broadcast_search().await;
            }
            last_search = timer.now_millis();
        }

        for (engine, port) in &engines {
            // Log and pace on Err (previously silently discarded with no backoff) —
            // poll_next_device's Err path is reserved for genuine socket faults (not the
            // TimedOut/Ok(None) transient case), which have no `.await` yield point of their
            // own, so a persistently-erroring socket could otherwise busy-spin for the rest of
            // the discovery window with zero operator-visible signal.
            match engine.poll_next_device(&mut buf).await {
                Ok(Some(device)) => {
                    if seen_serials.insert(device.serial.clone()) {
                        log::debug!("Discovered '{}' via port {}", device.serial, port);
                        devices.push(device);
                    }
                }
                // No pacing here: an `Ok(None)` pass that consumed no wall-clock time is
                // caught by the whole-pass guard below, which covers both the socket-yield
                // and the non-printer-packet cases without slowing a socket that does block.
                Ok(None) => {}
                Err(e) => {
                    log::debug!(
                        "poll_next_device on port {} failed: {:?} (pacing before retry)",
                        port,
                        e
                    );
                    let _ = timer
                        .sleep(core::time::Duration::from_millis(
                            SSDP_POLL_BACKOFF_MS,
                        ))
                        .await;
                }
            }
        }

        // Self-pace when a full pass over every engine consumed no measurable time. The
        // `AsyncUdpSocket` docs require impls not to busy-spin, but the trait is public and
        // unsealed, so a conforming-looking impl that returns instantly on "no data" would
        // otherwise turn this loop into a genuine 100%-CPU spin for the entire discovery
        // window. Costs nothing when the socket really does block — the branch is not taken.
        if timer.now_millis().saturating_sub(pass_start) == 0 {
            let _ = timer
                .sleep(core::time::Duration::from_millis(
                    SSDP_POLL_BACKOFF_MS,
                ))
                .await;
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
    use crate::models::PrinterModel;
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
                Ok((
                    len,
                    SocketAddr::from((IpAddr::V4(Ipv4Addr::new(192, 168, 1, 150)), 2021)),
                ))
            } else {
                Err(SocketError::TimedOut)
            }
        }
    }

    #[tokio::test]
    async fn test_discovery_engine_broadcast_and_poll() {
        let socket =
            MockDiscoverySocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)))
                .await
                .unwrap();
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
        assert_eq!(device.model, PrinterModel::P1S);
        // poll_next_device must stamp discovery_port itself, not rely on the
        // discover_devices() wrapper — Embassy callers use DiscoveryEngine directly.
        assert_eq!(device.discovery_port, SSDP_PORT);

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
        let socket = FailSocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)))
            .await
            .unwrap();
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
            async fn send_to(
                &self,
                _buf: &[u8],
                _target: SocketAddr,
            ) -> Result<usize, SocketError> {
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

        let socket = HalfFailSocket::bind(SocketAddr::from((IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)))
            .await
            .unwrap();
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
            async fn send_to(
                &self,
                _buf: &[u8],
                _target: SocketAddr,
            ) -> Result<usize, SocketError> {
                Ok(100)
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        // Use a real TokioTimer with a very short timeout to verify wall-clock
        // termination.
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

    #[tokio::test]
    async fn test_discover_devices_succeeds_in_degraded_mode_when_one_port_fails_to_bind() {
        use crate::io::tokio::TokioTimer;

        // If only one of SSDP_PORT/SSDP_PORT_ALT can be bound (e.g. another process already
        // holds it), discovery must still proceed on the one bound port rather than failing
        // outright — only both ports failing is fatal.
        struct SinglePortBindFailSocket;

        impl BindableUdpSocket for SinglePortBindFailSocket {
            async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
                if addr.port() == SSDP_PORT_ALT {
                    Err(SocketError::AddressInUse)
                } else {
                    Ok(Self)
                }
            }
        }

        impl AsyncUdpSocket for SinglePortBindFailSocket {
            async fn send_to(
                &self,
                _buf: &[u8],
                _target: SocketAddr,
            ) -> Result<usize, SocketError> {
                Ok(100)
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        let timer = TokioTimer::new();
        let devices = discover_devices::<SinglePortBindFailSocket, TokioTimer>(
            core::time::Duration::from_millis(100),
            &timer,
        )
        .await
        .expect("discovery should succeed in degraded single-port mode");

        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn test_discover_devices_succeeds_in_degraded_mode_when_first_port_fails_to_bind() {
        use crate::io::tokio::TokioTimer;

        // The bind loop used to return as soon as SSDP_PORT (the *first* port in the
        // list) failed to bind, so SSDP_PORT_ALT was never even attempted — degraded mode only
        // actually worked when the *second* port failed. Mirrors the sibling test above but
        // fails the other port to prove both orderings now succeed.
        struct FirstPortBindFailSocket;

        impl BindableUdpSocket for FirstPortBindFailSocket {
            async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
                if addr.port() == SSDP_PORT {
                    Err(SocketError::AddressInUse)
                } else {
                    Ok(Self)
                }
            }
        }

        impl AsyncUdpSocket for FirstPortBindFailSocket {
            async fn send_to(
                &self,
                _buf: &[u8],
                _target: SocketAddr,
            ) -> Result<usize, SocketError> {
                Ok(100)
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        let timer = TokioTimer::new();
        let devices = discover_devices::<FirstPortBindFailSocket, TokioTimer>(
            core::time::Duration::from_millis(100),
            &timer,
        )
        .await
        .expect("discovery should succeed in degraded single-port mode when the first port fails");

        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn test_discover_devices_tolerates_initial_broadcast_failure_on_one_engine() {
        use crate::io::tokio::TokioTimer;

        // The initial scan loop used `?` on each engine's broadcast_search(), so one
        // engine failing to send aborted the whole sweep before the listen loop was ever
        // reached — even though the other, healthy port could still have found printers. This
        // contradicted the degraded-mode design used everywhere else in discover_devices
        // (the bind loop and the periodic re-broadcast loop both tolerate per-port failure).
        struct PartialSendFailSocket {
            port: u16,
        }

        impl BindableUdpSocket for PartialSendFailSocket {
            async fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
                Ok(Self { port: addr.port() })
            }
        }

        impl AsyncUdpSocket for PartialSendFailSocket {
            async fn send_to(
                &self,
                _buf: &[u8],
                _target: SocketAddr,
            ) -> Result<usize, SocketError> {
                // The engine bound on SSDP_PORT fails every send (both multicast and
                // broadcast targets), so its broadcast_search() call always errors.
                if self.port == SSDP_PORT {
                    Err(SocketError::ConnectionRefused)
                } else {
                    Ok(100)
                }
            }
            async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError> {
                Err(SocketError::TimedOut)
            }
        }

        let timer = TokioTimer::new();
        let devices = discover_devices::<PartialSendFailSocket, TokioTimer>(
            core::time::Duration::from_millis(100),
            &timer,
        )
        .await
        .expect("a single engine's initial broadcast failure must not abort the whole sweep");

        assert!(devices.is_empty());
    }
}
