# bambino

Async Rust library for talking to Bambu Lab 3D printers over your local network. No Bambu Cloud, just direct MQTT, FTPS, and camera access from one codebase that compiles to desktop, ESP32 (ESP-IDF), and bare-metal (Embassy) targets.

## What it does

- **Discovery** — find printers on your LAN via SSDP (ports 2021/1990)
- **MQTT control** — connect to the printer's local broker (port 8883), send commands, receive telemetry
- **File transfer** — implicit FTPS (port 990) for listing, uploading, downloading, and managing files on the SD card
- **Camera** — complete binary JPEG streaming client (port 6000, A1/P1 series); RTSPS helpers for proxy integration and timestamp correction (port 322, X1/X2/H2/P2S series)
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
use bambino::models::resolve_model;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};

// Printers use self-signed certs, so we skip verification
let config = build_unsafe_client_config();
let tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

let model = resolve_model(serial, None);
let mut printer = PrinterClient::new(tls, TokioRawStreamFactory, ip, serial, access_code, model)
    .with_timer(TokioTimer::new())
    .with_connect_timeout(5);

// MQTT connects lazily on first use, or eagerly:
printer.connect_mqtt().await?;
```

If you already have a connected `BambuMqttClient` (tests, Embassy), wrap it directly:

```rust
use bambino::client::PrinterClient;

let mut printer = PrinterClient::from_mqtt(mqtt_client, serial, model);
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

Bed leveling, flow calibration, and vibration compensation run automatically as part of the print (all enabled by default in `PrintJobConfig`). Use the builder methods to change them:

```rust
let config = PrintJobConfig::new("job.3mf", "Metadata/plate_1.gcode", "My Print", 12345, "textured")
    .bed_leveling(false)
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
printer.change_filament(0, 1, 1, -1, -1).await?;           // load AMS 0, slot 1
printer.start_drying(0, 55, 480, true, "PA-CF").await?;     // dry at 55°C for 8h
printer.stop_drying(0).await?;
```

### File transfer

Add FTPS to a `PrinterClient` with `.with_ftps()`. The FTPS TLS connector is independent from MQTT's — some models require different TLS settings (e.g. TLS 1.2 only for FTPS data channels).

```rust
use bambino::io::tokio::{
    TokioTlsConnector, TokioRawStreamFactory,
    build_unsafe_client_config_with_options,
};

// Configure FTPS TLS (respecting model-specific requirements)
let ftps_config = build_unsafe_client_config_with_options(
    model.quirks().enforce_ftps_tls_1_2(),
);
let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

let mut printer = printer.with_ftps(ftps_tls, TokioRawStreamFactory);

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

**Binary JPEG (A1, P1 series) — port 6000.** Streams JPEG frames over a lightweight binary protocol on TLS:

```rust
use bambino::camera::binary::BambuBinaryCameraStream;

let mut cam = BambuBinaryCameraStream::new(tls_stream);
cam.authenticate(access_code).await?;

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
`Result<String, BambuError>`.

Since the printer uses self-signed TLS, most players can't connect directly. The typical setup is a local proxy that accepts plain `rtsp://`, wraps it in TLS, and forwards to the printer. Use `rewrite_rtsp_request_uri` to rewrite the request-line URI in transit:

```rust
use bambino::camera::rtsps::rewrite_rtsp_request_uri;

// Player sends:  rtsp://127.0.0.1:8554/streaming/live/1
// Printer needs: rtsps://192.168.1.150:322/streaming/live/1
let rewritten = rewrite_rtsp_request_uri(player_uri, printer_ip);
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

Both functions have `_with_options` variants that accept `force_tls_1_2: bool`. Some models (P2S, X2D) require TLS 1.2 only for FTPS data channels — check with `model.quirks().enforce_ftps_tls_1_2()`. `BambuFtpsClient::connect()` fails closed on those models: it errors unless `negotiated_version` reports exactly `Some(TlsVersion::Tls12)` (an undetermined `None` also rejects — never a silent pass-through).

This is platform-general — `TokioTlsConnector`, `EmbassyTlsConnector`, and `EspIdfTlsConnector` all implement `negotiated_version` for real. `EmbassyTlsConnector` always reports TLS 1.3 (`embedded-tls` 0.19 is TLS-1.3-only), so Embassy + P2S/X2D is unconditionally rejected rather than downgraded.

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

**Embassy TLS buffers:** `EmbassyTlsConnector` has no hidden static buffers — you supply the `embedded-tls` read/write scratch buffers yourself. Each connector owns one buffer pair for exactly one `connect()` call; concurrent connections (e.g. FTPS's control and data channels) need separate connectors:

```rust
use bambino::io::embassy::EmbassyTlsConnector;
use embedded_tls::{Aes128GcmSha256, TlsConfig};

let config = TlsConfig::new().with_server_name("printer-serial");
let mut read_buf = [0u8; 16384];
let mut write_buf = [0u8; 16384];
let connector: EmbassyTlsConnector<'_, Aes128GcmSha256, _> =
    EmbassyTlsConnector::new(&config, rng, &mut read_buf, &mut write_buf);
```

Calling `connect()` twice on the same connector returns `SocketError::Other` instead of a second connection — construct another `EmbassyTlsConnector` for a second concurrent connection.

**Embassy raw streams:** `EmbassyRawStreamFactory` wraps `embassy_net`'s own `TcpClient`/`TcpClientState` connection pool — used for both MQTT's lazy connect and FTPS's data channel. `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates `N` buffer pairs — `N = 1` covers FTPS's usage, since data-channel connections are always sequential (MQTT needs its own factory instance since it's a separate, concurrent connection). Both need `'static` storage (`static_cell::StaticCell` is the standard way to get that; it's not a bambino dependency). `dial`'s host must be a literal IPv4 address — Bambu printers are always addressed that way, so this isn't a limitation in practice:

```rust
use bambino::io::embassy::EmbassyRawStreamFactory;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use static_cell::StaticCell;

static TCP_CLIENT_STATE: StaticCell<TcpClientState<1, 2048, 2048>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 1, 2048, 2048>> = StaticCell::new();

let state = TCP_CLIENT_STATE.init(TcpClientState::new());
let client = TCP_CLIENT.init(TcpClient::new(stack, state));
let factory = EmbassyRawStreamFactory::new(client);

let mut printer = printer.with_ftps(ftps_tls, factory);
```

**ESP-IDF TLS timeouts:** `EspIdfTlsConnector` runs the handshake and all reads/writes in non-blocking mode, polling every 20ms on `WANT_READ`/`WANT_WRITE`/`EWOULDBLOCK` — so a `TimerProvider`-based timeout (e.g. `poll_until`) can actually preempt a stuck handshake or read/write instead of blocking forever on FFI.

**ESP-IDF TLS version query:** `EspIdfTlsConnector::negotiated_version` reads the real negotiated version via `esp_tls_get_ssl_context()` + mbedTLS's `mbedtls_ssl_get_version()`. Assumes the default mbedTLS backend (`CONFIG_ESP_TLS_USING_MBEDTLS=y`) — wolfSSL builds aren't supported yet.

**ESP-IDF FTPS/MQTT:** `EspIdfTlsConnector` wraps an already-connected raw stream (via `esp_idf_svc::tls::EspTls::adopt()`) — used for FTPS's control/data channels and MQTT's lazy connect alike, paired with `EspIdfRawStreamFactory` for the raw dial. Models where `model.quirks().uses_plaintext_ftps_data_channel()` is true skip TLS on the FTPS data channel entirely:

```rust
use bambino::io::esp_idf::{EspIdfTlsConnector, EspIdfRawStreamFactory};

let ftps_tls = EspIdfTlsConnector::new(); // or ::with_certs(ca_cert, client_auth) to verify
let mut printer = printer.with_ftps(ftps_tls, EspIdfRawStreamFactory);
```

## bambino-cli

A CLI built on our own library client, for testing against real printers and proving out the `std` build. Ships as a binary in the same crate, gated behind the `cli` feature so library consumers don't pull in terminal dependencies.

```sh
cargo build --bin bambino-cli --features cli
```

A `cargo bambino-cli` alias (`.cargo/config.toml`) wraps `cargo run --bin bambino-cli --features cli --`, so you don't have to type `--features cli` on every invocation — e.g. `cargo bambino-cli discover` instead of `cargo run --bin bambino-cli --features cli -- discover`. The examples below use the plain binary name (`bambino-cli ...`); substitute `cargo bambino-cli ...` if running from source instead of a built binary.

### Usage

```
Usage: bambino-cli [OPTIONS] <COMMAND>

Commands:
  discover  Scan the local subnet for nearby active printers
  info      Query expansion bus module and firmware versions
  monitor   Stream real-time status telemetry and HMS warnings
  dump      Dump the raw pushall JSON response and exit
  probe     Run command response capture suite and write report
  control   Dispatch a movement or hardware control command
  files     Traverse and transfer files on the printer's MicroSD card
  camera    Camera streaming operations
  help      Print this message or the help of the given subcommand(s)

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
Files actions:    list  upload  delete  space
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests
```

### Credentials

- **IP** and **Serial** — shown by `bambino-cli discover`
- **Access Code** — on the printer's touchscreen under Network > LAN Mode

## Known firmware quirks

### K-profile priming

The firmware silently ignores the first `extrusion_cali_get` command after connecting. `PrinterClient::get_k_profiles()` handles this automatically by sending a throwaway priming request first. If you manage priming yourself, call `set_k_profile_primed(true)` to skip it.

## Not yet implemented

- **MQTT-native homing/jogging on newer models.** Some models advertise support for `back_to_center` (homing) and `xyz_ctrl` (jogging) as structured JSON commands instead of raw G-code, gated by a `fun` capability bitmask. This library always uses G-code. Sourced from BambuStudio, unverified on real hardware — see [REF-MOTO-MQTTCTRL] in `reference/04_toolhead_thermal_motion.md`.

## Acknowledgements

Built on the reverse-engineering work of:

- [Bambuddy](https://github.com/maziggy/bambuddy)
- [ha-bambulab](https://github.com/greghesp/ha-bambulab/)
- [bambu-printer-manager](https://github.com/synman/bambu-printer-manager)
- [OpenBambuAPI](https://github.com/Doridian/OpenBambuAPI/)

## Safety Notice

This software communicates with and controls physical hardware capable of high temperatures and motion. It is experimental, based on reverse engineering, not affiliated with Bambu Lab, and is provided solely under the terms of the AGPL-3.0 license.

Use it entirely at your own risk. This software's API and Bambu Lab APIs are subject to change without notice. Always supervise printer operation. The authors and contributors assume no responsibility for hardware damage, personal injury, loss of data, or any other damages resulting from the use of this software. You are ultimately responsible for verifying commands before use.

## License

[AGPL-3.0](LICENSE)
