//! Current investigation: `FtpsClient::download_prefix` on ESP-IDF (GitHub issue #629).
//!
//! On a P1S, a FIN on an FTPS data channel does not stop a download: the printer sends the rest
//! of the file. Only an RST does, and the printer then closes the control connection too.
//! `download_prefix` marks the data channel with `RawStreamFactory::abort_on_drop`, which on
//! ESP-IDF does nothing: the source says lwIP already ends the connection with an RST, because
//! the data channel is closed with received data unread (`tcp.c`, `tcp_close_shutdown`) and any
//! data arriving after the close is answered with one (`tcp_in.c`, `TF_RXCLOSED`). This probe
//! checks that on hardware.
//!
//! `CYCLES` cycles, `SPACING` apart. Each cycle connects an `FtpsClient`, reads the first
//! `PREFIX_BYTES` of `PROBE_FTPS_PATH` with `download_prefix`, drops the spent client, then times
//! a fresh connect plus `list_directory("/")`. The printer serves one FTPS connection at a time,
//! so that time is the signal: right after an RST it is about as fast as any login (~1 s), while
//! a printer still sending the rest of the file after a FIN holds the next login back for as long
//! as that takes. Cycle 1 also logs the prefix's SHA-256. After the cycles all three ports
//! (8883, 990, 6000) are checked at +0 s, +30 s and +130 s.
//!
//! **Passes when** every cycle returns `PREFIX_BYTES` with the client poisoned, every follow-up
//! login and listing succeeds in about the time of an ordinary one, and every port accepts at
//! every check. Compare the SHA-256 with `head -c <PREFIX_BYTES> <file> | shasum -a 256` on a
//! computer.
//!
//! **Running it.** Set `PROBE_ACCESS_CODE` and `PROBE_FTPS_PATH` (a `.3mf` on the card larger
//! than `PREFIX_BYTES`; a bigger file makes a FIN-close more obvious) in `.env`, close every
//! other FTPS client and camera viewer, then:
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee prefix-1.log
//! ```
//!
//! Prior investigations (the connection teardown check, #385's out-of-memory sweep, and others)
//! are recoverable via `git log -- esp32-hw-probe/src/main.rs`.

use bambino::ftps::{CurrentDateTime, FtpsClient, TlsVersionCheck};
use bambino::io::esp_idf::{
    EspIdfRawStreamFactory, EspIdfTcpStream, EspIdfTimer, EspIdfTlsConnector,
};
use bambino::io::RawStreamFactory;
use bambino::PrinterIdentity;
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
const ACCESS_CODE: Option<&str> = option_env!("PROBE_ACCESS_CODE");
const FTPS_PATH: Option<&str> = option_env!("PROBE_FTPS_PATH");

use bambino::ftps::FTPS_PORT;
use bambino::mqtt::MQTTS_PORT as MQTT_PORT;
const CAMERA_PORT: u16 = 6000;

const CYCLES: usize = 6;
const SPACING: Duration = Duration::from_secs(3);
/// Covers a Bambu Studio `.3mf`'s plate and pick images (36 864 B on the P1S file #629 measured).
/// `embassy-hw-probe` reads less: its heap can't hold this beside two TLS sessions.
const PREFIX_BYTES: usize = 40 * 1024;
/// When the ports are checked after the last cycle, in seconds.
const CHECKPOINTS: [u64; 3] = [0, 30, 130];
/// Bound on one port check's connect.
const PORT_CHECK_BOUND: Duration = Duration::from_secs(10);
/// The FTPS work runs on its own thread: the main task's 8 KB stack (`sdkconfig.defaults`) is
/// too small for the FTPS connect futures (`src/io/CLAUDE.md`). 32 KB overflowed in the first
/// TLS handshake (`mbedtls_entropy_func`, under `esp_tls_conn_new_sync`) while the whole cycle
/// future sat on the stack too; it is boxed now, and this is the margin on top.
const WORKER_STACK: usize = 48 * 1024;

type Ftps = FtpsClient<EspIdfTcpStream, EspIdfTlsConnector, EspIdfRawStreamFactory, EspIdfTimer>;

fn addr(port: u16) -> SocketAddr {
    let ip: std::net::Ipv4Addr = PRINTER_IP
        .parse()
        .expect("PROBE_PRINTER_IP is an IPv4 address");
    SocketAddr::from((ip, port))
}

fn listing_time() -> CurrentDateTime {
    CurrentDateTime::new(2026, 10, 14, 12, 0)
}

/// SHA-256 through ESP-IDF's own mbedTLS, so the probe needs no hashing dependency.
fn sha256_hex(data: &[u8]) -> String {
    let mut out = [0u8; 32];
    // SAFETY: `data` and `out` are valid for the lengths passed; 0 selects SHA-256, not -224.
    let rc = unsafe { sys::mbedtls_sha256(data.as_ptr(), data.len(), out.as_mut_ptr(), 0) };
    if rc != 0 {
        return format!("<mbedtls_sha256 failed {rc}>");
    }
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// Runs one bambino call, logging how long it took.
async fn call<T>(
    label: &str,
    fut: impl core::future::Future<Output = Result<T, bambino::Error>>,
) -> Result<T, String> {
    let started = Instant::now();
    match fut.await {
        Ok(v) => {
            log::info!("    {label}: ok, {} ms", started.elapsed().as_millis());
            Ok(v)
        }
        Err(e) => {
            let e = format!("{label}: {e:?}");
            log::warn!("    {e}");
            Err(e)
        }
    }
}

async fn connect_ftps(access_code: &str) -> Result<Ftps, String> {
    let raw = EspIdfRawStreamFactory
        .dial(PRINTER_IP, FTPS_PORT)
        .await
        .map_err(|e| format!("control dial: {e:?}"))?;
    let timer = EspIdfTimer::new().map_err(|e| format!("timer: {e:?}"))?;
    call(
        "FtpsClient::connect",
        FtpsClient::connect(
            raw,
            EspIdfTlsConnector::unverified(),
            EspIdfRawStreamFactory,
            PrinterIdentity::new(PRINTER_IP, PRINTER_SERIAL, access_code),
            timer,
            TlsVersionCheck::Enforce,
        ),
    )
    .await
}

/// One cycle: a prefix read, then a timed fresh login and listing. Returns the cycle's summary.
async fn cycle(access_code: &str, path: &str, buf: &mut [u8], n: usize) -> Result<String, String> {
    let mut ftps = connect_ftps(access_code).await?;
    let got = call("download_prefix", ftps.download_prefix(path, buf)).await?;
    let poisoned = ftps.is_poisoned();
    if n == 1 {
        log::info!(
            "  prefix SHA-256 {} ({got} B, starts {:02x?})",
            sha256_hex(&buf[..got]),
            &buf[..got.min(8)]
        );
    }
    // A poisoned client skips QUIT: the printer has already closed the control connection.
    ftps.disconnect().await;

    let started = Instant::now();
    let mut fresh = connect_ftps(access_code).await?;
    let listed = call("list /", fresh.list_directory("/", listing_time())).await?;
    let relogin_ms = started.elapsed().as_millis();
    fresh.disconnect().await;

    let stopped = if got < buf.len() {
        "FILE SHORTER THAN THE BUFFER: not an early stop"
    } else if poisoned {
        "stopped early, client poisoned"
    } else {
        "FULL BUFFER BUT CLIENT NOT POISONED"
    };
    Ok(format!(
        "{got} B, {stopped}; fresh login + LIST ({} entries) {relogin_ms} ms",
        listed.len()
    ))
}

/// Whether `port` accepts a plain TCP connection within `PORT_CHECK_BOUND`, closed cleanly.
fn accepts(port: u16) -> bool {
    match TcpStream::connect_timeout(&addr(port), PORT_CHECK_BOUND) {
        Ok(stream) => {
            drop(stream);
            true
        }
        Err(e) => {
            log::warn!("    {port}: connect failed: {e}");
            false
        }
    }
}

fn unreachable_ports() -> Vec<u16> {
    [MQTT_PORT, FTPS_PORT, CAMERA_PORT]
        .into_iter()
        .filter(|port| !accepts(*port))
        .collect()
}

fn run(access_code: &'static str, path: &'static str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut buf = vec![0u8; PREFIX_BYTES];
    for n in 1..=CYCLES {
        // Boxed: `block_on` keeps its future on this thread's stack, and the cycle future holds
        // room for the connect, prefix-read and listing futures at once.
        let result = match block_on(Box::pin(cycle(access_code, path, &mut buf, n))) {
            Ok(r) => r,
            Err(e) => format!("FAILED: {e}"),
        };
        log::info!("  cycle {n}: {result}");
        results.push((format!("cycle {n}"), result));
        std::thread::sleep(SPACING);
    }
    let started = Instant::now();
    for at in CHECKPOINTS {
        if let Some(wait) = Duration::from_secs(at).checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
        let down = unreachable_ports();
        let result = if down.is_empty() {
            String::from("all ports accepting")
        } else {
            format!("PORTS NOT ACCEPTING {down:?}")
        };
        log::info!("  +{at}s after the last cycle: {result}");
        results.push((format!("+{at}s"), result));
    }
    results
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

    log::info!("esp32-hw-probe: FtpsClient::download_prefix on ESP-IDF (#629)");

    let (Some(access_code), Some(path)) = (ACCESS_CODE, FTPS_PATH) else {
        log::error!(
            "STOPPED: set PROBE_ACCESS_CODE and PROBE_FTPS_PATH (a .3mf on the card larger than \
             {PREFIX_BYTES} B) in esp32-hw-probe/.env, rebuild and flash."
        );
        park();
    };

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

    let worker = std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || run(access_code, path));
    let results = match worker.map(|handle| handle.join()) {
        Ok(Ok(results)) => results,
        Ok(Err(_)) => vec![(String::from("worker"), String::from("PANICKED"))],
        Err(e) => vec![(String::from("worker"), format!("could not start: {e}"))],
    };

    log::info!("================ download_prefix on ESP-IDF ================");
    for (step, result) in &results {
        log::info!("  {step}: {result}");
    }
    log::info!("============================================================");
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
