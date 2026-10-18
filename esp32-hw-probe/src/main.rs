//! Current investigation: `EspIdfTlsConnector::pinned` on hardware (GitHub issue #642).
//!
//! `with_certs([pin])` cannot verify a pinned certificate: mbedTLS trusts an end-entity anchor
//! only when it is self-issued, and still checks its name, so a P1S's CA-issued leaf fails with
//! `UntrustedAnchor` and a self-signed proxy cert under another name fails with `NameMismatch`.
//! `pinned()` runs the `unverified()` handshake and compares the peer's leaf byte for byte.
//!
//! The probe first captures both leaves with `unverified()` + `peer_chain_der`, the path a
//! consumer captures a pin with, then runs four handshakes with `pinned()`:
//!
//! 1. the proxy's own cert against the proxy, under a server name the cert does not carry;
//! 2. the P1S's own leaf against the P1S (port 8883), under its real serial;
//! 3. the P1S's leaf against the proxy;
//! 4. the proxy's cert against the P1S.
//!
//! **Passes when** 1 and 2 connect and 3 and 4 fail with
//! `CertificateInvalid(PinMismatch)`, never `UntrustedAnchor` or `NameMismatch`.
//!
//! **Running it.** On a computer on the same LAN, start a TLS server with a self-signed cert:
//!
//! ```sh
//! openssl req -x509 -newkey rsa:2048 -nodes -days 30 -subj /CN=pin-test-proxy \
//!     -keyout pin-key.pem -out pin-cert.pem
//! openssl s_server -accept 8884 -cert pin-cert.pem -key pin-key.pem
//! ```
//!
//! Set `PROBE_PIN_PROXY=<that computer's IP>:8884` in `.env`, then:
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee pin-1.log
//! ```
//!
//! Prior investigations are recoverable via `git log -- esp32-hw-probe/src/main.rs`.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTcpStream, EspIdfTlsConnector};
use bambino::io::{CertificateFailure, RawStreamFactory, SocketError, TlsConnector};
use bambino::mqtt::MQTTS_PORT;
use core::time::Duration;

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
const PIN_PROXY: Option<&str> = option_env!("PROBE_PIN_PROXY");

/// A server name neither certificate carries.
const MADE_UP_NAME: &str = "pin-test-not-the-cert-name";
/// The TLS work runs on its own thread: the main task's 8 KB stack (`sdkconfig.defaults`) is
/// too small for a TLS handshake (`src/io/CLAUDE.md`).
const WORKER_STACK: usize = 48 * 1024;

type TlsStream = <EspIdfTlsConnector as TlsConnector<EspIdfTcpStream>>::Stream;

/// One TLS peer: where to dial and what server name to send.
#[derive(Clone, Copy)]
struct Peer {
    label: &'static str,
    host: &'static str,
    port: u16,
    sni: &'static str,
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

/// Dials `peer` and runs `connector`'s handshake; the caller drops the session.
async fn handshake(connector: &EspIdfTlsConnector, peer: Peer) -> Result<TlsStream, SocketError> {
    let raw = EspIdfRawStreamFactory.dial(peer.host, peer.port).await?;
    connector.connect(peer.sni, raw).await
}

/// Captures `peer`'s leaf the way a consumer would: `unverified()` and `peer_chain_der`.
async fn capture(peer: Peer) -> Result<Vec<u8>, String> {
    let connector = EspIdfTlsConnector::unverified();
    let stream = handshake(&connector, peer)
        .await
        .map_err(|e| format!("unverified handshake: {e:?}"))?;
    let chain = connector
        .peer_chain_der(&stream)
        .ok_or("peer_chain_der returned None (CONFIG_MBEDTLS_SSL_KEEP_PEER_CERTIFICATE?)")?;
    let leaf = chain.into_iter().next().ok_or("empty peer chain")?;
    // The hash, not the subject: a printer leaf's CN is its serial.
    log::info!(
        "  captured {} leaf: {} B, SHA-256 {}",
        peer.label,
        leaf.len(),
        sha256_hex(&leaf)
    );
    Ok(leaf)
}

/// Runs one `pinned()` handshake and judges it against what the case expects.
fn case(n: usize, pin_label: &str, pin: &[u8], peer: Peer, expect_ok: bool) -> (String, String) {
    let connector = EspIdfTlsConnector::pinned(pin.to_vec());
    let result = block_on(Box::pin(handshake(&connector, peer))).map(drop);
    let pass = match &result {
        Ok(()) => expect_ok,
        Err(SocketError::CertificateInvalid(CertificateFailure::PinMismatch)) => !expect_ok,
        Err(_) => false,
    };
    let step = format!("{n}. {pin_label} pin vs {}", peer.label);
    let outcome = format!("{}: {result:?}", if pass { "PASS" } else { "FAIL" });
    log::info!("  {step}: {outcome}");
    (step, outcome)
}

fn run(proxy: Peer, printer: Peer) -> Vec<(String, String)> {
    let proxy_leaf = block_on(Box::pin(capture(proxy)));
    let printer_leaf = block_on(Box::pin(capture(printer)));
    let (proxy_leaf, printer_leaf) = match (proxy_leaf, printer_leaf) {
        (Ok(p), Ok(q)) => (p, q),
        (p, q) => {
            return vec![(
                String::from("capture"),
                format!("FAILED: proxy {:?}, printer {:?}", p.err(), q.err()),
            )];
        }
    };
    vec![
        case(1, "proxy", &proxy_leaf, proxy, true),
        case(2, "printer", &printer_leaf, printer, true),
        case(3, "printer", &printer_leaf, proxy, false),
        case(4, "proxy", &proxy_leaf, printer, false),
    ]
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

    log::info!("esp32-hw-probe: EspIdfTlsConnector::pinned (#642)");

    let Some((proxy_host, proxy_port)) = PIN_PROXY
        .and_then(|p| p.rsplit_once(':'))
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
    else {
        log::error!(
            "STOPPED: set PROBE_PIN_PROXY=<ip>:<port> of the openssl s_server in \
             esp32-hw-probe/.env, rebuild and flash."
        );
        park();
    };
    let proxy = Peer {
        label: "proxy",
        host: proxy_host,
        port: proxy_port,
        sni: MADE_UP_NAME,
    };
    let printer = Peer {
        label: "printer",
        host: PRINTER_IP,
        port: MQTTS_PORT,
        sni: PRINTER_SERIAL,
    };

    let peripherals = Peripherals::take().unwrap_or_else(|e| fail_setup("Peripherals", e));
    let sysloop = EspSystemEventLoop::take().unwrap_or_else(|e| fail_setup("event loop", e));
    let nvs_part = EspDefaultNvsPartition::take().unwrap_or_else(|e| fail_setup("NVS", e));
    // Held for the whole run: dropping it tears down Wi-Fi.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs_part) {
        Ok(wifi) => wifi,
        Err(e) => fail_setup("Wi-Fi", e),
    };

    let worker = std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || run(proxy, printer));
    let results = match worker.map(|handle| handle.join()) {
        Ok(Ok(results)) => results,
        Ok(Err(_)) => vec![(String::from("worker"), String::from("PANICKED"))],
        Err(e) => vec![(String::from("worker"), format!("could not start: {e}"))],
    };

    log::info!("================ EspIdfTlsConnector::pinned ================");
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
