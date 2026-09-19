//! Current investigation: GitHub issue #294 -- does `EspIdfTlsConnector::connect`'s handshake
//! poll loop actually run on ESP-IDF v5.5.5, now that `Config::timeout_ms` is pinned to `1`
//! instead of `0`?
//!
//! **Background.** `connect` relies on `esp_tls_conn_new_sync` returning between handshake
//! steps so that `TLS_POLL_INTERVAL`, the `connect_timeout` deadline and the task's yield point
//! are evaluated at all. That return is gated on `ret == 0 && cfg->timeout_ms <op> 0`, and
//! ESP-IDF changed both halves of that branch inside the 5.5 series (read off the provisioned
//! checkouts):
//!
//! | ESP-IDF | `<op>` | value returned on expiry |
//! |---|---|---|
//! | v5.5.3 / v5.5.4 / v6.0.1 | `>= 0` | `0`, reaching Rust as `EWOULDBLOCK` |
//! | v5.5.5 | `> 0` | `-1`, reaching Rust as the same opaque `ESP_FAIL` a real failure does |
//!
//! So the previous `timeout_ms = 0` disabled the bound outright on v5.5.5: the test is never
//! reached, `while (1)` runs the whole handshake, and the calling task blocks uninterruptibly
//! for its full duration (measured elsewhere: 113 of 113 handshakes reporting `1 steps` with
//! `0us polling`, worst case 33.8s, tripping the Task Watchdog ~30 times per unattended run).
//!
//! The fix sets `timeout_ms = 1`, which satisfies both comparisons, and adds
//! `take_esp_tls_error` so v5.5.5's `-1`-plus-`ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT` is
//! recognised as retryable rather than fatal. Both halves are needed: `timeout_ms = 1` alone
//! would turn the unbounded block into a hard `connect` failure ~1ms in.
//!
//! **What "pass" looks like**, with `.cargo/config.toml` pinned to `ESP_IDF_VERSION = "v5.5.5"`:
//!
//! 1. Every handshake **completes**. A failure here means `take_esp_tls_error` is not
//!    classifying v5.5.5's expiry `-1` as retryable, and the poll loop is aborting on it.
//! 2. Every handshake reports **`steps` > 1** and **non-zero `us polling`** in bambino's
//!    `ESP-TLS handshake with ... completed in ...` debug line. `1 steps, 0us polling` is the
//!    exact signature of the bug: it means `negotiate()` ran the whole handshake internally
//!    and the poll loop never executed.
//! 3. No handshake's wall time is dominated by a single uninterruptible block, and the Task
//!    Watchdog does not fire.
//!
//! Only the MQTT TLS port is exercised; no access code is needed, since the handshake
//! completes before MQTT authentication.
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
//! Prior investigations (issue #168's unverified-handshake probe, #160/#161's handshake-timing
//! and concurrent-connect probes, #157's certificate-failure probe, #145's multi-anchor bundle
//! probe, #65's concurrent-sleep probe) are recoverable via
//! `git log -- esp32-hw-probe/src/main.rs`, not kept live here -- see this directory's
//! `CLAUDE.md` for the reuse convention this follows.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use core::time::Duration;
use std::time::Instant;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
/// Passed to `TlsConnector::connect` as the TLS hostname, mirroring `src/client/connect.rs`.
/// Irrelevant to what this probe measures, but kept so a wire capture of this run looks like a
/// real client's, not a synthetic one -- and `EspIdfTlsConnector::connect`'s log lines redact
/// and report it either way.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

/// MQTT over TLS -- the same port `PrinterClient` dials, and reachable without an access code
/// since only the handshake (not MQTT authentication) is exercised.
const PRINTER_TLS_PORT: u16 = 8883;

/// Enough handshakes that a single lucky fast one cannot pass for a fix. The reference capture
/// this is compared against ran 113 handshakes and hit `1 steps` on every one, so the failure
/// mode is not intermittent -- the point of repeating is to catch the *other* direction, a
/// pathology that shows up only on a slow handshake.
const RUNS: usize = 25;

/// Spacing between handshakes. Matches the shorter of the two unattended runs the issue's
/// numbers come from, and keeps the printer from treating the loop as a reconnect storm.
const BETWEEN_RUNS: Duration = Duration::from_secs(3);

/// Generous enough that a slow-but-succeeding handshake still reports a clean result rather
/// than a `TimedOut` that would be misread as this fix not working. #160's probe measured
/// 1.7-4.0s as the normal range on this class of hardware.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Wall time past which a handshake is counted as slow. The issue's metric: 22.7% of v5.5.5
/// handshakes landed past this, against a normal range of 1.7-4.0s.
const SLOW_THRESHOLD: Duration = Duration::from_secs(2);

fn main() {
    esp_idf_svc::sys::link_patches();

    // `connect`'s per-handshake breakdown (`steps`, `us in esp_tls`, `us polling`) is logged at
    // debug level, and it is the whole instrument for this round -- `steps` is what separates
    // "the poll loop ran" from "one `negotiate()` call swallowed the entire handshake".
    // `CONFIG_LOG_MAXIMUM_LEVEL_DEBUG=y` in `sdkconfig.defaults` raises the compile-time
    // ceiling; this raises the runtime level for that one target, leaving ESP-IDF's own
    // components at their defaults rather than flooding the transcript.
    let logger = esp_idf_svc::log::init_from_esp_idf();
    if let Err(e) = logger
        .filter()
        .set_target_level("bambino::io::esp_idf", log::LevelFilter::Debug)
    {
        log::warn!(
            "could not raise bambino::io::esp_idf to debug: {e:?} -- `steps` counts will be \
             missing from this run"
        );
    }

    log::info!("esp32-hw-probe: issue #294 handshake step-bound check on ESP-IDF v5.5.5");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT} (TLS hostname {PRINTER_SERIAL})");
    log::info!("expecting every handshake to report steps > 1 and non-zero us polling");

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
    // handshake attempts below would fail for reasons unrelated to what this probe is testing.
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

/// Runs `RUNS` sequential handshakes, reporting each one's wall time and a summary at the end.
///
/// Wall time is measured here rather than read off `connect`'s own log line because the two
/// answer different questions: `connect` reports how the time inside it split between compute
/// and polling, while this measures what the *caller* experienced. Under the bug those are the
/// same number, which is itself the tell -- a handshake with no polling in it blocked its task
/// for the full duration.
fn run_probe() {
    let connector = EspIdfTlsConnector::new().with_connect_timeout(HANDSHAKE_TIMEOUT);

    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut slow = 0usize;
    let mut worst = Duration::ZERO;

    for run in 1..=RUNS {
        esp_idf_svc::hal::task::block_on(async {
            let raw = match EspIdfRawStreamFactory
                .dial(PRINTER_IP, PRINTER_TLS_PORT)
                .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    log::error!(
                        "run {run}/{RUNS} FAIL: TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} \
                         failed: {e:?} -- nothing about issue #294 can be measured if the \
                         printer isn't reachable"
                    );
                    failed += 1;
                    return;
                }
            };

            let started = Instant::now();
            let result = connector.connect(PRINTER_SERIAL, raw).await;
            let elapsed = started.elapsed();

            if elapsed > worst {
                worst = elapsed;
            }
            if elapsed > SLOW_THRESHOLD {
                slow += 1;
            }

            match result {
                Ok(stream) => {
                    completed += 1;
                    log::info!(
                        "run {run}/{RUNS} OK in {}ms, negotiated {:?} \
                         (read `steps` off the debug line above: >1 passes, 1 is the bug)",
                        elapsed.as_millis(),
                        connector.negotiated_version(&stream)
                    );
                }
                Err(e) => {
                    failed += 1;
                    log::error!(
                        "run {run}/{RUNS} FAIL after {}ms: {e:?} -- if this is a bare \
                         SocketError::Other/ConnectionRefused rather than TimedOut, the most \
                         likely cause is `take_esp_tls_error` not classifying v5.5.5's expiry \
                         `-1` (ESP_ERR_ESP_TLS_CONNECTION_TIMEOUT on the error handle) as \
                         retryable, so the poll loop aborts on its own pacing signal",
                        elapsed.as_millis()
                    );
                }
            }
        });

        if run < RUNS {
            std::thread::sleep(BETWEEN_RUNS);
        }
    }

    log::info!(
        "RESULT: {completed}/{RUNS} completed, {failed} failed, {slow} slower than {}s, worst {}ms",
        SLOW_THRESHOLD.as_secs(),
        worst.as_millis()
    );
    log::info!(
        "RESULT: pass requires {RUNS}/{RUNS} completed AND every `completed in ...` line above \
         showing steps > 1 with non-zero us polling. `1 steps, 0us polling` on any line means \
         the poll loop did not run and issue #294 is not fixed on this build."
    );
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
