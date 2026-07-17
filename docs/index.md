# Crate `bambino`

**Version:** 0.1.0

Talk to Bambu Lab 3D printers over your local network.

`bambino` is an async Rust library that speaks the Bambu Lab LAN protocol —
MQTT for commands and telemetry, implicit FTPS for file management, and SSDP
for printer discovery. It compiles to three targets from one codebase:

| Target | Runtime | TLS | Feature flags |
|--------|---------|-----|---------------|
| Host (desktop/server) | tokio | rustls | `default` = `["std", "tokio"]` |
| ESP-IDF (ESP32, FreeRTOS) | std threads | ESP-TLS | `esp-idf` |
| Bare-metal (embassy) | embassy | mbedtls-rs | `embassy` (implies `no_std` + `alloc`) |

All network I/O goes through abstract traits in the [`io`](io/index.md#io) module, so library
code never touches `tokio::` or `std::net::` directly.

# Quick start

```ignore
use bambino::client::{PrinterClient, TelemetryEvent};
use bambino::models::resolve_model;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTlsConnector, TokioTimer,
    build_unsafe_client_config,
};

async fn example() -> Result<(), bambino::Error> {
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
| `tokio` | Tokio runtime, rustls TLS (implies `std`) |
| `cli` | The `bambino-cli` binary (implies `tokio`) |
| `esp-idf` | ESP-IDF system services for embedded Linux-like targets (implies `std`) |
| `embassy` | Embassy async runtime, mbedtls-rs TLS, embassy-net (implies `no_std` + `alloc`) |
| `alloc` | Heap allocation for `no_std` environments (String, Vec, format!) |

# Module guide

- [`client`](client/index.md#client) — The main entry point. [`PrinterClient`](client/index.md#printerclient) wraps MQTT + FTPS into
  one coordinated interface with methods for thermal control, motion, print jobs, etc.
- [`mqtt`](mqtt/index.md#mqtt) — Low-level MQTT v3.1.1 client and command serialization.
- [`ftps`](ftps/index.md#ftps) — Implicit FTPS client for SD card file operations.
- [`discovery`](discovery/index.md#discovery) — SSDP-based printer discovery on the local network.
- [`types`](client/types/index.md#types) — Telemetry schemas, version info, and shared data types.
- [`models`](models/index.md#models) — Printer model identification from serial numbers.
- [`quirks`](quirks/index.md#quirks) — Per-model behavioral differences (fan mapping, door sensors, temp limits, etc.).
- [`io`](io/index.md#io) — Transport abstraction traits ([`AsyncIo`](io/index.md#asyncio), [`TlsConnector`](io/index.md#tlsconnector), etc.).
- [`ams`](ams/index.md#ams) — AMS filament system helpers (slot mapping, presence detection).
- [`camera`](camera/index.md#camera) — Camera streaming protocols (binary JPEG on port 6000, RTSPS on port 322).
- [`diagnostics`](diagnostics/index.md#diagnostics) — HMS alert decoding and K-profile (Linear Advance) management.
- [`error`](error/index.md#error) — The unified [`Error`](https://docs.rs/asn1_rs/latest/asn1_rs/error/enum.Error.html) type.

## Contents

- [Modules](#modules)
  - [`ams`](#ams)
  - [`camera`](#camera)
  - [`client`](#client)
  - [`diagnostics`](#diagnostics)
  - [`discovery`](#discovery)
  - [`error`](#error)
  - [`ftps`](#ftps)
  - [`io`](#io)
  - [`models`](#models)
  - [`mqtt`](#mqtt)
  - [`quirks`](#quirks)
  - [`types`](#types)
- [Types](#types)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`ams`](#ams) | mod | # AMS Filament System |
| [`camera`](#camera) | mod | # Camera & Video Streaming |
| [`client`](#client) | mod | # Printer Client |
| [`diagnostics`](#diagnostics) | mod | # Diagnostics & Calibration |
| [`discovery`](#discovery) | mod | # Printer Discovery (SSDP) |
| [`error`](#error) | mod | # Error Types |
| [`ftps`](#ftps) | mod | # FTPS File Transfer Client |
| [`io`](#io) | mod | # Transport Abstraction Layer |
| [`models`](#models) | mod | # Printer Model Identification |
| [`mqtt`](#mqtt) | mod | # MQTT Client & Command Serialization |
| [`quirks`](#quirks) | mod | # Model-Specific Quirks |
| [`types`](#types) | mod | # Types & Telemetry Schemas |

## Modules

- [`ams`](ams/index.md#ams) — # AMS Filament System
- [`camera`](camera/index.md#camera) — # Camera & Video Streaming
- [`client`](client/index.md#client) — # Printer Client
- [`diagnostics`](diagnostics/index.md#diagnostics) — # Diagnostics & Calibration
- [`discovery`](discovery/index.md#discovery) — # Printer Discovery (SSDP)
- [`error`](error/index.md#error) — # Error Types
- [`ftps`](ftps/index.md#ftps) — # FTPS File Transfer Client
- [`io`](io/index.md#io) — # Transport Abstraction Layer
- [`models`](models/index.md#models) — # Printer Model Identification
- [`mqtt`](mqtt/index.md#mqtt) — # MQTT Client & Command Serialization
- [`quirks`](quirks/index.md#quirks) — # Model-Specific Quirks
- [`types`](types/index.md#types) — # Types & Telemetry Schemas


---

## Types

### `Error`

```rust
enum Error {
    Network(crate::io::SocketError),
    TimerFailure(crate::io::TimerError),
    TlsHandshakeFailed,
    ProtocolViolation(std::borrow::Cow<'static, str>),
    Serialization,
    AccessDenied,
    Timeout,
    DiskWriteFailure,
    ModelMismatch(std::borrow::Cow<'static, str>),
}
```

Unified error type for the `bambino` crate.

This enum wraps all protocol, serialization, and transport-level failures
with localized error contexts. Under `std` environments, standard formatting
and source error tracing are derived automatically via `thiserror`.

#### Variants

- **`Network`**

  Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.

- **`TimerFailure`**

  Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS timer resource exhaustion).

- **`TlsHandshakeFailed`**

  Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
  This frequently occurs during self-signed certificate verification or SNI mismatches.

- **`ProtocolViolation`**

  Emitted when a printer violates expected protocol states or emits illegal data lines.

- **`Serialization`**

  Serializer and Deserializer mismatches during telemetry JSON parsing.

- **`AccessDenied`**

  Emitted when the provided 8-character LAN access code fails verification checks.

- **`Timeout`**

  Handshake, read, or write negotiations exceeded designated timeouts.

- **`DiskWriteFailure`**

  Upload verification failed — printer reported unexpected file size after transfer.

- **`ModelMismatch`**

  Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) not present on the active model target.

#### Trait Implementations

##### `impl Clone for Error`

- <span id="error-clone"></span>`fn clone(&self) -> Error` — [`Error`](error/index.md#error)

##### `impl Debug for Error`

- <span id="error-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for Error`

- <span id="error-display-fmt"></span>`fn fmt(&self, __formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result`

##### `impl Error for Error`

##### `impl ToString for Error`

- <span id="error-tostring-to-string"></span>`fn to_string(&self) -> String`

### `PrinterModel`

```rust
enum PrinterModel {
    X1C,
    X1E,
    X2D,
    A1Mini,
    A1,
    A2L,
    P1P,
    P1S,
    P2S,
    H2D,
    H2DPro,
    H2C,
    H2S,
    Unknown,
}
```

Enumeration of physical Bambu Lab printer models supported on the local interface.

#### Variants

- **`X1C`**

  X1 and X1C Series (CoreXY architecture, RTSP-capable)

- **`X1E`**

  X1E (Enterprise CoreXY architecture, wired Ethernet)

- **`X2D`**

  X2D Series (CoreXY architecture, dual auxiliary cooling)

- **`A1Mini`**

  A1 Mini (Constrained bed-slinger, binary camera stream)

- **`A1`**

  A1 (Standard bed-slinger, binary camera stream)

- **`A2L`**

  A2L Series

- **`P1P`**

  P1P (Early CoreXY architecture, binary camera stream)

- **`P1S`**

  P1S (Enclosed CoreXY architecture, binary camera stream)

- **`P2S`**

  P2S Series (RTSP-capable)

- **`H2D`**

  H2D (Dual-nozzle IDEX platform)

- **`H2DPro`**

  H2D Pro (Premium IDEX platform)

- **`H2C`**

  H2C (Vortek tool-changer + fixed hotend, 7 nozzles total)

- **`H2S`**

  H2S (Single-nozzle platform sharing H2 mechanics)

- **`Unknown`**

  Fallback variant for newly released or unrecognized printer targets

#### Implementations

- <span id="cratemodelsprintermodel-quirks"></span>`fn quirks(&self) -> &'static dyn ModelQuirks` — [`ModelQuirks`](quirks/index.md#modelquirks)

  Returns the [`ModelQuirks`](quirks/index.md#modelquirks) strategy for this model variant.

#### Trait Implementations

##### `impl Clone for PrinterModel`

- <span id="printermodel-clone"></span>`fn clone(&self) -> PrinterModel` — [`PrinterModel`](models/index.md#printermodel)

##### `impl Copy for PrinterModel`

##### `impl Debug for PrinterModel`

- <span id="printermodel-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrinterModel`

##### `impl Hash for PrinterModel`

- <span id="printermodel-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrinterModel`

- <span id="printermodel-partialeq-eq"></span>`fn eq(&self, other: &PrinterModel) -> bool` — [`PrinterModel`](models/index.md#printermodel)

