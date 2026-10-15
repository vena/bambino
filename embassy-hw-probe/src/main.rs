//! Current investigation: `FtpsClient::download_prefix` on embassy (GitHub issue #629).
//!
//! On a P1S, a FIN on an FTPS data channel does not stop a download: the printer sends the rest
//! of the file. Only an RST does, and the printer then closes the control connection too. So
//! `download_prefix` marks the data channel with `RawStreamFactory::abort_on_drop`, and on
//! embassy `EmbassySocketPool` sends the RST at once instead of its usual FIN-and-drain. This
//! probe checks that on hardware.
//!
//! **Stage P.** `CYCLES` cycles, `SPACING` apart. Each cycle connects an `FtpsClient`, reads the
//! first `PREFIX_BYTES` of `PROBE_FTPS_PATH` with `download_prefix`, drops the spent client, then
//! times a fresh connect plus `list_directory("/")`. The printer serves one FTPS connection at a
//! time, so that time is the signal: right after an RST it is about as fast as any login (~1 s),
//! while a printer still sending the rest of the file after a FIN holds the next login back for
//! as long as that takes. Cycle 1 also logs the prefix's SHA-256. After the cycles every port is
//! checked at +0 s, +30 s and +130 s.
//!
//! **Passes when** every cycle returns `PREFIX_BYTES` with the client poisoned, every follow-up
//! login and listing succeeds in about the time of an ordinary one, the pool's trace log shows
//! `embassy socket N aborted with an RST` once per cycle (and no `discarded N B received while
//! closing` for the data socket), and every port accepts at every check. A `Session dropped
//! without being closed properly` warning after each prefix read is expected: the printer has
//! already closed that control connection, so there is nothing to close. Compare the SHA-256
//! with `head -c <PREFIX_BYTES> <file> | shasum -a 256` on a computer.
//!
//! **Running it.** Set `PROBE_FTPS_PATH` in `.env` to a `.3mf` on the card larger than
//! `PREFIX_BYTES` (a bigger file makes a FIN-close more obvious), close every other FTPS client
//! and camera viewer, then:
//!
//! ```sh
//! cd embassy-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee prefix-1.log
//! ```
//!
//! The log carries the serial: check it and the access code before sharing it. The previous
//! investigation, the socket pool's FIN check, is the commit the README names.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use bambino::PrinterIdentity;
use bambino::ftps::{CurrentDateTime, FtpsClient};
use bambino::io::RawStreamFactory;
use bambino::io::embassy::{
    EmbassyRawStreamFactory, EmbassySocketBuffers, EmbassySocketPool, EmbassyTcpStream,
    EmbassyTimer, EmbassyTlsConnector,
};

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
/// The file stage P reads; optional so the build gate needs no real path.
const FTPS_PATH: Option<&str> = option_env!("PROBE_FTPS_PATH");

use bambino::ftps::FTPS_PORT;
use bambino::mqtt::MQTTS_PORT as MQTT_PORT;
const CAMERA_PORT: u16 = 6000;

const TX_SZ: usize = 2048;
const RX_SZ: usize = 2048;
/// An FTPS transfer holds two connections, control and data.
const POOL_N: usize = 2;

/// Stage P: cycles, spacing, and how much of the file each reads. 40 KiB covers a Bambu Studio
/// `.3mf`'s plate and pick images (36 864 B on the P1S file #629 measured) and fits beside two
/// TLS sessions (~48 KB each) in a C6's ~174 KB heap.
const CYCLES: usize = 6;
const SPACING: Duration = Duration::from_secs(3);
const PREFIX_BYTES: usize = 40 * 1024;
/// When the ports are checked after the last cycle, in seconds.
const CHECKPOINTS: [u64; 3] = [0, 30, 130];

const DIAL_BOUND: Duration = Duration::from_secs(10);
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

type Watchdog = Wdt<esp_hal::peripherals::TIMG1<'static>>;
type Tls = EmbassyTlsConnector<'static>;
type Ftps = FtpsClient<EmbassyTcpStream, Tls, EmbassyRawStreamFactory, EmbassyTimer>;

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

fn sha256_hex(data: &[u8]) -> String {
    sha2::Sha256::digest(data)
        .iter()
        .map(|b| alloc::format!("{b:02x}"))
        .collect()
}

fn listing_time() -> CurrentDateTime {
    CurrentDateTime::new(2026, 10, 14, 12, 0)
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

/// Checks all ports. If any is down, logs it, watches it, and returns `false`.
async fn check_ports(ctx: &mut Ctx, after: &str) -> bool {
    let checked = Instant::now();
    let unreachable = preflight(ctx).await;
    log::info!(
        "{}   {after}: ports down {unreachable:?}; {}",
        uptime(),
        heap()
    );
    if unreachable.is_empty() {
        return true;
    }
    log::error!("{} STOPPED {after}: ports down {unreachable:?}", uptime());
    watch(ctx, checked).await;
    false
}

// ---------------------------------------------------------------------------------------------
// Stage P: download_prefix

/// Connects bambino's `FtpsClient` through the pool.
async fn connect_ftps(ctx: &mut Ctx) -> Result<Ftps, String> {
    ctx.wdt.feed();
    let raw = match embassy_time::with_timeout(DIAL_BOUND, ctx.factory.dial(PRINTER_IP, FTPS_PORT))
        .await
    {
        Err(_) => return Err(alloc::format!("control dial >{}s", DIAL_BOUND.as_secs())),
        Ok(Err(e)) => return Err(alloc::format!("control dial failed {e:?}")),
        Ok(Ok(raw)) => raw,
    };
    call(
        "FtpsClient::connect",
        FtpsClient::connect(
            raw,
            ctx.connector(),
            ctx.factory,
            identity(),
            EmbassyTimer,
            bambino::ftps::TlsVersionCheck::Enforce,
        ),
    )
    .await
}

async fn disconnect_ftps(ftps: Ftps) {
    let _ = embassy_time::with_timeout(CALL_BOUND, ftps.disconnect()).await;
}

/// One cycle: a prefix read, then a timed fresh login and listing. Returns the cycle's summary.
async fn cycle(ctx: &mut Ctx, path: &str, buf: &mut [u8], n: usize) -> Result<String, String> {
    let mut ftps = connect_ftps(ctx).await?;
    let got = call("download_prefix", ftps.download_prefix(path, buf)).await?;
    let poisoned = ftps.is_poisoned();
    if n == 1 {
        log::info!(
            "{}   P: prefix SHA-256 {} ({got} B, starts {:02x?})",
            uptime(),
            sha256_hex(&buf[..got]),
            &buf[..got.min(8)]
        );
    }
    // A poisoned client skips QUIT and the TLS close: the printer has already closed the control
    // connection.
    disconnect_ftps(ftps).await;

    let started = Instant::now();
    let mut fresh = connect_ftps(ctx).await?;
    let listed = call("list /", fresh.list_directory("/", listing_time())).await?;
    let relogin_ms = started.elapsed().as_millis();
    disconnect_ftps(fresh).await;

    let stopped = if got < buf.len() {
        "FILE SHORTER THAN THE BUFFER: not an early stop"
    } else if poisoned {
        "stopped early, client poisoned"
    } else {
        "FULL BUFFER BUT CLIENT NOT POISONED"
    };
    Ok(alloc::format!(
        "{got} B, {stopped}; fresh login + LIST ({} entries) {relogin_ms} ms",
        listed.len()
    ))
}

async fn stage_p(ctx: &mut Ctx, path: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut buf = alloc::vec![0u8; PREFIX_BYTES];
    log::info!(
        "{} --- stage P: {CYCLES} prefix reads; {} ---",
        uptime(),
        heap()
    );
    for n in 1..=CYCLES {
        let result = match cycle(ctx, path, &mut buf, n).await {
            Ok(r) => r,
            Err(e) => alloc::format!("FAILED: {e}"),
        };
        log::info!("{}   P cycle {n}: {result}", uptime());
        results.push((alloc::format!("P{n}"), result));
        sleep_fed(SPACING, &mut ctx.wdt).await;
    }
    let started = Instant::now();
    for at in CHECKPOINTS {
        let wait = Duration::from_secs(at).checked_sub(started.elapsed());
        if let Some(wait) = wait {
            sleep_fed(wait, &mut ctx.wdt).await;
        }
        let label = alloc::format!("+{at}s after the last cycle");
        let up = check_ports(ctx, &label).await;
        results.push((
            label,
            String::from(if up {
                "all ports accepting"
            } else {
                "A PORT STOPPED ACCEPTING (see above)"
            }),
        ));
        if !up {
            break;
        }
    }
    results
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

    log::info!("embassy-hw-probe: FtpsClient::download_prefix on embassy (#629)");

    let Some(path) = FTPS_PATH else {
        log::error!(
            "STOPPED: PROBE_FTPS_PATH is not set. Add it to embassy-hw-probe/.env (a .3mf on the \
             card larger than {PREFIX_BYTES} B), rebuild and flash."
        );
        loop {
            Timer::after(Duration::from_secs(60)).await;
        }
    };

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
        mk_static!(StackResources<8>, StackResources::<8>::new()),
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

    // A port already down would make the stage measure nothing.
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

    let results = stage_p(&mut ctx, path).await;
    ctx.wdt.disable();

    log::info!("================ download_prefix on embassy ================");
    for (step, result) in &results {
        log::info!("  {step}: {result}");
    }
    log::info!("============================================================");

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
