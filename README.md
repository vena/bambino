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
    TokioSecureConnector, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};
use std::time::Duration;

// Printers use self-signed certs, so we skip verification
let config = build_unsafe_client_config();
let tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
let connector = TokioSecureConnector::new(tls, Duration::from_secs(5));

let model = resolve_model(serial, None);
let mut printer = PrinterClient::new(connector, ip, serial, access_code, model)
    .with_timer(TokioTimer::new());

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
printer.toggle_led("chamber_light", true).await?;
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
    TokioTlsConnector, TokioFtpDataStreamFactory,
    build_unsafe_client_config_with_options,
};

// Configure FTPS TLS (respecting model-specific requirements)
let ftps_config = build_unsafe_client_config_with_options(
    model.quirks().enforce_ftps_tls_1_2(),
);
let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

let mut printer = printer.with_ftps(ftps_tls, TokioFtpDataStreamFactory);

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

**RTSPS (X1, X2, H2, P2S series) — port 322.** RTSP behind implicit TLS with Digest auth. This library provides helpers for integrating with external media players — it does not include an RTSP client.

```rust
use bambino::camera::rtsps::build_rtsps_url;

let url = build_rtsps_url(ip, access_code);
// → rtsps://bblp:<code>@<ip>:322/streaming/live/1
```

Since the printer uses self-signed TLS, most players can't connect directly. The typical setup is a local proxy that accepts plain `rtsp://`, wraps it in TLS, and forwards to the printer. Use `rewrite_rtsp_request_uri` to fix Digest auth hashes when proxying:

```rust
use bambino::camera::rtsps::rewrite_rtsp_request_uri;

// Player sends:  rtsp://127.0.0.1:8554/streaming/live/1
// Printer needs: rtsps://192.168.1.150:322/streaming/live/1
let rewritten = rewrite_rtsp_request_uri(player_uri, printer_ip);
```

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

Both functions have `_with_options` variants that accept `force_tls_1_2: bool`. Some models (P2S, X2D) require TLS 1.2 only for FTPS data channels — use `model.quirks().enforce_ftps_tls_1_2()` to query this. If a misconfigured `TlsConnector` negotiates TLS 1.3 on a model that requires 1.2, `BambuFtpsClient::connect()` will return a `ProtocolViolation` error immediately.

This guarantee is platform-general, not tokio-only: `TokioTlsConnector`, `EmbassyTlsConnector`, and `EspIdfSecureConnector` all implement `negotiated_version` for real (see the "ESP-IDF TLS timeouts" caveat below for the one narrow case ESP-IDF can't cover). `EmbassyTlsConnector` always reports TLS 1.3 — `embedded-tls` 0.19 is a TLS 1.3-only client, so a P2S/X2D connection over Embassy is unconditionally rejected rather than silently downgraded to "unchecked."

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

**Embassy TLS buffers:** `EmbassyTlsConnector` has no hidden static buffers — you supply the `embedded-tls` read/write scratch buffers yourself, sized for your board's RAM budget. Each connector owns one buffer pair and hands it to exactly one `connect()` call, so opening N concurrent TLS connections (e.g. FTPS's control and data channels at once) means constructing N connectors:

```rust
use bambino::io::embassy::EmbassyTlsConnector;
use embedded_tls::{Aes128GcmSha256, TlsConfig};

let config = TlsConfig::new().with_server_name("printer-serial");
let mut read_buf = [0u8; 16384];
let mut write_buf = [0u8; 16384];
let connector: EmbassyTlsConnector<'_, Aes128GcmSha256, _> =
    EmbassyTlsConnector::new(&config, rng, &mut read_buf, &mut write_buf);
```

A second concurrent connection (e.g. FTPS's data channel) needs its own connector with its own buffer pair — construct another `EmbassyTlsConnector` rather than reusing this one; calling `connect()` twice on the same connector returns `SocketError::Other` instead of a second connection.

**Embassy FTPS:** `EmbassyFtpDataStreamFactory` supplies the raw TCP connections FTPS needs (one per `list_directory`/`upload_file`/`download_file` call, plus one to dial the control channel) — built on `embassy_net::tcp::client::TcpClient`, embassy-net's own connection pool, rather than a hand-rolled buffer scheme. `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates `N` buffer pairs; each connection checks one out and returns it to the pool on drop, so running out of pool slots is a `ConnectionRefused`-style error, not a panic — `N = 1` covers FTPS's actual usage pattern (data-channel connections are always sequential, never concurrent with each other). Both `TcpClientState` and the `TcpClient` built from it need `'static` storage, since `create_data_stream` is called repeatedly from `&self` and the returned connection's type can't carry a lifetime shorter than `'static` (see `EmbassyFtpDataStreamFactory`'s doc comment for why). `static_cell::StaticCell` is the standard way to get that in an Embassy app — add it as your own dependency, bambino doesn't require it. Note: the host passed to `create_data_stream` must be a literal IPv4 address — Bambu Lab printers are addressed by LAN IPv4 only (SSDP discovery never resolves a hostname, and the printers don't advertise IPv6), so this isn't a limitation in practice, but a hostname or IPv6 literal here will fail with `SocketError::InvalidInput`:

```rust
use bambino::io::embassy::EmbassyFtpDataStreamFactory;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use static_cell::StaticCell;

static TCP_CLIENT_STATE: StaticCell<TcpClientState<1, 2048, 2048>> = StaticCell::new();
static TCP_CLIENT: StaticCell<TcpClient<'static, 1, 2048, 2048>> = StaticCell::new();

let state = TCP_CLIENT_STATE.init(TcpClientState::new());
let client = TCP_CLIENT.init(TcpClient::new(stack, state));
let factory = EmbassyFtpDataStreamFactory::new(client);

let mut printer = printer.with_ftps(ftps_tls, factory);
```

**ESP-IDF TLS timeouts:** `EspIdfSecureConnector` runs the TLS handshake and all reads/writes with the underlying socket in non-blocking mode, so a `TimerProvider`-based timeout wrapped around ESP-IDF network I/O (e.g. `poll_until`) can now actually preempt a stuck handshake or read/write — this previously could not happen, since the handshake and I/O were blocking FFI calls a timeout has no way to interrupt. The mechanism is a fixed-interval poll (retry every 20ms via `EspIdfTimer::sleep` on `ESP_TLS_ERR_SSL_WANT_READ`/`_WRITE`/`EWOULDBLOCK`), not true readiness notification — `esp-idf-svc`/`esp-idf-hal` expose no async socket-readiness primitive for an arbitrary fd today. Real wake-on-ready is possible via `esp_idf_svc::tls::EspAsyncTls` plus the `async-io` crate and `MountedEventfs`, but that requires a new dependency and real app-side setup (a correctly-sized eventfd mount, a dedicated thread with a bumped stack, and working around an ESP-IDF main-task/async-io-thread priority inversion) — not something this crate can hide, and not necessary to fix the timeout-preemption problem, so it's left as a possible future upgrade rather than done now.

**ESP-IDF TLS version query:** `EspIdfSecureConnector::negotiated_version` reads the real negotiated version via `esp_tls_get_ssl_context()` + mbedTLS's `mbedtls_ssl_get_version()`. This assumes the default mbedTLS backend (`CONFIG_ESP_TLS_USING_MBEDTLS=y`) — a wolfSSL-configured build (`CONFIG_ESP_TLS_USING_WOLFSSL=y`) isn't supported today, since this crate has no `build.rs` forwarding the `esp_idf_esp_tls_using_wolfssl`-style cfg flags needed to detect the backend at compile time the way `esp-idf-svc` does for itself.

**ESP-IDF FTPS:** ESP-IDF's `EspTls` normally only dials its own TCP connection (`SecureConnect`, used for MQTT above) — FTPS needs `TlsConnector` instead (wrap an *already-connected* raw stream, since the control channel and each data channel start life as a plain `std::net::TcpStream`). `EspIdfTlsConnector` provides that via `esp_idf_svc::tls::EspTls::adopt()`, a safe wrapper `esp-idf-svc` ships for exactly this case — no raw mbedTLS FFI required. `EspIdfTcpStream` (the `RawIO` type) dials a blocking `std::net::TcpStream`; `EspIdfTlsConnector::connect()` flips it to non-blocking right before adopting it, so the handshake polls the same way `EspIdfSecureConnector`'s does (see "ESP-IDF TLS timeouts" above) — models whose `model.quirks().uses_plaintext_ftps_data_channel()` is true skip TLS entirely and read/write the blocking stream directly, which is fine since that path never goes through the connector:

```rust
use bambino::io::esp_idf::{EspIdfTlsConnector, EspIdfFtpDataStreamFactory};

let ftps_tls = EspIdfTlsConnector::new(); // or ::with_certs(ca_cert, client_auth) to verify
let mut printer = printer.with_ftps(ftps_tls, EspIdfFtpDataStreamFactory);
```

## bambino-cli

A CLI using our own library client for testing against real printers as proof-of-concept. Ships as a binary in the same crate, gated behind the `cli` feature so library consumers don't pull in a terminal UI crate (`crossterm`) and a log sink (`env_logger`) they never asked for.

```sh
cargo build --bin bambino-cli --features cli
```

A `cargo bambino-cli` alias (`.cargo/config.toml`) wraps `cargo run --bin bambino-cli --features cli --`, so you don't have to type `--features cli` on every invocation — e.g. `cargo bambino-cli discover` instead of `cargo run --bin bambino-cli --features cli -- discover`. The examples below use the plain binary name (`bambino-cli ...`); substitute `cargo bambino-cli ...` if running from source instead of a built binary.

### Usage

```sh
# Discovery
bambino-cli discover

# Printer info and firmware versions
bambino-cli info <ip> <serial> <access_code>

# Live telemetry dashboard
bambino-cli monitor <ip> <serial> <access_code>

# Commands
bambino-cli control <ip> <serial> <access_code> home
bambino-cli control <ip> <serial> <access_code> temp nozzle 220
bambino-cli control <ip> <serial> <access_code> temp bed 60
bambino-cli control <ip> <serial> <access_code> fan part 80
bambino-cli control <ip> <serial> <access_code> led chamber on
bambino-cli control <ip> <serial> <access_code> pause
bambino-cli control <ip> <serial> <access_code> resume
bambino-cli control <ip> <serial> <access_code> stop
bambino-cli control <ip> <serial> <access_code> speed sport
bambino-cli control <ip> <serial> <access_code> clear-error
bambino-cli control <ip> <serial> <access_code> airduct cooling
bambino-cli control <ip> <serial> <access_code> calibrate bed-leveling vibration

# AMS
bambino-cli control <ip> <serial> <access_code> ams dry 0 55 480 true PA-CF
bambino-cli control <ip> <serial> <access_code> ams dry-stop 0

# G-code (validated against model quirks)
bambino-cli control <ip> <serial> <access_code> gcode "G28"

# G-code (no safety checks — prompts for confirmation)
bambino-cli control <ip> <serial> <access_code> gcode-raw "M106 P1 S255"
bambino-cli control <ip> <serial> <access_code> gcode-raw --unsafe "M106 P1 S255"

# File management
bambino-cli files <ip> <serial> <access_code> list /
bambino-cli files <ip> <serial> <access_code> upload ./print.3mf /model/print.3mf
bambino-cli files <ip> <serial> <access_code> download /timelapse/video.mp4 ./video.mp4
bambino-cli files <ip> <serial> <access_code> space

# Camera snapshot (A1/P1 binary JPEG only)
bambino-cli camera <ip> <serial> <access_code> snapshot
bambino-cli camera <ip> <serial> <access_code> snapshot frame.jpg
```

Use `-v` for protocol-level debug logging.

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
