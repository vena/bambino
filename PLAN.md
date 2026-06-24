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

## 2. Remaining Work — Code Quality & Modernization

A deep audit of the full codebase identified systemic patterns worth addressing: discarded error sources, scattered magic numbers, repeated boilerplate, suboptimal collection choices, and quirks engine correctness issues. These phases are ordered by dependency — foundational work first, then consumers of that work.

**Phase dependency chain:** 19 → 20 → 21 → 22 → 23 → 24. Each phase may depend on changes made in earlier phases (e.g., Phase 20 uses the `From` impl from Phase 19; Phase 23 tests use constants from Phase 19; Phase 24 builds on the deduplicated quirks from Phase 20).

**Verification gate between each phase:** `cargo build && cargo build --no-default-features --features alloc --lib && cargo test` — every change must compile under both the default `tokio` feature set and the `no_std`+`alloc` library target.

### Phase 19: Error Handling & Constants Foundation

Upgrade the error type system and extract magic numbers into named constants. No other phase should be started before this one completes — `From` impls and named constants are used throughout later phases.

**Error handling (`src/error.rs`):**

The current structure: `BambuError` derives `Debug` only (not `Clone`). Under `std`, it uses `thiserror` for `Display`/`Error`. Under `no_std`, a manual `Display` impl duplicates all message strings (lines 62-78). Two separate variants exist for protocol errors: `ProtocolViolation(&'static str)` and `ProtocolViolationDynamic(String)` (the latter gated on `alloc`/`std`). `SocketError` in `src/io/mod.rs` already derives `Clone, Copy`.

* [x] Implement `From<SocketError> for BambuError` in `src/error.rs` — eliminates 29 manual `.map_err(BambuError::NetworkError)` calls (14 in `src/ftps/client.rs`, 10 in `src/mqtt/client.rs`, 4 in `src/camera/binary.rs`, 1 in `src/discovery/mod.rs`)
* [x] Unify `ProtocolViolation` and `ProtocolViolationDynamic` into a single `ProtocolViolation(Cow<'static, str>)` variant (gated on alloc/std; bare no_std keeps `&'static str` only). `ProtocolViolationDynamic` is only used in CLI code (`src/bin/bambino-cli/control.rs`, `storage.rs`), not the library — low urgency but cleaner API
* [x] Sync the manual no_std `Display` impl with thiserror annotations; add a `#[cfg(test)]` assertion comparing `format!("{}", variant)` output to catch future drift
* [x] Add `Clone` derive to `BambuError` (all variants are Clone-compatible once `Cow` replaces `String`)

**Named constants — extract magic numbers into `pub(crate) const` blocks:**
* [x] MQTT packet types in `src/mqtt/client.rs`
* [x] FTP response codes in `src/ftps/client.rs`
* [x] AMS tray state/ID boundaries in `src/ams/`
* [x] Camera frame constants in `src/camera/binary.rs`
* [x] HMS fault threshold and cancellation codes in `src/diagnostics/hms.rs`
* [x] Door sensor bitmask in `src/types/telemetry.rs`
* [x] FTPS misc in `src/ftps/client.rs`: port, chunk size, data buffer, size heuristic threshold, PASV port multiplier
* [x] MQTT misc in `src/mqtt/client.rs`: in-flight limit, keep-alive timeout
* [x] Discovery in `src/discovery/mod.rs`: re-broadcast interval
* [x] Telemetry in `src/types/telemetry.rs`: temperature unpacking divisor
* [x] Camera ports in `src/camera/`: RTSPS port, binary JPEG port, RTP clock frequency
* [x] Fan conversion in `src/quirks/mod.rs`: step divisor, rounding offset
* [x] Other: `i32::MAX as u64` in `clamp_task_id`, sequence ID start in `src/client.rs`, UDP recv timeout in `src/io/tokio.rs`

### Phase 20: Library DRY Refactoring

Extract repeated patterns into helper methods, reducing mechanical duplication. Depends on Phase 19 (`From` impl, constants).

* [x] Add `async fn publish_request<T: Serialize>(&mut self, request: &T) -> Result<u16, BambuError>` helper on `PrinterClient` in `src/client.rs` — replaces 18 identical `serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?; self.mqtt.publish_command(&payload).await` sequences
* [x] Deduplicate Z-axis safety G-code to a single location in `src/quirks/models/` — the identical `String::from("M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode")` literal is copy-pasted across all 6 model files (a1, p1, p2, x1, x2, h2). Consolidated to `DEFAULT_Z_MOVE_GCODE` constant in `src/quirks/mod.rs`; Phase 24 will later convert this to a parameterized format that uses the actual `distance`/`feedrate` arguments
* [x] Add `Vec::with_capacity` hints to MQTT encoding functions in `src/mqtt/client.rs` — `encode_remaining_length` (cap 4), `encode_connect`/`encode_subscribe`/`encode_publish_qos1` (pre-calculated from topic/payload lengths)
* [x] Propagate `From<SocketError>` from Phase 19 — replaced 17 `.map_err(BambuError::NetworkError)` calls with `?` (10 library, 7 CLI) where the calling function returns `Result<_, BambuError>`
* [x] Log discarded source errors — changed `.map_err(|_| SocketError::ConnectionReset)` closures in `src/mqtt/client.rs` `read_exact_packet` (3 occurrences) to `log::trace!` the source error before converting

### Phase 21: Collection Efficiency & API Surface Polish

Replace suboptimal data structures, restrict visibility, and clean up the public API. Depends on Phase 20 (new helper methods affect API shape).

* [x] Extract `BambuModel` enum and `resolve_model()` out of `src/discovery/parser.rs` into a new top-level `src/models.rs` module. Currently `BambuModel` lives inside the SSDP parser, but it is not an SSDP concern — it is the crate's canonical model identity enum consumed by `quirks/mod.rs` (for `impl BambuModel { fn quirks() }`), `client.rs`, and `ftps/client.rs`. None of those modules have anything to do with discovery. `resolve_model()` is pure serial-prefix-to-enum mapping, also not SSDP-specific. `SsdpDevice` stays in `discovery/parser.rs` and imports `BambuModel` from the new location. Re-export `BambuModel` from `src/lib.rs` as a public type. Update all `use crate::discovery::BambuModel` imports crate-wide (including CLI code and tests) to `use crate::models::BambuModel`. Update `CLAUDE.md` to reflect the new canonical location.
* [x] Replace `in_flight: Vec<u16>` with `BTreeSet<u16>` in `src/mqtt/client.rs`
* [x] Replace discovery deduplication linear scan with `BTreeSet<String>` in `src/discovery/mod.rs`
* [x] Hide dummy types (`DummyRawIo`, `DummyTls`, `DummyFactory`) with `#[doc(hidden)]` — must stay `pub` for default type parameters but should be hidden from docs
* [x] Change `clamp_task_id` return type from `String` to `u32`
* [x] Introduce `PrintJobConfig` struct to replace 8+ parameter `start_print()` and `ProjectFileRequest::new()`
* [x] Remove phase numbering comments in `src/lib.rs`
* [x] Remove `#![allow(async_fn_in_trait)]` lint suppression

### Phase 22: CLI Hardening

Extract shared connection logic, add input validation and timeouts. Depends on Phases 19-21 (library API changes must stabilize first).

* [x] Extract `connect_mqtt` into shared `src/bin/bambino-cli/connection.rs` — function in control.rs, inline-duplicated twice in monitor.rs. Include a `MqttClient` type alias
* [x] Add connection timeout wrapping `TcpStream::connect`
* [x] Add input validation for IP, serial, and access code parameters
* [x] Add file upload size guard in `storage.rs`
* [x] Clean up removed quirks check TODO comment in `control.rs`

### Phase 23: Test Infrastructure & Coverage

Consolidate test mocking, replace magic numbers, and expand edge case coverage. Depends on Phases 19-22 (tests validate all prior changes; uses constants from Phase 19).

* [x] Extract shared CONNECT/SUBSCRIBE parsing from `tests/client_test.rs` handshake helper into `tests/common/mock_mqtt.rs` — consolidated `handle_mqtt_handshake()` and `read_publish_payload()` as shared public functions; `run_mock_mqtt_broker` refactored to call `handle_mqtt_handshake`; `client_test.rs` imports from `common::mock_mqtt` instead of inlining
* [x] Replace magic packet type numbers in test assertions with named constants — added `PACKET_TYPE_*` and `HEADER_*` constants to `tests/common/mock_mqtt.rs`; all magic hex values (`0x10`, `0x82`, `0x32`, `0x40`, `0x20`, `0x90`, `0xD0`) replaced throughout mock broker and test assertions
* [x] Add `expect("context")` to bare `.unwrap()` calls in tests — replaced across `mock_mqtt.rs`, `mock_ftps.rs`, `mock_camera.rs`, `client_test.rs`, `mqtt_test.rs`, `camera_test.rs`
* [x] Add negative/failure tests for `PrinterClient` — `test_in_flight_saturation` (200 commands without PUBACKs, 201st rejected) and `test_connection_drop_during_operation` (server drop → `NetworkError`) in `client_test.rs`
* [x] Add edge case tests for MQTT — packet ID wraparound: 3 unit tests in `src/mqtt/client.rs` verifying `next_packet_id` skips 0 on `u16::MAX` wraparound
* [x] Add edge case tests for FTPS — extracted `parse_pasv_port()` from `negotiate_passive_port` in `src/ftps/client.rs` with 6 unit tests (valid, port-zero, missing parens, non-numeric, incomplete, empty); 3 parser tests in `src/ftps/parser.rs` (empty listing, whitespace-only, malformed lines skipped)

### Phase 24: Quirks Engine Overhaul

A deep audit of the quirks engine (`src/quirks/`) and its usage revealed correctness bugs, missing capabilities, architectural leaks, and dead code. This phase brings the engine into full alignment with the reference documentation (`reference/04_toolhead_thermal_motion.md` §4.2, `reference/06_cameras.md`) and enforces the design principle that ALL model-specific behavior routes through `model.quirks()`. Can be done independently of Phases 19-23 but is listed last since it's the largest scope.

**Correctness fixes:**
* [ ] Split H2 model family into separate quirks structs (`H2SQuirks`, `H2DQuirks`, `H2CQuirks`, `H2DProQuirks`) — currently all report `physical_nozzle_count=1`, but H2D/H2DPro=2 and H2C=7 (1 dedicated left nozzle + 6 interchangeable right-side hotends)
* [ ] Fix X1E active chamber heater — `X1Quirks` returns `false` for `has_active_chamber_heater()` but reference §4.2 explicitly lists X1E as supporting it. Either split X1C/X1E quirks or add differentiation
* [ ] Fix `relative_z_move_gcode` — all 6 implementations ignore `distance`/`feedrate` parameters, hardcoding `Z10.00 F3000`. Interpolate actual values and add per-model Z travel bounds validation

**Architectural violations — model-specific behavior outside quirks:**
* [ ] Move X2D auxiliary fan check out of `client.rs` — line 343 directly matches `BambuModel::X2D` instead of calling a quirks method. Add `supports_auxiliary_right_fan() -> bool` to `ModelQuirks`
* [ ] Refactor door sensor decoding — `PrintTelemetry::is_door_open(is_x1_series: bool)` embeds model knowledge in the telemetry layer. Move field selection logic (X1 uses `home_flag`, others use `stat`) into the quirks implementations themselves

**Missing quirks methods (documented in reference but not in trait):**
* [ ] Add `requires_wallclock_rtsp_timestamps() -> bool` — P2S camera timestamp freezing workaround. Currently a standalone function in `p2.rs`, not a trait method
* [ ] Add `auxiliary_fan_uses_percentage() -> bool` — X2D's right auxiliary fan (id: 160) uses 0-100% directly instead of 0-15 step conversion

**Trait cleanup — reduce duplication, remove dead code:**
* [ ] Add default implementation for `is_unsupported_command()` returning `false` — every model currently implements it identically, and no real filtering exists. Consider removing entirely if it serves no purpose
* [ ] Extract shared `is_unsafe_homing_command` default for bed-on-Z models — 5 of 6 models have identical G28 axis-check logic
* [ ] Extract shared `relative_z_move_gcode` default for CoreXY models (after fixing parameterization above)
* [ ] Remove orphaned standalone functions in `p2.rs` (`force_tls_v12_for_ftps()`, `requires_wallclock_rtsp_timestamps()`) once integrated into the trait

**Tests:**
* [ ] Add per-model quirks assertion tests validating nozzle counts, chamber capabilities, and camera ports against reference documentation
* [ ] Add unit tests for parameterized Z-move G-code output and limit clamping