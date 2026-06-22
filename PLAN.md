# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the architectural progress, finalized specifications, and the scheduled implementation phases for the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

*   **Abstract I/O & Platform Adaptations**: Standardized I/O over async trait bounds (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, `TimerProvider`) mapping to `embedded-io-async`. Built adapters for Host/Tokio (`tokio-rustls` with custom self-signed X.509 verification), ESP-IDF/FreeRTOS, and bare-metal Embassy (`embedded-tls`).
*   **SSDP Discovery Engine**: Integrated zero-copy multicast scanning (Port 2021) and passive NOTIFY parsing using `httparse`.
*   **Permissive State Telemetry**: Designed Serde schemas and custom deserializers (e.g. `sdcard` variation handling) for MQTT status reports, utilizing the `ModelQuirks` trait for static-dispatch model variations.
*   **MQTT Transport**: Implemented an async, transport-agnostic MQTT client with QoS 1 tracking, task ID clamping (`% 2147483647`), keep-alives, and 10s write-channel zombie detection.
*   **Implicit FTPS Client**: Formulated a custom FTPS client with whitespace-insensitive list parsing, automatic vsFTPd socket drops, and model-specific TLS 1.2 clamping.
*   **AMS & Camera Modules**: Structured hex tray presence parsing, index resolution, slicer mapping safety guards, RTP timestamp correction for P2S static video bugs, and metadata-validated binary camera stream processing (Port 6000).
*   **Diagnostics & Calibration**: Unpacks packed HMS telemetry codes to Wiki keys, and generates pressure-advance K-profile database manipulation and deletion requests.
*   **Protocol Integration Harness**: Established full duplex-stream integration mock tests for MQTT, FTPS, and binary camera streaming protocols, verifying concurrency and timeout recovery.

---

## 2. Future Development Phases

### Phase 10: Unified Printer Client Coordinator & Control API
*   **Core Objective**: Implement a high-level `Printer` client coordinator that aggregates the underlying MQTT, FTPS, and camera modules into a simple, unified, async API.
*   **Files & Modules Layout**:
    *   `src/client.rs` / `src/client/mod.rs`
    *   `tests/client_test.rs`
*   **Execution Sequence**:
    1.  **Define Printer Coordinator Struct**: Build `Printer` (or `PrinterClient`) that can be instantiated with either a discovered `SsdpDevice` or manual parameters (`ip`, `serial`, `access_code`, `model`).
    2.  **Encapsulate Connection Handshakes**: Handle secure MQTT and FTPS connection handshakes behind a unified `connect()` function, automatically injecting resolved model quirks.
    3.  **Expose Control Helper Methods**: Implement async helpers for standard user operations:
        *   `pause()`, `resume()`, `cancel()` (wrapping standard control requests).
        *   `home()` (with Bed-on-Z G28 safety checks and Bed-Slinger macros).
        *   `move_axis(x, y, z, feedrate)` (using reference-mode push/pop G-codes).
        *   `extrude(length, feedrate)` (relative extrusion G-code).
        *   `set_temperatures(hotend, bed, chamber)` (extracting target/actual telemetry).
        *   `set_fan_speeds(cooling, aux, exhaust)` (mapping 0-100% to 0-15 steps or G-codes).
        *   `toggle_led(node, turn_on)` (controlling ledctrl payloads).
    4.  **Verification**: Write integration mock tests in `tests/client_test.rs` verifying that calling client helpers transmits the correct MQTT commands and G-codes over mock streams.

### Phase 11: Comprehensive Quirk & Protocol Coverage Audit
*   **Core Objective**: Audit the codebase against the chapters of the `/reference` directory, ensuring 100% coverage of all edge-case commands, telemetry items, and model-specific quirks.
*   **Files & Modules Layout**:
    *   `src/quirks/mod.rs` & `src/quirks/models/`
    *   `src/mqtt/commands.rs` & `src/types/telemetry.rs`
    *   `tests/audit_test.rs`
*   **Execution Sequence**:
    1.  **Implement Remaining Control Payloads**: Add serialization builders for airduct damper modes (`set_airduct`), prompt sounds, and buzzer alarms (`buzzer_ctrl`).
    2.  **AMS Materials and Presets**: Ensure spool color indices, custom presets, and `ams_control` filament load/unload command sequences are fully integrated.
    3.  **Nozzle Kinematics and Tool Changer**: Support IDEX extruder mappings and H2C Vortek tool changer slot monitoring (IDs 16-21).
    4.  **Fan Speed Quantization Oscillation Filter**: Add a telemetry smoothing filter or state debounce to filter rapid fluctuations between adjacent steps (0-15).
    5.  **Verification**: Construct validation test scenarios in `tests/audit_test.rs` checking every edge-case command and telemetry conversion.

### Phase 12: Interactive Developer CLI/REPL Testing Utility
*   **Core Objective**: Create a developer command-line binary `bambu-cli` gated under `std` features, allowing developers to test and verify the library against real physical printers.
*   **Files & Modules Layout**:
    *   `src/bin/bambu-cli.rs`
*   **Execution Sequence**:
    1.  **Implement CLI Binary Structure**: Build a standard executable gated with `#[cfg(feature = "std")]`.
    2.  **Add Subcommands**:
        *   `discover`: Runs SSDP sweeps and prints serials, models, and IPs.
        *   `info <ip> <serial> <access_code>`: Resolves printer details and versions.
        *   `monitor <ip> <serial> <access_code>`: Streams real-time telemetry, HMS alerts, and status.
        *   `control <ip> <serial> <access_code> <action>`: Executes homing, movements, extrusions, LED, and pauses.
        *   `files <ip> <serial> <access_code> <list/upload/download>`: Inspects and transfers files on the SD card.
    3.  **Verification**: Validate that the CLI compiles cleanly under the default `std` feature flags and supports standard local developer executions (`cargo run --bin bambu-cli -- <args>`).

