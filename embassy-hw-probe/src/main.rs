//! First-ever execution of bambino's **embassy** backend on real hardware (GitHub issue
//! #292).
//!
//! Everything in `src/io/embassy.rs` has until now been verified only by mock tests and by
//! `tests/embassy_tls_version_test.rs`, which compiles the real `EmbassyTlsConnector` on the
//! host against a loopback rustls server. That proves the `mbedtls-rs` wiring and the
//! `negotiated_version` mapping and nothing about the embedded stack: no esp-hal, no
//! esp-radio, no embassy-net, no printer. This probe is the first thing that runs the
//! backend where it is meant to run.
//!
//! **Read the stage lines, not just the last one.** Most of the work here is stack bring-up
//! that happens before any bambino code executes, so a failure's stage number is the first
//! thing that tells you whether the crate under test was even reached. Stages 0-2 are
//! esp-hal/esp-radio/embassy-net; stage 3 onward is bambino.
//!
//! Stages:
//!
//! 0. Wi-Fi associates and embassy-net gets a DHCP lease.
//! 1. `EmbassyRawStreamFactory` dials the printer's MQTT port — plain TCP reachability,
//!    before any TLS.
//! 2. `EmbassyTlsConnector` completes a real TLS handshake against the printer and
//!    `negotiated_version` reports what was negotiated (expect `Tls12` on a P1/X1).
//! 3. `PrinterClient::connect_mqtt` + one decoded telemetry event.
//! 4. `FtpsClient::connect` + one `list_directory`.
//! 5. `EmbassyTimer` is monotonic and `TimerProvider::sleep` paces correctly.
//!
//! Heap headroom is logged between stages. `mbedtls-rs` allocates 16 KiB in + 16 KiB out
//! per `Session` by default, and three sessions are live at once by stage 4 — running out
//! of heap there is the single most likely hardware-only failure, and the numbers this
//! prints are what should go back into the `mbedtls-rs` dependency comment in the root
//! `Cargo.toml`.
//!
//! **Do not self-verify from this file.** Per `.claude/rules/wire-framing-hardware-
//! verification.md`, whoever runs the probe reports the transcript; an agent editing this
//! file cannot claim any of the above was confirmed.

#![no_std]
#![no_main]

extern crate alloc;

use bambino::client::PrinterClient;
use bambino::ftps::FtpsClient;
use bambino::ftps::parser::CurrentDateTime;
use bambino::identity::PrinterIdentity;
use bambino::io::embassy::{EmbassyRawStreamFactory, EmbassyTimer, EmbassyTlsConnector};
use bambino::io::{RawStreamFactory, TimerProvider, TlsConnector};

use embassy_executor::Spawner;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Runner, StackResources};
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
const ACCESS_CODE: &str = env!("PROBE_ACCESS_CODE");

/// Implicit-FTPS control port and MQTT port, as `src/ftps/protocol.rs` and
/// `src/mqtt/protocol.rs` use them. Spelled out here rather than imported because both are
/// `pub(crate)` in the library.
const MQTT_PORT: u16 = 8883;
const FTPS_PORT: u16 = 990;

/// TCP buffer sizes for the pools below. 2048 each matches the `EmbassyRawStreamFactory`
/// defaults documented in the README's Embassy section — raising them is the first thing to
/// try if a TLS record stalls, and the first thing to lower if the heap runs out.
const TX_SZ: usize = 2048;
const RX_SZ: usize = 2048;

/// `list_directory`'s year-rollover reference. This must be the **printer's** clock, not
/// the host's, and this probe has no way to learn it (that needs `MDTM`, which
/// `bambino-cli files clock-check` uses). The listing is still a valid stage-4 result — a
/// wrong reference only mis-stamps the *year* of entries whose `LIST` line omitted it, and
/// `FtpFile::year_is_inferred` marks exactly those. Don't read years off this run.
const LIST_CLOCK_REFERENCE: CurrentDateTime = CurrentDateTime {
    year: 2026,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
};

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: StaticCell<$t> = StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

/// Logs a stage banner plus the heap headroom at that point.
///
/// Heap is printed per stage rather than once at the end because the interesting number is
/// the *low-water mark* across the run, and the run is expected to fail partway through the
/// first few times it is flashed.
macro_rules! stage {
    ($n:literal, $($arg:tt)*) => {{
        log::info!("=== stage {}: {} ===", $n, format_args!($($arg)*));
        log::info!("heap: {}", esp_alloc::HEAP.stats());
    }};
}

/// The chip TRNG, presented as the `CryptoRng` that `mbedtls_rs::Tls::new` demands.
///
/// esp-hal's `Trng` already implements rand_core 0.10's `TryRng`, so this wrapper exists for
/// one reason: to assert `TryCryptoRng`, the marker that promises the stream is suitable for
/// key material. That promise is true here and is exactly what
/// `tests/embassy_tls_version_test.rs`'s `TestRng` cannot make — its SplitMix64 stand-in is
/// deterministic and is documented as never to be lifted into anything real. This is the
/// real thing: entropy from the hardware source, live only while the `TrngSource` is.
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

/// Fresh identity per stage. `PrinterIdentity` is `Clone`, but each stage constructing its
/// own keeps the credential in one place and makes a stage individually deletable when the
/// next investigation edits this file.
fn identity() -> PrinterIdentity {
    PrinterIdentity::new(PRINTER_IP, PRINTER_SERIAL, ACCESS_CODE)
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // Two heaps: the reclaimed ROM region first, then DRAM. MbedTLS is the reason the DRAM
    // figure is this large — 32 KiB of record buffers per live `Session`, three of them by
    // stage 4, on top of embassy-net's own buffers. If stage 4 dies with an allocation
    // failure, this is the number to raise (or shrink the sessions via `mbedtls-rs`'s
    // `ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` features).
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 110 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    stage!(0, "Wi-Fi association and DHCP");

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

    // The stack seed only needs to be unpredictable, not cryptographic — it salts TCP ISNs
    // and the DHCP xid. The TRNG below is built afterwards and reserved for TLS.
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
        Some(config) => log::info!("stage 0 OK: DHCP lease {}", config.address),
        // `wait_config_up` returning without a v4 config would mean the stack came up on a
        // protocol this build doesn't enable; treat it as a hard stop rather than proceeding
        // into stages that will fail confusingly.
        None => panic!("stage 0 FAILED: stack up but no IPv4 config"),
    }

    // TRNG from here on. `TrngSource` owns ADC1 for as long as it lives, and `Trng::try_new`
    // fails while no source is active — so this must outlive every TLS session, which is the
    // whole program. Leaking it into a `StaticCell` is how that lifetime is stated.
    let _trng_source: &'static mut TrngSource<'static> = mk_static!(
        TrngSource<'static>,
        TrngSource::new(peripherals.RNG, peripherals.ADC1)
    );
    let rng: &'static mut HwRng = mk_static!(HwRng, HwRng(Trng::try_new().expect("TRNG source")));

    // The one MbedTLS instance permitted per process. `src/io/CLAUDE.md` records that a
    // second `Tls::new` returns `AlreadyCreated`, which is why `EmbassyTlsConnector` holds a
    // `TlsReference` rather than a `Tls`; the three connectors below share this one, which
    // is precisely the case that design exists for.
    let tls: &'static mut mbedtls_rs::Tls<'static> = mk_static!(
        mbedtls_rs::Tls<'static>,
        mbedtls_rs::Tls::new(rng).expect("only one Tls instance may exist program-wide")
    );

    // One pool per concurrent connection. MQTT holds its socket open for the life of the
    // client, so it cannot share a pool with FTPS; FTPS needs two slots of its own because
    // its control channel stays open while a data channel is checked out for a transfer.
    let mqtt_pool: &'static TcpClient<'static, 1, TX_SZ, RX_SZ> = mk_static!(
        TcpClient<'static, 1, TX_SZ, RX_SZ>,
        TcpClient::new(
            stack,
            mk_static!(TcpClientState<1, TX_SZ, RX_SZ>, TcpClientState::new())
        )
    );
    let ftps_pool: &'static TcpClient<'static, 2, TX_SZ, RX_SZ> = mk_static!(
        TcpClient<'static, 2, TX_SZ, RX_SZ>,
        TcpClient::new(
            stack,
            mk_static!(TcpClientState<2, TX_SZ, RX_SZ>, TcpClientState::new())
        )
    );

    stage!(1, "raw TCP dial to {PRINTER_IP}:{MQTT_PORT}");
    {
        let factory = EmbassyRawStreamFactory::new(mqtt_pool);
        match factory.dial(PRINTER_IP, MQTT_PORT).await {
            Ok(stream) => {
                log::info!("stage 1 OK: TCP connected");
                // Returns its pool slot on drop — stage 2 needs it back.
                drop(stream);
            }
            Err(e) => panic!("stage 1 FAILED: dial: {e:?}"),
        }
    }

    stage!(2, "TLS handshake against {PRINTER_IP}:{MQTT_PORT}");
    {
        let factory = EmbassyRawStreamFactory::new(mqtt_pool);
        let connector = EmbassyTlsConnector::new(tls.reference());
        let raw = factory
            .dial(PRINTER_IP, MQTT_PORT)
            .await
            .expect("stage 2 FAILED: dial");

        // Verification stays off, matching the crate's unsafe-by-default convention: printer
        // certs chain to a private BBL CA, and this probe deliberately does not vendor
        // anchors. `peer_chain_der` is `None` on this backend by design (`mbedtls-rs`
        // exposes no peer-cert accessor), so a TOFU capture is not available here — that is
        // recorded behaviour, not a gap for this probe to close.
        //
        // No timeout race: `EmbassyTlsConnector::connect` has no bounded-connect loop of its
        // own, so a hang here is the documented behaviour of a handshake that never
        // completes, and seeing it hang is more informative on a first run than a timeout
        // that hides where it stopped.
        match connector.connect(PRINTER_SERIAL, raw).await {
            Ok(session) => {
                log::info!(
                    "stage 2 OK: handshake complete, negotiated_version = {:?}",
                    connector.negotiated_version(&session)
                );
                drop(session);
            }
            Err(e) => panic!("stage 2 FAILED: handshake: {e:?}"),
        }
    }

    stage!(3, "MQTT connect and one telemetry event");
    {
        let mut printer = PrinterClient::new(
            EmbassyTlsConnector::new(tls.reference()),
            EmbassyRawStreamFactory::<1, TX_SZ, RX_SZ>::new(mqtt_pool),
            identity(),
        )
        .with_timer(EmbassyTimer)
        .with_connect_timeout(15);

        match printer.connect_mqtt().await {
            Ok(()) => log::info!("stage 3: MQTT connected"),
            Err(e) => panic!("stage 3 FAILED: connect_mqtt: {e:?}"),
        }

        // Printers publish unprompted, but not necessarily soon; `pushall` makes the wait
        // bounded by the printer's response rather than by its reporting interval.
        if let Err(e) = printer.request_pushall().await {
            log::warn!(
                "stage 3: request_pushall failed ({e:?}); waiting for an unsolicited report"
            );
        }

        match embassy_time::with_timeout(Duration::from_secs(30), printer.poll_telemetry()).await {
            Ok(Ok(event)) => log::info!("stage 3 OK: telemetry event decoded: {event:?}"),
            Ok(Err(e)) => panic!("stage 3 FAILED: poll_telemetry: {e:?}"),
            Err(_) => panic!("stage 3 FAILED: no telemetry within 30s"),
        }
    }

    stage!(4, "FTPS connect and list_directory");
    {
        let control = EmbassyRawStreamFactory::<2, TX_SZ, RX_SZ>::new(ftps_pool)
            .dial(PRINTER_IP, FTPS_PORT)
            .await
            .expect("stage 4 FAILED: control dial");

        let mut ftps = match FtpsClient::connect(
            control,
            EmbassyTlsConnector::new(tls.reference()),
            EmbassyRawStreamFactory::<2, TX_SZ, RX_SZ>::new(ftps_pool),
            identity(),
            EmbassyTimer,
            // TLS-1.2 enforcement stays on. On a P2S/X2D this is the check that
            // `negotiated_version` had to be fixed for (#289); letting it run is the point.
            false,
        )
        .await
        {
            Ok(client) => client,
            Err(e) => panic!("stage 4 FAILED: FtpsClient::connect: {e:?}"),
        };

        match ftps.list_directory("/", LIST_CLOCK_REFERENCE).await {
            Ok(files) => {
                log::info!("stage 4 OK: {} entries in /", files.len());
                for file in files.iter().take(5) {
                    log::info!("  {file:?}");
                }
            }
            Err(e) => panic!("stage 4 FAILED: list_directory: {e:?}"),
        }
    }

    stage!(5, "EmbassyTimer monotonicity and pacing");
    {
        let timer = EmbassyTimer;
        let before = timer.now_millis();
        timer
            .sleep(core::time::Duration::from_millis(500))
            .await
            .expect("stage 5 FAILED: sleep");
        let after = timer.now_millis();
        let elapsed = after.saturating_sub(before);

        // A 500ms sleep that returns in 0ms would mean `now_millis` is not advancing, which
        // silently disables every timeout in the crate rather than failing loudly — the
        // reason this stage exists at all.
        assert!(
            after > before,
            "stage 5 FAILED: now_millis did not advance ({before} -> {after})"
        );
        log::info!("stage 5 OK: 500ms sleep measured {elapsed}ms");
    }

    log::info!("=== all stages complete ===");
    log::info!("final heap: {}", esp_alloc::HEAP.stats());

    // Standard bare-metal convention: `main` never returns. Ctrl-C detaches the monitor.
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
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
