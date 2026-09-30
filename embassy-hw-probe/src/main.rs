//! Current investigation: GitHub issue #385 on the **embassy** backend -- does a real
//! out-of-memory failure reach the caller as `SocketError::ResourceExhausted`?
//!
//! `EmbassyTlsConnector` now maps an mbedTLS allocation failure from `Session::new`,
//! `set_server_name`, the handshake and `close` to `ResourceExhausted`, and returns an
//! `EmbassyTlsStream` wrapper so reads and writes keep the mbedTLS code too. The mapping is
//! host-tested; this probe checks a real shortage takes that path on the chip.
//!
//! Same sweep as `esp32-hw-probe`'s #385 probe: hold ballast on the esp-alloc heap until N
//! bytes remain free, dial the printer and handshake, release the ballast, step N down. The
//! heap is shared: mbedTLS's `calloc` resolves to esp-alloc's (`esp-alloc` `malloc.rs`), as do
//! esp-radio's allocations. Coarse `COARSE_STEP` levels first, then `FINE_STEP` levels between
//! any two coarse neighbours whose outcomes differ.
//!
//! **Differences from the ESP-IDF probe, all forced by the platform:**
//!
//! - The dial cannot run out of memory: `EmbassyRawStreamFactory` hands out statically
//!   allocated `TcpClient` buffers. Every memory failure is in the TLS layer.
//! - No largest-free-block figure: esp-alloc 0.10 reports only total free.
//! - A failed *Rust* allocation (`Vec`, `CString`, ...) is not an error but a panic. That
//!   includes bambino's own `CString::new(host)` in `EmbassyTlsConnector::connect`, so a
//!   `Crashed` row whose panic message just above reads `memory allocation of N bytes failed`
//!   is a real finding: a path where bambino aborts instead of returning `ResourceExhausted`.
//! - Progress survives a crash in RTC fast RAM (`#[ram(unstable(rtc_fast, persistent))]`)
//!   rather than NVS. `esp-backtrace`'s `custom-halt` calls `custom_halt` below after printing
//!   a panic, which resets the board so the sweep resumes.
//! - The handshake has no timeout of its own, so each attempt is raced against
//!   `HANDSHAKE_TIMEOUT` to keep a starved Wi-Fi from hanging the sweep.
//!
//! **Reading the result:** pass = `Ok`, `ResourceExhausted`. Fail = `Other`/`ConnectionAborted`
//! at a level where memory was the cause (read the `mbedtls-rs ... failed: MbedtlsError(..)`
//! debug line just above: an `*_ALLOC_FAILED` code there is a miss). Noted = `TimedOut`,
//! `ConnectionReset`, `Crashed` (read its panic message).
//!
//! A new build (different `PROBE_BUILD_ID`, set by `build.rs`) or a finished sweep starts
//! fresh. `.cargo/config.toml` raises `bambino::io::embassy` to debug, which is where the raw
//! mbedTLS codes are logged; everything else stays at info.
//!
//! ```sh
//! cd embassy-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee 385-test1.log
//! ```
//!
//! Prior investigations (#292's first-run bring-up probe and its follow-ups) are recoverable
//! via `git log -- embassy-hw-probe/src/main.rs`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::alloc::Layout;

use bambino::io::embassy::{EmbassyRawStreamFactory, EmbassyTlsConnector};
use bambino::io::{RawStreamFactory, SocketError, TlsConnector};

use embassy_executor::Spawner;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};

use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::rng::{Rng, Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;

use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController};

use static_cell::StaticCell;

// Linked for their side effects only: `esp-backtrace` supplies the `#[panic_handler]` (and
// prints a symbolized backtrace through `esp-println`), `esp-alloc` the global allocator the
// `heap_allocator!` invocations below fill in.
use esp_alloc as _;
use esp_backtrace as _;
// Provides `memchr` for MbedTLS's X.509 code; nothing in this file calls it.
use tinyrlibc as _;

esp_bootloader_esp_idf::esp_app_desc!();

/// Compiled in from `.env` by `build.rs` — never committed. See `.env.example`.
const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");
/// Changes on every rebuild of `src/` (see `build.rs`); a stored sweep from another build is
/// discarded rather than resumed.
const BUILD_ID: &str = env!("PROBE_BUILD_ID");

const MQTT_PORT: u16 = 8883;
const TX_SZ: usize = 2048;
const RX_SZ: usize = 2048;

/// Highest level swept. One live `Session` peaked ~55 KB above baseline on the first hardware
/// run (`src/io/CLAUDE.md`), so this leaves room for a clean success at the top.
const START_LEFT: u32 = 80 * 1024;
const COARSE_STEP: u32 = 2 * 1024;
const FINE_STEP: u32 = 256;
const BALLAST_CHUNK: usize = 4 * 1024;
const BALLAST_MIN: usize = 16;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long to wait for the network to come back after an attempt starved it, before
/// resetting the board to resume.
const NETWORK_RECOVERY: Duration = Duration::from_secs(30);
const BETWEEN_ATTEMPTS: Duration = Duration::from_secs(3);

/// Sweep state in RTC fast RAM, which survives a software reset. Layout, in `u32` words:
/// `[MAGIC, build hash, done, pending level (0 = none), start, count, records...]`, each
/// record `[bytes left, free after fill, stage << 8 | outcome]`.
const MAGIC: u32 = 0x3853_5745; // "85SW"
const HEADER_WORDS: usize = 6;
const RECORD_WORDS: usize = 3;
const MAX_RECORDS: usize = 150;
const STATE_WORDS: usize = HEADER_WORDS + MAX_RECORDS * RECORD_WORDS;

#[ram(unstable(rtc_fast, persistent))]
static mut SWEEP: [u32; STATE_WORDS] = [0; STATE_WORDS];

fn state() -> &'static mut [u32; STATE_WORDS] {
    // SAFETY: single-threaded use from `main` only; the executor never runs two of these at
    // once and no interrupt handler touches it.
    unsafe { &mut *(&raw mut SWEEP) }
}

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

/// Called by `esp-backtrace` (feature `custom-halt`) after it has printed a panic: resets the
/// board so a sweep that crashed resumes from its stored state instead of halting.
#[unsafe(no_mangle)]
fn custom_halt() -> ! {
    esp_hal::system::software_reset()
}

/// The chip TRNG, presented as the `CryptoRng` that `mbedtls_rs::Tls::new` demands.
///
/// esp-hal's `Trng` already implements rand_core 0.10's `TryRng`, so this wrapper exists only
/// to assert `TryCryptoRng`, the marker that promises the stream is fit for key material.
struct HwRng(Trng);

impl rand_core::TryRng for HwRng {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.0.random())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok((self.0.random() as u64) << 32 | self.0.random() as u64)
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.0.read(dst);
        Ok(())
    }
}

// Safety of this marker is the hardware's: entropy comes from the chip's true RNG with the
// radio active, not from a PRNG seeded at boot.
impl rand_core::TryCryptoRng for HwRng {}

const STAGE_HANDSHAKE: u32 = 1;
const STAGE_CRASHED: u32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
enum Outcome {
    Ok = 0,
    ResourceExhausted = 1,
    TimedOut = 2,
    ConnectionReset = 3,
    ConnectionAborted = 4,
    Other = 5,
    Certificate = 6,
    OtherSocket = 7,
    DialFailed = 8,
}

impl Outcome {
    fn of(err: &SocketError) -> Self {
        match err {
            SocketError::ResourceExhausted => Outcome::ResourceExhausted,
            SocketError::TimedOut => Outcome::TimedOut,
            SocketError::ConnectionReset => Outcome::ConnectionReset,
            SocketError::ConnectionAborted => Outcome::ConnectionAborted,
            SocketError::Other(_) => Outcome::Other,
            SocketError::CertificateInvalid(_) => Outcome::Certificate,
            _ => Outcome::OtherSocket,
        }
    }

    fn name(code: u32) -> &'static str {
        match code {
            0 => "Ok",
            1 => "ResourceExhausted",
            2 => "TimedOut",
            3 => "ConnectionReset",
            4 => "ConnectionAborted",
            5 => "Other",
            6 => "CertificateInvalid",
            7 => "other SocketError",
            8 => "dial failed",
            _ => "unknown",
        }
    }

    /// `Some(true)` pass, `None` noted only. Nothing here is an automatic fail: `Other` and
    /// `ConnectionAborted` are only misses if their debug line shows an allocation code.
    fn verdict(code: u32) -> Option<bool> {
        match code {
            0 | 1 => Some(true),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Record {
    left: u32,
    free: u32,
    stage: u32,
    outcome: u32,
}

fn records() -> Vec<Record> {
    let s = state();
    let count = (s[5] as usize).min(MAX_RECORDS);
    (0..count)
        .map(|i| {
            let r = &s[HEADER_WORDS + i * RECORD_WORDS..][..RECORD_WORDS];
            Record {
                left: r[0],
                free: r[1],
                stage: r[2] >> 8,
                outcome: r[2] & 0xFF,
            }
        })
        .collect()
}

fn push_record(r: Record) {
    let s = state();
    let count = s[5] as usize;
    if count >= MAX_RECORDS {
        return;
    }
    let base = HEADER_WORDS + count * RECORD_WORDS;
    s[base] = r.left;
    s[base + 1] = r.free;
    s[base + 2] = (r.stage << 8) | (r.outcome & 0xFF);
    s[5] = count as u32 + 1;
}

fn build_hash() -> u32 {
    // FNV-1a; only needs to differ between builds.
    BUILD_ID
        .bytes()
        .fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// Resumes a sweep in progress, or starts fresh for a new build or after a finished one.
/// Returns the sweep's top level, `None` when it must be chosen fresh.
fn load_state() -> Option<u32> {
    let s = state();
    if s[0] != MAGIC || s[1] != build_hash() || s[2] == 1 {
        log::info!("starting a fresh sweep");
        s.fill(0);
        s[0] = MAGIC;
        s[1] = build_hash();
        return None;
    }
    if s[3] != 0 {
        log::warn!(
            "resuming after a reset during level {}; recording it as Crashed (its panic, if \
             any, is printed above the reset)",
            s[3]
        );
        push_record(Record {
            left: s[3],
            free: 0,
            stage: STAGE_CRASHED,
            outcome: 0,
        });
        s[3] = 0;
    } else {
        log::info!("resuming a sweep with {} level(s) done", s[5]);
    }
    Some(s[4])
}

/// The next level to try, or `None` when the sweep is complete.
fn next_level(results: &[Record], start: u32) -> Option<u32> {
    let done = |level: u32| results.iter().any(|r| r.left == level);

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

    let mut coarse: Vec<&Record> = results
        .iter()
        .filter(|r| (start - r.left.min(start)) % COARSE_STEP == 0)
        .collect();
    coarse.sort_by(|a, b| b.left.cmp(&a.left));
    for pair in coarse.windows(2) {
        let (hi, lo) = (pair[0], pair[1]);
        if (hi.stage, hi.outcome) == (lo.stage, lo.outcome) {
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

/// Allocates ballast until at most `left` bytes of heap are free. Uses the raw allocator so a
/// failed allocation returns null instead of panicking.
fn fill_to(left: usize, ballast: &mut Vec<(*mut u8, Layout)>) {
    let mut chunk = BALLAST_CHUNK;
    loop {
        let free = esp_alloc::HEAP.free();
        if free <= left || ballast.len() == ballast.capacity() {
            return;
        }
        let size = (free - left).min(chunk);
        if size < BALLAST_MIN {
            return;
        }
        let Ok(layout) = Layout::from_size_align(size, 4) else {
            return;
        };
        // SAFETY: nonzero size; the pointer is kept with its layout for `release`.
        let p = unsafe { alloc::alloc::alloc(layout) };
        if p.is_null() {
            // Fragmented: try smaller pieces before giving up.
            chunk /= 2;
            if chunk < BALLAST_MIN {
                return;
            }
            continue;
        }
        ballast.push((p, layout));
    }
}

fn release(ballast: &mut Vec<(*mut u8, Layout)>) {
    for (p, layout) in ballast.drain(..) {
        // SAFETY: each pointer came from `alloc` with this layout and is freed once.
        unsafe { alloc::alloc::dealloc(p, layout) };
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // Same two heaps as the bring-up probe: the reclaimed ROM region, then DRAM.
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 110 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    log::info!("embassy-hw-probe: issue #385 out-of-memory sweep");
    let stored_start = load_state();

    let station_config = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID)
            .with_password(WIFI_PASS.into()),
    );
    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("esp-radio wifi init");

    // The stack seed only needs to be unpredictable, not cryptographic.
    let seed_rng = Rng::new();
    let seed = (seed_rng.random() as u64) << 32 | seed_rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<4>, StackResources::<4>::new()),
        seed,
    );

    spawner.spawn(wifi_task(controller).expect("wifi task token"));
    spawner.spawn(net_task(runner).expect("net task token"));

    stack.wait_config_up().await;
    match stack.config_v4() {
        Some(config) => log::info!("DHCP lease {}", config.address),
        None => panic!("stack up but no IPv4 config"),
    }

    // `TrngSource` owns ADC1 for as long as it lives; it must outlive every TLS session.
    let _trng_source: &'static mut TrngSource<'static> = mk_static!(
        TrngSource<'static>,
        TrngSource::new(peripherals.RNG, peripherals.ADC1)
    );
    let rng: &'static mut HwRng = mk_static!(HwRng, HwRng(Trng::try_new().expect("TRNG source")));
    let tls: &'static mut mbedtls_rs::Tls<'static> = mk_static!(
        mbedtls_rs::Tls<'static>,
        mbedtls_rs::Tls::new(rng).expect("only one Tls instance may exist program-wide")
    );
    let pool: &'static TcpClient<'static, 1, TX_SZ, RX_SZ> = mk_static!(
        TcpClient<'static, 1, TX_SZ, RX_SZ>,
        TcpClient::new(
            stack,
            mk_static!(TcpClientState<1, TX_SZ, RX_SZ>, TcpClientState::new())
        )
    );

    let free = esp_alloc::HEAP.free() as u32;
    log::info!("baseline: {free} bytes free; {}", esp_alloc::HEAP.stats());
    let start = stored_start.unwrap_or_else(|| {
        let start = START_LEFT.min(free.saturating_sub(4 * 1024));
        state()[4] = start;
        start
    });
    log::info!("sweeping down from {start} bytes left");

    let mut ballast: Vec<(*mut u8, Layout)> = Vec::with_capacity(256);

    loop {
        let results = records();
        let Some(level) = next_level(&results, start) else {
            break;
        };
        drop(results);
        if state()[5] as usize >= MAX_RECORDS {
            log::warn!("record limit reached; stopping early");
            break;
        }

        wait_for_network(stack).await;
        state()[3] = level;

        // Built before the ballast so its own allocation doesn't count against `level`.
        let connector = EmbassyTlsConnector::new(tls.reference());
        let factory = EmbassyRawStreamFactory::new(pool);

        fill_to(level as usize, &mut ballast);
        let free_after = esp_alloc::HEAP.free() as u32;

        let outcome = match factory.dial(PRINTER_IP, MQTT_PORT).await {
            Err(e) => {
                log::info!("    dial failed: {e:?}");
                Outcome::DialFailed
            }
            Ok(raw) => {
                match embassy_time::with_timeout(
                    HANDSHAKE_TIMEOUT,
                    connector.connect(PRINTER_SERIAL, raw),
                )
                .await
                {
                    Err(_) => {
                        log::info!("    handshake did not finish in {HANDSHAKE_TIMEOUT:?}");
                        Outcome::TimedOut
                    }
                    Ok(Ok(mut stream)) => {
                        if let Err(e) = connector.close(&mut stream).await {
                            log::info!("    close failed: {e:?}");
                        }
                        Outcome::Ok
                    }
                    Ok(Err(e)) => {
                        log::info!("    handshake failed: {e:?}");
                        Outcome::of(&e)
                    }
                }
            }
        };

        release(&mut ballast);
        push_record(Record {
            left: level,
            free: free_after,
            stage: STAGE_HANDSHAKE,
            outcome: outcome as u32,
        });
        state()[3] = 0;
        log::info!(
            "  left {level:>6}  free {free_after:>6}  {}",
            Outcome::name(outcome as u32)
        );

        Timer::after(BETWEEN_ATTEMPTS).await;
    }

    state()[2] = 1;
    print_summary();

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Waits for Wi-Fi and DHCP after an attempt may have starved them; resets the board if they
/// don't come back, which the stored state turns into a resumed sweep.
async fn wait_for_network(stack: Stack<'static>) {
    if stack.is_config_up() {
        return;
    }
    log::warn!("network is down after the last attempt; waiting for it to recover");
    if embassy_time::with_timeout(NETWORK_RECOVERY, stack.wait_config_up())
        .await
        .is_err()
    {
        log::error!("network did not recover; resetting to resume the sweep");
        esp_hal::system::software_reset();
    }
}

fn print_summary() {
    let mut sorted = records();
    sorted.sort_by(|a, b| b.left.cmp(&a.left));

    let (mut pass, mut noted) = (0, 0);
    log::info!("============ issue #385 out-of-memory sweep (embassy) ============");
    log::info!("  {:>6}  {:>6}  result", "left", "free");
    for r in &sorted {
        let (tag, what) = if r.stage == STAGE_CRASHED {
            ("note ", "Crashed (see the panic printed before that reset)")
        } else if Outcome::verdict(r.outcome) == Some(true) {
            ("pass ", Outcome::name(r.outcome))
        } else {
            ("note ", Outcome::name(r.outcome))
        };
        if tag == "pass " {
            pass += 1;
        } else {
            noted += 1;
        }
        log::info!("  {:>6}  {:>6}  {tag}{what}", r.left, r.free);
    }
    log::info!(
        "RESULT: {pass} pass, {noted} noted. For each noted Other/ConnectionAborted, read the \
         `mbedtls-rs ... failed: MbedtlsError(..)` line above it: an *_ALLOC_FAILED code there \
         is a miss. For each Crashed, a `memory allocation of N bytes failed` panic is a path \
         where bambino aborts instead of returning ResourceExhausted."
    );
    log::info!("==================================================================");
}

/// Keeps the station associated, reconnecting after a drop.
#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(info) => {
                log::info!("wifi: connected {info:?}");
                let info = controller.wait_for_disconnect_async().await.ok();
                log::warn!("wifi: disconnected {info:?}");
            }
            Err(e) => log::warn!("wifi: connect failed: {e:?}"),
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}
