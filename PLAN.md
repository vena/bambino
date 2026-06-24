# Deep Code Review Plan

Comprehensive module-by-module review of the `bambino` crate: library, CLI tool, and test infrastructure.
Structured for incremental completion across multiple sessions — check off items as they are reviewed and resolved.

**Review scope:** correctness bugs, logic errors, protocol alignment with reference docs, code smells, unsafe soundness, error handling gaps, missing edge cases, test coverage, and no_std compatibility.

---

## How to Use This Plan

Each phase is a self-contained review unit targeting one module or subsystem. Within each phase, individual files are listed with specific review concerns. Mark items `[x]` as they are completed. Phases can be done in any order, though the suggested order moves from foundational modules outward.

When reviewing, always cross-reference the corresponding reference doc (listed in each phase header) to verify field names, byte offsets, protocol semantics, and behavioral expectations against the specification.

---

## Phase 1: Core Foundation (`src/error.rs`, `src/models.rs`, `src/lib.rs`)

**Reference:** N/A (internal plumbing)
**Lines:** ~329

- [x] **`src/error.rs`** (~179 lines)
  - [x] Verify `BambuError` variant completeness — `TlsHandshakeFailed` is defined but never constructed (TLS failures route through `SocketError` → `NetworkError`). Dead variant, kept for now.
  - [x] Audit `Display` impl under `no_std` — in sync with `thiserror`. `test_display_consistency` covers all 8 variants. ✓
  - [x] Check `Cow<'static, str>` usage in `ProtocolViolation` — all call sites pass `&'static str` literals via `.into()`. No dynamic strings currently. Design is sound for future use.
  - [x] Verify `From<SocketError>` conversion — preserves variant name via `{:?}` in Display. Adequate.
  - [x] Check `Clone` derive — all variants are cheap to clone (`SocketError` is `Copy`, `Cow::Borrowed` is cheap). ✓

- [x] **`src/models.rs`** (~117 lines)
  - [x] Verify `resolve_model()` covers all known Bambu model identifiers — all 13 models covered via prefix and dev_model fallback. ✓
  - [x] Check for model string variants that might be missed — **Fixed:** `&serial[0..3]` replaced with `serial.get(0..3).unwrap_or("")` to prevent panics on non-ASCII input. **Fixed:** removed redundant `m.contains("O1C2")` check (already caught by `m.contains("O1C")`).
  - [x] Verify `BambuModel` enum is exhaustive for all models referenced in `src/quirks/models/` — all 13 non-Unknown variants have quirks structs. ✓
  - [x] Audit the `quirks()` dispatch — every variant maps correctly; `Unknown` falls back to `X1CQuirks`. ✓. **Added** comprehensive tests for all prefix paths, all dev_model fallback paths, H2DPro/H2C resolution, and short/empty serial edge cases.

- [x] **`src/lib.rs`** (~33 lines)
  - [x] Verify `#![cfg_attr(not(feature = "std"), no_std)]` is correctly gated ✓
  - [x] Confirm all public modules are listed and re-exports are intentional ✓
  - [x] Check that `extern crate alloc` gating is correct for `no_std + alloc` ✓

---

## Phase 2: Platform Abstraction Layer (`src/io/`)

**Reference:** Chapters 1-3 (transport parameters)
**Lines:** ~475

- [ ] **`src/io/mod.rs`** (~170 lines)
  - [ ] Audit trait definitions: `AsyncIo`, `TlsConnector`, `AsyncUdpSocket`, `TimerProvider`
  - [ ] Verify trait methods have correct lifetime bounds and `Send`/`Sync` constraints for all three targets
  - [ ] Check associated type bounds — are they unnecessarily restrictive or insufficiently constrained?
  - [ ] Verify `SocketError` is flexible enough for all platform error types

- [ ] **`src/io/tokio.rs`** (~220 lines)
  - [ ] Audit `build_unsafe_client_config()` — is the TLS certificate bypass correctly scoped and documented?
  - [ ] Check `NoCertificateVerification` — does it properly implement the rustls verifier trait?
  - [ ] Verify `TokioTlsConnector` correctly maps to the `TlsConnector` trait
  - [ ] Audit socket error conversion (`to_socket_error`) — are all `std::io::Error` kinds mapped meaningfully?
  - [ ] Verify `TokioTimer` precision and cancellation behavior
  - [ ] Check for resource leaks in UDP socket lifecycle

- [ ] **`src/io/embassy.rs`** (~207 lines)
  - [ ] **CRITICAL:** Audit `unsafe impl Sync for SyncUnsafeCell<T>` — verify soundness; is `Sync` actually required? Is there concurrent access?
  - [ ] Audit `unsafe { &mut *TLS_READ_BUFFER.0.get() }` and write buffer — check for aliasing violations
  - [ ] Verify static buffer sizing is adequate for all expected TLS frames
  - [ ] Check `rand_core` / `rand_core_legacy` bridging — does the RNG seeding produce cryptographically adequate randomness on target hardware?
  - [ ] Verify trait implementations match the `mod.rs` trait contracts

- [ ] **`src/io/esp_idf.rs`** (~78 lines)
  - [ ] Verify trait implementations are complete and correct
  - [ ] Check error type conversions from ESP-IDF native errors

---

## Phase 3: MQTT Client & Commands (`src/mqtt/`)

**Reference:** `03_mqtt_telemetry.md` [REF-MQTT-CONN], `04_toolhead_thermal_motion.md` [REF-MOTO-GCODE]
**Lines:** ~1470

- [ ] **`src/mqtt/client.rs`** (~603 lines)
  - [ ] Audit MQTT packet construction — verify CONNECT, PUBLISH, SUBSCRIBE packet byte layouts against MQTT 3.1.1 spec
  - [ ] Check `PACKET_TYPE_*` and `MQTT_*` constants against the reference doc [REF-MQTT-CONN]
  - [ ] Verify remaining-length encoding/decoding handles multi-byte lengths correctly (values > 127)
  - [ ] Audit topic string construction — are topic paths correct for all printer models?
  - [ ] Check QoS handling — is QoS 0/1 used correctly for commands vs telemetry?
  - [ ] Verify connection keepalive logic and timeout handling
  - [ ] Audit the read loop — can partial MQTT packets cause panics or infinite loops?
  - [ ] Check buffer management — are there overflow risks with large telemetry payloads?
  - [ ] Verify sequence ID generation and wrapping behavior at `TASK_ID_MAX`

- [ ] **`src/mqtt/commands.rs`** (~847 lines)
  - [ ] Audit every command payload struct — verify serde field names match the reference docs exactly:
    - [ ] `GcodeLineRequest` against [REF-MOTO-GCODE]
    - [ ] `PrintSpeedRequest` against reference
    - [ ] Temperature commands against [REF-MOTO-GCODE]
    - [ ] Light control commands
    - [ ] Print job commands (`PrintJobConfig` builder)
    - [ ] Calibration commands
    - [ ] AMS-related commands against [REF-AMS-DECODE]
  - [ ] Verify all `#[serde(rename = "...")]` annotations match protocol field names
  - [ ] Check `TASK_ID_MAX` and `clamp_task_id()` — verify the clamp value is correct (i32::MAX)
  - [ ] Audit `PrintJobConfig` builder — can invalid configurations be constructed? Are required fields enforced?
  - [ ] Check `CalibrationOption` bitmask — verify flag values match protocol spec
  - [ ] Verify `PrintSpeed` enum values map to the correct protocol integers
  - [ ] Check that envelope wrappers (`print`, `pushing`, `system`, `info`) are used correctly per command type

- [ ] **`src/mqtt/mod.rs`** (~20 lines)
  - [ ] Verify re-exports are complete and intentional

---

## Phase 4: FTPS Client & Parser (`src/ftps/`)

**Reference:** `02_ftps.md` [REF-FTPS-CONN]
**Lines:** ~966

- [ ] **`src/ftps/client.rs`** (~685 lines)
  - [ ] Audit FTP command/response parsing — verify against [REF-FTPS-CONN]
  - [ ] Check `FTP_*` constants against reference doc connection parameters
  - [ ] Verify PASV mode port parsing (`parse_pasv_port`) — edge cases with malformed responses
  - [ ] Audit TLS session reuse for data connections — does it match Bambu's FTPS behavior?
  - [ ] Check file listing parsing — can malformed directory listings cause panics?
  - [ ] Verify upload/download correctness — are partial transfers handled?
  - [ ] Audit timeout handling during file transfers
  - [ ] Check for off-by-one errors in buffer reads
  - [ ] Verify the connection state machine — can commands be issued in wrong states?

- [ ] **`src/ftps/parser.rs`** (~267 lines)
  - [ ] Audit FTP response parsing — are multi-line responses handled correctly?
  - [ ] Check directory listing parsing against actual Bambu FTP output format
  - [ ] Verify date/time parsing in directory listings
  - [ ] Check for panics on malformed input — are all `.unwrap()` calls safe?

- [ ] **`src/ftps/mod.rs`** (~14 lines)
  - [ ] Verify re-exports

---

## Phase 5: Telemetry & Types (`src/types/`)

**Reference:** `03_mqtt_telemetry.md` (telemetry payloads)
**Lines:** ~754

- [ ] **`src/types/telemetry.rs`** (~743 lines)
  - [ ] Audit `TelemetryReport` struct — verify all fields against [REF-MQTT-CONN] telemetry schema
  - [ ] Check `PrintTelemetry` fields — verify field names and types match the protocol
  - [ ] Verify `DeviceTelemetry` fields
  - [ ] Check serde `default` annotations — are missing fields handled correctly with sensible defaults?
  - [ ] Audit `Option<>` wrapping — which fields are truly optional vs always-present?
  - [ ] Verify numeric field types (u8 vs u16 vs u32 vs i32 vs f32) match protocol value ranges
  - [ ] Check `is_door_open_from_home_flag()` and `is_door_open_from_stat()` — verify bitmask/flag logic against reference
  - [ ] Audit HMS telemetry embedding — are HMS codes correctly nested?
  - [ ] Check AMS telemetry fields — verify against [REF-AMS-DECODE]
  - [ ] Verify xcam and other optional subsystem fields

- [ ] **`src/types/mod.rs`** (~11 lines)
  - [ ] Verify re-exports

---

## Phase 6: Discovery (`src/discovery/`)

**Reference:** `01_network_discovery.md` [REF-NET-DISC]
**Lines:** ~555

- [ ] **`src/discovery/mod.rs`** (~322 lines)
  - [ ] Audit SSDP multicast implementation — verify multicast address, port, and M-SEARCH format against [REF-NET-DISC]
  - [ ] Check discovery timeout and retry logic
  - [ ] Verify device deduplication — can the same printer appear multiple times?
  - [ ] Audit UDP socket binding — does it work on multi-homed hosts?
  - [ ] Check for race conditions in concurrent discovery responses
  - [ ] Verify re-export of `BambuModel` for backward compatibility

- [ ] **`src/discovery/parser.rs`** (~233 lines)
  - [ ] Audit SSDP response parsing — are all required headers extracted?
  - [ ] Verify model string extraction and mapping to `BambuModel`
  - [ ] Check IP address parsing — are IPv6 addresses handled or explicitly excluded?
  - [ ] Verify serial number, device name extraction
  - [ ] Test with malformed/partial SSDP responses — does parsing fail gracefully?

---

## Phase 7: AMS Module (`src/ams/`)

**Reference:** `05_materials_ams.md` [REF-AMS-DECODE]
**Lines:** ~510

- [ ] **`src/ams/parser.rs`** (~226 lines)
  - [ ] Audit bitmask decoding — verify bit positions and masks against [REF-AMS-DECODE]
  - [ ] Check `AMS_*` constants against reference doc
  - [ ] Verify tray status parsing — are all tray states covered?
  - [ ] Check filament type/color parsing
  - [ ] Audit humidity and temperature value ranges

- [ ] **`src/ams/mapping.rs`** (~270 lines)
  - [ ] Audit slot/tray mapping logic — verify mapping algorithm against reference
  - [ ] Check for off-by-one errors in AMS unit and tray indexing (0-based vs 1-based)
  - [ ] Verify multi-AMS support — does mapping work with 1-4 AMS units?
  - [ ] Check edge cases: empty trays, mixed AMS types, external spool

- [ ] **`src/ams/mod.rs`** (~14 lines)
  - [ ] Verify re-exports

---

## Phase 8: Camera Module (`src/camera/`)

**Reference:** `06_cameras.md` [REF-CAM-RTSPS]
**Lines:** ~346

- [ ] **`src/camera/rtsps.rs`** (~138 lines)
  - [ ] Audit RTSP handshake implementation against [REF-CAM-RTSPS]
  - [ ] Verify DESCRIBE/SETUP/PLAY sequence
  - [ ] Check authentication handling
  - [ ] Verify `requires_wallclock_rtsp_timestamps()` quirk integration — is it applied correctly?
  - [ ] Audit session timeout handling

- [ ] **`src/camera/binary.rs`** (~169 lines)
  - [ ] Audit binary image protocol — verify packet structure against [REF-CAM-RTSPS] (or applicable camera binary section)
  - [ ] Check header parsing — are magic bytes, length fields, and checksums validated?
  - [ ] Verify JPEG frame extraction — are frame boundaries detected correctly?
  - [ ] Check buffer management — can oversized frames cause issues?

- [ ] **`src/camera/mod.rs`** (~39 lines)
  - [ ] Verify re-exports and module gating

---

## Phase 9: Diagnostics (`src/diagnostics/`)

**Reference:** `07_diagnostics_hms.md` [REF-DIAG-HMS]
**Lines:** ~591

- [ ] **`src/diagnostics/hms.rs`** (~224 lines)
  - [ ] Audit HMS code decoding — verify bitmask structure against [REF-DIAG-HMS]
  - [ ] Check severity level parsing
  - [ ] Verify module ID and sub-module ID extraction
  - [ ] Check HMS code-to-human-readable message mapping (if any)
  - [ ] Verify integration with `PrintTelemetry` HMS fields

- [ ] **`src/diagnostics/kprofile.rs`** (~346 lines)
  - [ ] Audit K-profile command construction — verify field names and structure
  - [ ] Check calibration data parsing
  - [ ] Verify the Payload+Request pattern is followed correctly
  - [ ] Check numeric precision — are floating-point comparisons safe?

- [ ] **`src/diagnostics/mod.rs`** (~21 lines)
  - [ ] Verify re-exports

---

## Phase 10: Quirks Engine (`src/quirks/`)

**Reference:** All chapters (model-specific behavior)
**Lines:** ~820

- [ ] **`src/quirks/mod.rs`** (~482 lines)
  - [ ] Audit `ModelQuirks` trait — are all default implementations correct?
  - [ ] Verify `is_unsafe_homing_command()` G-code parsing — edge cases: extra whitespace, lowercase, mixed case, trailing arguments
  - [ ] Audit `format_z_move_gcode()` bounds validation — are min/max Z values correct per model?
  - [ ] Check `relative_z_move_gcode()` default — is the G-code format correct?
  - [ ] Verify `z_max()` values per model family against printer specs
  - [ ] Audit capability methods: `requires_wallclock_rtsp_timestamps()`, `supports_auxiliary_right_fan()`, `auxiliary_fan_uses_percentage()`

- [ ] **`src/quirks/models/a1.rs`** (~65 lines) — verify A1 family quirks
- [ ] **`src/quirks/models/p1.rs`** (~60 lines) — verify P1 family quirks
- [ ] **`src/quirks/models/p2.rs`** (~58 lines) — verify P2 family quirks (new model — extra scrutiny)
- [ ] **`src/quirks/models/x1.rs`** (~106 lines) — verify X1C/X1E differentiation and TLS modes
- [ ] **`src/quirks/models/x2.rs`** (~62 lines) — verify X2 quirks
- [ ] **`src/quirks/models/h2.rs`** (~71 lines) — verify H2S/H2D/H2DPro/H2C differentiation
- [ ] **`src/quirks/models/mod.rs`** (~12 lines) — verify re-exports

For each model file:
  - [ ] Verify `is_unsupported_command()` lists match known unsupported commands for that model
  - [ ] Verify TLS mode is correct (TLS 1.2 vs 1.3, certificate behavior)
  - [ ] Verify fan rounding and percentage quirks
  - [ ] Verify door sensor method is correct (home_flag vs stat)
  - [ ] Cross-reference Z-max, bed type, and axis configuration against printer hardware specs

---

## Phase 11: Client Coordinator (`src/client.rs`)

**Reference:** All chapters (orchestration layer)
**Lines:** ~604

- [ ] Audit `PrinterClient` struct — are all fields necessary and correctly typed?
- [ ] Verify `publish_request` helper — does serialization handle all command types correctly?
- [ ] Check `next_sequence_id()` — is ID generation thread-safe? Does wrapping at `TASK_ID_MAX` work?
- [ ] Audit every public method:
  - [ ] `send_gcode()` — does it validate G-code? Does it check quirks for unsupported/unsafe commands?
  - [ ] `set_print_speed()` — correct enum-to-protocol mapping?
  - [ ] `set_bed_temperature()` / `set_nozzle_temperature()` — range validation?
  - [ ] `control_light()` — correct LED control values?
  - [ ] `start_print()` / `stop_print()` / `pause_print()` / `resume_print()`
  - [ ] `select_filament()` / AMS operations
  - [ ] Camera operations
  - [ ] Calibration operations
  - [ ] FTPS file operations
- [ ] Verify error propagation — are errors from MQTT/FTPS/camera correctly surfaced?
- [ ] Check for operations that should be mutually exclusive (e.g., printing while calibrating)
- [ ] Audit connection lifecycle — connect, reconnect, disconnect, cleanup

---

## Phase 12: CLI Tool (`src/bin/bambino-cli/`)

**Reference:** N/A (user-facing tool)
**Lines:** ~978

- [ ] **`src/bin/bambino-cli/main.rs`** (~163 lines)
  - [ ] Audit CLI argument parsing — are all subcommands well-defined?
  - [ ] Verify `env_logger` initialization from `-v` flag
  - [ ] Check error display for user-facing messages

- [ ] **`src/bin/bambino-cli/connection.rs`** (~81 lines)
  - [ ] Audit connection setup — is `build_unsafe_client_config()` appropriate here?
  - [ ] Check connection timeout handling
  - [ ] Verify credential passing

- [ ] **`src/bin/bambino-cli/discover.rs`** (~88 lines)
  - [ ] Audit discovery output formatting
  - [ ] Check timeout and retry behavior for user experience

- [ ] **`src/bin/bambino-cli/monitor.rs`** (~621 lines)
  - [ ] Audit TUI rendering — are crossterm operations correctly sequenced?
  - [ ] Check alternate screen and raw mode cleanup on exit/panic
  - [ ] Verify telemetry display accuracy — are values formatted correctly (temps, percentages, times)?
  - [ ] Audit key event handling — are all exit paths clean?
  - [ ] Check for potential panics in rendering with unexpected telemetry data

- [ ] **`src/bin/bambino-cli/control.rs`** (~307 lines)
  - [ ] Audit command dispatch — do CLI commands map to the correct client methods?
  - [ ] Check argument validation for user-provided values (temperatures, speeds, G-code)
  - [ ] Verify error messages are helpful

- [ ] **`src/bin/bambino-cli/storage.rs`** (~260 lines)
  - [ ] Audit FTPS file operations — upload, download, list, delete
  - [ ] Check file path handling — are paths sanitized? Can path traversal occur?
  - [ ] Verify progress reporting accuracy

- [ ] **`src/bin/bambino-cli/table.rs`** (~58 lines)
  - [ ] Audit table formatting — edge cases with long strings, Unicode, empty data

---

## Phase 13: Test Infrastructure & Coverage

### Integration Tests (`tests/`)
**Lines:** ~1398

- [ ] **`tests/common/mod.rs`** (~22 lines) — verify shared test utilities
- [ ] **`tests/common/io.rs`** (~60 lines) — audit mock IO implementations for correctness
- [ ] **`tests/common/mock_mqtt.rs`** (~220 lines)
  - [ ] Verify mock MQTT broker behavior matches real Bambu printer behavior
  - [ ] Check that mock responses are realistic and cover edge cases
- [ ] **`tests/common/mock_ftps.rs`** (~244 lines)
  - [ ] Verify mock FTP server behavior matches real Bambu FTPS
  - [ ] Check file operation simulation accuracy
- [ ] **`tests/common/mock_camera.rs`** (~88 lines)
  - [ ] Verify mock camera behavior
- [ ] **`tests/client_test.rs`** (~451 lines)
  - [ ] Are all public client methods tested?
  - [ ] Are error paths tested?
  - [ ] Are edge cases covered (disconnection during operation, concurrent calls)?
- [ ] **`tests/mqtt_test.rs`** (~134 lines)
  - [ ] Are MQTT packet edge cases tested (max-length payloads, malformed packets)?
- [ ] **`tests/ftps_test.rs`** (~87 lines)
  - [ ] Are FTP protocol edge cases tested?
- [ ] **`tests/camera_test.rs`** (~92 lines)
  - [ ] Are camera protocol edge cases tested?

### Inline Unit Tests (in `#[cfg(test)]` modules)

- [ ] **`src/error.rs`** — `test_display_consistency`: verify it tests ALL variants
- [ ] **`src/models.rs`** — model resolution tests: verify coverage of all model strings
- [ ] **`src/mqtt/client.rs`** — MQTT packet tests
- [ ] **`src/mqtt/commands.rs`** — command serialization tests: verify JSON output matches protocol
- [ ] **`src/ftps/client.rs`** — PASV parsing, FTP response tests
- [ ] **`src/ftps/parser.rs`** — directory listing tests
- [ ] **`src/ams/parser.rs`** — bitmask decoding tests
- [ ] **`src/ams/mapping.rs`** — slot mapping tests
- [ ] **`src/camera/binary.rs`** — binary frame parsing tests
- [ ] **`src/camera/rtsps.rs`** — RTSP handshake tests
- [ ] **`src/discovery/parser.rs`** — SSDP response parsing tests
- [ ] **`src/discovery/mod.rs`** — discovery integration tests
- [ ] **`src/diagnostics/hms.rs`** — HMS code decoding tests
- [ ] **`src/diagnostics/kprofile.rs`** — K-profile command tests
- [ ] **`src/quirks/mod.rs`** — quirks engine tests
- [ ] **`src/types/telemetry.rs`** — telemetry deserialization tests

---

## Phase 14: Cross-Cutting Concerns

These items span the entire codebase and should be checked after module-level review is complete.

- [ ] **no_std compatibility:** Run `cargo build --no-default-features --features alloc --lib` and verify clean compilation
- [ ] **Clippy:** Run `cargo clippy` across all feature combinations and resolve warnings
- [ ] **Feature gating consistency:** Grep for `#[cfg(feature = "...")]` — verify all gates are correct and no dead code paths exist
- [ ] **`alloc` imports under no_std:** Verify all `use alloc::{string::String, vec::Vec, format}` imports are correctly gated
- [ ] **Magic numbers audit:** Grep for numeric literals in non-const positions — should they be named constants?
- [ ] **Error message quality:** Spot-check error messages for clarity and actionability
- [ ] **Logging discipline:** Verify `log::debug!`/`trace!`/`warn!` usage is consistent — no `println!` in library code
- [ ] **Public API surface:** Review `pub` visibility — are internal helpers accidentally public?
- [ ] **Dependency versions:** Check `Cargo.toml` dep versions against latest — any known CVEs or breaking changes?
- [ ] **Reference doc alignment summary:** Compile a list of any protocol field names, constants, or behaviors in the code that deviate from the reference docs

---

## Progress Tracker

| Phase | Module | Files | Lines | Status |
|-------|--------|-------|-------|--------|
| 1 | Core Foundation | 3 | ~329 | **Complete** |
| 2 | Platform I/O | 4 | ~475 | Not started |
| 3 | MQTT | 3 | ~1470 | Not started |
| 4 | FTPS | 3 | ~966 | Not started |
| 5 | Telemetry & Types | 2 | ~754 | Not started |
| 6 | Discovery | 2 | ~555 | Not started |
| 7 | AMS | 3 | ~510 | Not started |
| 8 | Camera | 3 | ~346 | Not started |
| 9 | Diagnostics | 3 | ~591 | Not started |
| 10 | Quirks Engine | 8 | ~820 | Not started |
| 11 | Client Coordinator | 1 | ~604 | Not started |
| 12 | CLI Tool | 7 | ~978 | Not started |
| 13 | Test Infrastructure | 16+ | ~1398 | Not started |
| 14 | Cross-Cutting | — | — | Not started |
| **Total** | | **~58** | **~9,796** | |
