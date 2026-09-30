//! Current investigation: GitHub issues #384 and #386 on real mbedTLS.
//!
//! **#384.** A handshake whose trust store held fewer anchors than the connector was given
//! must report `CertificateInvalid(IncompleteTrustStore)`, not `UntrustedAnchor`, because the
//! anchor that went missing may be the one the printer chains to. `connect` decides this with
//! `count_handshake_anchors` in `src/io/esp_idf.rs`, which walks the failed SSL context's
//! `private_conf->private_ca_chain` through raw pointers. The decision logic is host-tested;
//! whether that walk reads the right memory after a failed handshake is not something the host
//! or `scripts/check-esp-idf.sh` can observe. A garbage DER anchor stands in for the anchor a
//! low-memory handshake would drop: mbedTLS skips it the same way, every time.
//!
//! **#386.** ESP-IDF stores mbedTLS codes in its error record negated, so before the fix a peer
//! dropping the connection mid-handshake reported `Other("... mbedtls Some(80)")` instead of
//! `ConnectionReset`. Case 5 dials `scripts/tls-reset-listener.py`, which reads the ClientHello
//! and closes: first with an RST (`MBEDTLS_ERR_NET_CONN_RESET`), then with a FIN
//! (`MBEDTLS_ERR_SSL_CONN_EOF`). Both must come back `ConnectionReset`.
//!
//! | # | Dial | Anchors | Expected |
//! |---|---|---|---|
//! | 1 | printer | all 5 | handshake OK (control) |
//! | 2 | printer | 1-4 (BBL CA withheld) | `UntrustedAnchor`: a full store still reports as before |
//! | 3 | printer | 1-4 + garbage | `IncompleteTrustStore`, with a "held 4 of 5" warning |
//! | 4 | printer | all 5 + garbage | handshake OK: a partial store is still usable |
//! | 5a | listener (RST) | all 5 | `ConnectionReset` |
//! | 5b | listener (FIN) | all 5 | `ConnectionReset` |
//!
//! **Reading the result.** Case 1 must pass or nothing else means anything. Case 3 is the #384
//! answer: `IncompleteTrustStore` means the anchor walk works on hardware; `UntrustedAnchor`
//! means it counted 5 (or more) where 4 loaded; a crash or panic there means the walk read bad
//! memory. Case 5: `Other(..)` means the sign fix did not take; `ConnectionAborted` means the
//! fix works but lwIP reported that close with an errno other than `ECONNRESET`/`EPIPE`.
//!
//! **Setup.** `certs/` holds the five BambuStudio anchors (not committed; regenerate as in
//! `git show 3827d46:esp32-hw-probe/src/main.rs`). Network and printer details come from the
//! gitignored `.env` (see `.env.example`); case 5 also needs `PROBE_RESET_LISTENER`, and is
//! skipped without it. On a machine on the same LAN, start the listener fresh (so its first
//! connection is the RST) before flashing:
//!
//! ```sh
//! scripts/tls-reset-listener.py --port 8884
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee 384-test1.log
//! ```
//!
//! No access code is needed: every case ends at the TLS handshake. Prior investigations
//! (#294's step-bound check, #157's certificate-failure probe, and others) are recoverable
//! via `git log -- esp32-hw-probe/src/main.rs`.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{CertificateFailure, RawStreamFactory, SocketError, TlsConnector};
use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// The five BambuStudio trust anchors, in the order they appear in `printer.cer`.
/// Index 4 (`bbl_5.der`) is the legacy self-signed `CN=BBL CA` a P1S chains to, so
/// withholding it is what produces a genuine untrusted-anchor rejection.
const BBL_ANCHORS: [&[u8]; 5] = [
    include_bytes!("../certs/bbl_1.der"), // CN=BBL CA2 RSA, self-signed
    include_bytes!("../certs/bbl_2.der"), // CN=BBL CA2 ECC, self-signed
    include_bytes!("../certs/bbl_3.der"), // CN=BBL CA2 RSA, issued by BBL CA
    include_bytes!("../certs/bbl_4.der"), // CN=BBL CA2 ECC, issued by BBL CA
    include_bytes!("../certs/bbl_5.der"), // CN=BBL CA, self-signed (the P1S anchor)
];

/// Sentinel in a case's `anchors` list meaning `GARBAGE_ANCHOR` rather than a BBL anchor.
const GARBAGE: usize = usize::MAX;

/// Not a certificate. `der_certs_to_pem_bundle` wraps it in PEM armour like any other anchor;
/// the PEM decode then succeeds and the DER parse fails, which is the branch mbedTLS counts
/// and skips (`mbedtls_x509_crt_parse`, "total_failed++; continue") -- the same outcome as an
/// anchor dropped for lack of memory.
const GARBAGE_ANCHOR: &[u8] = b"issue 384: deliberately not a DER certificate";

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
/// Passed to `TlsConnector::connect` as the TLS hostname, mirroring `src/client/connect.rs`.
/// The printer's leaf is `CN=<serial>` with no SAN, so verifying against the dialled IP would
/// fail the common-name check for reasons that have nothing to do with anchors.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");
/// `<ip>:<port>` of `scripts/tls-reset-listener.py`. Optional: case 5 is skipped without it.
const RESET_LISTENER: Option<&str> = option_env!("PROBE_RESET_LISTENER");

/// MQTT over TLS: the handshake is the whole test, and this port needs no access code.
const PRINTER_TLS_PORT: u16 = 8883;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Printer,
    ResetListener,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Expect {
    Ok,
    Cert(CertificateFailure),
    Reset,
}

/// What a case produced, reduced for comparison. The full `SocketError` is logged as it
/// happens, so nothing is lost by the reduction.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Outcome {
    Ok,
    Cert(CertificateFailure),
    Socket(SocketError),
    /// The target was not reachable or not configured, so this case is no evidence either way.
    NoEvidence(&'static str),
}

struct Case {
    name: &'static str,
    target: Target,
    anchors: &'static [usize],
    expect: Expect,
    /// Printed only when the case comes out the other way, so a failing run explains itself.
    on_surprise: &'static str,
}

const CASES: [Case; 6] = [
    Case {
        name: "1. all 5 anchors (control)",
        target: Target::Printer,
        anchors: &[0, 1, 2, 3, 4],
        expect: Expect::Ok,
        on_surprise: "the control handshake failed, so no other case in this run means \
                      anything -- check reachability, the anchors, and the clock first",
    },
    Case {
        name: "2. anchors 1-4, BBL CA withheld",
        target: Target::Printer,
        anchors: &[0, 1, 2, 3],
        expect: Expect::Cert(CertificateFailure::UntrustedAnchor),
        on_surprise: "a full 4-of-4 store no longer reports UntrustedAnchor. If this says \
                      IncompleteTrustStore, count_handshake_anchors undercounts a complete \
                      store and would suppress every legitimate TOFU prompt",
    },
    Case {
        name: "3. anchors 1-4 + garbage, BBL CA withheld",
        target: Target::Printer,
        anchors: &[0, 1, 2, 3, GARBAGE],
        expect: Expect::Cert(CertificateFailure::IncompleteTrustStore),
        on_surprise: "the #384 answer. UntrustedAnchor means count_handshake_anchors counted \
                      5+ where 4 loaded (check the version != 0 test and the chain walk); \
                      Socket/NoEvidence means the handshake failed before verification",
    },
    Case {
        name: "4. all 5 + garbage",
        target: Target::Printer,
        anchors: &[0, 1, 2, 3, 4, GARBAGE],
        expect: Expect::Ok,
        on_surprise: "a store missing only a garbage anchor failed a handshake the real \
                      anchor should pass -- mbedTLS no longer treats a partial store as usable",
    },
    Case {
        name: "5a. listener closes with RST after ClientHello",
        target: Target::ResetListener,
        anchors: &[0, 1, 2, 3, 4],
        expect: Expect::Reset,
        on_surprise: "the #386 answer. Other(..) means the error record is still read with the \
                      wrong sign; ConnectionAborted means the sign fix works but lwIP reported \
                      this RST as something other than ECONNRESET/EPIPE. Also confirm the \
                      listener was restarted, so this was its first (RST) connection",
    },
    Case {
        name: "5b. listener closes with FIN after ClientHello",
        target: Target::ResetListener,
        anchors: &[0, 1, 2, 3, 4],
        expect: Expect::Reset,
        on_surprise: "a FIN mid-handshake should be MBEDTLS_ERR_SSL_CONN_EOF -> ConnectionReset. \
                      Other(..) means the sign fix did not take on this path",
    },
];

fn main() {
    esp_idf_svc::sys::link_patches();

    // `map_esp_tls_connect_error`'s full `(esp_tls .., mbedtls ..)` codes are logged at debug;
    // raise just that target so a surprise in case 5 shows the raw recorded values.
    let logger = esp_idf_svc::log::init_from_esp_idf();
    if let Err(e) = logger
        .filter()
        .set_target_level("bambino::io::esp_idf", log::LevelFilter::Debug)
    {
        log::warn!("could not raise bambino::io::esp_idf to debug: {e:?}");
    }

    log::info!("esp32-hw-probe: issues #384 (short trust store) and #386 (mbedTLS code sign)");
    log::info!("printer {PRINTER_IP}:{PRINTER_TLS_PORT}, reset listener {RESET_LISTENER:?}");

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

    // Held for the rest of `main`: dropping the wifi driver tears down the interface and every
    // later connect would fail for reasons unrelated to what this probe is testing.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => wifi,
        Err(e) => {
            log::error!("FAIL setup: Wi-Fi association failed: {e:?}");
            park();
        }
    };

    let mut results: Vec<(&'static str, Expect, Outcome)> = Vec::new();
    let mut surprises = 0u32;
    let mut skipped = 0u32;

    for case in &CASES {
        log::info!("--- {} ---", case.name);
        let outcome = run_case(case);
        let matched = match (case.expect, &outcome) {
            (Expect::Ok, Outcome::Ok) => true,
            (Expect::Cert(expected), Outcome::Cert(actual)) => expected == *actual,
            (Expect::Reset, Outcome::Socket(SocketError::ConnectionReset)) => true,
            _ => false,
        };

        if let Outcome::NoEvidence(why) = &outcome {
            log::warn!("SKIP {}: {why}", case.name);
            skipped += 1;
        } else if matched {
            log::info!("PASS {}: got {outcome:?}", case.name);
        } else {
            log::error!(
                "SURPRISE {}: expected {:?}, got {outcome:?}",
                case.name,
                case.expect
            );
            log::error!("    -> {}", case.on_surprise);
            surprises += 1;
        }
        results.push((case.name, case.expect, outcome));

        // The printer drops an unauthenticated MQTT session on its own; give it a moment so a
        // lingering half-open connection can't perturb the next case.
        std::thread::sleep(Duration::from_secs(2));
    }

    log::info!("================ issues #384 / #386 probe summary ================");
    for (name, expected, actual) in &results {
        log::info!("  {name}: expected {expected:?}, got {actual:?}");
    }
    log::info!(
        "RESULT: {} matched, {surprises} surprise(s), {skipped} skipped. Read any SURPRISE \
         line above before drawing a conclusion; case 1 must pass for the rest to count.",
        CASES.len() as u32 - surprises - skipped
    );
    log::info!("==================================================================");

    park();
}

/// Runs one handshake and reduces the result to an [`Outcome`], logging the full error.
fn run_case(case: &Case) -> Outcome {
    let (host, port, tls_name) = match case.target {
        Target::Printer => (PRINTER_IP, PRINTER_TLS_PORT, PRINTER_SERIAL),
        Target::ResetListener => {
            let Some(listener) = RESET_LISTENER else {
                return Outcome::NoEvidence("PROBE_RESET_LISTENER is not set in .env");
            };
            let Some((ip, port)) = listener.rsplit_once(':') else {
                return Outcome::NoEvidence("PROBE_RESET_LISTENER must be <ip>:<port>");
            };
            let Ok(port) = port.parse::<u16>() else {
                return Outcome::NoEvidence("PROBE_RESET_LISTENER's port is not a number");
            };
            // The listener never answers, so the name is never checked; any name will do.
            (ip, port, "reset-listener.invalid")
        }
    };

    let certs: Vec<Vec<u8>> = case
        .anchors
        .iter()
        .map(|&i| {
            if i == GARBAGE {
                GARBAGE_ANCHOR.to_vec()
            } else {
                BBL_ANCHORS[i].to_vec()
            }
        })
        .collect();
    log::info!(
        "    {} anchor(s) supplied, dialing {host}:{port}",
        certs.len()
    );

    // Construction runs `report_anchor_bundle_parse`, whose "N of M anchor(s) failed to parse"
    // or "all M anchor(s) parsed" line lands here -- the construction-time half of #384.
    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let raw = match EspIdfRawStreamFactory.dial(host, port).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("    TCP dial to {host}:{port} failed: {e:?}");
                return Outcome::NoEvidence("TCP dial failed; target unreachable");
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
                log::info!("    handshake failed: {e:?}");
                Outcome::Socket(e)
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
