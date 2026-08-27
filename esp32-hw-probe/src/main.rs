//! Current investigation: GitHub issue #157's certificate-failure reporting.
//!
//! `SocketError::CertificateInvalid(CertificateFailure)` now carries *why* a peer's
//! certificate was rejected, and on ESP-IDF that detail is read out of band: mbedTLS
//! returns the same `MBEDTLS_ERR_X509_CERT_VERIFY_FAILED` (`-0x2700`) for every
//! verification failure — which esp-tls in turn flattens to `ESP_FAIL` — and the
//! flags naming the actual defect live in `mbedtls_ssl_get_verify_result`.
//! `query_verify_failure` in `src/io/esp_idf.rs` reads them off the live SSL context
//! via `esp_tls_get_ssl_context`.
//!
//! **The open question this probe answers.** `mbedtls_ssl_get_peer_cert` is documented
//! to return `NULL` after a *failed* handshake, so a failed context demonstrably does
//! not retain everything. Whether the *verify result* specifically survives to the
//! point `EspIdfTlsConnector::connect` builds its error is not something the host
//! unit tests or `scripts/check-esp-idf.sh` can observe — the flag-to-verdict mapping
//! is tested on the host, but only real mbedTLS can say whether there are any flags
//! left to map. See `.claude/rules/wire-framing-hardware-verification.md`.
//!
//! The code fails safe either way (no flags → `None` → the pre-existing opaque error),
//! so a negative result here is not a regression — it means the ESP-IDF half of #157
//! cannot work as written and needs the flags captured earlier, inside the negotiate
//! loop, before esp-tls tears anything down.
//!
//! Four cases, run back to back against one printer:
//!
//! | # | Anchors        | TLS name | Expected                      |
//! |---|----------------|----------|-------------------------------|
//! | 1 | all 5          | serial   | handshake OK                  |
//! | 2 | 1-4 (no BBL CA)| serial   | `CertificateInvalid(UntrustedAnchor)` |
//! | 3 | all 5          | bogus    | `CertificateInvalid(NameMismatch)`    |
//! | 4 | 1 only         | bogus    | `CertificateInvalid(UntrustedAnchor)` |
//!
//! **Cases 2 and 3 are the whole probe.** Before #157 both produced the byte-identical
//! `Other("ESP-IDF TLS handshake failed: ESP_FAIL")`. If they still match each other —
//! whatever they say — the verify result did not survive and nothing was gained. If
//! either comes back as `Other(..)`, the flags were already gone.
//!
//! Case 1 is the control: without a handshake that *succeeds*, cases 2-4 failing could
//! just mean the probe cannot reach the printer at all, and would prove nothing.
//!
//! Case 4 sets `NOT_TRUSTED` and `CN_MISMATCH` together and checks the precedence
//! documented on `map_mbedtls_verify_flags` holds against real mbedTLS. It matters for
//! more than tidiness: `UntrustedAnchor` is the one verdict a trust-on-first-use flow
//! may offer certificate capture for, so if mbedTLS reports this combination as a mere
//! name mismatch, a genuinely untrusted chain would be routed away from that prompt.
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
//! Prior investigations (e.g. issue #145's multi-anchor bundle probe, issue #65's
//! concurrent-sleep probe) are recoverable via `git log -- esp32-hw-probe/src/main.rs`,
//! not kept live here.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{CertificateFailure, RawStreamFactory, SocketError, TlsConnector};
use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// The five BambuStudio trust anchors, in the order they appear in `printer.cer`.
/// Index 4 (`bbl_5.der`) is the legacy self-signed `CN=BBL CA` a P1S chains to, so
/// withholding it (case 2) is what produces a genuine untrusted-anchor rejection.
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

/// The name for cases 3 and 4. Deliberately *not* serial-shaped: a wrong-but-plausible
/// serial in a tracked file reads like a leaked one, and `.invalid` is reserved by
/// RFC 2606 so it can never collide with a real printer's common name.
const BOGUS_TLS_NAME: &str = "not-this-printer.invalid";

/// MQTT over TLS. Chosen over FTPS because the handshake is the whole test and this port
/// needs no access code to reach it.
const PRINTER_TLS_PORT: u16 = 8883;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// What a case should produce. `Ok` is the successful-handshake control; every other case
/// names the exact `CertificateFailure` the backend is expected to report.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Expect {
    Ok,
    Cert(CertificateFailure),
}

/// What a case actually produced, reduced to the same shape for comparison. `Opaque` is
/// the pre-#157 behavior and the signature of the verify result *not* surviving.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Ok,
    Cert(CertificateFailure),
    /// Rejected, but with no certificate verdict attached — `Other(..)`, `TimedOut`, etc.
    Opaque,
    /// The printer was not reachable, so this case yielded no evidence either way.
    NoEvidence,
}

/// One row of the table in this file's header.
struct Case {
    name: &'static str,
    anchors: &'static [usize],
    /// `true` to dial with `BOGUS_TLS_NAME` instead of the real serial.
    bogus_name: bool,
    expect: Expect,
    /// Printed only when the case comes out the other way, so a failing run explains itself.
    on_surprise: &'static str,
}

const CASES: [Case; 4] = [
    Case {
        name: "1. all 5 anchors, real serial (control)",
        anchors: &[0, 1, 2, 3, 4],
        bogus_name: false,
        expect: Expect::Ok,
        on_surprise: "the control handshake failed, so cases 2-4 failing says nothing about \
                      certificate reporting — check reachability, anchors, and the clock \
                      before reading anything else in this transcript",
    },
    Case {
        name: "2. anchors 1-4, BBL CA withheld, real serial",
        anchors: &[0, 1, 2, 3],
        bogus_name: false,
        expect: Expect::Cert(CertificateFailure::UntrustedAnchor),
        on_surprise: "an untrusted chain was not reported as UntrustedAnchor. If this is \
                      Opaque, the verify result did not survive to where connect() builds \
                      its error and the ESP-IDF half of #157 does not work as written",
    },
    Case {
        name: "3. all 5 anchors, bogus TLS name",
        anchors: &[0, 1, 2, 3, 4],
        bogus_name: true,
        expect: Expect::Cert(CertificateFailure::NameMismatch),
        on_surprise: "a verified chain with a wrong name was not reported as NameMismatch. \
                      If this is Opaque, see case 2's note — same cause",
    },
    Case {
        name: "4. anchor 1 only, bogus TLS name (both flags)",
        anchors: &[0],
        bogus_name: true,
        expect: Expect::Cert(CertificateFailure::UntrustedAnchor),
        on_surprise: "mbedTLS resolved a chain that is BOTH untrusted and wrongly named \
                      differently than map_mbedtls_verify_flags' documented precedence. If \
                      this reported NameMismatch, a genuinely untrusted chain would be \
                      routed away from a trust-on-first-use capture prompt — fix the \
                      precedence, not this probe",
    },
];

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("esp32-hw-probe: issue #157 certificate-failure reporting probe");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT}");

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
    let mut results: [(&'static str, Expect, Outcome); 4] =
        [("", Expect::Ok, Outcome::NoEvidence); 4];

    for (slot, case) in CASES.iter().enumerate() {
        log::info!("--- {} ---", case.name);
        let outcome = run_case(case);
        let matched = match (case.expect, outcome) {
            (Expect::Ok, Outcome::Ok) => true,
            (Expect::Cert(expected), Outcome::Cert(actual)) => expected == actual,
            _ => false,
        };

        if matched {
            log::info!("PASS {}: got {outcome:?}, as expected", case.name);
        } else {
            log::error!(
                "SURPRISE {}: expected {:?}, got {outcome:?}",
                case.name,
                case.expect
            );
            log::error!("    -> {}", case.on_surprise);
            surprises += 1;
        }
        results[slot] = (case.name, case.expect, outcome);

        // The printer drops an unauthenticated MQTT session on its own; give it a moment
        // so a lingering half-open connection can't perturb the next case.
        std::thread::sleep(Duration::from_secs(2));
    }

    log::info!("================ issue #157 probe summary ================");
    for (name, expected, actual) in results {
        log::info!("  {name}: expected {expected:?}, got {actual:?}");
    }

    // The headline comparison, stated separately from the pass/fail tally because it is
    // the one thing the whole probe exists to establish: these two were byte-identical
    // before #157, and if they are still identical the change bought nothing on ESP-IDF.
    let (_, _, case2) = results[1];
    let (_, _, case3) = results[2];
    match (case2, case3) {
        (Outcome::Cert(a), Outcome::Cert(b)) if a != b => log::info!(
            "KEY RESULT: cases 2 and 3 are distinguishable ({a:?} vs {b:?}). The mbedTLS \
             verify result DOES survive a failed handshake, and the ESP-IDF backend can \
             report why a certificate was rejected."
        ),
        (Outcome::Opaque, _) | (_, Outcome::Opaque) => log::error!(
            "KEY RESULT: at least one of cases 2/3 came back with no certificate verdict. \
             The verify result does NOT survive to where connect() builds its error — the \
             flags must be captured earlier, inside the negotiate loop, or the ESP-IDF half \
             of #157 cannot work."
        ),
        (a, b) => log::error!(
            "KEY RESULT: cases 2 and 3 did not separate ({a:?} vs {b:?}). Whatever else this \
             transcript says, a consumer still cannot tell an untrusted anchor from a name \
             mismatch on this backend."
        ),
    }

    if surprises == 0 {
        log::info!("RESULT: all 4 cases matched.");
    } else {
        log::error!(
            "RESULT: {surprises} of 4 cases did not match — read the SURPRISE lines above \
             before drawing any conclusion."
        );
    }
    log::info!("=========================================================");

    park();
}

/// Runs one handshake against the printer and reduces the result to an [`Outcome`].
///
/// The full `SocketError` is logged verbatim in every failing case: `Other("ESP-IDF TLS
/// handshake failed: ESP_FAIL")` is exactly the pre-#157 string, so seeing it in the
/// transcript is the direct evidence that the verify result was already gone.
fn run_case(case: &Case) -> Outcome {
    let certs: std::vec::Vec<std::vec::Vec<u8>> = case
        .anchors
        .iter()
        .map(|&i| BBL_ANCHORS[i].to_vec())
        .collect();
    let tls_name = if case.bogus_name {
        BOGUS_TLS_NAME
    } else {
        PRINTER_SERIAL
    };
    log::info!(
        "    {} anchor(s), TLS name = {}",
        certs.len(),
        if case.bogus_name {
            BOGUS_TLS_NAME
        } else {
            "<serial>"
        }
    );

    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let raw = match EspIdfRawStreamFactory
            .dial(PRINTER_IP, PRINTER_TLS_PORT)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("    TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} failed: {e:?}");
                return Outcome::NoEvidence;
            }
        };

        match connector.connect(tls_name, raw).await {
            Ok(stream) => {
                log::info!(
                    "    handshake OK, negotiated {:?}",
                    connector.negotiated_version(&stream)
                );
                Outcome::Ok
            }
            Err(SocketError::CertificateInvalid(failure)) => {
                log::info!("    handshake rejected: CertificateInvalid({failure:?})");
                Outcome::Cert(failure)
            }
            Err(e) => {
                log::info!("    handshake rejected with no certificate verdict: {e:?}");
                Outcome::Opaque
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
