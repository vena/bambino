//! Current investigation: GitHub issue #145's multi-anchor CA bundle.
//!
//! `EspIdfTlsConnector::with_certs` now takes many DER certificates and re-encodes
//! them into one NUL-terminated PEM bundle, because `X509::der` stores a slice with
//! no trailing NUL and mbedTLS reads that as "single DER cert" — parsing the first
//! certificate and silently dropping the rest. Nothing on the host can observe that:
//! the encoder is unit-tested in `src/io/mod.rs`, but whether *mbedTLS* actually
//! loads every anchor out of the bundle needs a real handshake against a real
//! printer. See `.claude/rules/wire-framing-hardware-verification.md`.
//!
//! **Why the anchor order makes this decisive.** BambuStudio's `printer.cer` ships
//! five certificates and the self-signed legacy `CN=BBL CA` root — the only one a
//! P1S chains to — is **last** in that file. So a parser that stops after the first
//! certificate loads `BBL CA2 RSA` and cannot verify a P1S at all.
//!
//! Four cases, run back to back against one printer:
//!
//! | # | Anchors passed          | Expected | What a wrong result would mean          |
//! |---|-------------------------|----------|-----------------------------------------|
//! | 1 | all 5, file order       | OK       | the fix works: parse reached anchor #5  |
//! | 2 | certs 1-4 (no BBL CA)   | FAIL     | if OK, verification isn't enforced      |
//! | 3 | cert 1 only             | FAIL     | if OK, the P1S didn't need anchor #5    |
//! | 4 | cert 5 only (BBL CA)    | OK       | if FAIL, anchor #5 isn't the right one  |
//!
//! Case 1 is the fix. Case 3 is what the *old* code effectively loaded, so 1-vs-3 is
//! the A/B. Cases 2 and 4 are the controls that stop case 1 from being a false pass:
//! without case 2 failing, a success in case 1 could just mean mbedTLS is verifying
//! nothing (bundle-attach left on, or an insecure sdkconfig), and case 1 would prove
//! nothing at all.
//!
//! **Setup.** Certificates are not committed (see `.gitignore`) — regenerate with:
//!
//! ```sh
//! cd esp32-hw-probe && mkdir -p certs && cd certs \
//!   && awk '/-----BEGIN CERTIFICATE-----/{n++} n{print > ("bbl_" n ".pem")}' \
//!        ../../../BambuStudio/resources/cert/printer.cer \
//!   && for n in 1 2 3 4 5; do openssl x509 -in bbl_$n.pem -outform DER -out bbl_$n.der; done \
//!   && rm -f bbl_*.pem
//! ```
//!
//! Network and printer details come from a gitignored `esp32-hw-probe/.env`, read by
//! `build.rs` and compiled in via `env!(..)`, so no Wi-Fi password, printer IP, or
//! serial is written into a tracked file or typed on a command line where it would land
//! in shell history. Copy `.env.example` to `.env` and fill it in. (Root `CLAUDE.md`
//! treats serials as credentials.) No access code is needed: the TLS handshake completes
//! before MQTT authentication, which is all this probe reaches.
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee run.log
//! ```
//!
//! Prior investigations (e.g. issue #65's concurrent-sleep probe) are recoverable via
//! `git log -- esp32-hw-probe/src/main.rs`, not kept live here.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// The five BambuStudio trust anchors, in the order they appear in `printer.cer`.
/// Index 4 (`bbl_5.der`) is the legacy self-signed `CN=BBL CA` a P1S chains to — last,
/// which is exactly what makes case 1 discriminating.
const BBL_ANCHORS: [&[u8]; 5] = [
    include_bytes!("../certs/bbl_1.der"), // CN=BBL CA2 RSA, self-signed
    include_bytes!("../certs/bbl_2.der"), // CN=BBL CA2 ECC, self-signed
    include_bytes!("../certs/bbl_3.der"), // CN=BBL CA2 RSA, issued by BBL CA
    include_bytes!("../certs/bbl_4.der"), // CN=BBL CA2 ECC, issued by BBL CA
    include_bytes!("../certs/bbl_5.der"), // CN=BBL CA, self-signed (the P1S anchor)
];

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
/// Passed to `TlsConnector::connect` as the TLS hostname, mirroring `src/client/connect.rs`.
/// The printer's leaf is `CN=<serial>` with no SAN, so verifying against the dialled IP
/// would fail the common-name check for reasons that have nothing to do with anchors.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

/// MQTT over TLS. Chosen over FTPS because the handshake is the whole test and this port
/// needs no access code to reach it.
const PRINTER_TLS_PORT: u16 = 8883;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// One row of the table in this file's header.
struct Case {
    name: &'static str,
    anchors: &'static [usize],
    expect_ok: bool,
    /// Printed only when the case comes out the other way, so a failing run explains itself.
    on_surprise: &'static str,
}

const CASES: [Case; 4] = [
    Case {
        name: "1. all 5 anchors, printer.cer order",
        anchors: &[0, 1, 2, 3, 4],
        expect_ok: true,
        on_surprise: "the bundle did NOT reach anchor #5 — issue #145's fix is not working",
    },
    Case {
        name: "2. anchors 1-4 only, BBL CA withheld",
        anchors: &[0, 1, 2, 3],
        expect_ok: false,
        on_surprise: "handshake succeeded with no valid anchor: verification is NOT being \
                      enforced, so case 1 proves nothing (check sdkconfig for ESP_TLS_INSECURE \
                      / a still-attached cert bundle)",
    },
    Case {
        name: "3. anchor 1 only (what pre-fix DER loaded)",
        anchors: &[0],
        expect_ok: false,
        on_surprise: "the P1S verified against BBL CA2 RSA alone, so this printer never needed \
                      anchor #5 and cases 1/3 do not form a valid A/B",
    },
    Case {
        name: "4. anchor 5 only (BBL CA)",
        anchors: &[4],
        expect_ok: true,
        on_surprise: "BBL CA alone cannot verify this printer — the premise of the whole probe \
                      is wrong, re-check which root the P1S chains to",
    },
];

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("esp32-hw-probe: issue #145 multi-anchor CA bundle probe");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT}, TLS name = <serial>");

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

    // Held for the rest of `main`: dropping the wifi driver tears down the interface and
    // every later connect would fail for reasons unrelated to certificates.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => wifi,
        Err(e) => {
            log::error!("FAIL setup: Wi-Fi association failed: {e:?}");
            park();
        }
    };

    let mut surprises = 0u32;
    let mut results: [(&'static str, bool, bool); 4] = [("", false, false); 4];

    for (slot, case) in CASES.iter().enumerate() {
        log::info!("--- {} ---", case.name);
        let outcome = run_case(case.anchors);
        let matched = outcome == case.expect_ok;

        match (outcome, matched) {
            (true, true) => log::info!("PASS {}: handshake OK, as expected", case.name),
            (false, true) => log::info!("PASS {}: handshake rejected, as expected", case.name),
            (true, false) => {
                log::error!("SURPRISE {}: handshake OK, expected failure", case.name);
                log::error!("    -> {}", case.on_surprise);
            }
            (false, false) => {
                log::error!("SURPRISE {}: handshake failed, expected OK", case.name);
                log::error!("    -> {}", case.on_surprise);
            }
        }

        if !matched {
            surprises += 1;
        }
        results[slot] = (case.name, case.expect_ok, outcome);

        // The printer drops an unauthenticated MQTT session on its own; give it a moment
        // so a lingering half-open connection can't perturb the next case.
        std::thread::sleep(Duration::from_secs(2));
    }

    log::info!("================ issue #145 probe summary ================");
    for (name, expected, actual) in results {
        log::info!(
            "  {name}: expected {}, got {}",
            if expected { "OK" } else { "FAIL" },
            if actual { "OK" } else { "FAIL" }
        );
    }

    if surprises == 0 {
        log::info!(
            "RESULT: all 4 cases matched. mbedTLS loads every anchor from the PEM bundle, \
             verification is enforced, and anchor #5 (last in the file) is what verifies \
             this printer — which the pre-fix single-DER path could not have loaded."
        );
    } else {
        log::error!(
            "RESULT: {surprises} of 4 cases did not match. The multi-anchor claim is NOT \
             confirmed — read the SURPRISE lines above before drawing any conclusion."
        );
    }
    log::info!("=========================================================");

    park();
}

/// Runs one handshake against the printer with `anchors` as the trust set.
///
/// Returns whether the TLS handshake completed; every failure mode (dial, handshake)
/// collapses to `false` deliberately — the cases only ask "did TLS verify or not", and
/// the underlying error is logged for a human reading the transcript.
fn run_case(anchors: &[usize]) -> bool {
    let certs: std::vec::Vec<std::vec::Vec<u8>> =
        anchors.iter().map(|&i| BBL_ANCHORS[i].to_vec()).collect();
    log::info!("    passing {} anchor(s) to with_certs", certs.len());

    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let raw = match EspIdfRawStreamFactory
            .dial(PRINTER_IP, PRINTER_TLS_PORT)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                // Distinct from a verification failure: the printer was not reachable at all,
                // so this case yielded no evidence either way.
                log::error!("    TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} failed: {e:?}");
                return false;
            }
        };

        match connector.connect(PRINTER_SERIAL, raw).await {
            Ok(stream) => {
                log::info!(
                    "    handshake OK, negotiated {:?}",
                    connector.negotiated_version(&stream)
                );
                true
            }
            Err(e) => {
                log::info!("    handshake rejected: {e:?}");
                false
            }
        }
    })
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
