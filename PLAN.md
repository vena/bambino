# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the architectural progress, finalized specifications, and scheduled implementation phases for the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

*   **Asynchronous Transport Abstraction**: Decoupled, zero-dependency trait bounds mapping to `embedded-io-async`. Provides modular execution interfaces for standard hosts (Tokio/Rustls), RTOS (ESP-IDF), and bare-metal (Embassy/embedded-tls) targets.
*   **Multicast Discovery & Parser**: Zero-copy Port 2021 SSDP scanner that dynamically resolves model signatures and bypasses the H2S/H2D serial collision hazard.
*   **MQTT Telemetry & Control Transport**: Streamlined MQTT v3.1.1 packet client with in-flight QoS 1 tracking, 32-bit signed task-ID clamping, and 10-second write-channel zombie detection.
*   **Implicit FTPS Client**: Implicitly encrypted control and passive data channel pipeline supporting UNIX tokenization listings, TLS 1.2 strict version pinning, and write-flush validation.
*   **AMS, Camera & Calibration Decoders**: Specialized protocol-level helpers handling hex presence masks, multi-nozzle IDEX mappings, binary camera handshake structures (Port 6000), and diagnostic HMS-to-Wiki translation keys.
*   **Unified Client Coordinator**: High-level `PrinterClient` api wrapping safe homing, relative axis movements, manual extrusion, and fan speed calculations with compile-time, zero-overhead dummy parameters.

---

## 2. Future Development Phases

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
