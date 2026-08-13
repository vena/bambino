# bambino

Async Rust library for talking to Bambu Lab 3D printers over your local network. No Bambu Cloud, just direct MQTT, FTPS, and camera access from one codebase that compiles to desktop, ESP32 (ESP-IDF), and bare-metal (Embassy) targets.

**🤖 Notice:** In case it's not obvious, this was built with the assistance of AI.

## What it does

- **Discovery** — find printers on your LAN via SSDP (ports 2021/1990)
- **MQTT control** — connect to the printer's local broker (port 8883), send commands, receive telemetry
- **File transfer** — implicit FTPS (port 990) for listing, uploading, downloading, and managing files on the SD card
- **Camera** — complete binary JPEG streaming client (port 6000, A1/P1 series, A2L); RTSPS helpers for proxy integration and timestamp correction (port 322, X1/X2/H2/P2S series)
- **Model quirks** — per-model differences handled polymorphically: FTPS TLS requirements, fan step resolution, Z-axis homing safety, door sensors, camera protocols, nozzle counts (single/IDEX/tool changer), temperature limits, and chamber heater capabilities

## Two levels of API

`PrinterClient` is the high-level interface — it wraps MQTT (and optionally FTPS) with model-aware safety checks: temperature clamping to hardware limits, Z-axis homing validation, chamber heater capability guards, fan routing to the right controller, and automatic K-profile priming. Most users should start here.

For advanced use cases, `PrinterClient::mqtt().await?` and `PrinterClient::storage().await?` provide direct access to the underlying `BambuMqttClient` and `BambuFtpsClient` respectively, auto-connecting if needed. Use `mqtt()` to send custom MQTT payloads, manage zombie detection, or inspect in-flight state. Note that raw payloads bypass `PrinterClient`'s model-aware safety checks.

The underlying modules (`mqtt`, `ftps`, `discovery`, `camera`) are also public if you need direct protocol access — useful for custom integrations, firmware exploration, or when `PrinterClient` doesn't cover your use case.

## Quick start

```toml
[dependencies]
bambino = { path = "../bambino" }
```

### Discover printers

```rust
use bambino::discovery::discover_devices;
use bambino::io::tokio::{TokioTimer, TokioUdpSocket};
use std::time::Duration;

let timer = TokioTimer::new();
let printers = discover_devices::<TokioUdpSocket, _>(
    Duration::from_secs(5),
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
use bambino::models::resolve_model;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};

// Printers use self-signed certs, so we skip verification
let config = build_unsafe_client_config();
let tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

let model = resolve_model(serial, None);
let identity = PrinterIdentity { ip: ip.to_string(), serial: serial.to_string(), access_code: access_code.to_string(), model };
let mut printer = PrinterClient::new(tls, TokioRawStreamFactory, identity)
    .with_timer(TokioTimer::new())
    .with_connect_timeout(5);

// MQTT connects lazily on first use, or eagerly:
printer.connect_mqtt().await?;
```

If you already have a connected `BambuMqttClient` (tests, Embassy), wrap it directly:

```rust
use bambino::client::PrinterClient;

let mut printer = PrinterClient::from_mqtt(mqtt_client, model);
```

### Send commands

```rust
printer.request_pushall().await?;              // request full state dump
printer.home_axes(false).await?;               // "safe" homing (bare G28)
printer.set_bed_temperature(60).await?;        // clamped to model max
printer.set_nozzle_temperature(0, 220).await?; // nozzle 0 at 220°C
printer.set_led("chamber_light", true).await?;
printer.send_gcode("M106 P1 S255").await?;     // validated against model quirks
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

### Print jobs

```rust
use bambino::mqtt::PrintJobConfig;

let config = PrintJobConfig::new(
    "job.3mf",                      // file on SD card
    "Metadata/plate_1.gcode",       // plate gcode path inside the 3mf
    "My Print",                     // task name
    12345,                          // subtask ID
    "textured",                     // bed type
).with_ams(vec![0, -1, 1]);        // AMS slot mapping

printer.start_print(&config).await?;
```

Bed leveling, flow calibration, and vibration compensation run automatically as part of the print (all enabled by default in `PrintJobConfig`). Use the builder methods to change them — they take either a `bool` or a `CalibrationMode` (`Off`/`On`/`Auto`, matching BambuStudio's tri-state encoding):

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

```rust
printer.change_filament(0, 1, -1, -1).await?;               // load AMS 0, slot 1
printer.start_drying(0, 55, 8, 0, true, 20, false, "PA-CF").await?; // dry at 55°C for 8h
printer.stop_drying(0).await?;
```

### File transfer

Add FTPS to a `PrinterClient` with `.with_ftps()`. The FTPS TLS connector is independent from MQTT's — some models require different TLS settings (e.g. TLS 1.2 only for FTPS data channels).

```rust
use bambino::io::tokio::{
    TokioTlsConnector, TokioRawStreamFactory, TokioTimer,
    build_unsafe_client_config_with_options,
};

// Configure FTPS TLS (respecting model-specific requirements)
let ftps_config = build_unsafe_client_config_with_options(
    model.quirks().enforce_ftps_tls_1_2(),
);
let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

let mut printer = printer.with_ftps(ftps_tls, TokioRawStreamFactory, TokioTimer::new());

// storage() auto-connects on first call
let ftp = printer.storage().await?;
let files = ftp.list_directory("/", year, month, day, hour, min).await?;
ftp.upload_file("/model/print.3mf", &file_bytes).await?;
let data = ftp.download_file("/timelapse/video.mp4").await?;
let free = ftp.get_available_space().await?;
```

> **Direct protocol access:** For cases where `PrinterClient` isn't needed, `BambuFtpsClient::connect()` provides standalone FTPS access — see the [`ftps`](src/ftps/) module.

### Camera

Bambu printers use two different camera protocols depending on the model. Check which one with `model.quirks().camera_protocol()`.

**Binary JPEG (A1, P1 series, and A2L) — port 6000.** Streams JPEG frames over a lightweight binary protocol on TLS:

```rust
use bambino::camera::binary::BambuBinaryCameraStream;
use bambino::identity::PrinterIdentity;

let mut cam = BambuBinaryCameraStream::new(tls_stream);
cam.authenticate(&PrinterIdentity { ip: ip.to_string(), serial: serial.to_string(), access_code: access_code.to_string(), model: bambino::models::resolve_model(&serial, None) }).await?;

let mut frame = Vec::new();
loop {
    cam.read_next_frame(&mut frame).await?;
    // frame is a complete JPEG image
}
```

`authenticate()` returning `Ok(())` only means the 80-byte handshake packet was written and
flushed — the protocol has no ack byte, so it does not mean the printer accepted the access
code. A bad code surfaces later, on the *next* `read_next_frame()` call, as the same
`ConnectionReset` error a plain network blip would produce; there's no way to distinguish the
two from this API alone.

`read_next_frame` rejects frames above a configurable cap (default 10MB) to guard against
unbounded allocation. Constrained (`no_std`/Embassy) targets should lower it with
`BambuBinaryCameraStream::new(stream).with_max_frame_size(64 * 1024)` to match their actual
buffer budget — the 10MB default can exceed an embedded target's entire SRAM.

**RTSPS (X1, X2, H2, P2S series) — port 322.** RTSP behind implicit TLS with Digest auth. This library provides helpers for integrating with external media players — it does not include an RTSP client.

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

`rewrite_rtsp_request_uri` only rewrites the URI text in the request line — it does **not**
recompute or repair an already-computed Digest `Authorization` header. It's only useful to a
proxy that acts as its own independent RTSP client toward the printer (computing its own
Digest response against the rewritten URI). A transparent relay that forwards the player's
original `Authorization` header verbatim will still get a 401, since that header's
`response=` hash was computed by the player against its own local URI.

P2S models on certain firmware versions have a bug where RTP timestamps don't advance, causing video freezes. Use `RtpTimestampCorrector` to synthesize correct timestamps when `model.quirks().requires_wallclock_rtsp_timestamps()` is true.

## TLS configuration

By default, `build_unsafe_client_config()` skips certificate verification — necessary because all Bambu printers use self-signed certs. For environments where you can provision your own CA, use `build_verified_client_config()`:

> This section covers the `tokio` backend. ESP-IDF chooses its trust anchor differently and needs target configuration to skip verification at all — see "ESP-IDF certificate verification" under [Platform targets](#platform-targets).

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

`build_verified_client_config()` validates the printer's certificate against the given CA root(s) and checks its identity against the printer's serial number, falling back to Subject CN when no Subject Alternative Name is present (matching mbedtls's behavior on ESP-IDF/Embassy). `build_unsafe_client_config()` is unaffected — it never checks certificate identity.

Both functions have `_with_options` variants that accept `force_tls_1_2: bool`. Two models — P2S and X2D — need FTPS capped to TLS 1.2, but not because the protocol demands it: it's a firmware bug in their embedded vsFTPd (confirmed for P2S via an independent reverse-engineering project's own bug report; assumed-by-analogy for X2D, whose actual root cause is still unconfirmed). Check with `model.quirks().enforce_ftps_tls_1_2()`. `BambuFtpsClient::connect()` fails closed on those models: it errors unless `negotiated_version` reports exactly `Some(TlsVersion::Tls12)` (an undetermined `None` also rejects — never a silent pass-through).

This is platform-general — `TokioTlsConnector` and `EspIdfTlsConnector` implement `negotiated_version` for real. `EmbassyTlsConnector` cannot, so Embassy + P2S/X2D is unconditionally rejected by this check rather than downgraded — see the Embassy TLS section below for why, and for the opt-out.

`PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` opts out of the check entirely instead: `require_tls_1_2_if_enforced` logs a warning and returns `Ok(())` unconditionally, regardless of what (if anything) `negotiated_version` reports. This is a reliability tradeoff, not a safety hole — `upload_file`'s `SIZE` recheck and `download_file`'s unconditional `SIZE` recheck (run on both the `226` and `426` completion replies) already catch a truncated/corrupted transfer independently of this flag, so bypassing the version check risks more failed transfers/retries against P2S/X2D, never silently-corrupt data. Default is `false` (fail closed, unchanged). Only meaningful for the `embassy` feature — on `tokio`/`esp-idf`, use `force_tls_1_2` on the `TlsConnector` instead, since those platforms can actually negotiate TLS 1.2 for real.

## Platform targets

The default feature set (`tokio`) targets desktop/server. For embedded, swap the feature flag:

```toml
# ESP32 with ESP-IDF (std)
bambino = { path = "../bambino", default-features = false, features = ["esp-idf"] }

# Bare-metal with Embassy (no_std + alloc)
bambino = { path = "../bambino", default-features = false, features = ["embassy"] }
```

All network I/O goes through abstract traits (`AsyncIo`, `TlsConnector`, `TimerProvider`, etc.) so library code is platform-agnostic. Platform-specific implementations live in `io::tokio`, `io::esp_idf`, and `io::embassy`.

**Embassy note:** `discover_devices()` is not available on Embassy — the convenience function needs to bind its own UDP sockets, which Embassy can't do (sockets must be pre-allocated from the network stack). Use `DiscoveryEngine::new()` with a pre-bound `EmbassyUdpSocket` for manual discovery, or provide a pre-configured printer IP.

**Embassy TLS:** `EmbassyTlsConnector` wraps `mbedtls-rs` (real TLS 1.2+1.3, hardware-accelerated crypto on ESP32 targets). MbedTLS only permits one active library instance program-wide, so construct a single `mbedtls_rs::Tls` once at startup and hand out cheap `Copy` `TlsReference`s to as many connectors as you need (e.g. one for MQTT, one for FTPS's control channel, one for FTPS's data channel):

```rust
use bambino::io::embassy::EmbassyTlsConnector;
use mbedtls_rs::Tls;
use static_cell::StaticCell;

static TLS: StaticCell<Tls<'static>> = StaticCell::new();

// `rng` must be `&'static mut (dyn rand_core::CryptoRng + Send)` — e.g. another
// `StaticCell`-held hardware TRNG wrapper.
let tls: &'static mut Tls<'static> =
    TLS.init(Tls::new(rng).expect("only one Tls instance may exist program-wide"));

let mqtt_tls = EmbassyTlsConnector::new(tls.reference());
let ftps_tls = EmbassyTlsConnector::new(tls.reference());
```

There's no buffer-consumption limit — `connect()` can be called repeatedly on the same connector (`mbedtls-rs` allocates its own 16 KiB in/out record buffers per session). Certificate verification defaults to off, matching this crate's unsafe-by-default convention elsewhere; call `.with_ca_chain(cert)` to enable it, or `.with_client_credentials(creds)` for mTLS.

**`negotiated_version` always returns `None`** — `mbedtls-rs` exposes no API to read back the negotiated TLS version, so `BambuFtpsClient`'s TLS-1.2 enforcement check for P2S/X2D still fails closed under Embassy even with this real-TLS-1.2-capable backend (nothing forces the handshake to actually land on 1.2 over 1.3). Use `PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` to opt out of that check when talking to those two models under Embassy — see the "TLS configuration" section above.

**Embassy raw streams:** `EmbassyRawStreamFactory` wraps `embassy_net`'s own `TcpClient`/`TcpClientState` connection pool — used for both MQTT's lazy connect and FTPS's data channel. `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates `N` buffer pairs — `N = 1` covers FTPS's usage, since data-channel connections are always sequential (MQTT needs its own factory instance since it's a separate, concurrent connection). Both need `'static` storage (`static_cell::StaticCell` is the standard way to get that; it's not a bambino dependency). `dial`'s host must be a literal IPv4 address — Bambu printers are always addressed that way, so this isn't a limitation in practice:

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

**ESP-IDF TLS timeouts:** `EspIdfTlsConnector` runs the handshake and all reads/writes in non-blocking mode, polling every 20ms on `WANT_READ`/`WANT_WRITE`/`EWOULDBLOCK` — so a `TimerProvider`-based timeout (e.g. `poll_until`) can actually preempt a stuck handshake or read/write instead of blocking forever on FFI. `PrinterClient::with_connect_timeout()` and `EspIdfTlsConnector::with_connect_timeout()` are two independent budgets on this platform — the connector is opaque by the time it reaches `PrinterClient::new()`, so setting one doesn't affect the other. Set both explicitly and keep them in sync (including `0`, which disables the timeout on either).

**ESP-IDF TLS version query:** `EspIdfTlsConnector::negotiated_version` reads the real negotiated version via `esp_tls_get_ssl_context()` + mbedTLS's `mbedtls_ssl_get_version()`. Assumes the default mbedTLS backend (`CONFIG_ESP_TLS_USING_MBEDTLS=y`) — wolfSSL builds aren't supported yet.

**ESP-IDF FTPS/MQTT:** `EspIdfTlsConnector` wraps an already-connected raw stream (via `esp_idf_svc::tls::EspTls::adopt()`) — used for FTPS's control/data channels and MQTT's lazy connect alike, paired with `EspIdfRawStreamFactory` for the raw dial. Models where `model.quirks().uses_plaintext_ftps_data_channel()` is true skip TLS on the FTPS data channel entirely:

```rust
use bambino::io::esp_idf::{EspIdfTlsConnector, EspIdfRawStreamFactory, EspIdfTimer};

// Prefer with_certs — see "ESP-IDF certificate verification" below for why
// EspIdfTlsConnector::new() additionally requires two sdkconfig options.
let ftps_tls = EspIdfTlsConnector::with_certs(ca_cert, None);
let mut printer = printer.with_ftps(ftps_tls, EspIdfRawStreamFactory, EspIdfTimer::new()?);
```

**ESP-IDF certificate verification:** ESP-IDF picks exactly one trust anchor, checking them in a fixed order with mutually exclusive branches — its bundled public root CAs first, then a caller-supplied CA, then no verification at all. `esp-idf-svc` defaults the bundle **on** wherever `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE` is enabled, so this crate turns it off explicitly: a self-signed printer certificate can never chain to a public root, and leaving the default on would silently ignore a CA you passed to `with_certs`.

`EspIdfTlsConnector::with_certs(ca, client_auth)` is therefore the recommended path on this platform. The CA you supply becomes the sole trust anchor and needs no sdkconfig changes. Certificates are a runtime input — none are embedded in this crate.

`EspIdfTlsConnector::new()` skips verification, which on ESP-IDF requires **both** of these in the consuming app's `sdkconfig`:

```
CONFIG_ESP_TLS_INSECURE=y
CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY=y
```

Both are off by default, and no library call can enable them — ESP-IDF compiles the no-verification branch out otherwise. Without them, `set_client_config` returns `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED` and the connection fails immediately. That is the intended, documented outcome rather than a defect: the alternative is verifying against a trust anchor the caller never asked for. If you see that error, supply a CA via `with_certs` or enable the two options above.

This is the one place the ESP-IDF backend diverges from `io::tokio`, where `build_unsafe_client_config()` skips verification with no target configuration required.

## bambino-cli

A CLI built on our own library client, for testing against real printers and proving out the `std` build. Ships as a binary in the same crate, gated behind the `cli` feature so library consumers don't pull in terminal dependencies.

```sh
cargo build --bin bambino-cli --features cli
```

A `cargo bambino-cli` alias (`.cargo/config.toml`) wraps `cargo run --bin bambino-cli --features cli --`, so you don't have to type `--features cli` on every invocation — e.g. `cargo bambino-cli discover` instead of `cargo run --bin bambino-cli --features cli -- discover`.

### Usage

```
Usage: bambino-cli [OPTIONS] <COMMAND>

Commands:
  discover      Scan the local subnet for nearby active printers
  info          Query expansion bus module and firmware versions
  monitor       Stream real-time status telemetry and HMS warnings
  dump          Dump the raw pushall JSON response and exit
  probe         Run command response capture suite and write report
  control       Dispatch a movement or hardware control command
  files         Traverse and transfer files on the printer's MicroSD card
  camera        Camera streaming operations
  inspect-cert  Capture a printer's raw leaf TLS cert to disk for SAN/CN inspection
  verify-tls    Attempt a real CA-verified TLS handshake against a printer
  help          Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose connection and packet debugging output
  -h, --help     Print help

Most commands require positional args: <IP> <SERIAL> <ACCESS_CODE>
ACCESS_CODE may be omitted (or passed as "") to fall back to the
BAMBINO_ACCESS_CODE environment variable.
Run 'bambino-cli <COMMAND> --help' for full argument details.

Control actions:  home  move  extrude  fan  temp  led  speed  clear-error
                  airduct  calibrate  gcode  gcode-raw  pause  resume  stop
                  ams (dry | dry-stop)
                  gcode-raw skips model safety checks and normally prompts for an
                  interactive "yes" confirmation before sending; pass --unsafe to
                  skip that confirmation prompt too (e.g. for scripting).
Files actions:    list  upload  delete  space
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests
```

- **IP** and **Serial** — shown by `bambino-cli discover`
- **Access Code** — on the printer's touchscreen under Network > LAN Mode

## Known firmware quirks

### K-profile priming

Bambu firmware silently ignores the first `extrusion_cali_get` command after connecting. `PrinterClient::get_k_profiles()` handles this automatically by sending a throwaway priming request first. If you manage priming yourself, call `set_k_profile_primed(true)` to skip it.

### Native MQTT homing/jogging

Some newer models support `back_to_center` (homing) and `xyz_ctrl` (jogging) as structured JSON commands instead of raw G-code, gated by a `fun` capability bitmask. These native MQTT commands were sourced from BambuStudio and have not been verified against real hardware — see [REF-MOTO-MQTTCTRL] in `reference/04_toolhead_thermal_motion.md`. This library always uses G-code instead.

## Documentation

### API

Full API reference is generated straight from doc comments into [`docs/`](docs/index.md) — one markdown file per module. It covers all three platform targets (host, ESP-IDF, Embassy) merged into one tree.

### Protocol spec

[`reference/`](reference/00_index.md) is the reverse-engineered spec this library implements against — seven chapters (network/discovery, FTPS, MQTT/telemetry, thermal/motion, AMS, cameras, diagnostics/HMS), cross-referencing wire captures and prior open-source work in lieu of any official documentation from Bambu Lab. Individual claims are tagged with stable IDs like `[REF-MOTO-GCODE]` or `[REF-AMS-MAP]` so code comments can point at the exact section backing a decision. Start at `00_index.md` for the chapter map and terminology glossary — worth a read if you're debugging a firmware quirk this library doesn't already model, or just want to know why a given field is shaped the way it is.

## Acknowledgements

The [protocol spec](reference/00_index.md) and this library would not have been possible without the prior work of these excellent open source projects.

- [BambuStudio](https://github.com/bambulab/BambuStudio/)
- [OrcaSlicer](https://github.com/OrcaSlicer/OrcaSlicer/)
- [Bambuddy](https://github.com/maziggy/bambuddy/)
- [ha-bambulab](https://github.com/greghesp/ha-bambulab/)
- [bambu-printer-manager](https://github.com/synman/bambu-printer-manager/)
- [OpenBambuAPI](https://github.com/Doridian/OpenBambuAPI/)

## Safety Notice

This software communicates with and controls physical hardware capable of high temperatures and motion. It is experimental, based on reverse engineering, not affiliated with Bambu Lab, and is provided solely under the terms of the AGPL-3.0 license.

Use it entirely at your own risk. This software's API and Bambu Lab APIs are subject to change without notice. Always supervise printer operation. The author and contributors assume no responsibility for hardware damage, personal injury, loss of data, or any other damages resulting from the use of this software. You are ultimately responsible for verifying commands before use.

## License

[AGPL-3.0](LICENSE)
