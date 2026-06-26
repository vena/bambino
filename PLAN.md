# Deep Code Review Plan

Module-by-module review of the `bambino` crate. Phases 1–15 are complete (details in git history).
When completing a phase, replace its section with a 2–3 line summary — detailed write-ups belong in the commit message, not here.

---

## Completed Phases (1–15)

**Phases 1–7** (Core, I/O, MQTT, FTPS, Telemetry, Discovery, AMS): 32 fixes including panic-safe model resolution, QoS handling, PASV overflow, temp type corrections (`u32`→`f64`), SSDP fallback parsing, external spool ID collision fix. 52 new tests.

**Phases 8–11** (Camera, Diagnostics, Quirks, Client): 29 fixes including RTP timestamp wrap, K-profile priming, A1 Mini/A2L Z-limit splits, P2S/X2D chamber heater corrections, `send_gcode()` safety validation, temp clamping, capability gates (airduct/prompt sound/buzzer). 20 new tests.

**Phases 12–15** (CLI, Tests, Lint, Protocol Audit): 14 fixes including fan speed display (667% bug), ipcam nesting, `AirductMode` enum replacing inverted bool API, `fun` telemetry field (`Option<String>` hex), `ModelMismatch` context via `Cow<'static, str>`, `DiskWriteFailure` reword. 43 new tests. Full protocol alignment verified across all 7 reference docs against pybambu and Bambuddy.

**Phase 16** (Platform Abstraction Gaps): Added `SecureConnect` trait for ESP-IDF's "TLS manages its own transport" model with impls for Tokio (`TokioSecureConnector`) and ESP-IDF (`EspIdfSecureConnector` via `EspTls` syscalls). Added `TimerProvider::now_millis()` monotonic clock method (tokio: `std::time::Instant`, ESP-IDF: `esp_timer_get_time`, embassy: `embassy_time::Instant`). Refactored `discover_devices` from poll-count timing to wall-clock measurement. `TlsConnector` retained for FTPS data channel wrapping. 3 new tests.

**Phase 17** (Typed Library API & IDEX Schema): Added `TelemetryEvent` discriminated enum — `PrinterClient::poll_telemetry()` now returns typed `Report(Box<TelemetryReport>)` or `Unknown(MqttMessage)` with `.into_raw()` / `.raw()` / `.report()` accessors. `BambuMqttClient::poll_telemetry()` retained as raw escape hatch; `PrinterClient::poll_raw()` added. Added `ExtruderCollection` + `ExtruderInfo` to `DeviceTelemetry` for `device.extruder.info[]` IDEX schema (composite-packed temps via `unpack_temperature()`, active extruder index from state bitmask, AMS slot routing, z_bias). Split `monitor.rs` into `monitor/mod.rs` (lifecycle) + `monitor/dashboard.rs` (rendering). IDEX per-nozzle temp display fixed: dashboard now reads `device.extruder.info` first, falling back to top-level fields. 3 new tests.

**Current state:** 234 tests (199 unit + 35 integration), all passing. `no_std`+`alloc` clean. Clippy clean.

---

## Phase 18: Typed Command-Response Methods

### Problem

The library has request structs (`GetVersionRequest`, `ExtrusionCaliGetRequest`) and response structs (`ExtrusionCaliGetResponse`) but no typed round-trip API. Consumers must: call a fire-and-forget publish method, poll `poll_telemetry()` in a loop, pattern-match raw JSON to identify the response, and deserialize manually.

Additionally, `PrinterClient` is missing a `request_pushall()` convenience method — both CLI monitor functions (`dump` and `run` in `src/bin/bambino-cli/monitor/mod.rs`) construct `PushAllRequest` manually, serialize with `serde_json::to_vec`, and call `mqtt.publish_command()` directly. The monitor also bypasses `PrinterClient` entirely, using raw `BambuMqttClient` for its event loop.

### Core design constraint: the single-stream problem

`BambuMqttClient::poll_telemetry()` is a single MQTT read stream. If `get_version()` internally polls that stream waiting for its response, it will **consume and discard** any telemetry updates that arrive before the response. This is the central design problem of this phase.

**Recommended approach:** Internal polling methods should buffer consumed-but-unmatched messages into a `VecDeque<MqttMessage>` on `PrinterClient`. The public `poll_telemetry()` / `poll_raw()` methods drain this buffer before reading from the wire. This way, no messages are lost. The buffer lives on `PrinterClient` (not `BambuMqttClient`) so the low-level client stays stateless.

**Alternative considered:** Splitting into separate channels (e.g., command-response channel vs telemetry channel). Rejected because the MQTT broker delivers everything on one topic — there's no wire-level separation to leverage.

### Items (order matters — sequential dependencies noted)

**Independent (do anytime):**

- [ ] **`PrinterClient::request_pushall()`:** Fire-and-forget publish wrapper. Constructs `PushAllRequest` with `next_sequence_id()`, serializes, publishes. No response handling — this is purely a convenience method, not a typed round-trip. Defined in `src/client.rs`
- [ ] **Update README telemetry example:** `README.md` lines 121–133 still show `serde_json::from_slice(&msg.payload)` — update to use `TelemetryEvent` API (`printer.poll_telemetry().await?.report()`)

**Sequential (build in order):**

- [ ] **1. Add message buffer to `PrinterClient`:** Add `pending_messages: VecDeque<MqttMessage>` field. Modify `poll_telemetry()` and `poll_raw()` to drain the buffer before calling `self.mqtt.poll_telemetry()`. This is the foundation for all command-response methods. Uses `alloc::collections::VecDeque` for no_std compatibility
- [ ] **2. Add internal `poll_until()` helper on `PrinterClient`:** `async fn poll_until<F, T>(&mut self, matcher: F, timeout_secs: u64) → Result<T, BambuError>` — polls the MQTT stream, pushes non-matching messages into `pending_messages`, returns when `matcher` returns `Some(T)` or timeout expires (default ~10s). This is `pub(crate)` — not part of the public API
- [ ] **3. Add `VersionInfo` response struct:** Typed representation of `get_version` module array. Fields per module: `product_name: String`, `name: String`, `hw_ver: String`, `sw_ver: String`, `sn: String`, `visible: bool`. Currently only exists as raw `serde_json::Value` in CLI `control.rs` (lines 87–106). Defined in a new `src/types/version.rs` or alongside existing types
- [ ] **4. `PrinterClient::get_version() → Result<VersionInfo, BambuError>`:** Uses `poll_until()` to match on `info.command == "get_version"`, deserializes into `VersionInfo`. Replaces manual JSON matching in CLI `control.rs` `run_info()` (lines 22–130)
- [ ] **5. Refactor `get_k_profiles()` to return typed response:** Current signature returns `Result<u16, BambuError>` (packet ID). Change to `Result<ExtrusionCaliGetResponse, BambuError>` using `poll_until()`. K-profile priming quirk stays internal. Note: this is a breaking API change for any consumer currently using the packet ID return

**After the above:**

- [ ] **Migrate monitor `run()` to use `PrinterClient`:** Currently `src/bin/bambino-cli/monitor/mod.rs` creates a raw `BambuMqttClient` and runs its own `select!` loop with `mqtt.poll_telemetry()`, `mqtt.send_ping()`, and keyboard input. Migration requires: (a) `request_pushall()` exists, (b) the event loop calls `printer.poll_telemetry()` → `TelemetryEvent` instead of raw `mqtt.poll_telemetry()`. **Open question:** `PrinterClient` doesn't currently expose `send_ping()`. Options: (1) add `PrinterClient::send_ping()` as a passthrough, (2) have `PrinterClient` manage keep-alive internally, (3) keep the monitor holding a reference to the underlying `BambuMqttClient` for pings only. Option 1 is simplest. The `dump()` function should stay on raw `BambuMqttClient` — its purpose is debugging raw wire output

---

## Phase 19: Expanded CLI Control Commands

Add commonly useful control commands to the CLI for hardware testing. With Phase 18's typed responses, new commands that query state (e.g. version, calibration status) can use typed return values directly.

- [ ] **`speed <level>`** — Set print speed (silent, standard, sport, ludicrous) via `set_print_speed()`
- [ ] **`clear-error`** — Clear active print error codes via `clear_print_error()`
- [ ] **`airduct <cooling|heating|laser>`** — Switch airduct damper mode via `set_airduct_mode(AirductMode)` (H2/P2S/X2D)
- [ ] **`calibrate <options>`** — Trigger calibration routines via `start_calibration()` (bed-leveling, vibration, motor-noise, nozzle-height, heatbed-thermal)
- [ ] **`ams dry <ams_id> <temp> <time> <filament>`** — Start AMS drying cycle via `start_drying()`
- [ ] **`ams dry-stop <ams_id>`** — Stop AMS drying cycle via `stop_drying()`

---

## Progress Tracker

| Phase | Module | Status |
|-------|--------|--------|
| 1–15 | Core through Protocol Audit | **Complete** |
| 16 | Platform Abstraction Gaps | **Complete** |
| 17 | Typed Library API & IDEX Schema | **Complete** |
| 18 | Typed Command-Response Methods | Not started |
| 19 | Expanded CLI Commands | Not started |
