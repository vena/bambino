//! Current investigation: bambino's `EmbassySocketPool` on hardware.
//!
//! Checks that every connection bambino makes over embassy-net is closed cleanly, with a FIN,
//! and that the printer keeps accepting connections on every port throughout. Every dial goes
//! through `EmbassySocketPool` / `EmbassyRawStreamFactory`.
//!
//! | Stage | What it does | Passes when |
//! |---|---|---|
//! | 1, single drop (`SINGLE_ROUNDS` rounds) | one zero-byte drop on 990, then every port checked at each of `CHECKPOINTS` after it | every port accepts at every check |
//! | 2, replay | `REPLAY_CYCLES` cycles of a zero-byte drop on 990 plus a timed 8883 handshake, `SPACING` apart | every port still accepts |
//! | H, real flows | `PrinterClient`: MQTT connect + a telemetry report + disconnect; FTPS connect + list `/` + disconnect, `H_FTPS_CONNECTS` times; `connect_all` (MQTT + FTPS); then `H_DROPS` times an app dropping a live MQTT client, followed at once by a fresh one | every step succeeds |
//! | C, churn at the limit | `ROUNDS` rounds: hold `LIMIT` FTPS sessions, drop all, next round at once | every round gets all `LIMIT` |
//! | I, content | `FtpsClient`: upload `I_SIZES` files of known content back to back, list, download and compare each, log each SHA-256, keep the last `I_KEEP` | all byte-identical; the kept files' SHA-256 match a download from a computer |
//! | J, cancelled download | a download that stops reading partway and is cancelled after `J_CANCEL_AFTER`; the client dropped; then reconnect and download again | the retry connects promptly and the file is byte-identical |
//! | K, camera stopped mid-stream | camera stream dropped while frames arrive; a second client (the probe's own socket) opens 6000 at `K_SECOND_CLIENT_AT` | the second client gets frames each time |
//!
//! All ports are checked after each stage. If one stops accepting connections, the probe says
//! so, rechecks it with no time limit, and runs nothing further.
//!
//! **How long each closing connection took** comes from bambino's own `trace` log, enabled for
//! `bambino::io::embassy` in `.cargo/config.toml`: `embassy socket N closed M ms after its stream
//! was dropped`, `discarded N B received while closing`, `leased after N ms` (a dial that waited
//! for a closing socket), and a `warn` if one needed an RST.
//!
//! **Control build** (`--features control`): stage 1, round 1 only, with the drop done the way
//! embassy-net's `TcpClient` does it (`close()`, then the socket removed before the FIN is sent),
//! for comparison with the default build.
//!
//! **Running it.** Start with nothing else connected to the printer's FTPS service or camera,
//! and a few hundred KB free on its SD card. The default build, then the control build:
//!
//! ```sh
//! cd embassy-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee pool-check-1.log
//! cargo espflash flash --release --monitor --features control 2>&1 | tee pool-check-control-1.log
//! ```
//!
//! The probe creates `/bambino-probe/` and leaves the `I_KEEP` files there for checking from a
//! computer. The log carries the serial: check it and the access code before sharing it.

#![no_std]
#![no_main]
// The control build runs only stage 1, so the other stages' helpers go unused there.
#![cfg_attr(feature = "control", allow(dead_code, unused_imports, unused_macros))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use bambino::PrinterIdentity;
use bambino::camera::binary::BinaryCameraStream;
use bambino::client::{PrinterClient, TelemetryEvent};
use bambino::ftps::{CurrentDateTime, FtpsClient};
use bambino::io::embassy::{
    EmbassyRawStreamFactory, EmbassySocketBuffers, EmbassySocketPool, EmbassyTcpStream,
    EmbassyTimer, EmbassyTlsConnector,
};
use bambino::io::{RawStreamFactory, SocketError, TlsConnector};

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Instant, Timer};

use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::rng::{Rng, Trng, TrngSource};
use esp_hal::timer::timg::{MwdtStage, TimerGroup, Wdt};

use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController};

use sha2::Digest as _;
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
const ACCESS_CODE: &str = env!("PROBE_ACCESS_CODE");

use bambino::ftps::FTPS_PORT;
use bambino::mqtt::MQTTS_PORT as MQTT_PORT;
const CAMERA_PORT: u16 = 6000;

const TX_SZ: usize = 2048;
const RX_SZ: usize = 2048;
/// The most connections any stage holds at once: C's `LIMIT` FTPS sessions.
const POOL_N: usize = 4;

/// Stage 1: how many separate single drops, each fully checked before the next.
const SINGLE_ROUNDS: usize = 3;
/// Stage 1: when the ports are checked, in seconds after the drop.
const CHECKPOINTS: [u64; 6] = [3, 30, 60, 120, 150, 180];
/// Stage 2: cycles and spacing.
const REPLAY_CYCLES: usize = 6;
const SPACING: Duration = Duration::from_secs(3);
/// Stages 1 and 2: dial and handshake bounds.
const REPLAY_DIAL_BOUND: Duration = Duration::from_secs(60);
const REPLAY_HANDSHAKE_BOUND: Duration = Duration::from_secs(40);
/// Stage 2: a healthy dial on this LAN finishes in well under a second, a handshake in a few.
const SLOW_MS: u64 = 5_000;
const HANDSHAKE_SLOW_MS: u64 = 10_000;

/// Stage H: FTPS connects, telemetry events read looking for a report, live MQTT clients
/// dropped.
const H_FTPS_CONNECTS: usize = 4;
const H_EVENTS: usize = 10;
const H_DROPS: usize = 3;

/// Stage C: sessions per round (the P1S's measured FTPS limit) and rounds.
const LIMIT: usize = 4;
const ROUNDS: usize = 6;

/// Stage I: the probe's directory, and file sizes at the edges of the 2 KB socket buffers. The
/// last `I_KEEP` files are kept for checking from a computer. Capped at 8 KB: a transfer holds
/// two TLS sessions (~48 KB each) plus the file, and a C6 has ~174 KB of heap.
const DIR: &str = "/bambino-probe";
const I_SIZES: [usize; 8] = [1, 2047, 2048, 2049, 4095, 4096, 6144, 8192];
const I_KEEP: usize = 2;
/// Stage J: the file's size, how many raw data-channel bytes are read before reads stop (TLS
/// handshake plus part of the file), and when the download is cancelled.
const J_SIZE: usize = 8192;
const J_READ_CAP: usize = 5000;
const J_CANCEL_AFTER: Duration = Duration::from_secs(8);
/// Stage K: how much of the camera stream is read before the drop, when the second client tries
/// (seconds after the drop), and how much it reads.
const K_READ_BEFORE_DROP: usize = 4096;
const K_SECOND_CLIENT_AT: [u64; 2] = [5, 20];
const K_SECOND_CLIENT_READ: usize = 1024;

const DIAL_BOUND: Duration = Duration::from_secs(10);
const HANDSHAKE_BOUND: Duration = Duration::from_secs(20);
const READ_BOUND: Duration = Duration::from_secs(10);
/// Bound on one bambino call. Its own timeouts are shorter or equal, so this only catches a hang.
const CALL_BOUND: Duration = Duration::from_secs(60);
/// Bound on one port check's dial.
const PREFLIGHT_BOUND: Duration = Duration::from_secs(10);
/// Bound on waiting for a port check's FIN to be acked.
const TEARDOWN_BOUND: Duration = Duration::from_secs(5);

/// Watch: recheck interval while a port is down, fast at first, then slow for the long tail.
const WATCH_FAST: Duration = Duration::from_secs(60);
const WATCH_FAST_FOR: Duration = Duration::from_secs(15 * 60);
const WATCH_SLOW: Duration = Duration::from_secs(5 * 60);
/// Longest single step between watchdog feeds with margin; sleeps are fed in `FEED_STEP` chunks.
const WATCHDOG_SECS: u64 = 180;
const FEED_STEP: Duration = Duration::from_secs(60);

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

/// Called by `esp-backtrace` (feature `custom-halt`) after it has printed a panic. This probe
/// keeps no state across resets, so resetting just reruns it from the start.
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

/// Stage J's read cap for new data-channel streams; 0 means none.
static READ_CAP: AtomicUsize = AtomicUsize::new(0);

/// A pool stream whose reads can be made to stop for good after a byte count: an app that
/// stops reading mid-download.
struct Capped {
    inner: EmbassyTcpStream,
    cap: Option<usize>,
    read: usize,
}

impl embedded_io_async::ErrorType for Capped {
    type Error = embassy_net::tcp::Error;
}

impl embedded_io_async::Read for Capped {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let Some(cap) = self.cap else {
            return self.inner.read(buf).await;
        };
        if self.read >= cap {
            return core::future::pending().await;
        }
        let max = buf.len().min(cap - self.read);
        let n = self.inner.read(&mut buf[..max]).await?;
        self.read += n;
        Ok(n)
    }
}

impl embedded_io_async::Write for Capped {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().await
    }
}

/// bambino's factory, with `READ_CAP` applied to each new stream.
#[derive(Clone, Copy)]
struct CappedFactory(EmbassyRawStreamFactory);

impl RawStreamFactory<Capped> for CappedFactory {
    async fn dial(&self, host: &str, port: u16) -> Result<Capped, SocketError> {
        let inner = self.0.dial(host, port).await?;
        let cap = READ_CAP.load(Ordering::Relaxed);
        Ok(Capped {
            inner,
            cap: (cap > 0).then_some(cap),
            read: 0,
        })
    }
}

type Watchdog = Wdt<esp_hal::peripherals::TIMG1<'static>>;
type Tls = EmbassyTlsConnector<'static>;
type Ftps = FtpsClient<Capped, Tls, CappedFactory, EmbassyTimer>;

/// What the probe needs for every step.
struct Ctx {
    stack: Stack<'static>,
    factory: EmbassyRawStreamFactory,
    tls: &'static mbedtls_rs::Tls<'static>,
    rx: &'static mut [u8; RX_SZ],
    tx: &'static mut [u8; TX_SZ],
    wdt: Watchdog,
}

impl Ctx {
    fn connector(&self) -> Tls {
        EmbassyTlsConnector::unverified(self.tls.reference())
    }
}

/// A `PrinterClient` with MQTT and FTPS both dialing through the pool.
macro_rules! printer {
    ($ctx:expr) => {
        PrinterClient::new($ctx.connector(), $ctx.factory, identity())
            .with_timer(EmbassyTimer)
            .with_ftps($ctx.connector(), $ctx.factory, EmbassyTimer)
    };
}

fn identity() -> PrinterIdentity {
    PrinterIdentity::new(PRINTER_IP, PRINTER_SERIAL, ACCESS_CODE)
}

fn printer_addr(port: u16) -> core::net::SocketAddrV4 {
    let ip: core::net::Ipv4Addr = PRINTER_IP
        .parse()
        .expect("PROBE_PRINTER_IP is an IPv4 address");
    core::net::SocketAddrV4::new(ip, port)
}

/// Board uptime, for lining log lines up with checks made from a computer.
fn uptime() -> String {
    let s = Instant::now().as_secs();
    alloc::format!("[up {}h{:02}m{:02}s]", s / 3600, s / 60 % 60, s % 60)
}

fn minutes(d: Duration) -> String {
    let s = d.as_secs();
    alloc::format!("{}m{:02}s", s / 60, s % 60)
}

fn heap() -> String {
    alloc::format!("heap free {} B", esp_alloc::HEAP.free())
}

/// Deterministic file content: xorshift64 from `seed`, so a run can be reproduced.
fn content(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 32) as u8
        })
        .collect()
}

fn sha256_hex(data: &[u8]) -> String {
    sha2::Sha256::digest(data)
        .iter()
        .map(|b| alloc::format!("{b:02x}"))
        .collect()
}

/// Sleeps `d`, feeding the watchdog every `FEED_STEP`.
async fn sleep_fed(d: Duration, wdt: &mut Watchdog) {
    let until = Instant::now() + d;
    loop {
        wdt.feed();
        let now = Instant::now();
        if now >= until {
            return;
        }
        Timer::after((until - now).min(FEED_STEP)).await;
    }
}

/// Runs an infallible teardown step under [`CALL_BOUND`], logging if it overran.
async fn bounded(label: &str, fut: impl core::future::Future<Output = ()>) {
    if embassy_time::with_timeout(CALL_BOUND, fut).await.is_err() {
        log::warn!("    {label}: no result within the call bound");
    }
}

/// Runs one bambino call under `CALL_BOUND`, logging how long it took.
async fn call<T>(
    label: &str,
    fut: impl core::future::Future<Output = Result<T, bambino::Error>>,
) -> Result<T, String> {
    let started = Instant::now();
    let result = match embassy_time::with_timeout(CALL_BOUND, fut).await {
        Err(_) => Err(alloc::format!(
            "{label}: no result in {}s",
            CALL_BOUND.as_secs()
        )),
        Ok(Err(e)) => Err(alloc::format!("{label}: {e:?}")),
        Ok(Ok(v)) => Ok(v),
    };
    match &result {
        Ok(_) => log::info!("    {label}: ok, {} ms", started.elapsed().as_millis()),
        Err(e) => log::warn!("    {e}"),
    }
    result
}

/// Dials `port` through the pool under `bound`; `Err` says why it failed.
async fn dial(ctx: &Ctx, port: u16, bound: Duration) -> Result<EmbassyTcpStream, String> {
    let started = Instant::now();
    match embassy_time::with_timeout(bound, ctx.factory.dial(PRINTER_IP, port)).await {
        Err(_) => Err(alloc::format!(":{port} dial >{}s", bound.as_secs())),
        Ok(Err(e)) => Err(alloc::format!(":{port} dial failed {e:?}")),
        Ok(Ok(stream)) => {
            log::info!(
                "    :{port} dial {} ms",
                started.elapsed().as_millis()
            );
            Ok(stream)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Port checks

/// Checks that each port accepts a TCP connection, closing each one cleanly. Returns the ports
/// that did not. Uses the probe's own socket, not the pool.
async fn preflight(ctx: &mut Ctx) -> Vec<u16> {
    let mut unreachable = Vec::new();
    for port in [MQTT_PORT, FTPS_PORT, CAMERA_PORT] {
        ctx.wdt.feed();
        let mut socket = TcpSocket::new(ctx.stack, &mut ctx.rx[..], &mut ctx.tx[..]);
        match embassy_time::with_timeout(PREFLIGHT_BOUND, socket.connect(printer_addr(port))).await
        {
            Ok(Ok(())) => {
                socket.close();
                let _ = embassy_time::with_timeout(TEARDOWN_BOUND, socket.flush()).await;
            }
            Ok(Err(e)) => {
                log::warn!("    {port} refused: {e:?}");
                unreachable.push(port);
            }
            Err(_) => {
                log::warn!("    {port} did not answer within {PREFLIGHT_BOUND:?}");
                unreachable.push(port);
            }
        }
    }
    unreachable
}

/// Rechecks every port until all answer again, with no time limit. Returns how long they were
/// down, counted from `since`.
async fn watch(ctx: &mut Ctx, since: Instant) -> Duration {
    log::warn!(
        "{} WATCH: a port is not answering; rechecking every {}s for {} min, then every {} min, \
         with no limit",
        uptime(),
        WATCH_FAST.as_secs(),
        WATCH_FAST_FOR.as_secs() / 60,
        WATCH_SLOW.as_secs() / 60
    );
    loop {
        let wait = if since.elapsed() < WATCH_FAST_FOR {
            WATCH_FAST
        } else {
            WATCH_SLOW
        };
        sleep_fed(wait, &mut ctx.wdt).await;
        let unreachable = preflight(ctx).await;
        let down = since.elapsed();
        if unreachable.is_empty() {
            log::warn!(
                "{} WATCH: all ports answering again, {} after they went down",
                uptime(),
                minutes(down)
            );
            return down;
        }
        log::warn!(
            "{} WATCH: still down {unreachable:?}, {}",
            uptime(),
            minutes(down)
        );
    }
}

/// Checks all ports after a stage. If any is down, logs it, watches it, and returns `false`.
async fn check_ports(ctx: &mut Ctx, after: &str) -> bool {
    let checked = Instant::now();
    let unreachable = preflight(ctx).await;
    log::info!(
        "{}   after {after}: ports down {unreachable:?}; {}",
        uptime(),
        heap()
    );
    if unreachable.is_empty() {
        return true;
    }
    log::error!(
        "{} STOPPED after {after}: ports down {unreachable:?}",
        uptime()
    );
    watch(ctx, checked).await;
    false
}

// ---------------------------------------------------------------------------------------------
// Stages 1 and 2: zero-byte drops on 990

/// Dials 990 and drops the connection without sending a byte: what bambino does when FTPS's
/// `Session::new` fails. Returns whether the dial completed.
#[cfg(not(feature = "control"))]
async fn drop_raw(ctx: &mut Ctx) -> bool {
    match dial(ctx, FTPS_PORT, REPLAY_DIAL_BOUND).await {
        Ok(stream) => {
            drop(stream);
            true
        }
        Err(e) => {
            log::warn!("    {e}");
            false
        }
    }
}

/// The control: dials 990 on the probe's own socket and ends it the way embassy-net's
/// `TcpClient` does, `close()` and then the socket removed from the stack before the FIN is
/// sent. Returns whether the dial completed.
#[cfg(feature = "control")]
async fn drop_raw(ctx: &mut Ctx) -> bool {
    let mut socket = TcpSocket::new(ctx.stack, &mut ctx.rx[..], &mut ctx.tx[..]);
    match embassy_time::with_timeout(REPLAY_DIAL_BOUND, socket.connect(printer_addr(FTPS_PORT)))
        .await
    {
        Ok(Ok(())) => {
            socket.close();
            drop(socket);
            true
        }
        Ok(Err(e)) => {
            log::warn!("    990 control dial failed {e:?}");
            false
        }
        Err(_) => {
            log::warn!("    990 control dial >{}s", REPLAY_DIAL_BOUND.as_secs());
            false
        }
    }
}

/// One timed connection: `dial` is `None` if the dial did not finish, `handshake` is `Err` with
/// a reason if the handshake failed or did not finish.
#[cfg(not(feature = "control"))]
struct Sample {
    dial: Option<u64>,
    handshake: Result<u64, String>,
}

#[cfg(not(feature = "control"))]
impl Sample {
    fn slow(&self) -> bool {
        match (self.dial, &self.handshake) {
            (None, _) => true,
            (Some(d), _) if d >= SLOW_MS => true,
            (_, Ok(h)) => *h >= HANDSHAKE_SLOW_MS,
            (_, Err(_)) => true,
        }
    }

    fn describe(&self) -> String {
        let dial = match self.dial {
            None => alloc::format!("dial >{}s", REPLAY_DIAL_BOUND.as_secs()),
            Some(ms) => alloc::format!("dial {ms}"),
        };
        let hs = match &self.handshake {
            Ok(ms) => alloc::format!("hs {ms}"),
            Err(e) => alloc::format!("hs {e}"),
        };
        alloc::format!("{}{dial}/{hs}", if self.slow() { "SLOW " } else { "" })
    }
}

/// Dials `port` through the pool, handshakes, and closes properly.
#[cfg(not(feature = "control"))]
async fn timed_close(ctx: &Ctx, port: u16) -> Sample {
    let started = Instant::now();
    let raw = match embassy_time::with_timeout(
        REPLAY_DIAL_BOUND,
        ctx.factory.dial(PRINTER_IP, port),
    )
    .await
    {
        Err(_) => {
            return Sample {
                dial: None,
                handshake: Err(String::from("not reached")),
            };
        }
        Ok(Err(e)) => {
            return Sample {
                dial: Some(started.elapsed().as_millis()),
                handshake: Err(alloc::format!("dial failed {e:?}")),
            };
        }
        Ok(Ok(raw)) => raw,
    };
    let dial = Some(started.elapsed().as_millis());
    let connector = ctx.connector();
    let started = Instant::now();
    let handshake = match embassy_time::with_timeout(
        REPLAY_HANDSHAKE_BOUND,
        connector.connect(PRINTER_SERIAL, raw),
    )
    .await
    {
        Ok(Ok(mut stream)) => {
            let ms = started.elapsed().as_millis();
            if let Err(e) = connector.close(&mut stream).await {
                log::warn!("    close failed: {e:?}");
            }
            Ok(ms)
        }
        Ok(Err(e)) => Err(alloc::format!("failed {e:?}")),
        Err(_) => Err(alloc::format!(">{}s", REPLAY_HANDSHAKE_BOUND.as_secs())),
    };
    Sample { dial, handshake }
}

/// How one round or the replay ended.
enum Outcome {
    /// Every port accepted connections at every check.
    Survived,
    /// The drop's own dial did not complete: 990 was not accepting before this round.
    AlreadyDown,
    /// A port stopped accepting at the check `at`, and accepted again after `down`.
    Died { at: String, down: Duration },
}

impl Outcome {
    fn describe(&self) -> String {
        match self {
            Outcome::Survived => String::from("every port kept accepting"),
            Outcome::AlreadyDown => {
                String::from("990 was not accepting before the drop (dial did not complete)")
            }
            Outcome::Died { at, down } => alloc::format!(
                "A PORT STOPPED ACCEPTING ({at}), accepting again {} later",
                minutes(*down)
            ),
        }
    }
}

/// Stage 1: one zero-byte drop, then a port check at each of `CHECKPOINTS`.
async fn single_round(ctx: &mut Ctx) -> Outcome {
    let dropped = Instant::now();
    ctx.wdt.feed();
    if !drop_raw(ctx).await {
        watch(ctx, dropped).await;
        return Outcome::AlreadyDown;
    }
    log::info!("{}   dropped one zero-byte connection to 990", uptime());
    for cp in CHECKPOINTS {
        let until = dropped + Duration::from_secs(cp);
        sleep_fed(until.saturating_duration_since(Instant::now()), &mut ctx.wdt).await;
        let unreachable = preflight(ctx).await;
        log::info!(
            "{}   +{cp}s after the drop: ports down {unreachable:?}",
            uptime()
        );
        if !unreachable.is_empty() {
            let down = watch(ctx, dropped).await;
            return Outcome::Died {
                at: alloc::format!("first seen at the +{cp}s check"),
                down,
            };
        }
    }
    Outcome::Survived
}

/// Stage 2: zero-byte drops on 990 interleaved with timed 8883 handshakes, then a port check.
#[cfg(not(feature = "control"))]
async fn replay(ctx: &mut Ctx) -> Outcome {
    let mut first_dead_dial: Option<usize> = None;
    for n in 1..=REPLAY_CYCLES {
        ctx.wdt.feed();
        if !drop_raw(ctx).await && first_dead_dial.is_none() {
            first_dead_dial = Some(n);
        }
        ctx.wdt.feed();
        let sample = timed_close(ctx, MQTT_PORT).await;
        log::info!(
            "{}   replay cycle {n}: 8883 {}",
            uptime(),
            sample.describe()
        );
        Timer::after(SPACING).await;
    }
    let ended = Instant::now();
    let unreachable = preflight(ctx).await;
    log::info!(
        "{}   after the replay: ports down {unreachable:?}",
        uptime()
    );
    if unreachable.is_empty() {
        return Outcome::Survived;
    }
    let down = watch(ctx, ended).await;
    let at = match first_dead_dial {
        Some(n) => alloc::format!(
            "dials to 990 failed from cycle {n}; time counted from the end of the \
             replay"
        ),
        None => String::from("after the replay"),
    };
    Outcome::Died { at, down }
}

// ---------------------------------------------------------------------------------------------
// Stage H: real `PrinterClient` flows

#[cfg(not(feature = "control"))]
fn listing_time() -> CurrentDateTime {
    CurrentDateTime::new(2026, 10, 3, 12, 0)
}

#[cfg(not(feature = "control"))]
async fn stage_h(ctx: &mut Ctx) -> Result<String, String> {
    // MQTT, then FTPS, one at a time: MQTT + FTPS control + FTPS data is three TLS sessions,
    // more than a C6's heap holds (see the root `Cargo.toml`).
    ctx.wdt.feed();
    let mut p = printer!(ctx);
    call("connect_mqtt", p.connect_mqtt()).await?;
    let mut report = false;
    for n in 1..=H_EVENTS {
        ctx.wdt.feed();
        if let TelemetryEvent::Report(..) =
            call(&alloc::format!("telemetry event {n}"), p.poll_telemetry()).await?
        {
            report = true;
            break;
        }
    }
    bounded("disconnect_mqtt", p.disconnect_mqtt()).await;
    drop(p);
    let mut entries = 0;
    for n in 1..=H_FTPS_CONNECTS {
        ctx.wdt.feed();
        // A fresh client each time. This predates `disconnect_ftps` keeping the FTPS
        // configuration for a redial (#448); one client would now work too.
        let mut p = printer!(ctx);
        {
            let storage = call(&alloc::format!("FTPS connect {n}"), p.ftps()).await?;
            entries = call(
                &alloc::format!("list / {n}"),
                storage.list_directory("/", listing_time()),
            )
            .await?
            .len();
        }
        bounded(&alloc::format!("disconnect_ftps {n}"), p.disconnect_ftps()).await;
    }

    ctx.wdt.feed();
    let mut p = printer!(ctx);
    let started = Instant::now();
    let outcome = embassy_time::with_timeout(CALL_BOUND, p.connect_all())
        .await
        .map_err(|_| String::from("connect_all: no result"))?;
    log::info!(
        "    connect_all: {} ms, mqtt {:?}, ftps {:?}, camera {:?}",
        started.elapsed().as_millis(),
        outcome.mqtt,
        outcome.ftps,
        outcome.camera
    );
    let all_ok = matches!(outcome.mqtt, Some(Ok(()))) && matches!(outcome.ftps, Some(Ok(())));
    bounded("disconnect_ftps", p.disconnect_ftps()).await;
    bounded("disconnect_mqtt", p.disconnect_mqtt()).await;
    drop(p);

    let mut reconnects: Vec<u64> = Vec::new();
    for n in 1..=H_DROPS {
        ctx.wdt.feed();
        let mut p = printer!(ctx);
        call(&alloc::format!("drop {n}: connect_mqtt"), p.connect_mqtt()).await?;
        call(&alloc::format!("drop {n}: telemetry"), p.poll_telemetry()).await?;
        log::info!("    drop {n}: dropping the live client without disconnecting");
        drop(p);
        let mut p = printer!(ctx);
        let started = Instant::now();
        call(&alloc::format!("drop {n}: reconnect"), p.connect_mqtt()).await?;
        reconnects.push(started.elapsed().as_millis());
        call(
            &alloc::format!("drop {n}: telemetry after reconnect"),
            p.poll_telemetry(),
        )
        .await?;
        bounded("disconnect_mqtt", p.disconnect_mqtt()).await;
    }

    Ok(alloc::format!(
        "MQTT ok, telemetry report {}; {H_FTPS_CONNECTS} FTPS connects ok, `/` has {entries} \
         entries; connect_all {}; reconnect after a dropped live client {reconnects:?} ms",
        if report { "seen" } else { "NOT SEEN" },
        if all_ok { "ok" } else { "FAILED (see log)" }
    ))
}

// ---------------------------------------------------------------------------------------------
// Stage C: churn at the FTPS limit

/// Reads the FTP server's greeting line and checks it is a `220`.
#[cfg(not(feature = "control"))]
async fn read_greeting<S: embedded_io_async::Read>(tls: &mut S) -> Result<(), String> {
    use embedded_io_async::Error as _;
    let mut line: Vec<u8> = Vec::new();
    let mut buf = [0u8; 64];
    let deadline = Instant::now() + READ_BOUND;
    while !line.ends_with(b"\r\n") && line.len() < 256 {
        let left = deadline.saturating_duration_since(Instant::now());
        match embassy_time::with_timeout(left, tls.read(&mut buf)).await {
            Err(_) => return Err(alloc::format!("greeting >{}s", READ_BOUND.as_secs())),
            Ok(Err(e)) => return Err(alloc::format!("greeting read failed {:?}", e.kind())),
            Ok(Ok(0)) => return Err(String::from("greeting: connection closed")),
            Ok(Ok(n)) => line.extend_from_slice(&buf[..n]),
        }
    }
    if line.starts_with(b"220") {
        Ok(())
    } else {
        Err(alloc::format!(
            "greeting not 220: {:?}",
            String::from_utf8_lossy(&line[..line.len().min(40)])
        ))
    }
}

/// Holds one FTPS session: dial, TLS handshake, `220` greeting, then the TLS session dropped
/// to free its ~48 KB while the TCP connection stays open. To the printer that is a live,
/// logged-out FTPS session. Dropping the returned stream ends it with a FIN.
#[cfg(not(feature = "control"))]
async fn hold_ftps(ctx: &Ctx, label: &str) -> Result<EmbassyTcpStream, String> {
    let started = Instant::now();
    let mut stream = dial(ctx, FTPS_PORT, DIAL_BOUND).await?;
    let dial_ms = started.elapsed().as_millis();
    let connector = ctx.connector();
    let started = Instant::now();
    let held = match embassy_time::with_timeout(
        HANDSHAKE_BOUND,
        connector.connect(PRINTER_SERIAL, &mut stream),
    )
    .await
    {
        Err(_) => Err(alloc::format!("handshake >{}s", HANDSHAKE_BOUND.as_secs())),
        Ok(Err(e)) => Err(alloc::format!("handshake failed {e:?}")),
        Ok(Ok(mut tls)) => {
            let hs_ms = started.elapsed().as_millis();
            read_greeting(&mut tls).await.map(|()| hs_ms)
        }
    };
    match held {
        Ok(hs_ms) => {
            log::info!("    {label}: held (dial {dial_ms} / hs {hs_ms} ms)");
            Ok(stream)
        }
        Err(e) => Err(alloc::format!("dial {dial_ms} ms, then {e}")),
    }
}

#[cfg(not(feature = "control"))]
async fn stage_c(ctx: &mut Ctx) -> String {
    let mut full_rounds = 0;
    for round in 1..=ROUNDS {
        let mut held: Vec<EmbassyTcpStream> = Vec::new();
        for k in 1..=LIMIT {
            ctx.wdt.feed();
            match hold_ftps(ctx, &alloc::format!("C round {round} session {k}")).await {
                Ok(s) => held.push(s),
                Err(e) => log::warn!("    C round {round} session {k}: FAILED: {e}"),
            }
        }
        if held.len() == LIMIT {
            full_rounds += 1;
        }
        // Dropped together; the next round's first dial waits for them to close.
        drop(held);
    }
    alloc::format!("{full_rounds} of {ROUNDS} rounds got all {LIMIT} sessions")
}

// ---------------------------------------------------------------------------------------------
// Stages I and J: FTPS transfers

/// Connects bambino's `FtpsClient` through the pool.
#[cfg(not(feature = "control"))]
async fn connect_ftps(ctx: &mut Ctx) -> Result<Ftps, String> {
    ctx.wdt.feed();
    let factory = CappedFactory(ctx.factory);
    let raw =
        match embassy_time::with_timeout(DIAL_BOUND, factory.dial(PRINTER_IP, FTPS_PORT)).await {
            Err(_) => return Err(alloc::format!("control dial >{}s", DIAL_BOUND.as_secs())),
            Ok(Err(e)) => return Err(alloc::format!("control dial failed {e:?}")),
            Ok(Ok(raw)) => raw,
        };
    call(
        "FtpsClient::connect",
        FtpsClient::connect(
            raw,
            ctx.connector(),
            factory,
            identity(),
            EmbassyTimer,
            bambino::ftps::TlsVersionCheck::Enforce,
        ),
    )
    .await
}

#[cfg(not(feature = "control"))]
async fn disconnect_ftps(ftps: Ftps) {
    let _ = embassy_time::with_timeout(CALL_BOUND, ftps.disconnect()).await;
}

/// Stage I: upload files of known content back to back, list, download and compare.
#[cfg(not(feature = "control"))]
async fn stage_i(ctx: &mut Ctx) -> Result<String, String> {
    let mut ftps = connect_ftps(ctx).await?;
    // An earlier run may have left the directory behind; a failure here is not one.
    let _ = call("mkdir", ftps.create_directory(DIR)).await;
    let names: Vec<String> = I_SIZES
        .iter()
        .enumerate()
        .map(|(k, len)| {
            let kind = if k >= I_SIZES.len() - I_KEEP {
                "keep"
            } else {
                "i"
            };
            alloc::format!("{kind}-{k}-{len}.bin")
        })
        .collect();

    for (k, &len) in I_SIZES.iter().enumerate() {
        ctx.wdt.feed();
        let data = content(k as u64 + 1, len);
        log::info!(
            "    upload {}: seed {}, {len} B, {}",
            names[k],
            k + 1,
            heap()
        );
        let path = alloc::format!("{DIR}/{}", names[k]);
        call(
            &alloc::format!("upload {}", names[k]),
            ftps.upload_file(&path, &data),
        )
        .await?;
    }

    let listing = call("list", ftps.list_directory(DIR, listing_time())).await?;
    let mut listed_ok = 0;
    for (k, &len) in I_SIZES.iter().enumerate() {
        match listing.iter().find(|f| f.name == names[k]) {
            Some(f) if f.size == len as u64 => listed_ok += 1,
            Some(f) => log::warn!("    list: {} has size {} not {len}", names[k], f.size),
            None => log::warn!("    list: {} missing", names[k]),
        }
    }

    let mut identical = 0;
    for (k, &len) in I_SIZES.iter().enumerate() {
        ctx.wdt.feed();
        let path = alloc::format!("{DIR}/{}", names[k]);
        let got = call(
            &alloc::format!("download {}", names[k]),
            ftps.download_file(&path),
        )
        .await?;
        let want = content(k as u64 + 1, len);
        if got == want {
            identical += 1;
            log::info!("    {}: identical, sha256 {}", names[k], sha256_hex(&got));
        } else {
            let first_diff = got.iter().zip(&want).position(|(a, b)| a != b);
            log::warn!(
                "    {}: DIFFERENT: got {} B, want {len} B, first difference at {first_diff:?}",
                names[k],
                got.len()
            );
        }
    }

    for (k, name) in names.iter().enumerate() {
        if k < I_SIZES.len() - I_KEEP {
            let path = alloc::format!("{DIR}/{name}");
            let _ = call(&alloc::format!("delete {name}"), ftps.delete_file(&path)).await;
        } else {
            log::info!(
                "    KEPT for checking from a computer: {DIR}/{name}, sha256 {}",
                sha256_hex(&content(k as u64 + 1, I_SIZES[k]))
            );
        }
    }
    disconnect_ftps(ftps).await;
    Ok(alloc::format!(
        "{listed_ok}/{} listed with the right size, {identical}/{} downloaded identical",
        I_SIZES.len(),
        I_SIZES.len()
    ))
}

/// Stage J: a download cancelled while the printer is still sending, then a retry.
#[cfg(not(feature = "control"))]
async fn stage_j(ctx: &mut Ctx) -> Result<String, String> {
    let path = alloc::format!("{DIR}/j.bin");
    let want = content(99, J_SIZE);
    let mut ftps = connect_ftps(ctx).await?;
    call("upload j.bin", ftps.upload_file(&path, &want)).await?;

    READ_CAP.store(J_READ_CAP, Ordering::Relaxed);
    let cancelled = embassy_time::with_timeout(J_CANCEL_AFTER, ftps.download_file(&path)).await;
    READ_CAP.store(0, Ordering::Relaxed);
    let how = match cancelled {
        Err(_) => String::from("cancelled mid-download"),
        Ok(Ok(_)) => String::from("NOT CANCELLED: the download finished under the read cap"),
        Ok(Err(e)) => alloc::format!("NOT CANCELLED: the download failed first: {e:?}"),
    };
    log::info!("{}   J: {how}; dropping the client", uptime());
    // Dropping the client here, rather than `disconnect`, is what an app that gave up does; it
    // ends the control connection too. The pool log shows both sockets closing.
    drop(ftps);

    let started = Instant::now();
    let mut ftps = connect_ftps(ctx).await?;
    let reconnect_ms = started.elapsed().as_millis();
    let got = call("download j.bin again", ftps.download_file(&path)).await?;
    let _ = call("delete j.bin", ftps.delete_file(&path)).await;
    disconnect_ftps(ftps).await;
    Ok(alloc::format!(
        "{how}; reconnected in {reconnect_ms} ms; j.bin then downloaded {}",
        if got == want {
            "identical"
        } else {
            "DIFFERENT"
        }
    ))
}

// ---------------------------------------------------------------------------------------------
// Stage K: camera stopped mid-stream

/// Reads `n` bytes from `stream` or fails.
#[cfg(not(feature = "control"))]
async fn read_n<S: embedded_io_async::Read>(stream: &mut S, n: usize) -> Result<(), String> {
    use embedded_io_async::Error as _;
    let mut buf = [0u8; 512];
    let mut got = 0;
    let deadline = Instant::now() + READ_BOUND;
    while got < n {
        let left = deadline.saturating_duration_since(Instant::now());
        match embassy_time::with_timeout(left, stream.read(&mut buf)).await {
            Err(_) => {
                return Err(alloc::format!(
                    "read {got}/{n} B in {}s",
                    READ_BOUND.as_secs()
                ));
            }
            Ok(Err(e)) => return Err(alloc::format!("read failed after {got} B: {:?}", e.kind())),
            Ok(Ok(0)) => return Err(alloc::format!("closed after {got} B")),
            Ok(Ok(k)) => got += k,
        }
    }
    Ok(())
}

/// TLS handshake, camera authentication, and `n` bytes of the stream over `raw`, then
/// `close_notify` if `close` is set. Returns how long it took.
#[cfg(not(feature = "control"))]
async fn camera_session<R: bambino::io::AsyncIo>(
    ctx: &Ctx,
    raw: R,
    n: usize,
    close: bool,
) -> Result<u64, String> {
    let connector = ctx.connector();
    let started = Instant::now();
    let mut tls =
        match embassy_time::with_timeout(HANDSHAKE_BOUND, connector.connect(PRINTER_SERIAL, raw))
            .await
        {
            Err(_) => return Err(alloc::format!("handshake >{}s", HANDSHAKE_BOUND.as_secs())),
            Ok(Err(e)) => return Err(alloc::format!("handshake failed {e:?}")),
            Ok(Ok(tls)) => tls,
        };
    if let Err(e) = BinaryCameraStream::new(&mut tls)
        .authenticate(&identity().access_code)
        .await
    {
        return Err(alloc::format!("authenticate failed {e:?}"));
    }
    read_n(&mut tls, n).await?;
    let ms = started.elapsed().as_millis();
    if close {
        let _ = embassy_time::with_timeout(TEARDOWN_BOUND, connector.close(&mut tls)).await;
    }
    Ok(ms)
}

/// A second camera client on the probe's own socket, outside the pool: whether the camera
/// serves another client right now.
#[cfg(not(feature = "control"))]
async fn second_camera_client(ctx: &mut Ctx) -> Result<u64, String> {
    let mut socket = TcpSocket::new(ctx.stack, &mut ctx.rx[..], &mut ctx.tx[..]);
    match embassy_time::with_timeout(DIAL_BOUND, socket.connect(printer_addr(CAMERA_PORT))).await {
        Err(_) => return Err(alloc::format!("dial >{}s", DIAL_BOUND.as_secs())),
        Ok(Err(e)) => return Err(alloc::format!("dial failed {e:?}")),
        Ok(Ok(())) => {}
    }
    let connector = EmbassyTlsConnector::unverified(ctx.tls.reference());
    let started = Instant::now();
    let result = async {
        let mut tls = match embassy_time::with_timeout(
            HANDSHAKE_BOUND,
            connector.connect(PRINTER_SERIAL, &mut socket),
        )
        .await
        {
            Err(_) => return Err(alloc::format!("handshake >{}s", HANDSHAKE_BOUND.as_secs())),
            Ok(Err(e)) => return Err(alloc::format!("handshake failed {e:?}")),
            Ok(Ok(tls)) => tls,
        };
        if let Err(e) = BinaryCameraStream::new(&mut tls)
            .authenticate(&identity().access_code)
            .await
        {
            return Err(alloc::format!("authenticate failed {e:?}"));
        }
        read_n(&mut tls, K_SECOND_CLIENT_READ).await?;
        let _ = embassy_time::with_timeout(TEARDOWN_BOUND, connector.close(&mut tls)).await;
        Ok(started.elapsed().as_millis())
    }
    .await;
    socket.close();
    let _ = embassy_time::with_timeout(TEARDOWN_BOUND, socket.flush()).await;
    result
}

/// Stage K: a camera stream dropped while frames are arriving, then a second client.
#[cfg(not(feature = "control"))]
async fn stage_k(ctx: &mut Ctx) -> Result<String, String> {
    ctx.wdt.feed();
    let mut stream = dial(ctx, CAMERA_PORT, DIAL_BOUND).await?;
    let ms = camera_session(ctx, &mut stream, K_READ_BEFORE_DROP, false).await?;
    log::info!(
        "{}   K: {K_READ_BEFORE_DROP} B of camera stream in {ms} ms; dropping mid-stream",
        uptime()
    );
    drop(stream);
    let dropped = Instant::now();

    let mut second: Vec<String> = Vec::new();
    for at in K_SECOND_CLIENT_AT {
        let until = dropped + Duration::from_secs(at);
        sleep_fed(until.saturating_duration_since(Instant::now()), &mut ctx.wdt).await;
        let outcome = match second_camera_client(ctx).await {
            Ok(ms) => alloc::format!("+{at}s: second client got frames in {ms} ms"),
            Err(e) => alloc::format!("+{at}s: SECOND CLIENT FAILED: {e}"),
        };
        log::info!("{}   K {outcome}", uptime());
        second.push(outcome);
    }
    Ok(second.join("; "))
}

// ---------------------------------------------------------------------------------------------
// Sequencing

/// The default build: stages 1, 2, H, C, I, J, K, each followed by a port check, stopping at the
/// first port that stops answering.
#[cfg(not(feature = "control"))]
async fn run(ctx: &mut Ctx) -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();
    for round in 1..=SINGLE_ROUNDS {
        log::info!("{} --- stage 1, single drop, round {round} ---", uptime());
        let outcome = single_round(ctx).await;
        log::info!("{}   round {round}: {}", uptime(), outcome.describe());
        let died = !matches!(outcome, Outcome::Survived);
        results.push((alloc::format!("1 round {round}"), outcome.describe()));
        if died {
            return results;
        }
    }

    log::info!(
        "{} --- stage 2, replay, {REPLAY_CYCLES} cycles ---",
        uptime()
    );
    let outcome = replay(ctx).await;
    log::info!("{}   replay: {}", uptime(), outcome.describe());
    let died = !matches!(outcome, Outcome::Survived);
    results.push((String::from("2"), outcome.describe()));
    if died {
        return results;
    }

    macro_rules! stage {
        ($name:literal, $title:literal, $body:expr) => {
            log::info!("{} --- stage {}: {} ---", uptime(), $name, $title);
            let result: Result<String, String> = $body;
            results.push((
                String::from($name),
                match result {
                    Ok(r) => r,
                    Err(e) => alloc::format!("FAILED: {e}"),
                },
            ));
            if !check_ports(ctx, concat!("stage ", $name)).await {
                results.push((
                    String::from($name),
                    String::from("A PORT STOPPED ACCEPTING (see above)"),
                ));
                return results;
            }
        };
    }
    stage!("H", "real PrinterClient flows", stage_h(ctx).await);
    stage!("C", "churn at the FTPS limit", Ok(stage_c(ctx).await));
    stage!("I", "transfers with content check", stage_i(ctx).await);
    stage!("J", "cancelled download", stage_j(ctx).await);
    stage!("K", "camera stopped mid-stream", stage_k(ctx).await);
    results
}

/// The control build: stage 1, round 1 only, with the drop done the way `TcpClient` does it.
#[cfg(feature = "control")]
async fn run(ctx: &mut Ctx) -> Vec<(String, String)> {
    log::info!(
        "{} --- CONTROL: stage 1, one drop that removes the socket before its FIN ---",
        uptime()
    );
    let outcome = single_round(ctx).await;
    log::info!("{}   control: {}", uptime(), outcome.describe());
    alloc::vec![(String::from("control"), outcome.describe())]
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 110 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    log::info!(
        "embassy-hw-probe: bambino's EmbassySocketPool on hardware ({})",
        if cfg!(feature = "control") {
            "CONTROL build: silent drop"
        } else {
            "stages 1, 2, H, C, I, J, K"
        }
    );

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

    // Sockets: the pool's `POOL_N` (registered for good), the probe's own `TcpSocket`, DHCP,
    // DNS, and spares.
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<10>, StackResources::<10>::new()),
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
    // Shared, not unique: every connection takes its own `TlsReference` from it.
    let tls: &'static mbedtls_rs::Tls<'static> = mk_static!(
        mbedtls_rs::Tls<'static>,
        mbedtls_rs::Tls::new(rng).expect("only one Tls instance may exist program-wide")
    );
    // A `ConstStaticCell`, not `mk_static!`: the buffers are const-initialized in place, where
    // `mk_static!` would build them on the main task's stack before moving them.
    let pool: &'static EmbassySocketPool = {
        static BUFS: static_cell::ConstStaticCell<EmbassySocketBuffers<POOL_N, TX_SZ, RX_SZ>> =
            static_cell::ConstStaticCell::new(EmbassySocketBuffers::new());
        EmbassySocketPool::new(stack, BUFS.take())
    };
    spawner.spawn(socket_pool_task(pool).expect("socket pool task token"));

    // Armed after bring-up, which can legitimately take longer than one step.
    let mut wdt: Watchdog = TimerGroup::new(peripherals.TIMG1).wdt;
    wdt.set_timeout(
        MwdtStage::Stage0,
        esp_hal::time::Duration::from_secs(WATCHDOG_SECS),
    );
    wdt.enable();

    let mut ctx = Ctx {
        stack,
        factory: EmbassyRawStreamFactory::new(pool),
        tls,
        rx: mk_static!([u8; RX_SZ], [0; RX_SZ]),
        tx: mk_static!([u8; TX_SZ], [0; TX_SZ]),
        wdt,
    };

    // A port already down would make every stage measure nothing.
    let unreachable = preflight(&mut ctx).await;
    if !unreachable.is_empty() {
        log::error!(
            "STOPPED: the printer does not accept connections on {unreachable:?}. Check it from a \
             computer (`nc -z -G 5 <printer-ip> <port>`), then reset the board."
        );
        ctx.wdt.disable();
        loop {
            Timer::after(Duration::from_secs(60)).await;
        }
    }
    log::info!("{} all ports accepting at startup; {}", uptime(), heap());

    let results = run(&mut ctx).await;
    ctx.wdt.disable();

    log::info!("================ EmbassySocketPool on hardware ================");
    for (stage, result) in &results {
        log::info!("  {stage}: {result}");
    }
    log::info!("===============================================================");

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// Settles the pool's closing connections in the background.
#[embassy_executor::task]
async fn socket_pool_task(pool: &'static EmbassySocketPool) -> ! {
    pool.run().await
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
