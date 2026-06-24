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

---

## 2. Remaining Work — Protocol Coverage & API Completeness

The following phases cover gaps between the reference documentation (`reference/`) and the library's current implementation surface, identified via cross-audit against the Bambuddy reference application.

### Phase 13: Missing MQTT Command Structs

Add serializable request structs in `src/mqtt/commands.rs` for documented commands that have no representation:

* [x] `AmsChangeFilamentRequest` — load/unload filament from standard AMS or external spool. Requires `ams_id`, `slot_id`, `target`, `curr_temp`, `tar_temp`, `sequence_id`. Ref: `reference/05_materials_ams.md` §5.3.
* [x] `AmsFilamentDryingRequest` — start/stop AMS-HT dry-chamber heating. Requires `ams_id`, `mode` (start/stop), `dry_temp`, `dry_time` (minutes), `rotate_tray`, `filament`. Ref: `reference/05_materials_ams.md` §5.4.
* [x] `CleanPrintErrorRequest` — clear active error codes. Ref: `reference/03_mqtt_telemetry.md` §3.3.
* [x] `ExtrusionCaliSelRequest` — bind a stored K-profile calibration entry to an AMS material slot. Ref: `reference/05_materials_ams.md` §5.3.

### Phase 14: PrinterClient Helper Methods

Expose convenience methods on `PrinterClient` in `src/client.rs` for command structs that exist but lack client-level wrappers:

* [ ] `change_filament()` — wraps `AmsChangeFilamentRequest`
* [ ] `start_drying()` / `stop_drying()` — wraps `AmsFilamentDryingRequest`
* [ ] `clear_print_error()` — wraps `CleanPrintErrorRequest`
* [ ] `set_print_speed(level)` — wraps existing `PrintSpeedRequest`
* [ ] `skip_objects(ids)` — wraps existing `SkipObjectsRequest`
* [ ] `start_print(file, ams_mapping)` — wraps existing `ProjectFileRequest`
* [ ] `scan_rfid(tray_id)` — wraps existing `AmsGetRfidRequest`
* [ ] `start_calibration(type)` — wraps existing `CalibrationRequest`
* [ ] `select_k_profile(...)` — wraps `ExtrusionCaliSelRequest`

### Phase 15: FTPS File Operations

Add missing FTP commands to `BambuFtpsClient` in `src/ftps/client.rs`:

* [ ] `download_file(remote_path) -> Vec<u8>` — RETR command with passive data channel
* [ ] `create_directory(path)` — MKD command
* [ ] `remove_directory(path)` — RMD command
* [ ] `rename_file(from, to)` — RNFR + RNTO command pair

### Phase 16: Telemetry Struct Completeness

Extend `PrintTelemetry` in `src/types/telemetry.rs` with documented fields not yet captured:

* [ ] `print_error: Option<u32>` — active error code register
* [ ] `hms: Option<Vec<HmsEntry>>` with `HmsEntry { attr: u32, code: u32 }` — active hardware alerts
* [ ] `mc_print_sub_stage: Option<i32>` — print sub-stage identifier
* [ ] `ipcam_dev: Option<String>` — camera module state
* [ ] `ipcam_record: Option<String>` — recording status (enable/disable)
* [ ] `timelapse: Option<String>` — timelapse recording status
* [ ] `xcam: Option<serde_json::Value>` — AI detection settings
* [ ] Helper: `is_door_open(model) -> Option<bool>` — extract door sensor from `home_flag` (X1) or `stat` (P2S/X2D/H2) bit 23

### Phase 17: Discovery Port 1990 Support

The Bambu Lab wiki lists both ports 1990 and 2021 for device discovery. Currently only port 2021 is implemented.

* [ ] Bind a second socket to port 1990 in `discover_devices` (or run two engines concurrently)
* [ ] Send M-SEARCH to both ports
* [ ] Merge and deduplicate results by serial number
* [ ] Update `bambino-cli` discover command to report which port each printer was found on (verbose only)