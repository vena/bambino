# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the remaining integration milestones and core architectural state of the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

The `bambu-lan` library provides a platform-agnostic implementation of local network (LAN mode) protocols. Key architectural modules implemented include:
* **SSDP Network Discovery (Port 2021):** Zero-copy UDP datagram parser resolving printer models and serial number prefixes.
* **MQTT v3.1.1 Client (Port 8883):** Custom secure session manager with QoS 1 in-flight queues, keep-alives, and 10-second write-channel zombie detection.
* **Implicit FTPS Client (Port 990):** Implicitly encrypted file system traverser supporting PASSV session reuse, strict TLS 1.2 enforcement for vsFTPd compatibility, and chunked binary transfers.
* **Polymorphic Quirks Engine:** Decentralized traits (`src/quirks/models/`) managing family-specific constraints (such as bed-on-Z homing limits, fan speed rounding/debouncing, and active chamber heating blocks).
* **Command & Telemetry Schemas:** Decoders translating composite packed temperatures, door sensor bitmasks, structured IDEX nozzle info collections, and polymorphic AMS slicer mapping variables.

---

## 2. Future Development Phases

### Phase 12: Interactive Developer CLI/REPL Testing Utility
* **Core Objective**: Create a developer command-line binary `bambu-cli` gated under `std` features, allowing developers to test and verify the library against real physical printers.
* **Files & Modules Layout**:
  * `src/bin/bambu-cli.rs`
* **Execution Sequence**:
  1. **Implement CLI Binary Structure**: Build a standard executable gated with `#[cfg(feature = "std")]`.
  2. **Add Subcommands**:
     * `discover`: Runs SSDP sweeps and prints serials, models, and IPs.
     * `info <ip> <serial> <access_code>`: Resolves printer details and versions.
     * `monitor <ip> <serial> <access_code>`: Streams real-time telemetry, HMS alerts, and status.
     * `control <ip> <serial> <access_code> <action>`: Executes homing, movements, extrusions, LED, and pauses.
     * `files <ip> <serial> <access_code> <list/upload/download>`: Inspects and transfers files on the SD card.
  3. **Verification**: Validate that the CLI compiles cleanly under the default `std` feature flags and supports standard local developer executions (`cargo run --bin bambu-cli -- <args>`).
