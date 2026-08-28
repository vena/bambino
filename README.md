# bambino

Async Rust library for talking to Bambu Lab 3D printers over your local network. No Bambu Cloud, just direct MQTT, FTPS, and camera access from one codebase that compiles to desktop, ESP32 (ESP-IDF), and bare-metal (Embassy) targets.

**🤖 DISCLOSURE:** This was built with heavy assistance from AI. This exists because I wanted it for a personal project, and I barely know Rust myself! 3D printers are expensive and deal with [high temperatures](#safety-notice), so bear this in mind before you unleash my slop upon your baby. 

This project is not affiliated with or supported by Bambu Lab.

Huge shout-out to the projects in [Acknowledgements](#acknowledgements), without which I wouldn't have gotten far.

## What it does

- **Discovery**: find printers on your LAN via SSDP (ports 2021/1990)
- **MQTT control**: connect to the printer's local broker (port 8883), send commands, receive telemetry
- **File transfer**: implicit FTPS (port 990) for listing, uploading, downloading, and managing files on the SD card
- **Camera**: binary JPEG streaming client (port 6000, A1/P1 series, A2L); RTSPS helpers for proxy integration and timestamp correction (port 322, X1/X2/H2/P2S series)
- **Model quirks**: per-model differences handled polymorphically: FTPS TLS requirements, fan step resolution, Z-axis homing safety, door sensors, camera protocols, nozzle counts (single/IDEX/tool changer), temperature limits, and chamber heater capabilities

## Two levels of API

`PrinterClient` is the high-level interface; it wraps MQTT (and optionally FTPS) with model-aware safety checks: temperature clamping to hardware limits, Z-axis homing validation, chamber heater capability guards, fan routing to the right controller, and automatic K-profile priming. Most users should start here.

For advanced use cases, `PrinterClient::mqtt().await?` and `PrinterClient::storage().await?` provide direct access to the underlying `MqttClient` and `FtpsClient` respectively, auto-connecting if needed. Use `mqtt()` to send custom MQTT payloads, manage zombie detection, or inspect in-flight state. Note that raw payloads bypass `PrinterClient`'s model-aware safety checks.

The underlying modules (`mqtt`, `ftps`, `discovery`, `camera`) are also public if you need direct protocol access; useful for custom integrations, firmware exploration, or when `PrinterClient` doesn't cover your use case.

## Quick start

Not on crates.io yet, depend on it from GitHub:

```toml
[dependencies]
bambino = { git = "https://github.com/vena/bambino" }
```

### Discover printers

```rust
use bambino::discovery::discover_devices;
use bambino::io::tokio::{TokioTimer, TokioUdpSocket};
use std::time::Duration;

let timer = TokioTimer::new();
// Allow at least 20s: the P1S ignores M-SEARCH on port 2021 and is found only via its
// ~10.1s NOTIFY advertisements, so a shorter window returns empty results intermittently.
let printers = discover_devices::<TokioUdpSocket, _>(
    Duration::from_secs(20),
    &timer,
).await?;

for p in &printers {
    println!("{} ({:?}) at {}", p.name, p.model, p.ip);
}
```

### Connect

```rust
use bambino::client::PrinterClient;
use bambino::identity::PrinterIdentity;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};

// Printer certs chain to BBL's private CA, absent from OS trust stores;
// skip verification unless you can supply that CA (see "TLS configuration")
let config = build_unsafe_client_config();
let tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

// `new` derives the model from the serial prefix; construct the struct literal
// directly if you need to override that.
let identity = PrinterIdentity::new(ip, serial, access_code);
let model = identity.model;
let mut printer = PrinterClient::new(tls, TokioRawStreamFactory, identity)
    .with_timer(TokioTimer::new())
    .with_connect_timeout(5); // seconds; 0 disables the timeout

// MQTT connects lazily on first use, or eagerly:
printer.connect_mqtt().await?;
```

If you already have a connected `MqttClient` (tests, Embassy), wrap it directly:

```rust
use bambino::client::PrinterClient;

let mut printer = PrinterClient::from_mqtt(mqtt_client, model);
```

### Send commands

```rust
printer.request_pushall().await?;              // request full state dump
                                               // warning: calling this too often may slow older models!
printer.home_axes(false).await?;               // false = bare G28; true = Z-only (rejected on bed-on-Z models)
printer.set_bed_temperature(60).await?;        // clamped to model max
printer.set_nozzle_temperature(0, 220).await?; // nozzle 0 at 220°C
printer.set_led("chamber_light", true).await?; // turn on the chamber light
printer.send_gcode("M106 P1 S255").await?;     // with PrinterClient, gcode is checked against unsafe homing
```

### Read telemetry

```rust
use bambino::client::TelemetryEvent;

loop {
    match printer.poll_telemetry().await? {
        TelemetryEvent::Report(report, _raw) => {
            let (bed_actual, bed_target) = report.bed_temperatures();
            if let Some(print) = &report.print {
                println!(
                    "{:?} — bed {}°C/{}°C — {:?}%",
                    print.gcode_state, bed_actual, bed_target, print.mc_percent
                );
            }
        }
        TelemetryEvent::Unknown(_) => {}
    }
}
```

> **Timestamps:** printer-stamped time fields in telemetry come from an unsynced clock; see [Timestamps on LAN-mode printers](#timestamps-on-lan-mode-printers).

### Print jobs

```rust
use bambino::mqtt::PrintJobConfig;

let config = PrintJobConfig::new(
    "job.3mf",                      // file on SD card
    "Metadata/plate_1.gcode",       // plate gcode path inside the 3mf
    "My Print",                     // task name
    12345,                          // subtask ID
    "textured",                     // bed type
)
// One entry per filament the plate uses, in slicer order. Each value is a flat channel ID
// (`ams_id * 4 + slot_id`, so 0..=15 for standard AMS, 128..=135 for AMS-HT) or -1 for
// "not fed from the AMS". Out-of-range values are folded to -1 with a warning.
.with_ams(vec![0, -1, 1]);

printer.start_print(&config).await?;
```

Bed leveling, flow calibration, and vibration compensation run automatically as part of the print (all enabled by default in `PrintJobConfig`); vibration compensation is forced off on models that don't support it, regardless of the config. Use the builder methods to change them—they take either a `bool` or a `CalibrationMode` (`Off`/`On`/`Auto`, matching BambuStudio's tri-state encoding):

```rust
use bambino::mqtt::CalibrationMode;

let config = PrintJobConfig::new("job.3mf", "Metadata/plate_1.gcode", "My Print", 12345, "textured")
    .bed_leveling(CalibrationMode::Auto) // skip if leveled recently, matching BambuStudio's default
    .flow_calibration(false)
    .timelapse(false);
```

### Standalone calibration

Run calibration routines outside of a print:

```rust
use bambino::client::CalibrationOption;

printer.start_calibration(
    CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
).await?;
```

### AMS filament control

Both AMS calls take flat positional arguments; the addressing sentinels matter more than the
ordering, so they're spelled out here.

```rust
// change_filament(ams_id, slot_id, curr_temp, tar_temp)
//   ams_id:    0..=3 standard AMS unit · 128..=135 AMS-HT bus ID · 254/255 external spool
//   slot_id:   0..=3 slot within the unit · 254 external-spool load · 255 unload/retract
//   temps:     nozzle current/target in °C; -1 lets the firmware decide
printer.change_filament(0, 1, -1, -1).await?;   // load AMS 0, slot 1, firmware picks temps
printer.change_filament(0, 255, -1, -1).await?; // unload whatever AMS 0 currently has loaded

// start_drying(ams_id, temp, duration_hours, humidity, rotate_tray, cooling_temp,
//              close_power_conflict, filament)
//   temp:                 °C, clamped to 85 for AMS-HT bus IDs (128..=135), 65 otherwise
//   duration_hours:       hours, not minutes
//   humidity:             target %; 0 = firmware default
//   rotate_tray:          rotate trays during the cycle
//   cooling_temp:         °C to cool down to once drying finishes
//   close_power_conflict: override the unit's power-conflict interlock
//   filament:             filament type string, for the unit's own display/logic
printer.start_drying(0, 55, 8, 0, true, 20, false, "PA-CF").await?;
printer.stop_drying(0).await?;                  // ams_id only—every other field is zeroed
```

`start_drying` returns `Error::ModelMismatch` on P1P/P1S: that firmware acks the command and
then silently discards it instead of driving the AMS heater.

### File transfer

Add FTPS to a `PrinterClient` with `.with_ftps()`. The FTPS TLS connector is independent from MQTT's—some models require different TLS settings (e.g. TLS 1.2 only for FTPS data channels).

```rust
use bambino::io::tokio::{
    TokioTlsConnector, TokioRawStreamFactory, TokioTimer,
    build_unsafe_client_config_with_options,
};

// Configure FTPS TLS (respecting model-specific requirements)
let ftps_config = build_unsafe_client_config_with_options(
    model.quirks().enforces_ftps_tls_1_2(),
);
let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

let mut printer = printer.with_ftps(ftps_tls, TokioRawStreamFactory, TokioTimer::new());

// storage() auto-connects on first call
let ftp = printer.storage().await?;
let files = ftp.list_directory("/", printer_now).await?; // printer_now: ftps::CurrentDateTime
ftp.upload_file("/model/print.3mf", &file_bytes).await?;
let data = ftp.download_file("/timelapse/video.mp4").await?;
let free = ftp.get_available_space().await?;
```

> **Direct protocol access:** For cases where `PrinterClient` isn't needed, `FtpsClient::connect()` provides standalone FTPS access; see the [`ftps`](src/ftps/) module.
>
> **Timestamps:** file dates from a LAN-mode printer are unreliable; see [Timestamps on LAN-mode printers](#timestamps-on-lan-mode-printers) below.

### Timestamps on LAN-mode printers

Bambu printers in LAN mode will almost always have the wrong date and time set internally. They do not allow manually setting their clock, and do not contact NTP in LAN mode. They do not have an internal RTC battery, and their clock is reset any time they lose power (possibly to firmware build date).

Avoid presenting printer-reported absolute times if you can, they're almost always questionable. At the time of this writing, a measured P1S—fw 01.10.00.00, release dated 2026-03-30, but reset date 2026-02-02—is 203 days behind.

Neither MQTT nor FTPS has a command that asks the printer what time it is. Every absolute timestamp the printer sends comes from this clock, so none of them can be trusted on their own. Durations are not affected; a remaining-print-time or drying-time field measures a length of time, not a point in time.

This issue must be handled differently for MQTT and FTPS.

#### Over MQTT

Telemetry is pushed as it happens, so for anything you watch occur (a state change, an alert appearing in a report), record the arrival time from your own clock. Apart from network latency that is when it happened, and it doesn't depend on the printer's clock at all.

That only covers events you were connected for. Two fields report things that may have happened well before then, and arrival time says nothing about either:

- [`PrinterTelemetry::gcode_start_time`](src/types/telemetry/report.rs): unix epoch string for when the current job started, which for a long print can be many hours before you connect.
- [`HmsEntry::ts_unix`](src/types/telemetry/diagnostics.rs): `YYYYMMDDHHMMSS` UTC string for when an HMS alert was raised, including alerts still active from earlier.

Both come from the printer's clock and may be incorrect. The only way to place them in real time is to measure the offset once with the FTPS probe described below and compare it.

[`HmsEntry::ts_boot`](src/types/telemetry/diagnostics.rs) is the one field that isn't affected: it records when HMS entry was raised in *seconds since boot*. Use it to order entries, or to work out how long ago an alert fired within the current power cycle. It resets on reboot, and has only been confirmed on X2.

#### Over FTPS

FTPS file times are harder. Files are written in the past, may have been written at different power cycle resets, and the wire format drops information: for a recently-modified file a UNIX `LIST` line sends `Aug 24 15:03` instead of `Aug 24 2025`, so the year is simply not there. It has to be reconstructed against a reference clock, and the right reference is the **printer's**, since the printer's clock is what its FTP server compared against when it decided to drop the year.

For a **single file**, models may support `MDTM` and return a modification time with an explicit four-digit year, in one control-channel round trip and without needing a reference clock:

```rust
match ftp.modification_time("/timelapse/video.mp4").await? {
    Some(t) => println!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second),
    None => { /* firmware doesn't implement MDTM; see the fallback below */ }
}
```

Confirmed on a P1S. Other models are untested, which is why the return type is an `Option`.

`MDTM` queries one file at a time, so it isn't a way to date a whole directory. It is, however, a way to read the printer's clock to use as a reference time. Upload a throwaway file and the printer stamps it from its own clock as it writes, so the answer to an `MDTM` request is the printer's current datetime:

```rust
const PROBE: &str = "/bambino_clock_probe.txt";

ftp.upload_file(PROBE, b"probe").await?;
let printer_now = ftp.modification_time(PROBE).await?;
ftp.delete_file(PROBE).await?;

if let Some(t) = printer_now {
    println!("printer says: {:04}-{:02}-{:02} {:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute);
}
```

Feed that into `list_directory`'s `now: CurrentDateTime` parameter, and it automatically reconstructs missing years against the best reference available.

> Without `MDTM`, a similar probe could be done with `list_directory` (upload, fetch list, read time of the file name you uploaded), but only partly since recent files in a `LIST` return lack the year.

Whatever reference you pass, the rule for resolving years in a `LIST` is the same: given a reference, each entry without a year is assigned to the twelve months before that reference. Order is preserved as long as every file genuinely falls in that window, so sorting a listing by date usually works even when the years themselves are wrong.

Files dated *after* the reference don't fall in it, and are read as a year older than they are. Passing the printer's real clock removes the usual causes, but not all of them; these printers reset their clock backwards on power loss, so files written before a reset can carry timestamps in the printer's own future.

Passing host time adds two more ways to land outside the window:

- **The reference is earlier than the printer's clock.** Every file newer than the reference gets pushed back a year and sorts as the oldest instead of the newest.
- **The printer's clock is more than about six months behind the reference.** The printer only omits the year on files from its own last six months. With a large enough gap, the oldest of those land more than twelve months before the reference, wrap around, and sort as though they were recent.

None of this is detectable from a listing alone. If you can't supply the printer's clock, sorting by date is still reasonable to offer, as long as you don't display the dates themselves.

**None of this recovers when a file was truly written.** The probe tells you the printer's current time, and `MDTM` tells you what the printer thought the time was when it wrote the file. If the clock is wrong, both are wrong. A good reference makes the reconstructed year *more likely to be right, but never makes it certain.* 

Entries with a reconstructed year are flagged so you can decide if and how you want to present them:

```rust
for f in &files {
    if f.year_is_inferred {
        // month/day/HH:MM are the printer's own; the year is reconstructed
    }
}
```

### Camera

Bambu printers use two different camera protocols depending on the model. Check which one with `model.quirks().camera_protocol()`.

**Binary JPEG (A1, P1 series, and A2L), port 6000.** Streams JPEG frames over a lightweight binary protocol on TLS. Through `PrinterClient`, add a camera connector the same way you add FTPS; the connection and handshake happen lazily on the first frame read:

```rust
let mut printer = printer.with_camera(camera_tls, TokioRawStreamFactory);

let mut frame = Vec::new();
loop {
    printer.read_camera_frame(&mut frame).await?; // frame is a complete JPEG image
}
```

`read_camera_frame` bounds the read against the client's timer. `camera()` hands out the
underlying stream directly, and errors immediately on RTSPS models. Use
`.with_camera_max_frame_size(bytes)` to lower the frame cap, and `.attach_camera()` to inject
an already-connected stream (tests, Embassy).

The stream type is also usable standalone:

```rust
use bambino::camera::binary::BinaryCameraStream;
use bambino::identity::PrinterIdentity;

let mut cam = BinaryCameraStream::new(tls_stream);
cam.authenticate(&PrinterIdentity::new(ip, serial, access_code)).await?;

let mut frame = Vec::new();
loop {
    cam.read_next_frame(&mut frame).await?;
    // frame is a complete JPEG image
}
```

`authenticate()` returning `Ok(())` only means the 80-byte handshake packet was written and
flushed. The protocol has no ack byte, so it does not mean the printer accepted the access
code. A bad code surfaces later, on the *next* `read_next_frame()` call, as the same
`ConnectionReset` error a plain network blip would produce; there's no way to distinguish the
two from this API alone.

`read_next_frame` rejects frames above a configurable cap (default 10MB) to guard against
unbounded allocation. Constrained (`no_std`/Embassy) targets should lower it with
`BinaryCameraStream::new(stream).with_max_frame_size(64 * 1024)` to match their actual
buffer budget as the 10MB default can exceed an embedded target's entire SRAM.

**RTSPS (X1, X2, H2, P2S series), port 322.** RTSP behind implicit TLS with Digest auth. This library provides helpers for integrating with external media players. `bambino` does not include an RTSP client.

```rust
use bambino::camera::rtsps::build_rtsps_url;

let url = build_rtsps_url(ip, access_code)?;
// → rtsps://bblp:<code>@<ip>:322/streaming/live/1
```

`build_rtsps_url` validates that `access_code` is a non-empty ASCII alphanumeric string
(matching the documented 8-character LAN access code format) and returns
`Result<String, Error>`.

Since the printer uses self-signed TLS, most players can't connect directly. The typical setup is a local proxy that accepts plain `rtsp://`, wraps it in TLS, and forwards to the printer. Use `rewrite_rtsp_request_uri` to rewrite the request-line URI in transit:

```rust
use bambino::camera::rtsps::rewrite_rtsp_request_uri;

// Player sends:  rtsp://127.0.0.1:8554/streaming/live/1
// Printer needs: rtsps://192.168.1.150:322/streaming/live/1
let rewritten = rewrite_rtsp_request_uri(player_uri, printer_ip)?;
```

`rewrite_rtsp_request_uri` only rewrites the URI text in the request line. Tt does **not**
recompute or repair an already-computed Digest `Authorization` header. It's only useful to a
proxy that acts as its own independent RTSP client toward the printer (computing its own
Digest response against the rewritten URI). A transparent relay that forwards the player's
original `Authorization` header verbatim will still get a 401, since that header's
`response=` hash was computed by the player against its own local URI.

P2S models on certain firmware versions have a bug where RTP timestamps don't advance, causing video freezes. Use `RtpTimestampCorrector` to synthesize correct timestamps when `model.quirks().requires_wallclock_rtsp_timestamps()` is true.

## TLS configuration

By default, `build_unsafe_client_config()` skips certificate verification. A printer's leaf cert carries its serial in the CN and chains to BBL's own private CA, which is in no OS trust store—so the default exists because most callers have no anchor to verify against, not because the cert is unverifiable. If you hold the BBL CA certs (or provision your own), `build_verified_client_config()` performs a real chain-of-trust, handshake-signature, and CN-identity check; this has been confirmed end-to-end against a live P1S over both MQTT (8883) and FTPS (990):

> This section covers the `tokio` backend. ESP-IDF chooses its trust anchor differently and needs target configuration to skip verification at all; see "ESP-IDF certificate verification" under [Platform targets](#platform-targets).

```rust
use bambino::io::tokio::{build_verified_client_config, TokioTlsConnector};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

let ca_cert = CertificateDer::from_pem_file("printer-ca.pem").unwrap();

// Server-only verification
let config = build_verified_client_config(vec![ca_cert], None).unwrap();

// Mutual TLS (mTLS)
let client_cert = CertificateDer::from_pem_file("client.pem").unwrap();
let client_key = PrivateKeyDer::from_pem_file("client-key.pem").unwrap();
let config = build_verified_client_config(
    vec![ca_cert],
    Some((vec![client_cert], client_key)),
).unwrap();

let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
```

`build_verified_client_config()` validates the printer's certificate against the given CA root(s) and checks its identity against the printer's serial number, falling back to Subject CN when no Subject Alternative Name is present (matching mbedtls's behavior on ESP-IDF/Embassy). `build_unsafe_client_config()` is unaffected, it never checks certificate identity.

Both functions have `_with_options` variants that accept `force_tls_1_2: bool`. Two models (P2S and X2D) need FTPS capped to TLS 1.2, but not because the protocol demands it: it's a firmware bug in their embedded vsFTPd (confirmed for P2S via an independent reverse-engineering project's own bug report; assumed-by-analogy for X2D, whose actual root cause is still unconfirmed). Check with `model.quirks().enforces_ftps_tls_1_2()`. `FtpsClient::connect()` fails closed on those models: it errors unless `negotiated_version` reports exactly `Some(TlsVersion::Tls12)` (an undetermined `None` also rejects; never a silent pass-through).

This is platform-general: `TokioTlsConnector` and `EspIdfTlsConnector` implement `negotiated_version` for real. `EmbassyTlsConnector` cannot, so Embassy + P2S/X2D is unconditionally rejected by this check rather than downgraded; see the Embassy TLS section below for why, and for the opt-out.

`PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` opts out of the check entirely instead: `require_tls_1_2_if_enforced` logs a warning and returns `Ok(())` unconditionally, regardless of what (if anything) `negotiated_version` reports. This is a reliability tradeoff, not a safety hole: `upload_file`'s `SIZE` recheck and `download_file`'s unconditional `SIZE` recheck (run on both the `226` and `426` completion replies) already catch a truncated/corrupted transfer independently of this flag, so bypassing the version check risks more failed transfers/retries against P2S/X2D, never silently-corrupt data. Default is `false` (fail closed, unchanged). Only meaningful for the `embassy` feature. On `tokio`/`esp-idf`, use `force_tls_1_2` on the `TlsConnector` instead, since those platforms can actually negotiate TLS 1.2 for real.

## Platform targets

The default feature set (`tokio`) targets desktop/server. For embedded, swap the feature flag:

```toml
# ESP32 with ESP-IDF (std)
bambino = { git = "https://github.com/vena/bambino", default-features = false, features = ["esp-idf"] }

# Bare-metal with Embassy (no_std + alloc)
bambino = { git = "https://github.com/vena/bambino", default-features = false, features = ["embassy"] }
```

All network I/O goes through abstract traits (`AsyncIo`, `TlsConnector`, `TimerProvider`, etc.) so library code is platform-agnostic. Platform-specific implementations live in `io::tokio`, `io::esp_idf`, and `io::embassy`.

**Embassy note:** `discover_devices()` is not available on Embassy. The convenience function needs to bind its own UDP sockets, which Embassy can't do (sockets must be pre-allocated from the network stack). Use `DiscoveryEngine::new()` with a pre-bound `EmbassyUdpSocket` for manual discovery, or provide a pre-configured printer IP.

**Embassy TLS:** `EmbassyTlsConnector` wraps `mbedtls-rs` (real TLS 1.2+1.3, hardware-accelerated crypto on ESP32 targets). MbedTLS only permits one active library instance program-wide, so construct a single `mbedtls_rs::Tls` once at startup and hand out cheap `Copy` `TlsReference`s to as many connectors as you need (e.g. one for MQTT, one for FTPS's control channel, one for FTPS's data channel):

```rust
use bambino::io::embassy::EmbassyTlsConnector;
use mbedtls_rs::Tls;
use static_cell::StaticCell;

static TLS: StaticCell<Tls<'static>> = StaticCell::new();

// `rng` must be `&'static mut (dyn rand_core::CryptoRng + Send)`, e.g. another
// `StaticCell`-held hardware TRNG wrapper.
let tls: &'static mut Tls<'static> =
    TLS.init(Tls::new(rng).expect("only one Tls instance may exist program-wide"));

let mqtt_tls = EmbassyTlsConnector::new(tls.reference());
let ftps_tls = EmbassyTlsConnector::new(tls.reference());
```

There's no buffer-consumption limit. `connect()` can be called repeatedly on the same connector (`mbedtls-rs` allocates its own 16 KiB in/out record buffers per session). Certificate verification defaults to off, matching this crate's unsafe-by-default convention elsewhere; call `.with_ca_chain(cert)` to enable it, or `.with_client_credentials(creds)` for mTLS.

**`negotiated_version` always returns `None`**. `mbedtls-rs` exposes no API to read back the negotiated TLS version, so `FtpsClient`'s TLS-1.2 enforcement check for P2S/X2D still fails closed under Embassy even with this real-TLS-1.2-capable backend (nothing forces the handshake to actually land on 1.2 over 1.3). Use `PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` to opt out of that check when talking to those two models under Embassy; see the "TLS configuration" section above.

**Embassy raw streams:** `EmbassyRawStreamFactory` wraps `embassy_net`'s own `TcpClient`/`TcpClientState` connection pool, used for both MQTT's lazy connect and FTPS's data channel. `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates `N` buffer pairs; `N = 1` covers FTPS's usage, since data-channel connections are always sequential (MQTT needs its own factory instance since it's a separate, concurrent connection). Both need `'static` storage (`static_cell::StaticCell` is the standard way to get that; it's not a bambino dependency). `dial`'s host must be a literal IPv4 address. Bambu printers are always addressed that way, so this isn't a limitation in practice:

```rust
use bambino::io::embassy::{EmbassyRawStreamFactory, EmbassyTimer};
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use static_cell::StaticCell;

static TCP_CLIENT_STATE: StaticCell<TcpClientState<1, 2048, 2048>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 1, 2048, 2048>> = StaticCell::new();

let state = TCP_CLIENT_STATE.init(TcpClientState::new());
let client = TCP_CLIENT.init(TcpClient::new(stack, state));
let factory = EmbassyRawStreamFactory::new(client);

let mut printer = printer.with_ftps(ftps_tls, factory, EmbassyTimer);
```

**ESP-IDF TLS timeouts:** `EspIdfTlsConnector` runs the handshake and all reads/writes in non-blocking mode, polling every 20ms on `WANT_READ`/`WANT_WRITE`/`EWOULDBLOCK`, so a `TimerProvider`-based timeout (e.g. `poll_until`) can actually preempt a stuck handshake or read/write instead of blocking forever on FFI. `PrinterClient::with_connect_timeout()` and `EspIdfTlsConnector::with_connect_timeout()` are two independent budgets on this platform. The connector is opaque by the time it reaches `PrinterClient::new()`, so setting one doesn't affect the other. Set both explicitly and keep them in sync (including `0`, which disables the timeout on either).

**Expect `esp-tls` warnings during a normal ESP-IDF handshake.** `EspIdfTlsConnector::connect` pins `Config::timeout_ms = 0` so each `negotiate()` performs exactly one handshake step and then yields. That is what makes the 20ms poll interval and the connect deadline meaningful, since otherwise esp-tls busy-spins internally for up to 4s per call. The cost is that esp-tls logs `W esp-tls: Failed to open new connection in specified timeout` on every step that doesn't complete the handshake, roughly 55 lines for a typical 1.3s connect. They are expected and harmless: a real failure surfaces as `SocketError::TimedOut` from `connect`, not as these warnings. Raise the `esp-tls` tag's log level if they are noisy.

**Faster ESP32 handshakes: pick a curve the chip can accelerate.** Add this to your `sdkconfig.defaults`:

```
CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED=n
```

By default mbedTLS asks for Curve25519 first and the printer agrees to it, but the ESP32's crypto accelerator only handles P-192 and P-256 — so the key exchange runs in software. Turning Curve25519 off leaves P-256 at the front of the list, which the hardware does accelerate. Measured on an ESP32-C6 against a P1S, changing nothing else: client-side crypto dropped from 409 ms to 180 ms, and the whole handshake from 1318 ms to 1136 ms. Reproduced across two runs, and all three of the printer's TLS services (MQTT, FTPS, camera) connect normally without it.

Two things to check before you rely on it. This was only tested against a P1S, and printer models differ in their TLS behavior — P2S and X2D already need special handling elsewhere in this README. And the setting applies to your entire firmware image: if your device also talks to a server that requires Curve25519, that connection will break. It is a compile-time option, so it cannot be varied per connection.

There is no equivalent knob for the ~800 ms the printer itself takes to reply, which is the larger half of the handshake and is the same from a laptop. See [REF-NET-SECURE](reference/01_network_discovery.md) for the full measured breakdown, including why TLS session resumption does not help (the printer offers a session ID and then refuses to resume it).

**ESP-IDF TLS version query:** `EspIdfTlsConnector::negotiated_version` reads the real negotiated version via `esp_tls_get_ssl_context()` + mbedTLS's `mbedtls_ssl_get_version()`. Assumes the default mbedTLS backend (`CONFIG_ESP_TLS_USING_MBEDTLS=y`); wolfSSL builds aren't supported yet.

**ESP-IDF FTPS/MQTT:** `EspIdfTlsConnector` wraps an already-connected raw stream (via `esp_idf_svc::tls::EspTls::adopt()`), used for FTPS's control/data channels and MQTT's lazy connect alike, paired with `EspIdfRawStreamFactory` for the raw dial. Models where `model.quirks().uses_plaintext_ftps_data_channel()` is true skip TLS on the FTPS data channel entirely:

```rust
use bambino::io::esp_idf::{EspIdfTlsConnector, EspIdfRawStreamFactory, EspIdfTimer};

// Prefer with_certs; see "ESP-IDF certificate verification" below for why
// EspIdfTlsConnector::new() additionally requires two sdkconfig options.
// Takes every anchor you want to trust, not just one (DER, one Vec per cert).
let ftps_tls = EspIdfTlsConnector::with_certs(ca_certs, None);
let mut printer = printer.with_ftps(ftps_tls, EspIdfRawStreamFactory, EspIdfTimer::new()?);
```

**ESP-IDF certificate verification:** ESP-IDF picks exactly one trust-anchor *source*, checking them in a fixed order with mutually exclusive branches: its bundled public root CAs first, then a caller-supplied CA, then no verification at all. `esp-idf-svc` defaults the bundle **on** wherever `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is enabled, so this crate turns it off explicitly: a printer certificate chains to BBL's private CA, never to a public root, and leaving the default on would silently ignore the CAs you passed to `with_certs`.

`EspIdfTlsConnector::with_certs(ca_certs, client_auth)` is therefore the recommended path on this platform. The CAs you supply become the sole trust anchors and need no sdkconfig changes. Certificates are a runtime input; none are embedded in this crate.

It accepts an iterator of DER certificates rather than a single one, matching the tokio backend, because Bambu is mid-PKI-rollover: a P1S chains to the legacy `BBL CA` root, while newer models chain through a `BBL Device CA <model>-V2` intermediate to `BBL CA2 RSA`/`BBL CA2 ECC`, so covering the model range means trusting several roots at once. The certs are re-encoded internally into a single NUL-terminated PEM bundle, the only form mbedTLS parses as more than one certificate; concatenated DER would silently load just the first.

`EspIdfTlsConnector::new()` skips verification, which on ESP-IDF requires **both** of these in the consuming app's `sdkconfig`:

```
CONFIG_ESP_TLS_INSECURE=y
CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY=y
```

Both are off by default, and no library call can enable them; ESP-IDF compiles the no-verification branch out otherwise. Without them, `set_client_config` returns `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED` and the connection fails immediately. That is the intended, documented outcome rather than a defect: the alternative is verifying against a trust anchor the caller never asked for. If you see that error, supply CAs via `with_certs` or enable the two options above.

This is the one place the ESP-IDF backend diverges from `io::tokio`, where `build_unsafe_client_config()` skips verification with no target configuration required.

## bambino-cli

A CLI built on this crate's own `PrinterClient`, so you can exercise the library against a real printer without first building an application around it, which is useful for confirming a model behaves as documented, capturing wire data, or checking a change before it ships. It also keeps the `std` build honest by consuming the public API the way a consumer would. Ships as a binary in the same crate, gated behind the `cli` feature so library consumers don't pull in terminal dependencies.

```sh
cargo build --bin bambino-cli --features cli
```

Working inside a checkout of this repo, the `cargo bambino-cli` alias in `.cargo/config.toml` wraps `cargo run --bin bambino-cli --features cli --`, so `cargo bambino-cli discover` stands in for `cargo run --bin bambino-cli --features cli -- discover`. The alias comes from this repo's cargo config, so it isn't available to a project that depends on bambino; run or install the binary directly there.

### Usage

```
Usage: bambino-cli [OPTIONS] <COMMAND>

Commands:
  discover      Scan the local subnet for nearby active printers
  info          Query expansion bus module and firmware versions
  monitor       Stream real-time status telemetry and HMS warnings
  dump          Dump the raw pushall JSON response and exit (or every subsequent push, with --follow)
  probe         Run command response capture suite and write report
  ack-probe     Check which MQTT commands echo a correlatable `sequence_id` ack
  control       Dispatch a movement or hardware control command
  files         Traverse and transfer files on the printer's MicroSD card
  camera        Camera streaming operations
  inspect-cert  Capture a printer's raw leaf TLS cert to disk for SAN/CN inspection
  verify-tls    Attempt a real CA-verified TLS handshake against a printer
  help          Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose       Enable verbose connection and packet debugging output
      --with-certs <PATH>
                      Verify the printer's TLS certificate against these CA certs
                      instead of skipping verification. Accepts a cert file or a
                      directory of them; applies to every printer-facing subcommand.
  -h, --help          Print help

Most commands require positional args: <IP> <SERIAL> <ACCESS_CODE>
ACCESS_CODE may be omitted (or passed as "") to fall back to the
BAMBINO_ACCESS_CODE environment variable.
Run 'bambino-cli <COMMAND> --help' for full argument details.

Control actions:  home  move  extrude  fan  temp  led  speed  clear-error
                  airduct  calibrate  gcode  gcode-raw  pause  resume  stop
                  gcode-raw prompts for interactive confirmation unless --unsafe is
                  passed, and bypasses all model safety checks; see its --help.
                  ams (dry | dry-stop)
Files actions:    list  upload  delete  space  clock-check
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests
Ack-probe:        -o/--output  -t/--tests  --window
```

Without `--with-certs`, the CLI performs no certificate verification at all: traffic is
encrypted but the peer is unauthenticated. With it, every connection goes through
`CnFallbackServerVerifier` (chain of trust, handshake signature, CN-vs-serial identity), and
`-v` prints which anchor the chain resolved against. `verify-tls` requires the flag.

`ack-probe` dispatches real commands to determine which of them echo a correlatable
`sequence_id`. Depending on `-t/--tests`, that set can include physically-actuating commands
(`ams_change_filament`, `project_file`), so read its `--help` before running it against a
loaded printer.

- **IP** and **Serial**: shown by `bambino-cli discover`
- **Access Code**: on the printer's touchscreen under Network > LAN Mode

## Known firmware quirks

### K-profile priming

Bambu firmware silently ignores the first `extrusion_cali_get` command after connecting. `PrinterClient::get_k_profiles()` handles this automatically by sending a throwaway priming request first. If you manage priming yourself, call `set_k_profile_primed(true)` to skip it.

### Native MQTT homing/jogging

Some newer models support `back_to_center` (homing) and `xyz_ctrl` (jogging) as structured JSON commands instead of raw G-code, gated by a `fun` capability bitmask. These native MQTT commands were sourced from BambuStudio and have not been verified against real hardware; see [REF-MOTO-MQTTCTRL] in `reference/04_toolhead_thermal_motion.md`. This library always uses G-code instead.

## Documentation

### API

Full API reference is generated straight from doc comments into [`docs/`](docs/index.md), one markdown file per module. It covers all three platform targets (host, ESP-IDF, Embassy) merged into one tree.

### Protocol spec

[`reference/`](reference/README.md) is the reverse-engineered spec this library implements against: seven chapters (network/discovery, FTPS, MQTT/telemetry, thermal/motion, AMS, cameras, diagnostics/HMS), cross-referencing wire captures and prior open-source work in lieu of any official documentation from Bambu Lab. Individual claims are tagged with stable IDs like `[REF-MOTO-GCODE]` or `[REF-AMS-MAP]` so code comments can point at the exact section backing a decision. Start at `reference/README.md` for the chapter map and terminology glossary. Worth a read if you're debugging a firmware quirk this library doesn't already model, or just want to know why a given field is shaped the way it is.

The spec is original work, derived from wire captures against printers we own and from publicly available sources: the open-source projects listed under [Acknowledgements](#acknowledgements), Bambu Lab's public wiki and product pages, and observed firmware behaviour. It documents protocol facts for interoperability; it contains no Bambu Lab source code, firmware, or confidential material.

## Acknowledgements

The [protocol spec](reference/README.md) and this library would not have been possible without facts derived from prior work of these excellent open source projects.

- [BambuStudio](https://github.com/bambulab/BambuStudio/)
- [OrcaSlicer](https://github.com/OrcaSlicer/OrcaSlicer/)
- [Bambuddy](https://github.com/maziggy/bambuddy/)
- [ha-bambulab](https://github.com/greghesp/ha-bambulab/)
- [bambu-printer-manager](https://github.com/synman/bambu-printer-manager/)
- [OpenBambuAPI](https://github.com/Doridian/OpenBambuAPI/)
- [SpoolEase](https://github.com/yanshay/spoolease) - Not directly referenced for this project, but they deserve a special shout-out for many reasons.

## Safety Notice

This software communicates with and controls physical hardware capable of high temperatures and motion. It is experimental, based on reverse engineering, not affiliated with Bambu Lab, and is provided solely under the terms of the AGPL-3.0 license.

Use it entirely at your own risk. This software's API and Bambu Lab APIs are subject to change without notice. Always supervise printer operation. The author and contributors assume no responsibility for hardware damage, personal injury, loss of data, or any other damages resulting from the use of this software. You are ultimately responsible for verifying commands before use.

## License

[AGPL-3.0](LICENSE)
