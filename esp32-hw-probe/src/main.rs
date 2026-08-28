//! Current investigation: GitHub issue #160's TLS handshake cost breakdown.
//!
//! A downstream consumer measured the printer connect path phase by phase and found the
//! TLS handshake to be 94-98% of the interval (1.2-2.4s per handshake). Nothing measured
//! so far separates the two things that could account for it, and the fix is opposite in
//! each case:
//!
//! - **Poll pacing.** `EspIdfTlsConnector::connect` pins `Config::timeout_ms = 0` (issue
//!   #67), so each `esp_tls_low_level_conn` call takes exactly one handshake step and
//!   returns; the loop then sleeps `TLS_POLL_INTERVAL` (20ms) before the next. If the
//!   steps are essentially free, ~60 steps x 20ms *is* the handshake, and the fix is an
//!   adaptive or readiness-driven poll.
//! - **Genuine compute.** Chain verification three deep against five anchors, plus the
//!   key exchange, on a small RISC-V core could plausibly be hundreds of milliseconds of
//!   real work. Then the interval is irreducible at this layer and #160 closes.
//!
//! `src/io/esp_idf.rs`'s handshake loop now counts steps and accumulates the two halves
//! separately, reporting them on its existing summary line:
//!
//! ```text
//! ESP-TLS handshake with <host> completed in 1264ms (63 steps, 4821us in esp_tls, 1259402us polling)
//! ```
//!
//! **What this probe adds** is the repetition. The known range for the whole interval is
//! 1.7-4.0s across 26 real sessions downstream, so a single handshake cannot distinguish
//! a real difference from noise — #160 asks for the spread over several. This runs
//! [`RUNS`] handshakes back to back against one printer, each a fresh TCP dial and a
//! fresh connector, and stopwatches each one independently so the caller-side number can
//! be cross-checked against the crate's own (downstream saw them agree within 1ms).
//!
//! **Reading the result.** The ratio is the answer, not the total. Tally the breakdowns
//! straight out of the transcript:
//!
//! ```sh
//! grep -o '([0-9]* steps, [0-9]*us in esp_tls, [0-9]*us polling)' run.log
//! ```
//!
//! - `us polling` dominating → the poll interval is the cost. Worth then checking whether
//!   the fd is selectable, since waiting on readability removes the 20ms quantisation
//!   entirely rather than shrinking it.
//! - `us in esp_tls` dominating → the interval is a rounding error, #160 closes as
//!   irreducible here, and the follow-up is a consumer-side progress concern.
//!
//! The two sums should add to roughly the reported duration; if they do not, the time is
//! going somewhere neither counter covers and that is itself the finding.
//!
//! **Treat run 1 as suspect.** Downstream saw run 1 come out slowest and explicitly
//! declined to build on it — within one boot it may be warm-up, run order, or proximity
//! to Wi-Fi association rather than anything about TLS. The summary below separates it
//! from the rest for that reason; do not read a first-run difference as signal.
//!
//! **Chip caveat.** The downstream measurement was an ESP32-P4 at 360MHz. This probe
//! defaults to an ESP32-C6 (160MHz, single core) — see this directory's `CLAUDE.md` for
//! retargeting. The polling half is fixed at 20ms per step on any chip and only the
//! compute half scales with clock, so the inference runs one way only: if a C6 shows
//! polling dominant, a P4 is more so and the conclusion carries. If a C6 shows compute
//! dominant, that says nothing about the P4, which would do the same work faster.
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
//! Prior investigations (e.g. issue #157's certificate-failure probe, issue #145's
//! multi-anchor bundle probe, issue #65's concurrent-sleep probe) are recoverable via
//! `git log -- esp32-hw-probe/src/main.rs`, not kept live here.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use core::time::Duration;
use std::time::Instant;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// The five BambuStudio trust anchors, in the order they appear in `printer.cer`.
///
/// All five are used on every run here, unlike issue #157's probe which withheld some to
/// force rejections: #160 measures the *successful* path, and the anchor count is part of
/// what is being measured — chain verification against the full bundle is one of the two
/// candidate explanations for the handshake cost.
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
/// would fail the common-name check and this probe would measure a rejection, not a
/// handshake.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

/// MQTT over TLS. Chosen over FTPS because the handshake is the whole test and this port
/// needs no access code to reach it.
const PRINTER_TLS_PORT: u16 = 8883;

/// Generous enough that a slow-but-succeeding handshake still yields a breakdown rather
/// than a `TimedOut`. The observed worst case downstream is ~4s.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// How many handshakes to run. #160 asks for a spread rather than a single figure, and
/// eight is enough to see one against a 1.7-4.0s known range while still being a short
/// enough transcript to read by eye.
const RUNS: usize = 8;

/// Settle time between runs. The printer drops an unauthenticated MQTT session on its own;
/// this keeps a lingering half-open connection from perturbing the next run's timing,
/// which is the whole measurement here.
const BETWEEN_RUNS: Duration = Duration::from_secs(3);

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("esp32-hw-probe: issue #160 TLS handshake cost breakdown");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT}, {RUNS} runs, all 5 anchors");

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
    // every later connect would fail for reasons unrelated to the handshake.
    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => wifi,
        Err(e) => {
            log::error!("FAIL setup: Wi-Fi association failed: {e:?}");
            park();
        }
    };

    // `None` marks a run that produced no timing — an unreachable printer or a rejected
    // handshake measures nothing and must not be averaged in as if it were a fast run.
    let mut totals: [Option<u128>; RUNS] = [None; RUNS];

    for (slot, total) in totals.iter_mut().enumerate() {
        log::info!("--- run {} of {RUNS} ---", slot + 1);
        *total = run_handshake();
        std::thread::sleep(BETWEEN_RUNS);
    }

    report(&totals);
    park();
}

/// Runs one full dial-and-handshake and returns the caller-side wall time in milliseconds.
///
/// Returns `None` for anything that isn't a completed handshake: a rejection or a dial
/// failure yields no breakdown to read, and its elapsed time is not comparable to a
/// successful run's.
///
/// The connector is rebuilt every run rather than hoisted out of the loop. That is
/// deliberate: it keeps each run a from-scratch connect exactly as a consumer performs it,
/// and it means the anchor-bundle work (five PEM anchors decoded, re-encoded, and parsed —
/// ~10ms downstream) is inside no run's stopwatch but repeated identically for all of them.
fn run_handshake() -> Option<u128> {
    let certs: std::vec::Vec<std::vec::Vec<u8>> =
        BBL_ANCHORS.iter().map(|anchor| anchor.to_vec()).collect();

    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let dial_start = Instant::now();
        let raw = match EspIdfRawStreamFactory
            .dial(PRINTER_IP, PRINTER_TLS_PORT)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("    TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} failed: {e:?}");
                return None;
            }
        };
        log::info!("    tcp connect {}ms", dial_start.elapsed().as_millis());

        // Stopwatched on this side as well as inside the crate so the two can be compared:
        // if the caller's figure and the crate's `completed in Xms` disagree, time is being
        // spent outside the loop the breakdown covers and the breakdown is not the whole
        // story.
        let handshake_start = Instant::now();
        let outcome = connector.connect(PRINTER_SERIAL, raw).await;
        let elapsed = handshake_start.elapsed().as_millis();

        match outcome {
            Ok(stream) => {
                log::info!(
                    "    handshake OK in {elapsed}ms (caller stopwatch), negotiated {:?}",
                    connector.negotiated_version(&stream)
                );
                // Dropped here rather than at the end of the run so the TCP teardown is not
                // counted against `BETWEEN_RUNS`' settle time.
                drop(stream);
                Some(elapsed)
            }
            Err(e) => {
                log::error!("    handshake failed after {elapsed}ms: {e:?}");
                log::error!(
                    "    -> no breakdown from this run. A run that does not complete \
                     measures nothing; check reachability, anchors, and the clock."
                );
                None
            }
        }
    })
}

/// Prints the run-total summary, with run 1 held apart per this file's header.
///
/// Only totals are summarised here. The step/compute/poll breakdown is emitted by the
/// crate itself on its `ESP-TLS handshake with ...` line, one per run above — this
/// function deliberately does not try to scrape those back out of the log, since parsing
/// a log line the crate is free to reword would make the probe silently wrong later.
fn report(totals: &[Option<u128>; RUNS]) {
    log::info!("================ issue #160 probe summary ================");

    for (slot, total) in totals.iter().enumerate() {
        match total {
            Some(ms) => log::info!("  run {}: {ms}ms", slot + 1),
            None => log::info!("  run {}: no measurement", slot + 1),
        }
    }

    let completed: std::vec::Vec<u128> = totals.iter().flatten().copied().collect();
    if completed.is_empty() {
        log::error!(
            "RESULT: no handshake completed, so there is nothing to read. This transcript \
             says nothing about #160 either way."
        );
        log::info!("=========================================================");
        return;
    }

    let min = completed.iter().min().copied().unwrap_or(0);
    let max = completed.iter().max().copied().unwrap_or(0);
    let mean = completed.iter().sum::<u128>() / completed.len() as u128;
    log::info!(
        "  {} of {RUNS} completed: min {min}ms, max {max}ms, mean {mean}ms",
        completed.len()
    );

    // Run 1 is reported separately rather than excluded: downstream saw it come out
    // slowest within a single boot and could not tell warm-up from noise at n=1. Naming
    // the gap is useful; averaging it in silently, or dropping it silently, is not.
    if let Some(first) = totals[0] {
        let rest: std::vec::Vec<u128> = totals[1..].iter().flatten().copied().collect();
        if !rest.is_empty() {
            let rest_mean = rest.iter().sum::<u128>() / rest.len() as u128;
            log::info!(
                "  run 1 was {first}ms against a {rest_mean}ms mean for runs 2-{RUNS}. \
                 One boot cannot separate warm-up from noise — do not build on this gap."
            );
        }
    }

    log::info!(
        "KEY RESULT: the totals above are NOT the answer — the ratio inside each run is. \
         Read the crate's own per-run line: `grep -o '([0-9]* steps, [0-9]*us in esp_tls, \
         [0-9]*us polling)' run.log`. Polling dominant means the 20ms TLS_POLL_INTERVAL is \
         the cost and an adaptive or readiness-driven poll is the fix; esp_tls dominant \
         means the handshake is genuine compute and #160 closes as irreducible at this \
         layer. If the two sums do not add to roughly the reported duration, the time is \
         going somewhere neither counter covers and that is the finding."
    );
    log::info!("=========================================================");
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
