# Deep Code Review Plan

Module-by-module review of the `bambino` crate. Phases 1–15 are complete (details in git history).
When completing a phase, replace its section with a 2–3 line summary — detailed write-ups belong in the commit message, not here.

---

## Completed Phases (1–15)

**Phases 1–7** (Core, I/O, MQTT, FTPS, Telemetry, Discovery, AMS): 32 fixes including panic-safe model resolution, QoS handling, PASV overflow, temp type corrections (`u32`→`f64`), SSDP fallback parsing, external spool ID collision fix. 52 new tests.

**Phases 8–11** (Camera, Diagnostics, Quirks, Client): 29 fixes including RTP timestamp wrap, K-profile priming, A1 Mini/A2L Z-limit splits, P2S/X2D chamber heater corrections, `send_gcode()` safety validation, temp clamping, capability gates (airduct/prompt sound/buzzer). 20 new tests.

**Phases 12–15** (CLI, Tests, Lint, Protocol Audit): 14 fixes including fan speed display (667% bug), ipcam nesting, `AirductMode` enum replacing inverted bool API, `fun` telemetry field (`Option<String>` hex), `ModelMismatch` context via `Cow<'static, str>`, `DiskWriteFailure` reword. 43 new tests. Full protocol alignment verified across all 7 reference docs against pybambu and Bambuddy.

**Phase 16** (Platform Abstraction Gaps): Added `SecureConnect` trait for ESP-IDF's "TLS manages its own transport" model with impls for Tokio (`TokioSecureConnector`) and ESP-IDF (`EspIdfSecureConnector` via `EspTls` syscalls). Added `TimerProvider::now_millis()` monotonic clock method (tokio: `std::time::Instant`, ESP-IDF: `esp_timer_get_time`, embassy: `embassy_time::Instant`). Refactored `discover_devices` from poll-count timing to wall-clock measurement. `TlsConnector` retained for FTPS data channel wrapping. 3 new tests.

**Current state:** 227 tests (192 unit + 35 integration), all passing. `no_std`+`alloc` clean. Clippy clean.

---

## Phase 17: Monitor Typed Telemetry Refactor & IDEX Schema

Structural refactor of the CLI monitor from raw JSON to typed telemetry structs. The library returns `MqttMessage` with raw bytes — state accumulation and deserialization is the consumer's responsibility. This phase makes the CLI a better consumer.

**Design constraints:**
- The library should NOT accumulate state — that's the application's job
- The library SHOULD return predictable, typed structures for developer UX (evaluate whether `poll_telemetry()` should return something more useful than `MqttMessage`)
- The monitor currently doesn't use `PrinterClient` — evaluate whether it should, and whether `PrinterClient` should offer typed telemetry deserialization (without accumulation)

- [ ] **Evaluate library-level telemetry return type:** `poll_telemetry() → MqttMessage` forces every consumer to deserialize from raw bytes. Consider `poll_telemetry() → TelemetryReport` or a typed enum distinguishing print/device/info responses. The library handles deserialization once; consumers handle accumulation
- [ ] **Add `device.extruder.info` to telemetry schema:** IDEX per-nozzle current temperatures live in `device.extruder.info[]` per [REF-THER-DECODE §Dual-Extruder]. Currently only `device.nozzle.info` exists in `DeviceTelemetry`. Cross-reference against pybambu test mocks and Bambuddy backend services for field names
- [ ] **Refactor monitor state accumulation:** Keep JSON merge for partial updates, deserialize accumulated map into `PrinterTelemetry` via `serde_json::from_value()` before each render. Rendering function takes typed struct instead of string-key map
- [ ] **Split monitor.rs into module:** `monitor/mod.rs` (connection lifecycle, event loop, state accumulation) + `monitor/dashboard.rs` (rendering logic and helpers). Current file is 621 lines and does too many things
- [ ] **Fix IDEX per-nozzle temperature display:** Once `device.extruder.info` schema exists, extract right nozzle actual temp and left nozzle target temp from the per-nozzle array

---

## Phase 18: Expanded CLI Control Commands

Add commonly useful control commands to the CLI for hardware testing.

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
| 17 | Monitor Typed Telemetry | Not started |
| 18 | Expanded CLI Commands | Not started |
