# bambino

Async Rust library for talking to Bambu Lab 3D printers over your local network (LAN mode). No cloud, no Bambu Studio — just direct MQTT, FTPS, and camera access.

Designed for use on host machines, powerful ESP32 platforms with `std` support (like the ESP32-P4 via ESP-IDF), and `no_std` embedded targets (via Embassy). Same codebase across all three.

## What it does

- **Discovery** — finds printers on your LAN via SSDP (ports 2021/1990)
- **MQTT control** — connect to the printer's local broker on port 8883, send commands, stream telemetry
- **File transfer** — implicit FTPS on port 990 for listing, uploading, and deleting files on the SD card
- **Camera** — binary JPEG streaming on port 6000 (A1/P1) and RTSPS on port 322 (X1/X2/H2/P2S)
- **Model quirks** — handles per-model differences (TLS data channel modes, fan step rounding, Z-axis safety, etc.) so you don't have to

Tested against: X1C, X1E, X2D, A1, A1 Mini, A2L, P1P, P1S, P2S, H2D, H2D Pro, H2C, H2S.

## Quick start

```toml
[dependencies]
bambino = { path = "../bambino" }
```

### Find printers

```rust
use bambino::discovery::discover_devices;
use bambino::io::tokio::{TokioTimer, TokioUdpSocket};
use std::time::Duration;

let devices = discover_devices::<TokioUdpSocket, TokioTimer>(
    Duration::from_secs(20),
    &TokioTimer,
).await?;

for d in &devices {
    println!("{:?} at {} ({})", d.model, d.ip, d.serial);
}
```

### Connect and send commands

```rust
use bambino::client::PrinterClient;
use bambino::discovery::resolve_model;
use bambino::mqtt::BambuMqttClient;
use bambino::io::tokio::{build_unsafe_client_config, TokioTlsConnector, TokioTimer};
use bambino::io::{TlsConnector, TokioIo};

// TLS + MQTT handshake
let config = build_unsafe_client_config();
let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
let tcp = tokio::net::TcpStream::connect("192.168.1.158:8883").await?;
let tls = connector.connect("192.168.1.158", 8883, TokioIo(tcp)).await?;
let mqtt = BambuMqttClient::connect::<TokioTimer>(tls, serial, access_code).await?;

let model = resolve_model(serial, None);
let mut printer = PrinterClient::new(mqtt, serial, model);

printer.home_axes(false).await?;
printer.set_bed_temperature(60).await?;
printer.set_nozzle_temperature(0, 220).await?;
printer.toggle_led("chamber_light", true).await?;
```

### Read telemetry

```rust
loop {
    let msg = printer.poll_telemetry().await?;
    let report: TelemetryReport = serde_json::from_slice(&msg.payload)?;
    if let Some(print) = &report.print {
        println!("{:?} — {:?}%", print.gcode_state, print.progress);
    }
}
```

### Transfer files

```rust
use bambino::ftps::BambuFtpsClient;

let mut ftp = BambuFtpsClient::connect(
    raw_control, tls_connector, data_factory, model, ip, access_code,
).await?;

let files = ftp.list_directory("/", year, month, day, hour, min).await?;
ftp.upload_file("/model/print.3mf", &file_bytes).await?;
ftp.delete_file("/model/old.3mf").await?;
```

### Camera frames

```rust
// Binary JPEG (A1, P1P, P1S) — port 6000
use bambino::camera::binary::BambuBinaryCameraStream;

let mut cam = BambuBinaryCameraStream::new(tls_stream);
cam.authenticate(access_code).await?;

let mut frame = Vec::new();
cam.read_next_frame(&mut frame).await?; // frame is a JPEG

// RTSPS URL (X1, X2, H2, P2S) — port 322
use bambino::camera::rtsps::build_rtsps_url;
let url = build_rtsps_url(ip, access_code);
// rtsps://bblp:<code>@<ip>:322/streaming/live/1
```

## Platform targets

The default feature set (`tokio`) targets desktop. For embedded, swap the feature flag:

```toml
# ESP32 with ESP-IDF
bambino = { path = "../bambino", default-features = false, features = ["esp-idf"] }

# Bare-metal with Embassy
bambino = { path = "../bambino", default-features = false, features = ["embassy"] }
```

## bambino-cli

A small CLI for testing against real printers. Ships as a binary in the same crate.

```sh
cargo build --bin bambino-cli
```

### Usage

```sh
# Find printers
bambino-cli discover

# Hardware info
bambino-cli info <ip> <serial> <access_code>

# Live telemetry dashboard
bambino-cli monitor <ip> <serial> <access_code>

# Control
bambino-cli control <ip> <serial> <access_code> home
bambino-cli control <ip> <serial> <access_code> temp nozzle 220
bambino-cli control <ip> <serial> <access_code> fan part 80
bambino-cli control <ip> <serial> <access_code> led chamber on
bambino-cli control <ip> <serial> <access_code> pause

# File management
bambino-cli files <ip> <serial> <access_code> list /
bambino-cli files <ip> <serial> <access_code> upload ./print.3mf /model/print.3mf
bambino-cli files <ip> <serial> <access_code> space
```

Add `-v` for protocol-level debug output, or set `BAMBU_VERBOSE=1`.

### Where to find your credentials

- **IP** and **Serial** — `bambino-cli discover` will show them
- **Access Code** — on the printer's LCD under Network > LAN Mode

## License

AGPL 3.0