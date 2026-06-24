# Bambu Lab LAN Protocol Client Crate (`bambino`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the structural status and architectural design of the `bambino` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

The `bambino` library provides a platform-agnostic abstraction of local network (LAN mode) protocols. For the next session, understand the following completed architectural foundations:

* **Platform-Agnostic I/O Boundaries**: Core operations are decoupled from standard system dependencies. The transport layer relies on abstract traits (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, and `TimerProvider`), enabling identical client code to compile across standard host operating systems (Tokio), RTOS microcontrollers (ESP-IDF), and bare-metal environments (Embassy).
* **Polymorphic Quirks Engine**: Printer-specific variations, mechanical safety interlocks (such as Z-axis homing crash protection on CoreXY machines), and unsupported commands are managed polymorphically. Model-specific constraints are encapsulated in decoupled strategy structs (e.g., `P1Quirks`, `X1Quirks`) implementing the `ModelQuirks` trait, resolved via the static `model.quirks()` strategy dispatcher.
* **Developer Verification CLI (`bambino-cli`)**: A lightweight, dependency-free binary module directory (`src/bin/bambino-cli/`) providing subcommands representing each library transport protocol:
  * `discover`: Broadcast/multicast dual SSDP network scanner.
  * `info`: Expansion bus module and version query utility across all hardware tracks (supports polymorphic matching of root vs nested `info` payload structures).
  * `monitor`: Real-time telemetry, composite thermal unpacking, and live HMS decoder.
  * `control`: Safer coordinate movement, manual extrusion feed, fan speed rounding, and lighting.
  * `files`: Passive implicit FTPS file-system listing, space allocation check, chunked upload, and deletion.
* **Full MQTT Command Coverage** (Phases 13–14): All documented MQTT command types have serializable request structs in `src/mqtt/commands.rs`, including AMS filament change/drying, error clearing, and K-profile calibration binding. Every command struct is exposed through a corresponding convenience method on `PrinterClient` in `src/client.rs` (e.g., `change_filament()`, `start_drying()`, `clear_print_error()`, `set_print_speed()`, `skip_objects()`, `start_print()`, `start_calibration()`, `select_k_profile()`).
* **Complete FTPS File Operations** (Phase 15): `BambuFtpsClient` supports the full lifecycle of remote filesystem operations: listing, upload, download (`RETR`), deletion, directory creation/removal (`MKD`/`RMD`), and rename (`RNFR`+`RNTO`).
* **Full Telemetry Struct Coverage** (Phase 16): `PrintTelemetry` captures all documented wire fields including `print_error`, HMS hardware alerts, print sub-stage, camera/timelapse state, xcam AI detection settings, and door sensor extraction via `is_door_open(model)`.
* **Dual-Port SSDP Discovery** (Phase 17): `discover_devices` binds sockets on both ports 2021 and 1990, sends M-SEARCH queries to each, and deduplicates results by serial number, covering the full range of Bambu Lab firmware discovery behavior.
* **Structured Logging** (Phase 18): Library diagnostic output uses the `log` crate facade (`log::debug!`, `log::trace!`, `log::warn!`) with no `#[cfg]` gates. The CLI initializes `env_logger` from the `-v` flag. No `println!`-based verbose logging remains in library code.

---

## 2. Remaining Work — Protocol Coverage & API Completeness

All phases (13–18) identified in the cross-audit against the Bambuddy reference application have been completed. The library's MQTT command surface, FTPS operations, telemetry structs, discovery engine, and logging infrastructure are now fully aligned with the reference documentation.