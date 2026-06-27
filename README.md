# bambino

Async Rust library for talking to Bambu Lab 3D printers over your local network (LAN mode). No cloud, no Bambu Studio — just direct MQTT, FTPS, and camera access.

Designed for use on host machines, powerful ESP32 platforms with `std` support (like the ESP32-P4 via ESP-IDF), and `no_std` embedded targets (via Embassy). Same codebase across all three.

## What it does

- **Discovery** — finds printers on your LAN via SSDP (ports 2021/1990)
- **MQTT control** — connect to the printer's local broker on port 8883, send commands, stream telemetry
- **File transfer** — implicit FTPS on port 990 for listing, uploading, downloading, and deleting files on the SD card
- **Camera** — binary JPEG streaming on port 6000 (A1/P1) and RTSPS on port 322 (X1/X2/H2/P2S)
- **Model quirks** — handles per-model differences polymorphically: TLS data channel modes, fan step rounding, Z-axis homing safety, door sensor routing, camera timestamp corrections, nozzle counts (single/IDEX/tool changer), and chamber heater capabilities

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
    &TokioTimer::new(),
).await?;

for d in &devices {
    println!("{:?} at {} ({})", d.model, d.ip, d.serial);
}
```

### Connect and send commands

```rust
use bambino::client::PrinterClient;
use bambino::models::resolve_model;
use bambino::mqtt::BambuMqttClient;
use bambino::io::TokioIo;
use bambino::io::tokio::{build_unsafe_client_config, TokioTlsConnector};

// TLS + MQTT handshake
let config = build_unsafe_client_config();
let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
let tcp = tokio::net::TcpStream::connect("192.168.1.158:8883").await?;
let tls = connector.connect("192.168.1.158", 8883, TokioIo(tcp)).await?;
let mqtt = BambuMqttClient::connect(tls, serial, access_code).await?;

let model = resolve_model(serial, None);
let mut printer = PrinterClient::new(mqtt, serial, model);

printer.home_axes(false).await?;
printer.set_bed_temperature(60).await?;
printer.set_nozzle_temperature(0, 220).await?;
printer.toggle_led("chamber_light", true).await?;
```

### Custom TLS certificates

By default, `build_unsafe_client_config()` skips certificate verification — matching how Bambu printers present self-signed certs. If you want to verify the printer's certificate or use mutual TLS (mTLS), use `build_verified_client_config()` instead:

```rust
use bambino::io::tokio::{build_verified_client_config, TokioTlsConnector};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

// Load CA cert to verify the printer's certificate
let ca_cert = CertificateDer::from_pem_file("printer-ca.pem").unwrap();

// Server verification only
let config = build_verified_client_config(vec![ca_cert], None).unwrap();

// Or with mutual TLS (client certificate + key)
let client_cert = CertificateDer::from_pem_file("client.pem").unwrap();
let client_key = PrivateKeyDer::from_pem_file("client-key.pem").unwrap();
let config = build_verified_client_config(
    vec![ca_cert],
    Some((vec![client_cert], client_key)),
).unwrap();

let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
// Use connector exactly like the unsafe version
```

The `_with_options` variant adds a `force_tls_1_2` flag for FTPS connections to P2S/X2D models.

### AMS filament control

```rust
printer.change_filament(0, 1, 1, -1, -1).await?;  // load slot 1
printer.start_drying(0, 55, 480, true, "PA-CF").await?;
```

### Print jobs

```rust
use bambino::client::{PrintSpeed, CalibrationOption};
use bambino::mqtt::PrintJobConfig;

printer.set_print_speed(PrintSpeed::Sport).await?;
printer.start_calibration(
    CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
).await?;

let config = PrintJobConfig::new(
    "job.3mf",
    "Metadata/plate_1.gcode",
    "My Print",
    12345,
    "textured",
).with_ams(vec![0, -1, 1]);

printer.start_print(&config).await?;
```

### Read telemetry

```rust
loop {
    let event = printer.poll_telemetry().await?;
    if let Some(report) = event.report() {
        if let Some(print) = &report.print {
            println!("{:?} — {:?}%", print.gcode_state, print.mc_percent);
        }
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
let data = ftp.download_file("/timelapse/video.mp4").await?;
ftp.create_directory("/model/subfolder").await?;
ftp.rename_file("/model/old.3mf", "/model/backup.3mf").await?;
ftp.delete_file("/model/old.3mf").await?;
ftp.remove_directory("/model/subfolder").await?;
```

### Camera streaming

Bambu printers expose camera feeds through two different protocols depending on the model:

**Binary JPEG (A1, P1P, P1S) — port 6000.** These models stream discrete JPEG frames over a lightweight binary protocol. Connect via TLS, send an 80-byte auth handshake, then read frames in a loop:

```rust
use bambino::camera::binary::BambuBinaryCameraStream;

let mut cam = BambuBinaryCameraStream::new(tls_stream);
cam.authenticate(access_code).await?;

let mut frame = Vec::new();
loop {
    cam.read_next_frame(&mut frame).await?;
    // frame contains a complete JPEG image
}
```

**RTSPS (X1, X2, H2, P2S) — port 322.** These models host an RTSP server behind implicit TLS with Digest authentication. This library provides helper utilities for integration with external media frameworks (FFmpeg, GStreamer, VLC) — it does not include an RTSP client or TLS proxy.

```rust
use bambino::camera::rtsps::build_rtsps_url;

// Generate the authenticated URL for your media framework
let url = build_rtsps_url(ip, access_code);
// → rtsps://bblp:<code>@<ip>:322/streaming/live/1
```

The printer's self-signed TLS certificate means most media players can't connect directly. The typical approach is a local decryption proxy that accepts plain `rtsp://` on localhost, wraps it in TLS, and forwards to the printer. When doing this, RTSP Digest auth hashes must match the printer's URI — use `rewrite_rtsp_request_uri` to rewrite the proxy-local URI before forwarding:

```rust
use bambino::camera::rtsps::rewrite_rtsp_request_uri;

// Player sends:  rtsp://127.0.0.1:8554/streaming/live/1
// Printer needs: rtsps://192.168.1.150:322/streaming/live/1
let rewritten = rewrite_rtsp_request_uri(player_uri, printer_ip);
```

P2S printers on certain firmware versions have a bug where RTP timestamps don't advance, causing video freezes. Use `RtpTimestampCorrector` to synthesize correct timestamps when `model.quirks().requires_wallclock_rtsp_timestamps()` returns true.

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
bambino-cli control <ip> <serial> <access_code> speed sport
bambino-cli control <ip> <serial> <access_code> clear-error
bambino-cli control <ip> <serial> <access_code> airduct cooling
bambino-cli control <ip> <serial> <access_code> calibrate bed-leveling vibration
bambino-cli control <ip> <serial> <access_code> ams dry 0 55 480 true PA-CF
bambino-cli control <ip> <serial> <access_code> ams dry-stop 0

# Send gcode—with some safety checks
bambino-cli control <ip> <serial> <access_code> gcode "G28"

# Send gcode—without safety checks
bambino-cli control <ip> <serial> <access_code> gcode-raw "M106 P1 S255"  # prompts for confirmation
bambino-cli control <ip> <serial> <access_code> gcode-raw --unsafe "M106 P1 S255"  # bypasses confirmation

# File management
bambino-cli files <ip> <serial> <access_code> list /
bambino-cli files <ip> <serial> <access_code> upload ./print.3mf /model/print.3mf
bambino-cli files <ip> <serial> <access_code> space

# Camera (A1/P1 binary JPEG protocol only)
bambino-cli camera <ip> <serial> <access_code> snapshot            # saves snapshot.jpg
bambino-cli camera <ip> <serial> <access_code> snapshot frame.jpg  # custom output path
```

Add `-v` for protocol-level debug output.

### Where to find your credentials

- **IP** and **Serial** — `bambino-cli discover` will show them
- **Access Code** — on the printer's LCD under Network > LAN Mode

## Firmware quirks

### K-Profile priming

The firmware ignores the first `extrusion_cali_get` command received after connecting. `PrinterClient::get_k_profiles()` handles this automatically by sending a throwaway priming request before the real query. If you manage priming yourself or target firmware that doesn't need it, call `set_k_profile_primed(true)` to skip the automatic prime.

## Acknowledgements

Bambino would not have been possible without the reverse-engineering work of other excellent projects.

*  [Bambuddy](https://github.com/maziggy/bambuddy)
*  [ha-bambulab](https://github.com/greghesp/ha-bambulab/)
*  [bambu-printer-manager](https://github.com/synman/bambu-printer-manager)
*  [OpenBambuAPI](https://github.com/Doridian/OpenBambuAPI/)

## License

[AGPL-3.0](LICENSE)
