//! Current investigation: does ESP-IDF close every connection bambino drops cleanly?
//!
//! embassy-net cannot: its `TcpClient` removes a dropped socket from the stack before the FIN is
//! sent. On ESP-IDF, the source says every teardown sends one: dropping `EspIdfTcpStream` closes
//! its `std::net::TcpStream`, `EspTls::drop` calls `esp_tls_conn_destroy`, which calls `close()`
//! on the socket (`esp_tls.c`), and lwIP's `close()` sends a FIN, or an RST if unread data is
//! waiting, retrying from its timer when short of memory (`tcp.c`, `tcp_close_shutdown`). This
//! probe checks that on hardware, and that the printer keeps accepting connections on every port
//! afterwards.
//!
//! Every phase is `CYCLES` cycles on port 990, `SPACING` apart. After each cycle a plain TCP
//! connection to 990 is timed and closed cleanly, so a dial's time shows what the previous
//! cycles left behind. After each phase all three ports (8883, 990, 6000) are checked, and
//! rechecked every `PORT_RETRY` for up to `PORT_RECOVERY_LIMIT`; if a port is still refusing
//! then, the remaining phases are skipped.
//!
//! | Phase | Each cycle on 990 |
//! |---|---|
//! | Close | `EspIdfRawStreamFactory::dial`, TLS handshake, `TlsConnector::close`, drop |
//! | DropRaw | `dial`, then drop without sending anything |
//! | MidDrop | `dial`, TLS handshake cancelled after `CANCEL_AFTER`, drop |
//!
//! DropRaw is also what bambino does when `EspTls::adopt` fails for lack of memory: `adopt`
//! hands the unreleased socket back with the error, and it is dropped the same way.
//!
//! **Reading the result.** Every phase must leave every port accepting connections.
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee teardown-1.log
//! ```
//!
//! Prior investigations (#385's out-of-memory sweep, #384/#386's trust-store check, and
//! others) are recoverable via `git log -- esp32-hw-probe/src/main.rs`.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use core::time::Duration;
use std::net::{SocketAddr, TcpStream};
use std::time::Instant;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::task::block_on;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

const MQTT_PORT: u16 = 8883;
const FTPS_PORT: u16 = 990;
const CAMERA_PORT: u16 = 6000;

const CYCLES: usize = 6;
const SPACING: Duration = Duration::from_secs(3);
/// Bound on the timed 990 dial after each cycle; long enough to time a slow dial.
const DIAL_BOUND: Duration = Duration::from_secs(60);
/// Bound on the port check, and on the Close phase's handshake.
const PORT_CHECK_BOUND: Duration = Duration::from_secs(10);
const HANDSHAKE_BOUND: Duration = Duration::from_secs(20);
/// Where MidDrop cuts the handshake: after the ClientHello, before the printer's answer
/// (~800 ms) has been processed.
const CANCEL_AFTER: Duration = Duration::from_millis(300);
/// A healthy dial on this LAN finishes in well under a second.
const SLOW_MS: u128 = 5_000;
/// How long a port may refuse connections after a phase before the run stops, and how often it
/// is rechecked meanwhile.
const PORT_RECOVERY_LIMIT: Duration = Duration::from_secs(300);
const PORT_RETRY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Close,
    DropRaw,
    MidDrop,
}

fn addr(port: u16) -> SocketAddr {
    let ip: std::net::Ipv4Addr = PRINTER_IP
        .parse()
        .expect("PROBE_PRINTER_IP is an IPv4 address");
    SocketAddr::from((ip, port))
}

/// One phase cycle's teardown on 990, through bambino's own ESP-IDF types.
fn teardown(phase: Phase) {
    block_on(async {
        let raw = match EspIdfRawStreamFactory.dial(PRINTER_IP, FTPS_PORT).await {
            Ok(raw) => raw,
            Err(e) => {
                log::warn!("    990: dial failed {e:?}");
                return;
            }
        };
        match phase {
            Phase::DropRaw => drop(raw),
            Phase::Close => {
                let connector = EspIdfTlsConnector::unverified().with_connect_timeout(HANDSHAKE_BOUND);
                match connector.connect(PRINTER_SERIAL, raw).await {
                    Ok(mut stream) => {
                        if let Err(e) = connector.close(&mut stream).await {
                            log::warn!("    990: close failed {e:?}");
                        }
                    }
                    Err(e) => log::warn!("    990: handshake failed {e:?}"),
                }
            }
            Phase::MidDrop => {
                let connector = EspIdfTlsConnector::unverified();
                let started = Instant::now();
                match embassy_futures::select::select(
                    connector.connect(PRINTER_SERIAL, raw),
                    embassy_time::Timer::after(embassy_time::Duration::from_millis(
                        CANCEL_AFTER.as_millis() as u64,
                    )),
                )
                .await
                {
                    embassy_futures::select::Either::Second(()) => log::info!(
                        "    990: handshake cancelled after {} ms",
                        started.elapsed().as_millis()
                    ),
                    embassy_futures::select::Either::First(r) => log::warn!(
                        "    990: handshake finished before the cancel ({:?}); this cycle does \
                         not test a cancelled handshake",
                        r.map(|_| ())
                    ),
                }
            }
        }
    });
}

/// A plain TCP connection to `port`, closed cleanly. Returns the connect time in ms, or `None`
/// if it did not connect within `bound`.
fn timed_connect(port: u16, bound: Duration) -> Option<u128> {
    let started = Instant::now();
    match TcpStream::connect_timeout(&addr(port), bound) {
        Ok(stream) => {
            let ms = started.elapsed().as_millis();
            drop(stream);
            Some(ms)
        }
        Err(e) => {
            log::warn!(
                "    {port}: connect failed after {} ms: {e}",
                started.elapsed().as_millis()
            );
            None
        }
    }
}

fn unreachable_ports() -> Vec<u16> {
    [MQTT_PORT, FTPS_PORT, CAMERA_PORT]
        .into_iter()
        .filter(|port| timed_connect(*port, PORT_CHECK_BOUND).is_none())
        .collect()
}

/// `Ok(None)`: all ports accepted at once. `Ok(Some(ms))`: all accepted again after `ms`.
/// `Err(ports)`: still refusing after `PORT_RECOVERY_LIMIT`.
fn port_health(after: &str) -> Result<Option<u128>, Vec<u16>> {
    let started = Instant::now();
    loop {
        let unreachable = unreachable_ports();
        if unreachable.is_empty() {
            if started.elapsed() < PORT_RETRY {
                return Ok(None);
            }
            let ms = started.elapsed().as_millis();
            log::warn!("    ports: all accepting again {ms} ms after {after}");
            return Ok(Some(ms));
        }
        if started.elapsed() >= PORT_RECOVERY_LIMIT {
            log::error!(
                "    ports: {unreachable:?} still not accepting {}s after {after}",
                PORT_RECOVERY_LIMIT.as_secs()
            );
            return Err(unreachable);
        }
        log::warn!("    ports: {unreachable:?} not accepting after {after}; retrying");
        std::thread::sleep(PORT_RETRY);
    }
}

fn describe(dial: Option<u128>) -> String {
    match dial {
        None => format!("990 dial >{}s", DIAL_BOUND.as_secs()),
        Some(ms) if ms >= SLOW_MS => format!("SLOW 990 dial {ms} ms"),
        Some(ms) => format!("990 dial {ms} ms"),
    }
}

fn main() {
    sys::link_patches();

    // `bambino::io` at debug: the raw `esp_tls`/mbedTLS codes behind a failed handshake.
    let logger = esp_idf_svc::log::init_from_esp_idf();
    if let Err(e) = logger
        .filter()
        .set_target_level("bambino::io", log::LevelFilter::Debug)
    {
        log::warn!("could not raise bambino::io to debug: {e:?}");
    }

    log::info!("esp32-hw-probe: connection teardown on ESP-IDF");

    let peripherals = Peripherals::take().unwrap_or_else(|e| fail_setup("Peripherals", e));
    let sysloop = EspSystemEventLoop::take().unwrap_or_else(|e| fail_setup("event loop", e));
    let nvs_part = EspDefaultNvsPartition::take().unwrap_or_else(|e| fail_setup("NVS", e));
    // Held for the whole run: dropping it tears down Wi-Fi.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs_part) {
        Ok(wifi) => wifi,
        Err(e) => fail_setup("Wi-Fi", e),
    };

    let unreachable = unreachable_ports();
    if !unreachable.is_empty() {
        log::error!(
            "STOPPED: the printer does not accept connections on {unreachable:?}. Fix that \
             (check from a computer with `nc -z <printer-ip> <port>`), then reset the board."
        );
        park();
    }
    log::info!("preflight: 8883, 990 and 6000 all accept connections");

    let mut summary: Vec<(Phase, Vec<Option<u128>>, Result<Option<u128>, Vec<u16>>)> = Vec::new();
    for phase in [Phase::Close, Phase::DropRaw, Phase::MidDrop] {
        log::info!("--- phase {phase:?}: {CYCLES} cycles ---");
        let mut dials = Vec::with_capacity(CYCLES);
        for n in 1..=CYCLES {
            teardown(phase);
            let dial = timed_connect(FTPS_PORT, DIAL_BOUND);
            log::info!("  {phase:?} cycle {n}: {}", describe(dial));
            dials.push(dial);
            std::thread::sleep(SPACING);
        }
        let health = port_health(&format!("{phase:?}"));
        let dead = health.is_err();
        summary.push((phase, dials, health));
        if dead {
            break;
        }
    }

    log::info!("================ connection teardown on ESP-IDF ================");
    for (phase, dials, health) in &summary {
        let slow = dials
            .iter()
            .filter(|d| d.is_none_or(|ms| ms >= SLOW_MS))
            .count();
        let list: Vec<String> = dials.iter().map(|d| describe(*d)).collect();
        log::info!("  {phase:?}: {slow}/{CYCLES} slow; {}", list.join(", "));
        match health {
            Ok(None) => log::info!("    after: all ports accepting"),
            Ok(Some(ms)) => log::info!("    after: refused at first, all accepting after {ms} ms"),
            Err(ports) => log::info!(
                "    after: {ports:?} still refusing after {}s; later phases skipped",
                PORT_RECOVERY_LIMIT.as_secs()
            ),
        }
    }
    log::info!("RESULT: every phase must leave every port accepting connections.");
    log::info!("==================================================================");
    park();
}

fn connect_wifi(
    modem: esp_idf_svc::hal::modem::Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>, sys::EspError> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().expect("PROBE_WIFI_SSID too long"),
        password: WIFI_PASS.try_into().expect("PROBE_WIFI_PASS too long"),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().sta_netif().get_ip_info()?;
    // SSID deliberately not logged: the run transcript gets pasted into an issue.
    log::info!("Wi-Fi associated, got IP {:?}", ip.ip);

    Ok(wifi)
}

fn fail_setup(what: &str, e: sys::EspError) -> ! {
    log::error!("FAIL setup: {what}: {e:?}");
    park();
}

/// ESP-IDF `main` is not meant to return; park so the monitor keeps the transcript on screen.
fn park() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
