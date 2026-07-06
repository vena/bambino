# bambino

# bambino

Talk to Bambu Lab 3D printers over your local network.

`bambino` is an async Rust library that speaks the Bambu Lab LAN protocol —
MQTT for commands and telemetry, implicit FTPS for file management, and SSDP
for printer discovery. It compiles to three targets from one codebase:

| Target | Runtime | TLS | Feature flags |
|--------|---------|-----|---------------|
| Host (desktop/server) | tokio | rustls | `default` = `["std", "tokio"]` |
| ESP-IDF (ESP32, FreeRTOS) | std threads | ESP-TLS | `esp-idf` |
| Bare-metal (embassy) | embassy | embedded-tls | `embassy` (implies `no_std` + `alloc`) |

All network I/O goes through abstract traits in the [`io`] module, so library
code never touches `tokio::` or `std::net::` directly.

# Quick start

```ignore
use bambino::client::{PrinterClient, TelemetryEvent};
use bambino::models::resolve_model;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};

async fn example() -> Result<(), bambino::BambuError> {
    // Printers use self-signed certs, so we skip verification
    let tls_config = build_unsafe_client_config();
    let tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(tls_config));

    // Create a lazy client — MQTT connects automatically on first use
    let model = resolve_model("SERIAL123456", None);
    let mut printer = PrinterClient::new(
        tls, TokioRawStreamFactory, "192.168.1.100", "SERIAL123456", "12345678", model,
    )
    .with_timer(TokioTimer::new());

    // First method call triggers the MQTT connection
    printer.request_pushall().await?;

    loop {
        match printer.poll_telemetry().await? {
            TelemetryEvent::Report(report, _raw) => {
                let (bed_actual, bed_target) = report.bed_temperatures();
                println!("Bed: {}°C / {}°C target", bed_actual, bed_target);
            }
            TelemetryEvent::Unknown(_) => {}
        }
    }
}
```

# Feature flags

| Flag | What it enables |
|------|-----------------|
| `std` | Standard library, `thiserror`, `serde`/`serde_json` std features |
| `tokio` | Tokio runtime, rustls TLS, CLI binary (implies `std`) |
| `esp-idf` | ESP-IDF system services for embedded Linux-like targets (implies `std`) |
| `embassy` | Embassy async runtime, embedded-tls, embassy-net (implies `no_std` + `alloc`) |
| `alloc` | Heap allocation for `no_std` environments (String, Vec, format!) |

# Module guide

- [`client`] — The main entry point. [`client::PrinterClient`] wraps MQTT + FTPS into
  one coordinated interface with methods for thermal control, motion, print jobs, etc.
- [`mqtt`] — Low-level MQTT v3.1.1 client and command serialization.
- [`ftps`] — Implicit FTPS client for SD card file operations.
- [`discovery`] — SSDP-based printer discovery on the local network.
- [`types`] — Telemetry schemas, version info, and shared data types.
- [`models`] — Printer model identification from serial numbers.
- [`quirks`] — Per-model behavioral differences (fan mapping, door sensors, temp limits, etc.).
- [`io`] — Transport abstraction traits ([`io::AsyncIo`], [`io::TlsConnector`], etc.).
- [`ams`] — AMS filament system helpers (slot mapping, presence detection).
- [`camera`] — Camera streaming protocols (binary JPEG on port 6000, RTSPS on port 322).
- [`diagnostics`] — HMS alert decoding and K-profile (Linear Advance) management.
- [`error`] — The unified [`BambuError`] type.

## Modules

### [`bambino`](bambino.md)

*12 modules*

### [`ams`](ams.md)

*2 modules*

### [`ams::mapping`](ams/mapping.md)

*1 enum, 1 struct, 4 functions*

### [`ams::parser`](ams/parser.md)

*4 functions*

### [`camera`](camera.md)

*1 enum, 2 constants, 2 modules*

### [`camera::binary`](camera/binary.md)

*1 function, 1 struct*

### [`camera::rtsps`](camera/rtsps.md)

*1 struct, 2 functions*

### [`client`](client.md)

*1 struct, 2 modules*

### [`client::types`](client/types.md)

*2 structs, 5 enums*

### [`diagnostics`](diagnostics.md)

*2 modules*

### [`diagnostics::hms`](diagnostics/hms.md)

*1 enum, 2 functions, 2 structs*

### [`diagnostics::kprofile`](diagnostics/kprofile.md)

*1 function, 15 structs*

### [`discovery`](discovery.md)

*1 function, 1 module, 1 struct, 3 constants*

### [`discovery::parser`](discovery/parser.md)

*1 function, 1 struct*

### [`error`](error.md)

*1 enum*

### [`ftps`](ftps.md)

*2 modules*

### [`ftps::client`](ftps/client.md)

*1 struct*

### [`ftps::parser`](ftps/parser.md)

*1 function, 1 struct*

### [`io`](io.md)

*1 module, 3 enums, 6 traits*

### [`io::tokio`](io/tokio.md)

*5 functions, 7 structs*

### [`models`](models.md)

*1 enum, 1 function*

### [`mqtt`](mqtt.md)

*2 modules*

### [`mqtt::client`](mqtt/client.md)

*2 structs*

### [`mqtt::commands`](mqtt/commands.md)

*1 function, 6 modules*

### [`mqtt::commands::ams`](mqtt/commands/ams.md)

*10 structs*

### [`mqtt::commands::control`](mqtt/commands/control.md)

*10 structs*

### [`mqtt::commands::gcode`](mqtt/commands/gcode.md)

*2 structs*

### [`mqtt::commands::hardware`](mqtt/commands/hardware.md)

*1 enum, 8 structs*

### [`mqtt::commands::print_job`](mqtt/commands/print_job.md)

*1 enum, 3 structs*

### [`mqtt::commands::status`](mqtt/commands/status.md)

*4 structs*

### [`quirks`](quirks.md)

*1 module, 1 struct, 1 trait, 2 functions*

### [`quirks::models`](quirks/models.md)

*7 modules*

### [`quirks::models::a1`](quirks/models/a1.md)

*2 structs, 5 constants*

### [`quirks::models::a2`](quirks/models/a2.md)

*1 struct, 3 constants*

### [`quirks::models::h2`](quirks/models/h2.md)

*4 structs, 5 constants*

### [`quirks::models::p1`](quirks/models/p1.md)

*1 struct, 3 constants*

### [`quirks::models::p2`](quirks/models/p2.md)

*1 struct, 3 constants*

### [`quirks::models::x1`](quirks/models/x1.md)

*2 structs, 7 constants*

### [`quirks::models::x2`](quirks/models/x2.md)

*1 struct, 4 constants*

### [`types`](types.md)

*2 modules*

### [`types::telemetry`](types/telemetry.md)

*1 struct, 2 functions, 4 modules*

### [`types::telemetry::ams`](types/telemetry/ams.md)

*5 structs*

### [`types::telemetry::device`](types/telemetry/device.md)

*11 structs*

### [`types::telemetry::diagnostics`](types/telemetry/diagnostics.md)

*4 structs*

### [`types::telemetry::report`](types/telemetry/report.md)

*2 structs*

### [`types::version`](types/version.md)

*2 structs*

