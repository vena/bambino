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

- [x] **`src/io/mod.rs`** (~170 lines)
  - [x] Audit trait definitions: `AsyncIo`, `TlsConnector`, `AsyncUdpSocket`, `TimerProvider` — all sound. No `Send`/`Sync` bounds (correct: embassy single-threaded doesn't need them, tokio gets them from concrete types). ✓
  - [x] Verify trait methods have correct lifetime bounds and `Send`/`Sync` constraints for all three targets ✓
  - [x] Check associated type bounds — `TlsConnector::Stream: AsyncIo` is sufficient ✓
  - [x] Verify `SocketError` is flexible enough — `Other(&'static str)` is `Copy`, covers all platforms ✓
  - [x] **Changed:** `TimerProvider::sleep` signature changed from static to `&self` to support instance-based platform timers (ESP-IDF async timer)

- [x] **`src/io/tokio.rs`** (~220 lines)
  - [x] Audit `build_unsafe_client_config()` — correctly uses Ring provider, `with_safe_default_protocol_versions()` (supports TLS 1.2 + 1.3). ✓
  - [x] Check `NoCertificateVerification` — all four `ServerCertVerifier` methods implemented correctly. `supported_verify_schemes()` covers all common schemes (ED448 absent but unused by printers). ✓
  - [x] Verify `TokioTlsConnector` correctly maps to the `TlsConnector` trait — SNI uses IP address, which is fine since `NoCertificateVerification` skips hostname checking per [REF-NET-SECURE]. ✓
  - [x] Audit socket error conversion (`to_socket_error`) — comprehensive mapping; catch-all `Other` loses specificity for rare error kinds (`BrokenPipe`, `PermissionDenied`), acceptable. ✓
  - [x] Verify `TokioTimer` precision and cancellation behavior — delegates to `tokio::time::sleep`. ✓
  - [x] Check for resource leaks in UDP socket lifecycle — socket joins multicast group per [REF-NET-DISC], nonblocking set before Tokio conversion. ✓

- [x] **`src/io/embassy.rs`** (~207 lines)
  - [x] **FIXED:** `unsafe impl Sync` narrowed from blanket `<T>` to concrete `[u8; 16384]`. Added `AtomicBool` guard (`TLS_BUFFERS_IN_USE`) with RAII `BufferGuard` and `GuardedTlsConnection` wrapper — prevents aliased `&mut` references and releases buffers on drop.
  - [x] **FIXED:** Removed `SimpleRng` (constant-output fake CryptoRng). `EmbassyTlsConnector` is now generic over `Rng: CryptoRng + RngCore` — callers must provide a platform-appropriate hardware RNG.
  - [x] Verify static buffer sizing — 16384 bytes matches max TLS record size (RFC 5246). ✓
  - [x] Verify trait implementations match the `mod.rs` trait contracts ✓

- [x] **`src/io/esp_idf.rs`** (~78 lines)
  - [x] **FIXED:** `EspIdfTimer` now wraps `EspAsyncTimer` from `esp-idf-svc` instead of blocking `std::thread::sleep()`. Provides proper async sleep that integrates with FreeRTOS scheduler.
  - [x] **FIXED:** `EspIdfUdpSocket::bind()` now joins multicast group `239.255.255.250` per [REF-NET-DISC] (previously missing — would silently drop NOTIFY advertisements).
  - [x] Check error type conversions — identical mapping to tokio's `to_socket_error()`, acceptable given mutually exclusive feature gates. ✓
  - [x] **Noted:** No `TlsConnector` implementation — trait mismatch between ESP-IDF's `EspTls` (manages own TCP) and `TlsConnector` (wraps existing stream). Tracked in Phase 14.

---

## Phase 3: MQTT Client & Commands (`src/mqtt/`)

**Reference:** `03_mqtt_telemetry.md` [REF-MQTT-CONN], `04_toolhead_thermal_motion.md` [REF-MOTO-GCODE]
**Lines:** ~1470

- [x] **`src/mqtt/client.rs`** (~603 lines)
  - [x] Audit MQTT packet construction — CONNECT, PUBLISH, SUBSCRIBE, PUBACK, PINGREQ byte layouts verified against MQTT 3.1.1 spec. ✓
  - [x] Check `PACKET_TYPE_*` and `MQTT_*` constants against [REF-MQTT-CONN] — all match. ✓
  - [x] Verify remaining-length encoding/decoding handles multi-byte lengths correctly — encoding and decoding both handle 1-4 byte lengths with overflow guard. ✓
  - [x] Audit topic string construction — `device/{serial}/report` and `device/{serial}/request` match [REF-MQTT-CONN]. **Fixed:** pre-computed `request_topic` field replaces per-publish `format!()` allocation.
  - [x] Check QoS handling — **Fixed:** packet ID extraction now triggers for `qos >= 1` (was `qos == 1`), correctly handling QoS 0/1/2 packet structure. PUBACK remains gated on QoS 1 only.
  - [x] Verify connection keepalive logic and timeout handling — `MQTT_KEEP_ALIVE_SECS: 30`, `MQTT_ZOMBIE_TIMEOUT_SECS: 10` per [REF-MQTT-ZOMBIE], `MQTT_STALE_CONNECTION_SECS: 60` per [REF-MQTT-CONN]. ✓
  - [x] Audit the read loop — `read_exact_packet` blocks on `read_exact`; no partial-packet panic or infinite loop risk. ✓
  - [x] Check buffer management — **Fixed:** added `MQTT_MAX_PAYLOAD_BYTES` (1 MiB) upper bound in `read_exact_packet` to prevent OOM from malformed remaining-length headers.
  - [x] Verify sequence ID generation and wrapping behavior — starts at 2, wraps via `wrapping_add(1)`, skips 0. Tested. ✓
  - [x] **Fixed:** removed dead `Timer: TimerProvider` generic parameter from `connect()` and all call sites (CLI, tests). Cleaned up unused `DummyTimer`/`TokioTimer` imports.
  - [x] **Fixed:** CONNACK non-zero return code now logged at `warn` level before returning `AccessDenied`.

- [x] **`src/mqtt/commands.rs`** (~847 lines)
  - [x] Audit every command payload struct — all serde field names verified against reference docs:
    - [x] `GCodeRequest` — `gcode_line` command, `param` with `\n` suffix, `print:` envelope. ✓
    - [x] `PrintSpeedRequest` — `print_speed` command, `param` as string. ✓. **Fixed:** `new()` visibility narrowed to `pub(crate)`; external consumers use `PrinterClient::set_print_speed(PrintSpeed)`.
    - [x] Temperature commands via G-code wrapper — no dedicated structs (correct per architecture). ✓
    - [x] `LedCtrlRequest` — `ledctrl` command, `system:` envelope, timing fields zeroed for on/off. ✓
    - [x] `ProjectFileRequest` / `PrintJobConfig` — all fields match [REF-MQTT-LIFECYCLE]. **Fixed:** `timelapse`, `layer_inspect`, `nozzle_offset_cali` now configurable via builder methods instead of hardcoded. `nozzle_offset_cali` uses `Option<bool>` to defer to quirks engine default.
    - [x] `CalibrationRequest` — `option` bitmask, `print:` envelope. ✓
    - [x] AMS commands (`AmsFilamentSettingRequest`, `AmsControlRequest`, `AmsGetRfidRequest`, `AmsChangeFilamentRequest`, `AmsFilamentDryingRequest`) — all fields match [REF-MQTT-LIFECYCLE] and [REF-AMS-DRYER]. **Fixed:** `AmsFilamentSettingRequest::new()` accepts `Option<&str>` for `tray_sub_brands` (was hardcoded `"{type} Basic"`).
  - [x] Verify `#[serde(rename = "...")]` — only `AirductPayload::mode_id` → `"modeId"`, matches ref camelCase. ✓
  - [x] Check `TASK_ID_MAX` and `clamp_task_id()` — `i32::MAX as u64`, returns `u32`. Tested. ✓
  - [x] Audit `PrintJobConfig` builder — sensible defaults, builder pattern, `AmsMappingTable` polymorphism verified by tests. ✓
  - [x] Check `CalibrationOption` bitmask — values 2/4/8/16/32 match ref bits 1-5. Bits 0/6 (internal calibrations) deliberately omitted from public API. ✓
  - [x] Verify `PrintSpeed` enum — values 1-4 match ref `"1"`-`"4"` string mapping in `client.rs`. ✓
  - [x] Envelope wrappers verified — `pushing:` (pushall), `info:` (get_version), `system:` (ledctrl), `print:` (all others). ✓

- [x] **`src/mqtt/mod.rs`** (~20 lines)
  - [x] Verify re-exports — **Fixed:** added missing `ProjectAmsMapping2Entry` re-export (needed by `PrintJobConfig::with_ams_mapping2()`).

---

## Phase 4: FTPS Client & Parser (`src/ftps/`)

**Reference:** `02_ftps.md` [REF-FTPS-CONN]
**Lines:** ~966

- [x] **`src/ftps/client.rs`** (~685 lines)
  - [x] Audit FTP command/response parsing — verify against [REF-FTPS-CONN]. **Fixed:** `read_response` now accumulates multi-line response bodies (was discarding continuation lines, breaking STAT fallback). Added `FTP_MAX_RESPONSE_LINE_BYTES` (4096) guard to `read_line_raw` to prevent OOM. Added `FTP_MAX_RESPONSE_LINES` (100) iteration limit to `read_response` to prevent infinite loops on malformed input.
  - [x] Check `FTP_*` constants against reference doc connection parameters — all match [REF-FTPS-CONN]. ✓
  - [x] Verify PASV mode port parsing (`parse_pasv_port`) — edge cases with malformed responses. **Fixed:** arithmetic overflow on port computation now uses `u32` intermediate with range validation.
  - [x] Audit TLS session reuse for data connections — matches Bambu's FTPS behavior. `TlsConnector` wraps each data channel. **Fixed:** P2S/X2D TLS 1.2 enforcement via `build_unsafe_client_config_with_options(force_tls_1_2)` driven by `enforce_ftps_tls_1_2()` quirk. CLI storage.rs updated.
  - [x] Check file listing parsing — malformed listings gracefully skipped via `continue`. ✓
  - [x] Verify upload/download correctness — uploads verified via SIZE after both 226 and 426 paths per [REF-FTPS-XFER]. **Fixed:** upload error propagation — network errors from `read_response` now propagate as `NetworkError` instead of being mapped to `DiskWriteFailure`.
  - [x] Audit timeout handling during file transfers — timeouts rely on underlying socket/TLS layer. Post-upload 226 wait can take up to 300s per [REF-FTPS-FLUSH], handled by blocking on `read_response`. ✓
  - [x] Check for off-by-one errors in buffer reads — chunked upload loop correctly handles `data.len()` boundary. ✓
  - [x] Verify the connection state machine — sequential command/response pairs, no out-of-order risk. **Added:** `TYPE I` binary mode command during connect to prevent ASCII-mode line ending corruption per RFC 959. **Added:** `disconnect()` method sending `QUIT` for clean session teardown. **Fixed:** `write_command` no longer allocates a `String` per command — uses two `write_all` calls.

- [x] **`src/ftps/parser.rs`** (~267 lines)
  - [x] Audit FTP response parsing — multi-line responses now handled correctly (see `read_response` fix above). ✓
  - [x] Check directory listing parsing against actual Bambu FTP output format — whitespace-insensitive tokenization via `split_whitespace()` matches [REF-FTPS-OPS] variable padding. Filenames with spaces correctly reconstructed via `join(" ")`. ✓
  - [x] Verify date/time parsing in directory listings — temporal rollover heuristic correctly decrements year when parsed (month, day, hour, minute) tuple exceeds current time. ✓
  - [x] Check for panics on malformed input — all parsing uses `and_then`/`ok()`/`unwrap_or()` with `continue` on failure. No `.unwrap()` calls on user input. ✓

- [x] **`src/ftps/mod.rs`** (~14 lines)
  - [x] Verify re-exports — `BambuFtpsClient`, `FtpDataStreamFactory`, `FtpFile`, `parse_unix_listing` all exported. ✓

---

## Phase 5: Telemetry & Types (`src/types/`)

**Reference:** `03_mqtt_telemetry.md`, `04_toolhead_thermal_motion.md` [REF-THER-DECODE], `05_materials_ams.md` [REF-AMS-DECODE]
**Lines:** ~754
**Status:** **Complete.** All 8 issues fixed, 17 new tests added (111 total). Verified against P1S real wire capture.
**Verified against:** Real P1S wire capture (`tests/mocks/P1S.json`), pybambu mock captures (H2D real, P1P real, + 9 fabricated mocks)

- [x] **`src/types/telemetry.rs`** — All review items complete. All fixes applied:
  - [x] **Fix A (CRITICAL):** Temp fields `u32` → `f64` (wire sends floats on P1S/P1P/A1). `unpack_temperature()` now accepts `f64`. Monitor updated to use `as_f64()`.
  - [x] **Fix B (CRITICAL):** `device` nesting fixed — added `device: Option<DeviceTelemetry>` to `PrintTelemetry`, moved `ctc` into `DeviceTelemetry`. Both pushall and incremental paths now captured.
  - [x] **Fix C (CRITICAL):** `CtcInfo::temp` `f32` → `u32` (composite-packed integers)
  - [x] **Fix D:** Added `VirtualTray` struct with full P1S/H2D schema, `vt_tray: Option<VirtualTray>` on `PrintTelemetry`
  - [x] **Fix E:** Added `bed_target_temper: Option<f64>` field
  - [x] **Fix F (CRITICAL):** `AmsTray.id` `u8` → `String` (wire sends strings). AMS parser tests updated.
  - [x] **Fix G:** Created `IpcamTelemetry` struct replacing flat `ipcam_dev`/`ipcam_record`/`timelapse` fields. Captures `mode_bits`, `resolution`, `tutk_server`.
  - [x] **Fix H:** Added `#[serde(alias = "total_layer_num")]` to `total_layers`, added `mc_percent: Option<i32>`
  - [x] **Tests:** End-to-end P1S.json test + 16 unit tests covering: `deserialize_permissive_bool` variants, `parse_hex_string` variants, ethernet bitmask, temperature boundaries, CTC deserialization, device nesting paths, `power_on_flag`, `total_layer_num` alias, `mc_percent`, `VirtualTray`, nozzle info standard/IDEX keys

- [x] **`src/types/mod.rs`** — Re-exports updated: added `AirductCollection`, `AirductPart`, `AmsDrySetting`, `CtcInfo`, `CtcTelemetry`, `IpcamTelemetry`, `VirtualTray`.

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
- [ ] **ESP-IDF `TlsConnector` gap:** ESP-IDF's `EspTls` manages its own TCP connection internally, which doesn't fit the `TlsConnector<RawStream>` "wrap an existing stream" trait model. Evaluate whether to (a) refactor `TlsConnector` into a higher-level `SecureConnect` trait supporting both models, or (b) use raw mbedtls bindings to wrap an existing socket fd. Blocked on ESP-IDF target maturity.
- [x] **Dead generic parameter:** `BambuMqttClient::connect<Timer: TimerProvider>` — **Removed** in Phase 3. All call sites updated.

---

## Progress Tracker

| Phase | Module | Files | Lines | Status |
|-------|--------|-------|-------|--------|
| 1 | Core Foundation | 3 | ~329 | **Complete** |
| 2 | Platform I/O | 4 | ~475 | **Complete** |
| 3 | MQTT | 3 | ~1470 | **Complete** |
| 4 | FTPS | 3 | ~966 | **Complete** |
| 5 | Telemetry & Types | 2 | ~754 | **Complete** |
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
