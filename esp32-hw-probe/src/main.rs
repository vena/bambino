//! Current investigation: GitHub issue #385 -- does a real out-of-memory failure reach the
//! caller as `SocketError::ResourceExhausted` on ESP-IDF?
//!
//! The mapping (mbedTLS `*_ALLOC_FAILED`, `ESP_ERR_NO_MEM`, `ENOMEM`) is host-tested. What only
//! hardware shows is that a real shortage takes those paths end to end. Which allocation fails
//! first depends on how much memory is left, so this probe sweeps it:
//!
//! 1. Hold "ballast" in internal RAM until exactly N bytes remain free. mbedTLS allocates from
//!    the same pool (`CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC=y`; the C6 has no PSRAM).
//! 2. Dial the printer and handshake with all five BBL anchors; record where it failed and how.
//! 3. Free the ballast (memory is back to normal) and step N down.
//!
//! A coarse sweep (`COARSE_STEP`) runs first; then every gap between two coarse levels whose
//! outcomes differ is re-swept at `FINE_STEP`, so each boundary is found without hand-tuning.
//! The sweep ends early once `FLOOR_RUN` levels in a row fail at the dial: below that the
//! network stack itself is starved and nothing reaches TLS.
//!
//! **Self-recovery.** Starving the heap can crash Wi-Fi or trip a watchdog. Progress lives in
//! NVS: each level is marked pending before its attempt and recorded after. After a reboot the
//! pending level is recorded as `Crashed(<reset reason>)` and the sweep resumes. A new build
//! (different ELF SHA-256) starts a fresh sweep; re-flashing or resetting after a finished
//! sweep also starts fresh.
//!
//! **Reading the result.** Per level the summary shows bytes left, the largest free block (a
//! TLS input buffer needs ~16 KB contiguous), the stage, and the outcome.
//!
//! - Expected, pass: `Ok`; `ResourceExhausted` at dial (timer, `ENOMEM`) or handshake (TLS
//!   object, mbedTLS allocation, `ESP_ERR_NO_MEM` record); `IncompleteTrustStore` (the trust
//!   store lost the P1S anchor to a real allocation failure -- #384 under real pressure).
//! - **Fail:** `Other(..)` whose debug line above carries an allocation code (`mbedtls
//!   Some(-10368)` and friends, or `esp_tls Some(257)`) -- a memory failure that missed the
//!   mapping. `UntrustedAnchor` -- the store ran short and #384's check did not notice.
//! - Noted, not a bambino verdict: `TimedOut`/`ConnectionReset`/`ConnectionAborted` (lwIP or
//!   Wi-Fi starved of buffers mid-handshake), `Crashed(..)` (ESP-IDF itself fell over).
//!
//! Reads and writes after the handshake are not covered: without `CONFIG_MBEDTLS_DYNAMIC_BUFFER`
//! mbedTLS allocates its record buffers during setup, so a connected session does not allocate.
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee 385-test1.log
//! ```
//!
//! Prior investigations (#384/#386's trust-store and code-sign check, #294's step-bound check,
//! and others) are recoverable via `git log -- esp32-hw-probe/src/main.rs`.

use bambino::io::esp_idf::{EspIdfRawStreamFactory, EspIdfTlsConnector};
use bambino::io::{CertificateFailure, RawStreamFactory, SocketError, TlsConnector};
use core::ffi::c_void;
use core::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::sys;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// The five BambuStudio trust anchors; all are supplied, so the control handshake succeeds.
const BBL_ANCHORS: [&[u8]; 5] = [
    include_bytes!("../certs/bbl_1.der"),
    include_bytes!("../certs/bbl_2.der"),
    include_bytes!("../certs/bbl_3.der"),
    include_bytes!("../certs/bbl_4.der"),
    include_bytes!("../certs/bbl_5.der"), // CN=BBL CA, the P1S anchor
];

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");
const PRINTER_TLS_PORT: u16 = 8883;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Internal, byte-addressable RAM: the pool mbedTLS, lwIP and Wi-Fi allocate from.
const CAPS: u32 = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;

/// Highest level swept. A handshake needs roughly 16 KB in + 4 KB out record buffers plus
/// context and trust store; this leaves room for a clean success at the top.
const START_LEFT: u32 = 80 * 1024;
const COARSE_STEP: u32 = 2 * 1024;
const FINE_STEP: u32 = 256;
/// Consecutive dial-stage failures that end the coarse sweep.
const FLOOR_RUN: usize = 3;
/// Largest single ballast allocation; big enough to fill quickly, small enough that the
/// allocator can place it.
const BALLAST_CHUNK: usize = 16 * 1024;
/// Bytes of slack when topping up: below this the per-allocation header dominates.
const BALLAST_MIN: usize = 32;
/// Spacing between attempts so the printer does not treat the sweep as a reconnect storm.
const BETWEEN_ATTEMPTS: Duration = Duration::from_secs(3);

const NVS_NAMESPACE: &str = "oomsweep";
const KEY_BUILD: &str = "build";
const KEY_PENDING: &str = "pending";
const KEY_RESULTS: &str = "results";
const KEY_DONE: &str = "done";
/// The sweep's top level. Stored because it is derived from free memory at boot, which varies
/// between boots: recomputing it after a crash-reboot would shift the whole grid.
const KEY_START: &str = "start";
/// One record: bytes left (u32), largest free block (u32), stage (u8), outcome (u8).
const RECORD_LEN: usize = 10;
const MAX_RECORDS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Stage {
    Dial = 0,
    Handshake = 1,
    /// Rebooted mid-attempt; the outcome byte holds the reset reason.
    Reboot = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Outcome {
    Ok = 0,
    ResourceExhausted = 1,
    IncompleteTrustStore = 2,
    UntrustedAnchor = 3,
    OtherCert = 4,
    TimedOut = 5,
    ConnectionReset = 6,
    ConnectionAborted = 7,
    Other = 8,
    OtherSocket = 9,
}

impl Outcome {
    fn from_u8(v: u8) -> Option<Self> {
        use Outcome::*;
        [
            Ok,
            ResourceExhausted,
            IncompleteTrustStore,
            UntrustedAnchor,
            OtherCert,
            TimedOut,
            ConnectionReset,
            ConnectionAborted,
            Other,
            OtherSocket,
        ]
        .into_iter()
        .find(|o| *o as u8 == v)
    }

    fn of(err: &SocketError) -> Self {
        match err {
            SocketError::ResourceExhausted => Outcome::ResourceExhausted,
            SocketError::CertificateInvalid(CertificateFailure::IncompleteTrustStore) => {
                Outcome::IncompleteTrustStore
            }
            SocketError::CertificateInvalid(CertificateFailure::UntrustedAnchor) => {
                Outcome::UntrustedAnchor
            }
            SocketError::CertificateInvalid(_) => Outcome::OtherCert,
            SocketError::TimedOut => Outcome::TimedOut,
            SocketError::ConnectionReset => Outcome::ConnectionReset,
            SocketError::ConnectionAborted => Outcome::ConnectionAborted,
            SocketError::Other(_) => Outcome::Other,
            _ => Outcome::OtherSocket,
        }
    }

    /// Verdict for the summary: `Some(true)` pass, `Some(false)` fail, `None` noted only.
    fn verdict(self) -> Option<bool> {
        match self {
            Outcome::Ok | Outcome::ResourceExhausted | Outcome::IncompleteTrustStore => Some(true),
            Outcome::UntrustedAnchor | Outcome::OtherCert => Some(false),
            // Includes `Other`, which is only a failure if it hid an allocation code: its debug
            // line decides.
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Record {
    left: u32,
    largest: u32,
    stage: Stage,
    outcome: u8,
}

impl Record {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.left.to_le_bytes());
        out.extend_from_slice(&self.largest.to_le_bytes());
        out.push(self.stage as u8);
        out.push(self.outcome);
    }

    fn decode(b: &[u8]) -> Option<Self> {
        let stage = match b[8] {
            0 => Stage::Dial,
            1 => Stage::Handshake,
            2 => Stage::Reboot,
            _ => return None,
        };
        Some(Self {
            left: u32::from_le_bytes(b[0..4].try_into().ok()?),
            largest: u32::from_le_bytes(b[4..8].try_into().ok()?),
            stage,
            outcome: b[9],
        })
    }

    /// The part two neighbouring levels must share to count as "the same result".
    fn kind(&self) -> (Stage, u8) {
        (self.stage, self.outcome)
    }

    fn describe(&self) -> String {
        match self.stage {
            Stage::Reboot => format!("Crashed(reset reason {})", self.outcome),
            stage => match Outcome::from_u8(self.outcome) {
                Some(o) => format!("{stage:?}: {o:?}"),
                None => format!("{stage:?}: unknown outcome {}", self.outcome),
            },
        }
    }
}

fn main() {
    sys::link_patches();

    // `map_esp_tls_connect_error`'s `(esp_tls .., mbedtls ..)` codes are logged at debug; they
    // are what decides whether an `Other` hid an allocation failure.
    // `bambino::io`, not just `::esp_idf`: `map_std_io_error` logs the raw errno behind an
    // `Other("ESP-IDF platform BSD network error")` there, and the first run could not say
    // which errno that was.
    let logger = esp_idf_svc::log::init_from_esp_idf();
    if let Err(e) = logger
        .filter()
        .set_target_level("bambino::io", log::LevelFilter::Debug)
    {
        log::warn!("could not raise bambino::io to debug: {e:?}");
    }

    log::info!("esp32-hw-probe: issue #385 out-of-memory sweep");

    let peripherals = Peripherals::take().unwrap_or_else(|e| fail_setup("Peripherals", e));
    let sysloop = EspSystemEventLoop::take().unwrap_or_else(|e| fail_setup("event loop", e));
    let nvs_part = EspDefaultNvsPartition::take().unwrap_or_else(|e| fail_setup("NVS", e));
    let nvs = EspNvs::new(nvs_part.clone(), NVS_NAMESPACE, true)
        .unwrap_or_else(|e| fail_setup("NVS namespace", e));

    let mut results = load_state(&nvs);

    let mut wifi = match connect_wifi(peripherals.modem, sysloop, nvs_part) {
        Ok(wifi) => wifi,
        Err(e) => fail_setup("Wi-Fi", e),
    };

    // SAFETY: plain heap queries.
    let (free, largest) = unsafe {
        (
            sys::heap_caps_get_free_size(CAPS),
            sys::heap_caps_get_largest_free_block(CAPS),
        )
    };
    log::info!("baseline: {free} bytes free, largest block {largest}");

    let start = match nvs.get_u32(KEY_START) {
        Ok(Some(start)) if !results.is_empty() => start,
        _ => {
            let start = START_LEFT.min((free as u32).saturating_sub(4 * 1024));
            if let Err(e) = nvs.set_u32(KEY_START, start) {
                log::warn!("could not store the sweep start: {e:?}");
            }
            start
        }
    };
    log::info!("sweeping down from {start} bytes left");
    let mut ballast: Vec<*mut c_void> = Vec::with_capacity(512);

    // Coarse sweep, then fine sweeps between differing neighbours. The plan is recomputed from
    // the stored results each time, so a reboot mid-sweep picks up where it left off.
    loop {
        let Some(level) = next_level(&results, start) else {
            break;
        };
        if results.len() >= MAX_RECORDS {
            log::warn!("record limit reached; stopping early");
            break;
        }

        ensure_wifi(&mut wifi);
        if let Err(e) = nvs.set_u32(KEY_PENDING, level) {
            log::warn!("could not mark level {level} pending: {e:?}");
        }

        let record = attempt(level, &mut ballast);
        log::info!(
            "  left {:>6}  largest {:>6}  {}",
            record.left,
            record.largest,
            record.describe()
        );
        results.push(record);
        save_results(&nvs, &results);
        let _ = nvs.remove(KEY_PENDING);

        std::thread::sleep(BETWEEN_ATTEMPTS);
    }

    let _ = nvs.set_u8(KEY_DONE, 1);
    print_summary(&results);
    park();
}

/// Loads a sweep in progress, or starts a fresh one for a new build or after a finished run.
fn load_state(nvs: &EspNvs<NvsDefault>) -> Vec<Record> {
    // SAFETY: `esp_app_get_description` returns a pointer to static app metadata.
    let build = unsafe { (*sys::esp_app_get_description()).app_elf_sha256 };
    let mut buf = [0u8; 32];
    let same_build = matches!(nvs.get_blob(KEY_BUILD, &mut buf), Ok(Some(b)) if b == build);
    let finished = matches!(nvs.get_u8(KEY_DONE), Ok(Some(1)));

    if !same_build || finished {
        log::info!(
            "starting a fresh sweep ({})",
            if same_build {
                "previous sweep finished"
            } else {
                "new build"
            }
        );
        let _ = nvs.remove(KEY_RESULTS);
        let _ = nvs.remove(KEY_PENDING);
        let _ = nvs.remove(KEY_DONE);
        if let Err(e) = nvs.set_blob(KEY_BUILD, &build) {
            log::warn!("could not store build id: {e:?} -- a reboot will restart the sweep");
        }
        return Vec::new();
    }

    let mut blob = vec![0u8; MAX_RECORDS * RECORD_LEN];
    let mut results: Vec<Record> = match nvs.get_blob(KEY_RESULTS, &mut blob) {
        Ok(Some(b)) => b
            .chunks_exact(RECORD_LEN)
            .filter_map(Record::decode)
            .collect(),
        _ => Vec::new(),
    };

    if let Ok(Some(level)) = nvs.get_u32(KEY_PENDING) {
        // SAFETY: reads the reset reason recorded by the ROM/bootloader.
        let reason = unsafe { sys::esp_reset_reason() };
        log::warn!(
            "resuming after a reboot during level {level} (reset reason {reason}); recording \
             it as Crashed"
        );
        results.push(Record {
            left: level,
            largest: 0,
            stage: Stage::Reboot,
            outcome: reason as u8,
        });
        save_results(nvs, &results);
        let _ = nvs.remove(KEY_PENDING);
    } else {
        log::info!("resuming a sweep with {} level(s) done", results.len());
    }
    results
}

fn save_results(nvs: &EspNvs<NvsDefault>, results: &[Record]) {
    let mut blob = Vec::with_capacity(results.len() * RECORD_LEN);
    for r in results {
        r.encode(&mut blob);
    }
    if let Err(e) = nvs.set_blob(KEY_RESULTS, &blob) {
        log::warn!("could not save results: {e:?} -- a reboot would lose this progress");
    }
}

/// The next level to try, or `None` when the sweep is complete.
fn next_level(results: &[Record], start: u32) -> Option<u32> {
    let done = |level: u32| results.iter().any(|r| r.left == level);

    // Coarse phase: walk down until done, or until the floor is reached.
    let mut coarse: Vec<&Record> = results
        .iter()
        .filter(|r| (start - r.left.min(start)) % COARSE_STEP == 0)
        .collect();
    coarse.sort_by(|a, b| b.left.cmp(&a.left));
    let floor_reached = coarse.len() >= FLOOR_RUN
        && coarse[coarse.len() - FLOOR_RUN..]
            .iter()
            .all(|r| r.stage == Stage::Dial);
    if !floor_reached {
        let mut level = start;
        loop {
            if !done(level) {
                return Some(level);
            }
            if level < COARSE_STEP {
                break;
            }
            level -= COARSE_STEP;
        }
    }

    // Fine phase: between each pair of adjacent coarse levels whose results differ.
    for pair in coarse.windows(2) {
        let (hi, lo) = (pair[0], pair[1]);
        if hi.kind() == lo.kind() {
            continue;
        }
        let mut level = hi.left - FINE_STEP;
        while level > lo.left {
            if !done(level) {
                return Some(level);
            }
            level -= FINE_STEP;
        }
    }
    None
}

/// Runs one attempt with `left` bytes of internal RAM free. Ballast is always released
/// before returning, so the caller's NVS writes and Wi-Fi have their memory back.
fn attempt(left: u32, ballast: &mut Vec<*mut c_void>) -> Record {
    // Built before the ballast so its own allocations don't count against `left`.
    let certs: Vec<Vec<u8>> = BBL_ANCHORS.iter().map(|a| a.to_vec()).collect();
    let connector =
        EspIdfTlsConnector::with_certs(certs, None).with_connect_timeout(HANDSHAKE_TIMEOUT);

    fill_to(left as usize, ballast);
    // SAFETY: plain heap queries.
    let (free, largest) = unsafe {
        (
            sys::heap_caps_get_free_size(CAPS),
            sys::heap_caps_get_largest_free_block(CAPS),
        )
    };

    let (stage, outcome) = esp_idf_svc::hal::task::block_on(async {
        let raw = match EspIdfRawStreamFactory
            .dial(PRINTER_IP, PRINTER_TLS_PORT)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                log::info!("    dial failed: {e:?}");
                return (Stage::Dial, Outcome::of(&e));
            }
        };
        match connector.connect(PRINTER_SERIAL, raw).await {
            Ok(_stream) => (Stage::Handshake, Outcome::Ok),
            Err(e) => {
                log::info!("    handshake failed: {e:?}");
                (Stage::Handshake, Outcome::of(&e))
            }
        }
    });

    release(ballast);
    if (free as u32).abs_diff(left) > 1024 {
        log::warn!("    ballast landed at {free} free, not {left}");
    }
    Record {
        left,
        largest: largest as u32,
        stage,
        outcome: outcome as u8,
    }
}

/// Allocates ballast until at most `left` bytes of internal RAM are free, taking the largest
/// block first so the memory that remains is as contiguous as the heap allows.
fn fill_to(left: usize, ballast: &mut Vec<*mut c_void>) {
    loop {
        // SAFETY: plain heap queries and an allocation whose pointer is kept for `release`.
        let (free, largest) = unsafe {
            (
                sys::heap_caps_get_free_size(CAPS),
                sys::heap_caps_get_largest_free_block(CAPS),
            )
        };
        if free <= left || ballast.len() == ballast.capacity() {
            return;
        }
        let size = (free - left).min(largest).min(BALLAST_CHUNK);
        if size < BALLAST_MIN {
            return;
        }
        let p = unsafe { sys::heap_caps_malloc(size, CAPS) };
        if p.is_null() {
            return;
        }
        ballast.push(p);
    }
}

fn release(ballast: &mut Vec<*mut c_void>) {
    for p in ballast.drain(..) {
        // SAFETY: each pointer came from `heap_caps_malloc` and is freed exactly once.
        unsafe { sys::heap_caps_free(p) };
    }
}

/// Reconnects Wi-Fi if a previous attempt starved it; restarts the board if that fails, which
/// the NVS state turns into a resumed sweep rather than a lost one.
fn ensure_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) {
    if matches!(wifi.is_up(), Ok(true)) {
        return;
    }
    log::warn!("Wi-Fi is down after the last attempt; reconnecting");
    if wifi.connect().and_then(|()| wifi.wait_netif_up()).is_err() {
        log::error!("Wi-Fi reconnect failed; restarting to resume the sweep");
        unsafe { sys::esp_restart() };
    }
}

fn print_summary(results: &[Record]) {
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| b.left.cmp(&a.left));

    let (mut pass, mut fail, mut noted) = (0, 0, 0);
    log::info!("================ issue #385 out-of-memory sweep ================");
    log::info!("  {:>6}  {:>7}  result", "left", "largest");
    for r in &sorted {
        let verdict = match r.stage {
            Stage::Reboot => None,
            _ => Outcome::from_u8(r.outcome).and_then(Outcome::verdict),
        };
        let tag = match verdict {
            Some(true) => {
                pass += 1;
                "pass "
            }
            Some(false) => {
                fail += 1;
                "FAIL "
            }
            None => {
                noted += 1;
                "note "
            }
        };
        log::info!("  {:>6}  {:>7}  {tag}{}", r.left, r.largest, r.describe());
    }
    log::info!(
        "RESULT: {pass} pass, {fail} FAIL, {noted} noted. For each `note ... Other`, read its \
         `ESP-IDF TLS handshake failed: ... (esp_tls .., mbedtls ..)` debug line: an allocation \
         code there is a FAIL. Routes reached are the distinct Dial/Handshake ResourceExhausted \
         rows; match each to the ESP-IDF error line printed just before it."
    );
    log::info!("================================================================");
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
