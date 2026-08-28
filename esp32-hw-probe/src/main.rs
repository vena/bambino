//! Current investigation: GitHub issue #161 — does connecting MQTT, FTPS and the camera
//! concurrently actually beat connecting them one at a time, against a real printer?
//!
//! `PrinterClient::connect_all()` interleaves the three dial+TLS sequences on one task. The
//! model says that should cost one peer wait plus three compute terms rather than three of
//! each, because #160 established the peer wait dominates and overlaps freely. The open
//! question is whether the printer agrees: it runs three separate TLS daemons, but on one
//! embedded SoC, so three simultaneous handshakes may contend on *its* CPU and give the
//! overlap back. Phases E and F below measure exactly that, and a null or negative result
//! is a real outcome — see `report_connect`.
//!
//! Everything below this point is issue #160's completed investigation, kept because its
//! phases still run and because its findings are what make #161's prediction falsifiable.
//!
//! ## Issue #160 (closed): is the handshake poll-paced or compute-bound?
//!
//! Round 6 asked whether forcing the ECDH curve onto P-256 recovers the local crypto cost.
//!
//! **The finding this round tests.** A stock ESP-IDF build enables Curve25519 and mbedTLS
//! ranks x25519 first among supported groups, so every handshake negotiated
//! `ECDH curve: x25519` (confirmed in round 5's mbedTLS debug output). The ESP32-C6's ECC
//! accelerator handles P-192/P-256 only — `ecc_alt.c:56` falls back to software for anything
//! else — so the key exchange has been running entirely in software with the hardware idle.
//! That is the prime suspect for the single ~265ms step that dominates local compute.
//!
//! `sdkconfig.defaults` now sets `CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED=n`, leaving
//! secp256r1 at the head of the list, and mbedTLS debug at level 2 so the negotiated curve
//! is visible. **Check the `ECDH curve:` line before reading any timing** — if it still says
//! x25519 the substitution did not happen and the numbers mean nothing.
//!
//! Read `esp_tls` compute rather than the total: it held at 409.4ms and 409.5ms across two
//! earlier rounds (+/-2%), which makes it a far more sensitive instrument than the total,
//! which swings 820-1644ms with the peer. If the ~265ms slowest step collapses to tens of
//! milliseconds, the curve was the cost.
//!
//! Level 2 debug prints ~30 lines per handshake, which is far lighter than the level 3 used
//! in round 5 (that inflated handshakes roughly 3x and made its timings unusable) but is not
//! free. Treat this round as locating the effect; re-run with both debug options off for a
//! clean number.
//!
//! A downstream consumer measured the printer connect path phase by phase and found the
//! TLS handshake to be 94-98% of the interval (1.2-2.4s per handshake). Rounds 1-5 on an
//! ESP32-C6 against a P1S settled the question #160 was opened to ask.
//!
//! **Round 1 (8 runs, 20ms poll interval).** Mean 1415ms: ~409ms inside
//! `esp_tls_low_level_conn`, ~1005ms sleeping, the two summing to the reported duration.
//!
//! **Round 2 (8 runs, 5ms poll interval).** Steps scaled 3.94x, poll time moved 0.4%
//! (1004.8ms -> 1008.9ms). **`TLS_POLL_INTERVAL` is not the cost** — the loop was waiting
//! for a peer that had not answered, not sleeping through one that had. 5ms was in fact
//! marginally *worse*, since ~150 extra calls cost ~26ms of per-call overhead.
//!
//! **Host control.** The same handshake from a laptop on the same LAN (4.7ms RTT) takes
//! 805ms +/- 2%. The printer is slow for everyone, so ~790ms of the wait is the peer.
//! That leaves roughly: ~790ms peer, ~435ms local mbedTLS compute, ~190ms unexplained.
//!
//! **Round 5 (session resumption).** Ruled out, and by protocol observation rather than
//! timing: the printer advertises a 32-byte session ID, issues no ticket, and declines the
//! ID when it is offered back byte for byte — mbedTLS logs "no session has been resumed" and
//! mints a fresh session. See `git log -- esp32-hw-probe/src/main.rs` for that probe, which
//! also documents two client-side traps: esp-tls re-offers the session on every handshake
//! step (fatal under `timeout_ms = 0`), and a non-NULL `esp_tls_get_client_session()` does
//! not mean the peer offered anything resumable.
//!
//! **Wi-Fi power save: tested, and it buys nothing.** ESP-IDF's `WIFI_PS_MIN_MODEM` default
//! was the last standing candidate for the ~190ms this device waits beyond the laptop's
//! ~790ms, on the theory that a dozing station has inbound frames held at the AP until the
//! next DTIM beacon. Measured with `esp_wifi_set_ps(WIFI_PS_NONE)` against the default, back
//! to back in one boot: the difference was not significant against this rig's ~200ms floor.
//! Deliberately not written up in `reference/` — the crate owns no Wi-Fi code and cannot set
//! this, so leaving power save to the consumer is the right default and there is no bambino
//! recommendation to make either way. Recorded here only so it does not get re-run.
//!
//! **Still untested:** `esp_tls_cfg_t.ciphersuites_list` could pin a non-ECDHE suite,
//! dropping the key exchange on both sides, but costs forward secrecy and so would have to
//! be an opt-in rather than a default.
//!
//! **Rounds 3 and 4 (8 runs each, `TCP_NODELAY` on).** Round 3 looked like a win — polling
//! mean 1004.8ms -> 904.8ms, median 1052.6ms -> 842.4ms. Round 4, on *identical* code, came
//! back at 1029.9ms mean / 962.3ms median, i.e. round 1's numbers. **`TCP_NODELAY` has no
//! demonstrated effect on this path**: the spread between two identical configurations
//! (141ms) is larger than the 97ms round 3 appeared to save. It stays in the crate as
//! hygiene — standard for small-message protocols, and MQTT command traffic after the
//! handshake has exactly the shape Nagle penalises — not as a measured improvement.
//!
//! The general lesson for anyone extending this probe: at n=8 against a peer that swings
//! 820-1644ms, only effects larger than ~200ms are visible at all. Do not read a single
//! round's mean or median as signal; run the control configuration twice before believing
//! any change, which is exactly what rounds 3 and 4 accidentally did.
//!
//! **Round 4 also closed the compute accounting**, and the per-step buckets sum to the
//! reported compute total to the microsecond. Local work per handshake is:
//!
//! | part | cost | what it is |
//! |---|---|---|
//! | first step | 26.0ms (+/-0.2 across 8 runs) | SSL setup and trust-store parse |
//! | 1-2 mid steps | 54-57ms each | discrete crypto operations |
//! | one big step | 260-315ms | the burst, at step #30-65 |
//! | `<1ms` polls | 33-72 of them, 1.6-6.0ms total | polls that found nothing |
//!
//! Two things fall out. Per-call overhead is ~0.05ms, so the poll loop costs essentially
//! nothing — a third independent confirmation that `TLS_POLL_INTERVAL` is not the problem.
//! And the chunking is fluid: two 54ms steps pair with a 265ms burst, one 57ms step pairs
//! with a 315ms burst, for a constant ~398ms of client crypto either way. That is the same
//! work arriving in different numbers of records, not different work.
//!
//! **The trust-store parse is 26ms**, not the ~120ms it was hypothesised to be before this
//! round. That kills the idea of trimming the anchor bundle to the one anchor a printer
//! chains to — it could never have saved more than 26ms, and the reliability cost is real:
//! only the P1S has been verified (see `src/io/CLAUDE.md`), the bundle's `BBL CA` plus four
//! `BBL CA2` entries look like an in-progress migration, and issue #145 is precisely the
//! failure where a partial store verifies some models, fails others, and looks identical to
//! a clean handshake in the log. Do not revive this; the prize is small and a firmware
//! update can move a printer onto a chain the trimmed store no longer covers.
//!
//! `src/io/esp_idf.rs`'s handshake loop now counts steps and accumulates the two halves
//! separately, reporting them on its existing summary line:
//!
//! ```text
//! ESP-TLS handshake with <host> completed in 1264ms (63 steps, 4821us in esp_tls, 1259402us polling, slowest step 391204us at #7)
//! ```
//!
//! `slowest step` is what separates one blocking asymmetric operation from the same total
//! spread thinly across every call: a ~400ms maximum means a single ECDHE or RSA op that no
//! poll interval can overlap, a ~10ms maximum means per-call overhead, which is reducible.
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
//! grep -o '([0-9]* steps.*)' run.log
//! ```
//!
//! - **Total drops toward ~1225ms** (peer + compute, with the residual gone) → Nagle was
//!   the residual and `TCP_NODELAY` is the fix. Expect polling time, not compute, to fall.
//! - **Total unchanged at ~1415ms** → the residual is elsewhere: Wi-Fi power save
//!   (`WIFI_PS_MIN_MODEM` is ESP-IDF's default and nothing here overrides it) is the next
//!   suspect, testable with one `esp_wifi_set_ps(WIFI_PS_NONE)` call.
//! - **`slowest step` near ~400ms** → the local compute is one blocking asymmetric
//!   operation, irreducible without hardware acceleration that is already enabled.
//!   **Near ~10ms** → it is spread across calls as per-call overhead, and fewer, larger
//!   steps would recover it.
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
//! retargeting. Only the ~435ms compute term scales with clock; the ~790ms peer term does
//! not, and is why the C6's totals land inside the P4's measured range despite less than
//! half the clock. Do not expect a faster chip to move the dominant term.
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

use bambino::client::PrinterClient;
use bambino::identity::PrinterIdentity;
use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTimer, EspIdfTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector};
use bambino::models::PrinterModel;
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

/// MQTT over TLS. Chosen for the *timing* phase because the handshake is the whole test and
/// this port needs no access code to reach it.
const PRINTER_TLS_PORT: u16 = 8883;

/// Every TLS endpoint this crate dials, for the curve-compatibility phase.
///
/// These are **separate daemons**, not one server on three ports, so a curve the MQTT broker
/// accepts is not automatically a curve vsFTPd accepts. That matters here because
/// `CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED=n` is a global mbedTLS setting: it changes what
/// the ClientHello offers on *every* connection this firmware makes. `reference/02_ftps.md`
/// documents vsFTPd as the component with all of this printer family's TLS eccentricities
/// (P2S mishandling TLS 1.3 session tickets, X2D failing TLS 1.3 handshakes outright), so it
/// is the likeliest place for a curve restriction to bite.
///
/// Camera port is the P1/A1-series binary JPEG socket (`CAMERA_PORT_BINARY_JPEG`); an X1 or
/// H2 would need `CAMERA_PORT_RTSPS` (322) instead — see `src/camera/mod.rs`. Only the TLS
/// handshake is exercised, never the protocol on top, so no access code is needed for any of
/// them.
const TLS_ENDPOINTS: [(&str, u16); 3] = [
    ("MQTT", 8883),
    ("FTPS control (vsFTPd)", 990),
    ("camera binary JPEG", 6000),
];

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

/// Model assumed by the issue #161 connect phases.
///
/// This is not cosmetic. `connect_all()` dials the camera only when the model's quirks
/// report `CameraProtocol::BinaryJpeg`; on an RTSPS model it reports that channel as "not
/// attempted" and the phases below would silently compare two channels instead of three.
/// Change this if the target is not a P1S, and read the guard in `run_connect` — it says so
/// out loud rather than letting a two-channel number be read as a three-channel one.
const PROBE_MODEL: PrinterModel = PrinterModel::P1S;

/// Runs per connect phase. Each run is three full handshakes, so this is 3x as expensive per
/// run as the #160 timing phase. Four is enough here because the predicted effect (~2s) is an
/// order of magnitude above this rig's ~200ms measurement floor — unlike #160's rounds, which
/// were chasing effects near that floor and needed eight.
const CONNECT_RUNS: usize = 4;

/// Outer per-channel connect budget for the #161 phases, in seconds.
///
/// Deliberately far above the ~1.4s a handshake costs. The question these phases ask is
/// whether three concurrent handshakes make each other *slower*; a tight budget would convert
/// exactly that outcome into a `TimedOut` and destroy the measurement instead of recording it.
/// Kept in step with `HANDSHAKE_TIMEOUT`, which bounds the same handshake from inside the
/// connector — the two are structurally independent, per `with_connect_timeout`'s doc comment.
const CONNECT_BUDGET_SECS: u64 = 30;

/// Stack for the thread running the issue #161 phases.
///
/// The main task's 8KB is not enough: a three-channel `PrinterClient` overflowed it by more
/// than 14KB. 64KB is chosen with a wide margin rather than tuned — this is a measurement rig
/// and a second stack-protection panic mid-run would cost another flash cycle to discover.
/// It is not evidence about what a consumer needs; a consumer that builds its client once at
/// startup and does not hold the builder chain live has a different profile entirely.
const CONNECT_PHASE_STACK: usize = 64 * 1024;

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
        *total = run_handshake(PRINTER_TLS_PORT);
        std::thread::sleep(BETWEEN_RUNS);
    }

    report(&totals);

    // Curve compatibility across the printer's three TLS daemons. The timing phase above only
    // proves the MQTT broker accepts what the ClientHello now offers; this proves the other
    // two do too, on this model. It is a pass/fail check, not a measurement — one handshake
    // each, and the durations are ignored.
    log::info!("=== curve compatibility across TLS endpoints ===");
    let mut refused = 0u32;
    for (name, port) in TLS_ENDPOINTS {
        log::info!("--- {name} on port {port} ---");
        if run_handshake(port).is_some() {
            log::info!("PASS {name}: handshake completed");
        } else {
            log::error!("FAIL {name}: handshake did not complete on port {port}");
            refused += 1;
        }
        std::thread::sleep(BETWEEN_RUNS);
    }

    if refused == 0 {
        log::info!(
            "RESULT: all {} TLS endpoints handshook with Curve25519 disabled, so P-256 (or \
             another remaining curve) is acceptable to every daemon on THIS model. One model \
             only — P2S and X2D have documented TLS quirks of their own and are unverified.",
            TLS_ENDPOINTS.len()
        );
    } else {
        log::error!(
            "RESULT: {refused} of {} TLS endpoints refused to handshake. Disabling Curve25519 \
             is NOT safe to recommend: it is a global mbedTLS setting, so it breaks these \
             endpoints for the sake of the MQTT handshake's ~230ms. Check whether the failing \
             daemon needs x25519 specifically before going further.",
            TLS_ENDPOINTS.len()
        );
    }

    // Last, because it is the only phase that authenticates and therefore the only one that
    // can leave real sessions on the printer. Everything above measures handshakes and is
    // unaffected by whether this phase runs at all.
    //
    // On its own thread because it does not fit in the main task's 8KB. Measured: it
    // overflowed with a stack pointer ~14KB below the bounds ESP-IDF reported, and it did so
    // in the *serial* phase, before `connect_all()` ran at all — so this is the cost of a
    // three-channel `PrinterClient`, not a cost of concurrency. `with_ftps`/`with_camera` are
    // consuming builders that each return a differently-typed `PrinterClient` by value, so
    // several copies of a large generic struct can be live at once while the chain is
    // evaluated. Raising `CONFIG_ESP_MAIN_TASK_STACK_SIZE` would work too, but a dedicated
    // stack is what `sdkconfig.defaults` recommends for exactly this, and it keeps the
    // requirement attached to the code that has it rather than to every task in the image.
    match std::thread::Builder::new()
        .stack_size(CONNECT_PHASE_STACK)
        .spawn(run_connect_phases)
    {
        Ok(handle) => {
            if handle.join().is_err() {
                log::error!("issue #161 phases panicked; see the backtrace above");
            }
        }
        Err(e) => log::error!("could not spawn the issue #161 phase thread: {e:?}"),
    }

    park();
}

/// Runs both issue #161 connect phases and reports the serial-vs-concurrent comparison.
///
/// Returns without measuring anything if `PROBE_ACCESS_CODE` is unset: unlike the handshake
/// phases above, these reach past TLS into MQTT/FTPS/camera authentication, so the access
/// code is genuinely required rather than merely nice to have.
fn run_connect_phases() {
    if option_env!("PROBE_ACCESS_CODE").is_none() {
        log::warn!(
            "SKIP issue #161 phases: PROBE_ACCESS_CODE is not set. These phases authenticate \
             on all three channels, so unlike the handshake phases above they cannot run \
             without it. Uncomment it in esp32-hw-probe/.env and reflash."
        );
        return;
    }

    log::info!("=== issue #161: three channels, serial vs concurrent ===");
    log::info!(
        "Each run connects MQTT + FTPS + camera on a fresh client. Phase E does it the way \
         the crate has always done it (one after another); phase F calls connect_all(), which \
         interleaves the three on one task. Both phases run in the same boot, back to back, so \
         the AP, signal and printer state are as close to identical as they can be."
    );

    let mut serial_totals: [Option<u128>; CONNECT_RUNS] = [None; CONNECT_RUNS];
    for (slot, total) in serial_totals.iter_mut().enumerate() {
        log::info!(
            "--- phase E (serial) run {} of {CONNECT_RUNS} ---",
            slot + 1
        );
        *total = run_connect(false);
        std::thread::sleep(BETWEEN_RUNS);
    }

    let mut concurrent_totals: [Option<u128>; CONNECT_RUNS] = [None; CONNECT_RUNS];
    for (slot, total) in concurrent_totals.iter_mut().enumerate() {
        log::info!(
            "--- phase F (connect_all) run {} of {CONNECT_RUNS} ---",
            slot + 1
        );
        *total = run_connect(true);
        std::thread::sleep(BETWEEN_RUNS);
    }

    report_connect(&serial_totals, &concurrent_totals);
}

/// Connects all three channels on a fresh client and returns total wall time in milliseconds.
///
/// `concurrent` selects `connect_all()` over the three sequential `connect_*` calls. Both
/// paths end in the same state, which is the point: the only variable is whether the three
/// dial+TLS sequences overlap.
///
/// Returns `None` unless **all three** channels connected. A run where the camera was refused
/// is not comparable to one where it succeeded — it skipped a whole handshake's worth of work
/// and would look like a win. Partial outcomes are logged in full before being discarded,
/// because a channel that fails only under concurrency is itself the finding.
///
/// The client is rebuilt every run for the same reason `run_handshake` rebuilds its connector:
/// every run is a from-scratch connect exactly as a consumer performs it.
fn run_connect(concurrent: bool) -> Option<u128> {
    // Checked per run rather than once, so the transcript carries the reason next to the
    // numbers it invalidates. On an RTSPS model connect_all() skips the camera entirely and
    // the two phases would be comparing two channels, not three.
    if PROBE_MODEL.quirks().camera_protocol() != bambino::camera::CameraProtocol::BinaryJpeg {
        log::error!(
            "PROBE_MODEL {PROBE_MODEL:?} uses RTSPS for its camera, so connect_all() will not \
             dial a camera channel and this comparison covers two channels, not three. Set \
             PROBE_MODEL to the actual target before reading these numbers."
        );
    }

    let certs: std::vec::Vec<std::vec::Vec<u8>> =
        BBL_ANCHORS.iter().map(|anchor| anchor.to_vec()).collect();
    let connector = || {
        EspIdfTlsConnector::with_certs(certs.clone(), None).with_connect_timeout(HANDSHAKE_TIMEOUT)
    };

    // Three independent connectors and three independent timers, matching how a real consumer
    // wires this up: `with_ftps`/`with_camera` take their own TLS connector precisely because
    // some models need different TLS settings per channel.
    let (timer, ftps_timer) = match (EspIdfTimer::new(), EspIdfTimer::new()) {
        (Ok(timer), Ok(ftps_timer)) => (timer, ftps_timer),
        _ => {
            log::error!("    EspIdfTimer::new() failed; no measurement from this run");
            return None;
        }
    };

    let mut client = PrinterClient::new(
        connector(),
        EspIdfRawStreamFactory,
        PrinterIdentity {
            ip: PRINTER_IP.into(),
            serial: PRINTER_SERIAL.into(),
            access_code: option_env!("PROBE_ACCESS_CODE").unwrap_or_default().into(),
            model: PROBE_MODEL,
        },
    )
    .with_timer(timer)
    .with_connect_timeout(CONNECT_BUDGET_SECS)
    .with_ftps(connector(), EspIdfRawStreamFactory, ftps_timer)
    .with_camera(connector(), EspIdfRawStreamFactory);

    esp_idf_svc::hal::task::block_on(async {
        let start = Instant::now();

        let (mqtt, ftps, camera) = if concurrent {
            let outcome = client.connect_all().await;
            (outcome.mqtt, outcome.ftps, outcome.camera)
        } else {
            // Sequential on purpose, each awaited to completion before the next starts —
            // this is the baseline the crate has always had, not a strawman.
            let mqtt = Some(client.connect_mqtt().await);
            let ftps = Some(client.connect_ftps().await);
            let camera = Some(client.connect_camera().await);
            (mqtt, ftps, camera)
        };

        let elapsed = start.elapsed().as_millis();

        for (name, result) in [("mqtt", &mqtt), ("ftps", &ftps), ("camera", &camera)] {
            match result {
                Some(Ok(())) => log::info!("    {name}: connected"),
                Some(Err(e)) => log::error!("    {name}: FAILED {e:?}"),
                None => log::warn!("    {name}: not attempted"),
            }
        }

        let all_up = matches!(
            (&mqtt, &ftps, &camera),
            (Some(Ok(())), Some(Ok(())), Some(Ok(())))
        );
        if !all_up {
            log::error!(
                "    -> no measurement from this run: a run that did not bring up all three \
                 channels did less work than one that did, and averaging it in would read as \
                 a speed-up. If this only happens in phase F, the printer is refusing \
                 concurrent connections and THAT is the result."
            );
            return None;
        }

        log::info!("    all three channels up in {elapsed}ms");
        Some(elapsed)
    })
}

/// Prints the issue #161 serial-vs-concurrent comparison.
fn report_connect(serial: &[Option<u128>], concurrent: &[Option<u128>]) {
    log::info!("================ issue #161 connect summary ================");

    let mean = |runs: &[Option<u128>]| -> Option<u128> {
        let completed: std::vec::Vec<u128> = runs.iter().flatten().copied().collect();
        match completed.is_empty() {
            true => None,
            false => Some(completed.iter().sum::<u128>() / completed.len() as u128),
        }
    };

    for (label, runs) in [
        ("phase E serial", serial),
        ("phase F connect_all", concurrent),
    ] {
        for (slot, total) in runs.iter().enumerate() {
            match total {
                Some(ms) => log::info!("  {label} run {}: {ms}ms", slot + 1),
                None => log::info!("  {label} run {}: no measurement", slot + 1),
            }
        }
    }

    let (Some(serial_mean), Some(concurrent_mean)) = (mean(serial), mean(concurrent)) else {
        log::error!(
            "RESULT: at least one phase produced no complete run, so there is nothing to \
             compare. Check the per-channel lines above: if phase F alone is empty, the \
             printer refused concurrent connections and #161 should be closed as measured."
        );
        log::info!("===========================================================");
        return;
    };

    log::info!("  phase E (serial) mean:      {serial_mean}ms");
    log::info!("  phase F (connect_all) mean: {concurrent_mean}ms");

    if concurrent_mean < serial_mean {
        log::info!("  saved {}ms per connect", serial_mean - concurrent_mean);
    } else {
        log::warn!(
            "  concurrent was {}ms SLOWER",
            concurrent_mean - serial_mean
        );
    }

    log::info!(
        "KEY RESULT: the means above are only half of it — read the crate's own per-handshake \
         lines in both phases before concluding anything. The model predicts phase F costs one \
         peer wait plus three compute terms instead of three of each. If the totals improved \
         but each individual handshake got slower, the printer is contending on its own CPU \
         and the win will not hold on a busier machine. If individual handshake times are \
         unchanged and only the total dropped, the overlap is real and free. A phase F that is \
         slower, or that fails channels phase E connects, closes #161 as measured-and-rejected \
         rather than landing it — that is a legitimate outcome, not a failed run."
    );
    log::info!("===========================================================");
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
fn run_handshake(port: u16) -> Option<u128> {
    let certs: std::vec::Vec<std::vec::Vec<u8>> =
        BBL_ANCHORS.iter().map(|anchor| anchor.to_vec()).collect();

    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    esp_idf_svc::hal::task::block_on(async {
        let dial_start = Instant::now();
        let raw = match EspIdfRawStreamFactory.dial(PRINTER_IP, port).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("    TCP dial to {PRINTER_IP}:{port} failed: {e:?}");
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
         Read the crate's own per-run line: `grep -o '([0-9]* steps.*)' run.log`. \
         Polling dominant means the 20ms TLS_POLL_INTERVAL is \
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
