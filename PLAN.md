# bambino — Lazy connections and API consistency

**Important:** Before starting any phase, read this document in its entirety. Read the `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

**When completing a phase:** Update this PLAN.md marking the phase complete. Update the completed phases summary, strictly including **only** what is necessary to inform clean sessions implementing the next phases which cannot be learned from the code itself. Once summarized, remove the phase from PLAN.md.

---

## Phases 1–13: Complete

Non-obvious decisions a future session cannot derive from the code alone:

- **`from_mqtt()` does not reseed `sequence_counter`** — `ensure_mqtt()` reseeds from `TimerProvider::now_millis()` on lazy connect to de-correlate independent sessions, but `from_mqtt()` (tests and Embassy) intentionally starts at `INITIAL_SEQUENCE_ID` so injected fixture responses remain predictable.
- **`move_relative()`/`extrude()` warn on unhomed axis, never error** — the policy is warn-and-proceed; the `log::warn!` calls in motion.rs are deliberate, not a placeholder for a future error return.
- **`wait_for_homing()` overrides `command_timeout_secs` to 90s** — homing takes far longer than normal commands; the override is intentional.
- **Clap positional `bool` requires explicit action** — `AmsAction::Dry.rotate` uses `#[arg(action = ArgAction::Set, value_parser = BoolishValueParser::new())]`. Clap defaults `bool` fields to `ArgAction::SetTrue` (flag semantics), which panics for positional args at startup. Any future positional bool in the CLI needs the same treatment.
- **`SecureConnect` is gone; MQTT and FTPS now share one connection shape.** `PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory>` has two symmetric `TlsConnector`+`RawStreamFactory` trios — MQTT's mandatory (no defaults), FTPS's defaulted to dummy types. `RawStreamFactory<RawIO>` (`src/io/mod.rs`, next to `TlsConnector`) replaces the old FTPS-only `FtpDataStreamFactory`; its method is `dial(&self, host, port)`, not `create_data_stream`. Platform impls: `Tokio/EspIdf/EmbassyRawStreamFactory` (dropped the `Ftp` prefix — they're protocol-neutral). `PrinterClient::new(tls, factory, ip, serial, access_code, model)` takes MQTT's pair directly, mirroring `.with_ftps(tls, factory)`'s shape.
- **`MqttRawIO` needs a `PhantomData` field on `PrinterClient`.** It only appears in where-clause bounds (`MqttTls: TlsConnector<MqttRawIO>`, `MqttFactory: RawStreamFactory<MqttRawIO>`), never directly in a field type (unlike `FtpsRawIO`, which appears directly in the `ftps: Option<BambuFtpsClient<FtpsRawIO, ...>>` field) — Rust's E0392 unused-type-parameter check doesn't count where-clause appearances, so `pub(crate) _mqtt_raw_io: PhantomData<MqttRawIO>` is required and must be threaded through every constructor/builder (`new`, `from_mqtt`, `with_timer`, `with_ftps`).
- **`from_mqtt()` uses `PreConnected` for both the MQTT `Tls` and `Factory` slots.** `PreConnected<IO>` implements `TlsConnector<IO>` as an identity passthrough (`connect` returns `Ok(raw_stream)` unchanged — never actually exercised, since `from_mqtt()` never calls `ensure_mqtt()`'s connect path) and `RawStreamFactory<IO>` as `Err(SocketError::NotConnected)` (genuinely unreachable — `ensure_mqtt()` short-circuits on `self.mqtt.is_some()` before either is called). `DummySecureConnect` is deleted outright (no replacement needed — `PreConnected` covers the same role for both new slots).
- **Connect-timeout is now centralized on `PrinterClient`, not per-connector.** `connect_timeout_secs` (default 10s, `.with_connect_timeout(secs)`) bounds `ensure_mqtt()`/`ensure_ftps()`'s *entire* dial+connect sequence via `race_against_connect_timeout()` (`src/client/mod.rs`), which reuses `race()`/`Raced` (now `pub(crate)` in `src/io/mod.rs`, not `src/mqtt/client.rs`) and its `has_real_clock()` guard (skips the race under `DummyTimer`). For `ensure_ftps()` specifically, "entire sequence" means the raw dial *and* `BambuFtpsClient::connect()`'s full login handshake raced together as one future, not just the TCP dial. This replaced the old per-platform `TokioSecureConnector`/`EspIdfSecureConnector::connect_timeout` (both deleted) and closed a pre-existing gap: FTPS's raw dial had no timeout at all before this. `EspIdfTlsConnector::connect_timeout` (handshake-only, for direct non-`PrinterClient` callers) is untouched and still exists as a separate, lower layer.
- **Camera integration follows the same lazy-connect shape as MQTT/FTPS** (`PLAN.md` former Phase 13) — a third `CameraRawIO`/`CameraTls`/`CameraFactory` trio (type params 8–10, defaulted to `DummyRawIo`/`DummyTls`/`DummyFactory`) plus `ensure_camera()`/`connect_camera()`/`camera_connected()`/`with_camera()`/`with_camera_port()`/`with_camera_max_frame_size()` in `src/client/mod.rs`, and `camera()`/`read_camera_frame()`/`disconnect_camera()`/`attach_camera()` in the new `src/client/camera.rs` (mirrors `storage.rs`'s shape). `ensure_camera()` checks `model.quirks().camera_protocol() == CameraProtocol::BinaryJpeg` **before** checking whether `.with_camera()` was ever called, so an RTSPS model (X1/X2/H2/P2S) gets a clear "wrong protocol" error immediately, never a dial attempt or a "not configured" error.
- **`race`, `Raced`, and `read_chunk` now live in `src/io/mod.rs`** (`pub(crate)`), not `src/mqtt/client.rs` — relocated so `camera/binary.rs` didn't have to reach into a sibling protocol module for transport-agnostic utilities built on `AsyncIo`/`TimerProvider`. `mqtt/client.rs` imports them from `crate::io`; `read_exact_packet`/`FrameReadState` stayed put (genuinely MQTT-specific). `BambuBinaryCameraStream::read_next_frame_with_timer()` (`src/camera/binary.rs`) reuses them via its own resumable `CameraFrameReadState`; the public `read_next_frame()` delegates to it under `DummyTimer` — never give it an independent implementation, since two implementations sharing one `read_state` field is a desync hazard.
- **`read_chunk`'s no-deadline branch (`DummyTimer`) now maps a `0`-byte read to `SocketError::ConnectionReset`**, matching the with-deadline branch — previously it forwarded `stream.read()`'s result unchanged, so a legitimate EOF looked identical to "no bytes yet," and callers' fill-loops (`while *filled < buf.len() { ... }`) would spin forever instead of erroring. Caught by a camera regression test expecting a clean disconnect error; benefits any `DummyTimer`-based caller, including `BambuMqttClient::connect()`'s CONNACK/SUBACK read (not independently re-verified against real hardware for this specific edge case).
---

## Phase 14: Door-open and active-fault telemetry accessors

### Problem

Same gap `print_status()` filled for `gcode_state`, found while auditing for other missing helpers after Phase 8: `ModelQuirks::is_door_open(&self, telemetry: &PrinterTelemetry)` (`src/quirks/mod.rs`) already does full per-model dispatch (X1 reads `home_flag` bit 23, H2/P2/X2 read the `stat` hex string, A1/A2 hardcode `false` — no sensor) and `diagnostics::hms::{decode_print_error, decode_hms_alert}` (`src/diagnostics/hms.rs`) already decode fault state — both fully tested — but `PrinterClient` caches neither, so consumers must manually retain a `TelemetryReport` and call these themselves on every check.

### Design (resolved — implement as stated)

- **`door_open()`**: cache `last_door_open: Option<bool>` on `PrinterClient`, set inside `poll_telemetry()` by calling `self.model.quirks().is_door_open(print)` whenever `report.print` is present (mirrors `last_home_flag`/`last_gcode_state`). Expose `pub fn door_open(&self) -> Option<bool>`. **On models without a door sensor (`has_door_sensor() == false`), `door_open()` must return `None`, not `Some(false)`**, regardless of telemetry observed — distinguishes "no sensor, inapplicable" from "sensor confirms closed." This is a deliberate deviation from the raw quirks method's contract (which returns `false` for sensorless models) — the cached accessor adds the `None` case on top.
- **`active_fault()`**: cache `last_print_error: Option<u32>` (the raw register, same "cache raw, decode on access" shape as `last_home_flag`), set inside `poll_telemetry()` from `print.print_error`. Expose `pub fn active_fault(&self) -> Option<DecodedPrintError>`, computed via `decode_print_error(self.last_print_error?)`. Unlike `is_all_axes_homed()`, collapsing "no telemetry observed yet" and "observed, register reads 0 (no fault)" into the same `None` is acceptable here — both cases warrant the same caller action (nothing to address).
- **`hms` array caching is out of scope for this phase** — it's a `Vec<HmsEntry>` (clone-per-update cost, unlike the scalar fields above) and not needed to close the gap that motivated this phase. Revisit only if a concrete need for multi-alert state (not just "is there a genuine fault") comes up.
- Both caches must be populated only inside `poll_telemetry()`, not `poll_raw()`/`poll_wire()`/`poll_until()` — those bypass deserialization and never touch report fields, so they cannot update either cache.

### Verification

Unit tests mirroring `test_print_status_cache_from_telemetry`/`test_home_flag_cache_and_advisory_warnings` (`tests/client_test.rs`): inject synthetic `stat`/`home_flag`/`print_error` telemetry against models on both sides of `has_door_sensor()` (e.g. X1C/H2D vs. P1S/A1) and confirm `door_open()`/`active_fault()` update correctly, including the `None`-for-no-sensor and pre-first-telemetry cases.

---

## Phase 15: AMS/tray and progress/temperature telemetry accessors (investigation)

### Problem

Beyond door-open and active-fault (Phase 14), `PrinterTelemetry` has other fields a consumer might reasonably want cached accessors for — AMS/tray state (`ams`, `ams_status`, `vt_tray`, `tray_exist_bits`) and print progress/temperature (`mc_percent`, `mc_remaining_time`, `layer_num`/`total_layers`, `bed_temper`/`nozzle_temper`/`chamber_temper`). Unlike Phase 14's two fields, there isn't a single obvious "first accessor" here — AMS state spans multiple interrelated sub-fields with their own existing types (`AmsStatusReport`, `AmsTray`, `AmsUnit` in `src/types/telemetry/ams.rs`), and progress/temperature fields change continuously rather than representing a discrete state flag consulted *between* polls at a decision point (which is what makes homed/busy/door/fault worth caching).

### Design questions to answer first

- Do these warrant `PrinterClient`-level caching at all, or is "call `poll_telemetry()` yourself and read the report" sufficient? Door-open/active-fault/homed/busy are valuable to cache specifically because they're consulted *between* polls (e.g. before issuing a command); progress/temperature are typically consumed directly off each report inside a monitoring loop already — caching may not address a real gap.
- If AMS state is worth exposing, what's the right shape — one bundled accessor, or several scoped to specific questions (e.g. "is a tray loaded", "is this filament dry")?
- `TelemetryReport::bed_temperatures()` (`src/types/telemetry/mod.rs`) already exists as a purpose-built multi-wire-format decoder — confirm whether a caching accessor adds anything beyond what it already provides before building one.

### Scope

Answer the design questions based on the current codebase, then write a concrete implementation plan. Do not start implementation without a plan.

---

## Progress Tracker

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Foundation types and library adapters | Complete |
| 2 | `PrinterClient` struct migration (backward compatible) | Complete |
| 3 | Lazy MQTT connection and constructor redesign | Complete |
| 4 | Lazy FTPS connection and API alignment | Complete |
| 5 | Documentation | Complete |
| 6 | Move message buffer to `BambuMqttClient` | Complete |
| 7 | Advisory homed-state tracking from `home_flag` | Complete |
| 8 | Homing completion detection | Complete |
| 9 | Sequence ID correlation hygiene for query commands | Complete |
| 10 | CLI dependency leakage | Complete |
| 11 | Migrate CLI argument parsing to `clap` | Complete |
| 12 | Replace `SecureConnect` with `TlsConnector`+`RawStreamFactory` for MQTT | Complete |
| 13 | Camera integration in `PrinterClient` | Complete |
| 14 | Door-open and active-fault telemetry accessors | Not Started |
| 15 | AMS/tray and progress/temperature telemetry accessors (investigation) | Not Started |
