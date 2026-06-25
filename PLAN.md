# Deep Code Review Plan

Comprehensive module-by-module review of the `bambino` crate: library, CLI tool, and test infrastructure.
Structured for incremental completion across multiple sessions — check off items as they are reviewed and resolved.

**Review scope:** correctness bugs, logic errors, protocol alignment with reference docs, code smells, unsafe soundness, error handling gaps, missing edge cases, test coverage, and no_std compatibility.

---

## How to Use This Plan

Each phase is a self-contained review unit targeting one module or subsystem. Within each phase, individual files are listed with specific review concerns. Mark items `[x]` as they are completed. Phases can be done in any order, though the suggested order moves from foundational modules outward.

When reviewing, always cross-reference the corresponding reference doc (listed in each phase header) to verify field names, byte offsets, protocol semantics, and behavioral expectations against the specification.

---

## Phases 1–6: Completed Reviews

<details>
<summary>Phase 1: Core Foundation — 2 fixes, comprehensive model resolution tests added</summary>

**Files:** `src/error.rs`, `src/models.rs`, `src/lib.rs` (~329 lines)

**Fixes:**
- `resolve_model()`: `&serial[0..3]` → `serial.get(0..3).unwrap_or("")` to prevent panics on non-ASCII/short input
- Removed redundant `O1C2` check (already caught by `O1C` match)

**Verified:** All 13 model variants have quirks structs, `Unknown` falls back to `X1CQuirks`, `BambuError` Display consistency tested across std/no_std
</details>

<details>
<summary>Phase 2: Platform I/O — 4 fixes across embassy/esp-idf, TimerProvider signature change</summary>

**Files:** `src/io/mod.rs`, `tokio.rs`, `embassy.rs`, `esp_idf.rs` (~475 lines)

**Fixes:**
- `TimerProvider::sleep` changed from static to `&self` for ESP-IDF async timer support
- Embassy: narrowed `unsafe impl Sync` to concrete type, added `AtomicBool` buffer guard, removed fake `SimpleRng` — `EmbassyTlsConnector` now generic over `Rng: CryptoRng + RngCore`
- ESP-IDF: `EspIdfTimer` now wraps `EspAsyncTimer` (was blocking `thread::sleep`), `bind()` now joins multicast group

**Noted:** ESP-IDF `TlsConnector` gap tracked in Phase 16
</details>

<details>
<summary>Phase 3: MQTT — 6 fixes including QoS handling, OOM guard, dead generic removal</summary>

**Files:** `src/mqtt/client.rs`, `commands.rs`, `mod.rs` (~1470 lines)

**Fixes:**
- Packet ID extraction: `qos == 1` → `qos >= 1` for correct QoS 0/1/2 handling
- Added `MQTT_MAX_PAYLOAD_BYTES` (1 MiB) OOM guard in `read_exact_packet`
- Pre-computed `request_topic` field (was per-publish `format!()` allocation)
- Removed dead `Timer: TimerProvider` generic from `connect()` and all call sites
- `PrintJobConfig`: `timelapse`, `layer_inspect`, `nozzle_offset_cali` now configurable via builder
- `AmsFilamentSettingRequest::new()` accepts `Option<&str>` for `tray_sub_brands`
- Added missing `ProjectAmsMapping2Entry` re-export
</details>

<details>
<summary>Phase 4: FTPS — 5 fixes including multi-line response parsing, PASV overflow, TLS 1.2 enforcement</summary>

**Files:** `src/ftps/client.rs`, `parser.rs`, `mod.rs` (~966 lines)

**Fixes:**
- `read_response` now accumulates multi-line bodies (was discarding continuations). Added `FTP_MAX_RESPONSE_LINE_BYTES` (4096) and `FTP_MAX_RESPONSE_LINES` (100) guards
- PASV port computation: arithmetic overflow fix via `u32` intermediate with range validation
- P2S/X2D TLS 1.2 enforcement via `enforce_ftps_tls_1_2()` quirk
- Upload error propagation: network errors no longer mapped to `DiskWriteFailure`
- Added `TYPE I` binary mode during connect, `disconnect()` method, allocation-free `write_command`
</details>

<details>
<summary>Phase 5: Telemetry — 8 fixes (4 critical), 17 new tests. Verified against real P1S wire capture</summary>

**Files:** `src/types/telemetry.rs`, `mod.rs` (~754 lines)

**Critical fixes:** temp fields `u32`→`f64`, `device` nesting for pushall/incremental, `CtcInfo::temp` `f32`→`u32`, `AmsTray.id` `u8`→`String`

**Other fixes:** Added `VirtualTray`, `IpcamTelemetry`, `bed_target_temper`, `total_layer_num` alias, `mc_percent`

**Re-exports added:** `AirductCollection`, `AirductPart`, `AmsDrySetting`, `CtcInfo`, `CtcTelemetry`, `IpcamTelemetry`, `VirtualTray`
</details>

<details>
<summary>Phase 6: Discovery — 5 fixes, 9 new tests. NT/ST fallback, new SsdpDevice fields</summary>

**Files:** `src/discovery/mod.rs`, `parser.rs` (~555 lines)

**Fixes:**
- UTF-8 bail-out: refactored into `extract_headers()` helper; non-UTF-8 optional headers skip instead of discarding entire packet
- NT/ST fallback model resolution per [REF-NET-DISC] Protocol Violation #7
- Added `signal_dbm: Option<i32>`, `bind_state`, `security_link` fields to `SsdpDevice`
- `elapsed_millis` now advances on every poll cycle (was only on timeouts — could run indefinitely)
- `broadcast_search()` returns `Err` when both multicast and broadcast sends fail

**CLAUDE.md:** Updated BambuModel canonical path — `src/models.rs` is single source, no discovery re-export
</details>

<details>
<summary>Phase 7: AMS — 2 fixes (1 correctness bug), 16 new tests. External spool ID fix, API hardening</summary>

**Files:** `src/ams/parser.rs`, `mapping.rs`, `mod.rs` (~510 lines)

**Fixes:**
- `resolve_global_tray_id`: returned `tray_id` for external spools (254/255) instead of `ams_id` — caused IDEX external spool global IDs to collide with standard AMS slot 0. Changed return type to `Option<u8>` with range validation (`AMS_MAX_STANDARD_ID` constant added, rejects invalid ams_id 4–127/136–253 and tray_id ≥ 4)
- `resolve_printing_global_id`: simplified to propagate `Option` via `?` chains

**Re-exports added:** `MaterialSource`, `AmsMapping2Entry`, `resolve_printing_global_id`

**Verified correct:** Bitmask decoding formula, all `AMS_*` constants, shutdown telemetry exception, flat/structured mapping builders, external spool safety validation, stale tray data cleansing (state 10 handled implicitly via `is_type_cleared`)
</details>

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
  - [ ] `start_print()` quirks integration: when `PrintJobConfig.nozzle_offset_cali` is `None`, apply `self.model.quirks().supports_nozzle_offset_calibration()` as the default before calling `ProjectFileRequest::from_config()`
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

## Phase 14: Lint & Compatibility Sweep

Mechanical verification pass — run tools, fix warnings, ensure consistency.

- [ ] **no_std compatibility:** Run `cargo build --no-default-features --features alloc --lib` and verify clean compilation
- [ ] **Clippy:** Run `cargo clippy` across all feature combinations and resolve warnings
- [ ] **Feature gating consistency:** Grep for `#[cfg(feature = "...")]` — verify all gates are correct and no dead code paths exist
- [ ] **`alloc` imports under no_std:** Verify all `use alloc::{string::String, vec::Vec, format}` imports are correctly gated
- [ ] **Magic numbers audit:** Grep for numeric literals in non-const positions — should they be named constants?
- [ ] **Logging discipline:** Verify `log::debug!`/`trace!`/`warn!` usage is consistent — no `println!` in library code
- [ ] **Public API surface:** Review `pub` visibility — are internal helpers accidentally public?
- [x] **Dead generic parameter:** `BambuMqttClient::connect<Timer: TimerProvider>` — **Removed** in Phase 3. All call sites updated.

---

## Phase 15: Dependency & Protocol Audit

Read-heavy analysis — documentation alignment, dependency health, error UX.

- [ ] **Dependency versions:** Check `Cargo.toml` dep versions against latest — any known CVEs or breaking changes?
- [ ] **Reference doc alignment summary:** Compile a list of any protocol field names, constants, or behaviors in the code that deviate from the reference docs
- [ ] **Error message quality:** Spot-check error messages for clarity and actionability

---

## Phase 16: Platform Abstraction Gaps

Design work on the trait layer — requires architectural decisions. Blocked on ESP-IDF target maturity.

- [ ] **ESP-IDF `TlsConnector` gap:** ESP-IDF's `EspTls` manages its own TCP connection internally, which doesn't fit the `TlsConnector<RawStream>` "wrap an existing stream" trait model. Evaluate whether to (a) refactor `TlsConnector` into a higher-level `SecureConnect` trait supporting both models, or (b) use raw mbedtls bindings to wrap an existing socket fd.
- [ ] **`TimerProvider`-based discovery deadline:** `discover_devices` tracks elapsed time by counting socket poll cycles (~100ms each) rather than measuring wall-clock time. This is inaccurate and prevents platform-agnostic timeout semantics. Evaluate adding a `timeout` method or `select`-style deadline to `TimerProvider`, and refactor `discover_devices` to use it instead of the poll counter.

---

## Progress Tracker

| Phase | Module | Files | Lines | Status |
|-------|--------|-------|-------|--------|
| 1 | Core Foundation | 3 | ~329 | **Complete** |
| 2 | Platform I/O | 4 | ~475 | **Complete** |
| 3 | MQTT | 3 | ~1470 | **Complete** |
| 4 | FTPS | 3 | ~966 | **Complete** |
| 5 | Telemetry & Types | 2 | ~754 | **Complete** |
| 6 | Discovery | 2 | ~555 | **Complete** |
| 7 | AMS | 3 | ~510 | **Complete** |
| 8 | Camera | 3 | ~346 | Not started |
| 9 | Diagnostics | 3 | ~591 | Not started |
| 10 | Quirks Engine | 8 | ~820 | Not started |
| 11 | Client Coordinator | 1 | ~604 | Not started |
| 12 | CLI Tool | 7 | ~978 | Not started |
| 13 | Test Infrastructure | 16+ | ~1398 | Not started |
| 14 | Lint & Compatibility | — | — | Not started |
| 15 | Dependency & Protocol Audit | — | — | Not started |
| 16 | Platform Abstraction Gaps | — | — | Blocked |
| **Total** | | **~58** | **~9,796** | |
