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

<details>
<summary>Phase 8: Camera — 4 fixes (1 correctness bug), 9 new tests. RTP overflow fix, API refactor</summary>

**Files:** `src/camera/rtsps.rs`, `binary.rs`, `mod.rs` (~346 lines)

**Fixes:**
- `RtpTimestampCorrector::correct_timestamp()`: `f64 as u32` saturated to `u32::MAX` after ~13.25 hours of streaming instead of wrapping — broke the exact frame-freeze workaround it was designed for. Fixed via `u64` intermediate truncation
- `RtpTimestampCorrector` API refactored: single `correct_timestamp(elapsed, embedded_rtp)` method split into `init(embedded_rtp)` constructor + `correct(elapsed_secs)` method — `embedded_rtp` was silently ignored after first call
- `rewrite_rtsp_request_uri`: parameter renamed from `request_line` → `request_uri` to match actual semantics
- Module doc in `binary.rs`: corrected "5MB" → "10MB" to match `CAMERA_FRAME_MAX_SIZE` and [REF-CAM-BINARY]

**Verified correct:** Binary handshake layout (magic/command ID/offsets), JPEG SOI/EOI validation, 10MB frame limit, RTSPS URL format, URI rewrite path/query preservation, `requires_wallclock_rtsp_timestamps()` P2S-only override, port constants (322/6000), `no_std` alloc gating

**Deferred to Phase 10:** `CameraProtocol` enum is unused internally — `camera_stream_port() -> u16` returns raw port numbers instead of the type-safe enum. Integrate `CameraProtocol` into `ModelQuirks` trait.
</details>

<details>
<summary>Phase 9: Diagnostics — 6 fixes, 11 new tests. K-profile priming, response type, HmsEntry timestamps</summary>

**Files:** `src/diagnostics/hms.rs`, `kprofile.rs`, `mod.rs`, `src/types/telemetry.rs`, `src/client.rs` (~591 lines)

**Fixes:**
- `KProfileEntry.n_coef`: added `skip_serializing_if = "Option::is_none"` — was serializing `None` as `null` to firmware instead of omitting the field
- `decode_print_error`: removed dead `hex_val.len() != 8` check (format! always produces 8 chars for u32), replaced string-slicing with mathematical short_code computation
- K-profile priming: added doc comment to `ExtrusionCaliGetRequest` warning about firmware quirk [REF-DIAG-KPROF §7.3], added `PrinterClient::get_k_profiles()` with auto-prime and `set_k_profile_primed()` opt-out
- Added `ExtrusionCaliGetResponse` / `ExtrusionCaliGetResponsePayload` for deserializing printer's profile database reply
- Added `ts_boot: Option<u64>` and `ts_unix: Option<String>` to `HmsEntry` (present on X2/H2/P2 models, verified against pybambu MOCK-X2D.json)
- Updated `mod.rs` re-exports to include `ExtrusionCaliGetResponse`

**Verified correct:** HMS bitmask structure (attr_high/low, code_high/low), wiki key format (underscore-delimited), short-code format (attr_high + code_low), severity extraction `(attr >> 8) & 0x0F`, module ID `(attr >> 24) & 0xFF`, fault threshold `0x4000`, cancel echo filtering, all kprofile command field names and envelope structures, setting ID validation, `setting_id` intentionally absent from `ExtrusionCaliSelPayload`, CLI HMS integration in `monitor.rs`

**Tests added:** all severity levels (Fatal/Serious/Warning/Info/Unknown), cancel echo A (`0300_400C`), print_error cancel echo A+B, print_error status step, real X2D HMS entry (severity=6→Unknown), real MISC HMS entry (module=0x0C, Warning), standard+IDEX delete JSON serialization, `n_coef` None omission + Some inclusion, response deserialization

**CLAUDE.md:** Added Diagnostics architecture section. **README.md:** Added Firmware quirks section with K-profile priming.
</details>

---

<details>
<summary>Phase 10: Quirks Engine — 10 fixes (8 correctness bugs), 3 new tests, new a2.rs module. Cross-referenced against pybambu, Bambuddy, and Bambu Lab specs</summary>

**Files:** `src/quirks/mod.rs`, `models/{a1,a2,p1,p2,x1,x2,h2}.rs`, `models/mod.rs`, `src/bin/bambino-cli/camera.rs`, `reference/04_toolhead_thermal_motion.md` (~820 lines)

**Fixes:**
- `A1MiniQuirks` split from `A1Quirks`: A1 Mini has 180mm Z-max but shared `A1Quirks` defaulted to 256mm — could command Z moves 76mm beyond physical limit. Added macro-based `impl_a1_shared!` with parameterized `z_max`
- `A2LQuirks` extracted to new `models/a2.rs`: A2L has 330×320×325mm build volume (Z=325mm) but was sharing `A1Quirks` at 256mm. Separate product line warranted its own module
- Per-model `z_max()` values set from Bambu Lab official specs. H2 series uses conservative (dual-nozzle) Z limits since the quirks engine cannot know which nozzle mode is active during a print:
  - H2S: 340mm (single nozzle only)
  - H2D/H2D Pro: 320mm (conservative; 325mm in single-nozzle mode)
  - H2C: 320mm (conservative; 325mm with right nozzle only)
  - X2D: 256mm (conservative for aux/dual nozzle; main nozzle reaches 260mm)
- `P2Quirks::has_active_chamber_heater()` `true`→`false`: P2S has a chamber temperature sensor and airduct-based heat retention but no PTC heater element — M141 is silently ignored by P2S firmware. Confirmed by pybambu (`ACTIVE_CHAMBER_HEATER` excludes P2S) and Bambuddy (`CHAMBER_HEATER_MODELS` excludes P2S). Reference doc corrected
- `X2Quirks::has_active_chamber_heater()` `false`→`true`: X2D ships with an active PTC chamber heater. Confirmed by both pybambu and Bambuddy
- `P2Quirks`: added `supports_auxiliary_right_fan()→true` and `auxiliary_fan_uses_percentage()→true`. P2S mock telemetry contains `big_fan2_speed` and fan ID `160` in airduct control lists. Confirmed by pybambu (`SECONDARY_AUX_FAN` includes `p2_printers`)
- **[From Phase 8]** Replaced `camera_stream_port()→u16` with `camera_protocol()→CameraProtocol` on `ModelQuirks` trait. All model impls return `CameraProtocol::Rtsps` or `::BinaryJpeg`. CLI `camera.rs` uses type-safe protocol matching instead of raw port number comparison. Port derivable via `CameraProtocol::default_port()`

**Reference doc fix:** `reference/04_toolhead_thermal_motion.md` M141 section corrected — P2S removed from active chamber heater list, X2D and full H2 series added

**Verified correct:** `is_unsafe_homing_command()` (whitespace/case/trailing args), `format_z_move_gcode()` (G-code sequence matches [REF-MOTO-GCODE], boundary handling), `FanSpeedDebouncer` (3-frame persistence, large-shift bypass), `fan_step_to_percentage()` (rounding formula), all door sensor routing (X1→`home_flag`, others→`stat`, per [REF-NET-DOOR]), `enforce_ftps_tls_1_2()` (P2S+X2D only), `stg_cur` idle bug (A1+P1 only), `requires_wallclock_rtsp_timestamps()` (P2S only), `is_unsupported_command()` (no-op scaffolding, acceptable), `quirks/models/mod.rs` re-exports

**Tests added:** `test_a1_mini_quirks` (z_max=180, Z-move rejection at 200mm, acceptance at 150mm), `test_a2l_quirks` (z_max=325, Z-move bounds), z_max assertions added to all H2 model tests. All existing per-model tests updated for `camera_protocol()`, corrected chamber heater and aux fan assertions

**Cross-reference sources:** pybambu (Home Assistant integration `models.py`), Bambuddy (`printer_manager.py`, `bambu_mqtt.py`), pybambu mock telemetry (`MOCK-P2S.json`, `MOCK-X2D.json`), Bambu Lab official comparison page (bambulab.com/en/compare)
</details>

---

<details>
<summary>Phase 11: Client Coordinator — 6 fixes (2 correctness bugs), 6 new tests. Chamber guard, nozzle offset quirks default, temp clamping, safe G-code dispatch</summary>

**Files:** `src/client.rs`, `src/mqtt/commands.rs` (~650 lines)

**Fixes:**
- `set_chamber_temperature()`: guard changed from `ignores_chamber_temperature()` to `!has_active_chamber_heater()` — was allowing M141 on X1C and P2S (have sensor but no PTC heater, firmware silently ignores). Doc comment corrected to list X1E/X2D/H2 series as supported models
- `ProjectFileRequest::from_config()` now takes `BambuModel` parameter and resolves `nozzle_offset_cali: None` via `model.quirks().supports_nozzle_offset_calibration()` — was defaulting to `false` via `unwrap_or(false)`, causing IDEX models (X2D, H2D, H2D Pro, H2C) to skip nozzle offset calibration by default
- `send_gcode()` now validates against `is_unsafe_homing_command()` and `is_unsupported_command()` before dispatch. Raw escape hatch available via new `send_gcode_raw()` method. All internal callers (`home_axes`, `move_relative`, `extrude`, thermal setters, fan control) use `send_gcode_raw()` since they perform their own validation
- `set_bed_temperature()`, `set_nozzle_temperature()`, `set_chamber_temperature()` now clamp to model-specific max values (`bed_temp_max()`, `nozzle_temp_max()`, `chamber_temp_max()`) with `log::warn` on clamp
- `next_sequence_id()` wraps at `TASK_ID_MAX` (32-bit signed integer limit) instead of `u64::MAX` to stay within firmware parsing constraints [REF-MQTT-ENV]

**Verified correct:** `PrinterClient` struct fields (all necessary, correctly typed), `publish_request` (uses `serde_json::to_vec` per convention, error mapping correct), `poll_telemetry` (clean delegation), `set_print_speed` (correct enum-to-string mapping), fan PWM calculation (no overflow), print lifecycle commands (pause/resume/stop — correct command strings), AMS operations (clean pass-through), K-profile priming (auto-prime + opt-out), error propagation (all paths correctly surface MQTT/serialization errors), connection lifecycle (coordinator wraps pre-connected clients, implicit cleanup via Drop), `home_axes` and `move_relative` (correct quirks integration), thread safety (`&mut self` on all mutation methods), `storage()` accessor (`Option<&mut>`, handles unattached FTPS), no mutually exclusive operation guards needed (firmware handles conflicts)

**Tests added:** `test_send_gcode_rejects_unsafe_homing` (P1S bed-on-Z partial homing rejected), `test_send_gcode_raw_bypasses_safety` (raw escape hatch passes unsafe command through), `test_temperature_clamping` (bed/nozzle/chamber clamped to X1E model maxima), `test_nozzle_offset_cali_quirks_default_idex` (X2D auto-enables), `test_nozzle_offset_cali_quirks_default_single_nozzle` (P1S defaults to disabled), `test_nozzle_offset_cali_explicit_override` (explicit false overrides X2D quirks default)

**Tests updated:** `test_thermal_guards_and_temperatures` — chamber temp positive case changed from X1C (no heater) to X1E (has PTC heater), added X1C negative case (sensor without heater correctly rejected)
</details>

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
| 8 | Camera | 3 | ~346 | **Complete** |
| 9 | Diagnostics | 3 | ~591 | **Complete** |
| 10 | Quirks Engine | 8 | ~820 | **Complete** |
| 11 | Client Coordinator | 2 | ~650 | **Complete** |
| 12 | CLI Tool | 7 | ~978 | Not started |
| 13 | Test Infrastructure | 16+ | ~1398 | Not started |
| 14 | Lint & Compatibility | — | — | Not started |
| 15 | Dependency & Protocol Audit | — | — | Not started |
| 16 | Platform Abstraction Gaps | — | — | Blocked |
| **Total** | | **~58** | **~9,796** | |
