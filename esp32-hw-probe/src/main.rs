//! Current investigation: GitHub issue #168 -- does `EspIdfTlsConnector::new()`'s anchor-less
//! path (the `crt_bundle_attach`-hijack fix in `src/io/esp_idf.rs`) actually complete a TLS
//! handshake against a real printer, and can `peer_chain_der` read the presented chain back
//! out of it afterward?
//!
//! **Background.** `with_certs(vec![], None)`/`new()` used to reach ESP-IDF's
//! `esp_tls_set_client_config` with none of `cacert_buf`/`crt_bundle_attach`/
//! `skip_server_cert_verify` set, which failed every handshake with an opaque
//! `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED` (captured against a real ESP32-P4/P1S in the issue).
//! `esp_idf_svc::tls::Config` (0.52.1) has no field for `skip_server_cert_verify`, so the fix
//! installs a custom `crt_bundle_attach` hook that forces `MBEDTLS_SSL_VERIFY_NONE` directly,
//! bypassing `Config`/`EspTls::negotiate` and calling `esp_tls_conn_new_sync` through
//! `EspTls::context_handle()` instead. That's all verified against ESP-IDF's C source and
//! compiles clean under `scripts/check-esp-idf.sh` -- what it has NOT had is a real handshake
//! against a real printer, which is what this probe is for.
//!
//! **What "pass" looks like:** the handshake completes (no `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED`,
//! no other failure), and `peer_chain_der` returns at least one certificate afterward -- that's
//! the actual downstream use case (issue #168 links to a TOFU-capture consumer that needs the
//! presented leaf even though nothing was verified). Only the MQTT TLS port is exercised: no
//! access code is needed, since the handshake completes before MQTT authentication.
//!
//! **Setup.** Network and printer details come from a gitignored `esp32-hw-probe/.env`, read by
//! `build.rs` and compiled in via `env!(..)` -- see `.env.example`. Root `CLAUDE.md` treats the
//! serial as a credential, so it is never written to a tracked file or typed where it would land
//! in shell history.
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee run.log
//! ```
//!
//! Prior investigations (issues #160/#161's handshake-timing and concurrent-connect probes,
//! #157's certificate-failure probe, #145's multi-anchor bundle probe, #65's concurrent-sleep
//! probe) are recoverable via `git log -- esp32-hw-probe/src/main.rs`, not kept live here --
//! see this directory's `CLAUDE.md` for the reuse convention this follows.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
/// Passed to `TlsConnector::connect` as the TLS hostname, mirroring `src/client/connect.rs`.
/// Irrelevant to whether verification runs here (this probe's connector never checks the
/// name), but kept anyway so a wire capture of this run looks like a real client's, not a
/// synthetic one -- and `EspIdfTlsConnector::connect`'s log lines redact and report it either
/// way.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

/// MQTT over TLS -- the same port `PrinterClient` dials, and reachable without an access code
/// since only the handshake (not MQTT authentication) is exercised.
const PRINTER_TLS_PORT: u16 = 8883;

/// Generous enough that a slow-but-succeeding handshake still reports a clean result rather
/// than a `TimedOut` that would be misread as this fix not working. #160's probe measured
/// 1.7-4.0s as the normal range on this class of hardware.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::init_from_esp_idf();

    log::info!("esp32-hw-probe: issue #168 unverified-TLS handshake + peer_chain_der");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT} (TLS hostname {PRINTER_SERIAL})");

    let peripherals = match Peripherals::take() {
        Ok(p) => p,
        Err(e) => {
            log::error!("FAIL setup: Peripherals::take() failed: {e:?}");
            park();
        }
    };
    let sysloop = match EspSystemEventLoop::take() {
        Ok(s) => s,
        Err(e) => {
            log::error!("FAIL setup: EspSystemEventLoop::take() failed: {e:?}");
            park();
        }
    };
    let nvs = match EspDefaultNvsPartition::take() {
        Ok(n) => n,
        Err(e) => {
            log::error!("FAIL setup: EspDefaultNvsPartition::take() failed: {e:?}");
            park();
        }
    };

    // Held for the rest of `main`: dropping the wifi driver tears down the interface and the
    // handshake attempt below would fail for reasons unrelated to what this probe is testing.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => wifi,
        Err(e) => {
            log::error!("FAIL setup: Wi-Fi association failed: {e:?}");
            park();
        }
    };

    run_probe();
    park();
}

/// Dials the printer, runs one unverified TLS handshake, and checks `peer_chain_der`.
///
/// Two independent things are being checked, and both are logged explicitly rather than
/// folded into one pass/fail, because a partial result (handshake OK, no chain) is itself a
/// finding: it would mean the `crt_bundle_attach` hook disables verification but something
/// about the no-anchor path stops `CONFIG_MBEDTLS_SSL_KEEP_PEER_CERTIFICATE` from applying,
/// which is a different bug than the one issue #168 opened with.
fn run_probe() {
    let connector = EspIdfTlsConnector::new().with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let raw = match EspIdfRawStreamFactory
            .dial(PRINTER_IP, PRINTER_TLS_PORT)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                log::error!(
                    "FAIL: TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} failed: {e:?} -- \
                     nothing about issue #168 can be tested if the printer isn't reachable"
                );
                return;
            }
        };

        match connector.connect(PRINTER_SERIAL, raw).await {
            Ok(stream) => {
                log::info!(
                    "PASS: unverified handshake completed, negotiated {:?}",
                    connector.negotiated_version(&stream)
                );

                match connector.peer_chain_der(&stream) {
                    Some(chain) if !chain.is_empty() => {
                        log::info!(
                            "PASS: peer_chain_der returned {} certificate(s), leaf first",
                            chain.len()
                        );
                        for (i, cert) in chain.iter().enumerate() {
                            log::info!("  [{i}] {} DER bytes", cert.len());
                        }
                        log::info!(
                            "RESULT: issue #168's fix works end to end on this hardware -- \
                             unverified handshake succeeded and the presented chain was \
                             readable afterward, which is the TOFU-capture use case the issue \
                             was opened for."
                        );
                    }
                    Some(_) | None => {
                        log::error!(
                            "FAIL: handshake completed but peer_chain_der returned no \
                             certificates. The crt_bundle_attach fix may be working while \
                             something else (CONFIG_MBEDTLS_SSL_KEEP_PEER_CERTIFICATE?) blocks \
                             reading the chain back -- see src/io/CLAUDE.md's peer_chain_der \
                             entry."
                        );
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "FAIL: unverified handshake did not complete: {e:?} -- if this is \
                     SocketError::Other citing ESP_ERR_MBEDTLS_SSL_SETUP_FAILED, the \
                     crt_bundle_attach fix did not take effect on this build; check whether \
                     CONFIG_MBEDTLS_CERTIFICATE_BUNDLE is actually enabled in sdkconfig"
                );
            }
        }
    });
}

fn connect_wifi(
    // `'static` because the returned `EspWifi<'static>` borrows it for the rest of `main`.
    modem: esp_idf_svc::hal::modem::Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>, esp_idf_svc::sys::EspError> {
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
    // SSID deliberately not logged: the run transcript gets read back and pasted into an
    // issue, and the network name is the user's, not evidence for this investigation.
    log::info!("Wi-Fi associated, got IP {:?}", ip.ip);

    Ok(wifi)
}

/// ESP-IDF `main` is not meant to return; park so the monitor keeps the transcript on screen.
fn park() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
