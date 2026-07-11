**Status:** COMPLETE

# bambino Deep Review — 2026-07-11

Full-crate correctness sweep of the `bambino` async Rust printer-control library (host/ESP-IDF/Embassy targets). Methodology: the crate's `src/` and `tests/` trees were partitioned into 20 review units along module/concern boundaries (splitting large modules like `ftps`, `io`, `client`, `quirks`, `types/telemetry` in two where a single agent couldn't deeply read everything; merging small/thin directories together). One `general-purpose` subagent was spawned per unit, in parallel, each with only its own file list, the relevant `.claude/rules/*.md` invariants and nested `CLAUDE.md` files, and root `CLAUDE.md`/`README.md` in full.

**Scope exclusions (per the `deep-review` skill's standing policy for this crate):**
- Minor security issues are out of scope — this crate is explicitly LAN-only by design (see `README.md`'s Safety Notice). Cert-verification bypass, plaintext fallback, etc. are not flagged unless implemented incorrectly vs. their *own* stated behavior.
- Style/refactor suggestions and naming preferences are out of scope, except where a name actively misrepresents behavior (inverted boolean sense, a function that does the opposite of what it claims) — that's a correctness/footgun risk, not style.
- Abstraction/pattern-duplication commentary is only in scope when it's root-cause context on a `CONFIRMED` bug already found (an invariant enforced only by convention across similar call sites, where a bug already exists because of it) — not as standalone architecture criticism.

**Confidence tiers:** each finding is tagged `CONFIRMED` (agent is sure it's a real bug) or `PLAUSIBLE` (looks real but couldn't be fully verified — e.g. can't confirm the failure path triggers, or the invariant it'd violate is itself ambiguous). Only `CONFIRMED` findings get promoted to a `BUG-ID` in `BACKLOG.md` during this sweep; `PLAUSIBLE` findings are collected in their own section below for a human to manually triage later.

This file is meant to be read standalone by a fresh session with no other context from the sweep that produced it. **Caveat:** file:line references may have drifted if other commits landed on `main` since this sweep ran — verify against current source before acting on a finding.

## 1. src/ams/ — NO CONFIRMED ISSUES (4 PLAUSIBLE, see below)

## 2. src/bin/bambino-cli/ (control, storage, probe, monitor)

### src/bin/bambino-cli/storage.rs:200-227 — BUG-057 (Sev3)
**Issue:** `run_clock_check`'s cleanup-delete error is silently dropped when the preceding directory listing also fails.
**Detail:** `delete_result` (line ~204) is computed unconditionally, but if `listing` (line ~200) errors, `listing?` (line ~206) returns before `delete_result?` (line ~226) is ever checked — despite an in-code comment promising the probe file is always cleaned up. A failed cleanup on this path leaves `/bambino_clock_probe.txt` orphaned on the printer's SD card with no diagnostic.
**Suggested fix:** Check `delete_result` too when `listing` errors and fold both failures into the reported error.

(1 additional PLAUSIBLE finding — see the Plausible section below.)

## 3. src/bin/bambino-cli/ (main, connection, discover, camera, table, cert/tls)

### src/bin/bambino-cli/main.rs:116 — BUG-058 (Sev3)
**Issue:** `control`'s `override_usage` text drops `<IP> <SERIAL> [ACCESS_CODE]` on all three `ams` sub-subcommand lines, same class as BUG-016/commit 68f380f.
**Detail:** Every other line in this usage string correctly shows `bambino-cli control <IP> <SERIAL> [ACCESS_CODE] <action>...`, but the three `ams dry`/`ams dry-stop`/`ams help` lines omit the prefix, so a user copying the shown invocation gets a clap parse error.
**Suggested fix:** Prefix all three `ams` lines with `bambino-cli control <IP> <SERIAL> [ACCESS_CODE] `.

No other findings (CONFIRMED or PLAUSIBLE) in this unit — `connection.rs`, `discover.rs`, `inspect_cert.rs`, `verify_tls.rs`, `camera.rs`, `table.rs` all checked clean, including no access-code logging anywhere in `src/bin/bambino-cli/`.

## 4. src/camera/ + tests/camera_test.rs — NO ISSUES FOUND

## 5. src/client/ (connect, telemetry, ams, motion, hardware, thermal) — NO ISSUES FOUND

## 6. src/client/ (mod, types, print, storage, camera, dummy) — NO CONFIRMED ISSUES (1 PLAUSIBLE, see below)

## 7. tests/client_test.rs

### tests/client_test.rs:783 — BUG-059 (Sev3)
**Issue:** No test asserts the `extrude_cali_flag` (flow calibration) wire field.
**Detail:** README documents flow calibration as a `PrintJobConfig` default that runs automatically. `test_start_print_wire_payload` and every other print-job test assert `bed_leveling`/`vibration_cali` but never `extrude_cali_flag`, and no test calls `.flow_calibration(false)` to check the field flips to `0`. A regression in `ProjectFileRequest::from_config` (src/mqtt/commands/print_job.rs:251) would ship undetected.
**Suggested fix:** Add `assert_eq!(json["print"]["extrude_cali_flag"], 1)` to `test_start_print_wire_payload`, plus a dedicated test for `.flow_calibration(false)` asserting `0`.

(2 additional PLAUSIBLE findings — see the Plausible section below.)

## 8. src/diagnostics/

### src/client/connect.rs:150-153 (contract owned by src/diagnostics/kprofile.rs) — BUG-056 (Sev2)
**Issue:** `k_profile_primed` is never reset when the MQTT connection is torn down, so a reconnect skips the required K-profile priming request and `get_k_profiles()` hangs until timeout.
**Detail:** Per README/`.claude/rules/kprofile-priming.md`, firmware ignores the first `extrusion_cali_get` after MQTTS connection establishment; `PrinterClient::get_k_profiles()` (src/client/ams.rs:271-294) sends a throwaway prime only when `self.k_profile_primed == false`, then sets it `true`. That flag is only initialized `false` in the two constructors (`new()`, `from_mqtt()`) and is never reset by `disconnect_mqtt()` (connect.rs:150-153), which only clears `self.mqtt = None`. Sequence: connect → `get_k_profiles()` primes and sets the flag → `disconnect_mqtt()` → reconnect via `ensure_mqtt()`/`attach_mqtt()` establishes a fresh MQTT session → `get_k_profiles()` is called again but, since the flag is still `true` from the old session, no priming request is sent — the real `extrusion_cali_get` is silently swallowed by firmware and the caller's `poll_until` blocks until timeout.
**Suggested fix:** Reset `self.k_profile_primed = false` in `disconnect_mqtt()` and/or wherever a fresh MQTT dial/attach happens — priming state is per-connection, not per-`PrinterClient`.

`src/diagnostics/hms.rs` and `src/diagnostics/kprofile.rs`'s own task-ID clamping (all 5 constructors, via `ClampedTaskId`) were checked and are clean.

## 9. src/discovery/

### src/discovery/parser.rs:175-225 (`parse_ssdp_payload`) — BUG-060 (Sev2)
**Issue:** Parser accepts any HTTP-like SSDP packet with `USN`+`LOCATION` headers as a printer record — it never requires any Bambu-specific signal.
**Detail:** The module docstring claims the parser "differentiates Bambu Lab printers from general UPnP devices," but `dev_model` and every other Bambu-specific header are optional and only affect `model`/`raw_model_str` — they never gate acceptance. Ordinary LAN devices (routers, TVs, Chromecasts, other vendors' printers) that reply to/NOTIFY SSDP multicast with a `USN`+`LOCATION` parse successfully; `resolve_model` falls back to `BambuModel::Unknown` but the record is still returned, and `discover_devices()` dedupes purely on `serial` with no filter on `model != Unknown`. Result: non-printer LAN devices can appear in `discover_devices()`'s results.
**Suggested fix:** Require a positive Bambu signal to accept the packet (e.g. `nt`/`st` containing `bambulab-com`, or a recognized `DevModel`/serial prefix), and/or filter `BambuModel::Unknown` records out of `discover_devices()`'s return.

Embassy-unavailability of `discover_devices()` verified as structurally enforced (trait-bound, not just documented) — see `.claude/rules/udp-socket-binding.md`. `mod.rs`'s degraded-mode bind/broadcast handling, per-poll backoff, and `discovery_port` stamping (BUG-009/010/024/046) remain correctly implemented. (3 additional PLAUSIBLE findings — see the Plausible section below.)

## 10. src/error.rs, src/models.rs, src/lib.rs — NO ISSUES FOUND

Live-verified: `cargo build --no-default-features --features alloc --lib` and `cargo check --no-default-features --features embassy --lib` both pass. `BambuError`'s `thiserror`/manual `no_std` `Display` impls checked byte-identical across all 9 variants. `resolve_model()` cross-checked against `MODEL_MATRIX.csv`'s serial-prefix table, all 13 prefixes match. `lib.rs` feature-gate wiring (`cli`, `tokio`, `esp-idf`, `embassy`) matches `Cargo.toml` and the documented conventions.

## 11. src/ftps/ (client, mod) + tests/ftps_test.rs + tests/common/mock_ftps.rs — NO CONFIRMED ISSUES (2 PLAUSIBLE, see below)

Everything else checked and found correct: `validate_ftp_path` called by every path-taking method; TLS-1.2 fail-closed logic (P2S/X2D) matches README exactly on both control and data channels; FTPS-poisoning invariant holds across every transport-failure site; TLS SNI uses `serial` not `ip` everywhere; mock is honest about its own wire-framing testing limits per `.claude/rules/wire-framing-hardware-verification.md`.

## 12. src/ftps/ (parser, protocol)

### src/ftps/parser.rs:144-167 — BUG-061 (Sev3)
**Issue:** `day`/`hour`/`minute` parsed as bare `u8` with no calendar-range validation, unlike `month` (validated via lookup table).
**Detail:** `"Jun 99 88:70 file.gcode"` is silently accepted with `day=99,hour=88,minute=70` pushed into the returned `Vec<FtpFile>`. `bambino-cli`'s `printer_mtime_as_utc` happens to guard via `Date::from_calendar_date(...).ok()?`, but `print_file_listing_table` has no such guard and prints garbage verbatim; any other consumer of the public `FtpFile` type gets unvalidated fields.
**Suggested fix:** Reject entries (skip, matching the existing silent-skip convention) where `day` isn't `1..=31` or `hour`/`minute` aren't `0..=23`/`0..=59`.

### src/ftps/protocol.rs:239-260 — BUG-062 (Sev3)
**Issue:** `read_response`'s header-establishing branch silently drops a reply line whose 4th byte is neither `' '` nor `'-'`, instead of surfacing it as free text (inconsistent with the analogous in-body case, which does preserve it verbatim).
**Detail:** A non-conformant reply shaped like `"200\r\n"` (code immediately followed by CRLF, no separating space) leaves `header_code` unset and the loop just `continue`s, discarding the line outright. Worst case this burns lines out of the `FTP_MAX_RESPONSE_LINES` budget and surfaces as a generic "exceeded maximum line count" error, obscuring the real reply.
**Suggested fix:** Add an explicit `else` arm that either treats the line as the terminal reply (empty text) or returns a clear `ProtocolViolation`.

(3 additional PLAUSIBLE findings — see the Plausible section below.)

## 13. src/io/ (mod, tokio) + tests/common/io.rs + tests/common/mod.rs — NO ISSUES FOUND

`negotiated_version()` traced through tokio-rustls/rustls and confirmed to correctly feed the P2S/X2D fail-closed FTPS enforcement. `CnFallbackServerVerifier` performs real chain-of-trust + signature validation, not a bypass. TLS SNI always uses `serial`, never `ip`. No `std`-specific leak into no_std-facing trait signatures (`Cargo.toml` feature implications for `tokio`/`esp-idf`/`alloc`/`embassy` all correct). `read_chunk`'s per-read deadline matches `.claude/rules/wire-read-deadline.md`; `TokioUdpSocket` bind ordering matches `.claude/rules/udp-socket-binding.md`.

## 14. src/io/ (esp_idf, embassy)

### src/io/embassy.rs:196-217 — BUG-063 (Sev3)
**Issue:** `EmbassyTlsConnector::connect` collapses every distinct mbedtls-rs failure (`Session::new`, `set_server_name`, `connect()`) into the same `SocketError::ConnectionAborted`, with no `log::debug!` of the underlying error at any of the 3 sites.
**Detail:** Cert-verification failure, config/allocation error, bad-hostname rejection, and a real handshake timeout/reset are all indistinguishable to the caller and invisible in logs — unlike ESP-IDF's `map_esp_tls_connect_error`, which maps distinct cases and always preserves the real error. On Embassy, debugging a P2S/X2D FTPS connect failure (already handicapped by `negotiated_version()` always returning `None`) becomes opaque.
**Suggested fix:** `log::debug!("{:?}", err)` at each of the three `.map_err` sites before collapsing to `ConnectionAborted`.

### src/io/esp_idf.rs:701-702 — BUG-064 (Sev3)
**Issue:** `EspTls::adopt(raw_stream)` failure is mapped to a fixed string with the real `EspError` discarded and not logged, unlike every other FFI error site in this file (which follow `src/io/CLAUDE.md`'s documented "opaque fallback carries the formatted `EspError`" invariant).
**Detail:** If adopting an already-connected fd into `EspTls` fails (resource exhaustion, invalid fd state), the actual cause is unrecoverable from logs.
**Suggested fix:** `log::debug!("ESP-TLS adopt of raw socket failed: {e}")` before mapping to `SocketError::Other`, consistent with `map_esp_tls_connect_error`.

All other invariants verified clean: `negotiated_version()` on both connectors (ESP-IDF real-read, Embassy honest `None`), single-process-wide `mbedtls_rs::Tls` constraint respected, `EmbassyUdpSocket` correctly omits `BindableUdpSocket`, `EspIdfUdpSocket`'s 15ms pacing sleep is real. (2 additional PLAUSIBLE findings — see the Plausible section below.)

## 15. src/mqtt/client/ + tests/mqtt_test.rs + tests/common/mock_mqtt.rs — NO CONFIRMED ISSUES (2 PLAUSIBLE, see below)

`FrameReadState`/`read_exact_packet` resumption, MQTT packet encoding, pending-buffer eviction/matching, and `tick_zombie_check`'s 10s/60s boundary arithmetic all checked correct against MQTT v3.1.1 and this crate's documented invariants.

## 16. src/mqtt/commands/ + src/mqtt/mod.rs — NO CONFIRMED ISSUES (1 PLAUSIBLE, see below)

All 18 constructors across `mod.rs`/`ams.rs`/`control.rs`/`gcode.rs`/`hardware.rs`/`status.rs` type-enforce task-ID clamping via `ClampedTaskId`; Payload+Request pattern (Key Invariant #3) followed consistently everywhere; `serde_json::to_vec` (never `to_string`) used at the one real wire-transmission call site; field names cross-checked against `reference/05_materials_ams.md`/`reference/03_mqtt_telemetry.md` and match.

## 17. src/quirks/mod.rs

### src/quirks/mod.rs:260-268 — BUG-065 (Sev1)
**Issue:** `format_z_move_gcode` does not reject `NaN` distance, so a `NaN` Z-move bypasses the travel-limit safety check and gets sent to hardware as literal G-code.
**Detail:** The guard is `distance == 0.0 || distance.abs() > z_max`; for `NaN` both comparisons are `false` (any comparison against `NaN` is `false`), so the function falls through to `format!(...)` and returns a command containing `G0 ZNaN`. Reachable from the public API: `src/client/motion.rs:150` (`move_relative`) only special-cases `distance == 0.0` before calling `relative_z_move_gcode(distance, feedrate)` → `format_z_move_gcode`. A `NaN` distance (bad upstream computation, deserialized float) sails past both guards and gets written straight to `send_gcode_raw`, transmitting a malformed command to the physical printer instead of the intended `ModelMismatch` error.
**Suggested fix:** Guard with `distance.is_nan() || distance == 0.0 || distance.abs() > z_max`.

### src/quirks/mod.rs:101-103 — BUG-066 (Sev3)
**Issue:** Default `z_max()` hardcodes `256.0` instead of a named `pub(crate) const`, inconsistent with this same file's own `FAN_STEP_MAX`/`FAN_ROUNDING_OFFSET` convention for exactly this class of literal.
**Suggested fix:** Add `pub(crate) const DEFAULT_Z_MAX_MM: f32 = 256.0;` near `FAN_STEP_MAX` and reference it from the default `z_max()` impl.

(2 additional PLAUSIBLE findings — see the Plausible section below.)

## 18. src/quirks/models/ — NO CONFIRMED ISSUES (1 PLAUSIBLE, see below)

All temperature limits, chamber-heater/sensor flags, nozzle counts, door-sensor routing, camera protocols, and FTPS TLS overrides cross-checked against `MODEL_MATRIX.csv` and `reference/04_toolhead_thermal_motion.md`/`reference/03_mqtt_telemetry.md` with no copy-paste mismatches. X1C's voltage-inverted bed ceiling matches `.claude/rules/bed-temp-voltage.md` exactly.

## 19. src/types/ + src/types/telemetry/ (ams, device, diagnostics, mod, report) — NO ISSUES FOUND

## 20. src/types/telemetry/tests.rs

### src/types/telemetry/tests.rs:1531-1539 — BUG-067 (Sev3)
**Issue:** `test_progress_field_removed` is a verbatim duplicate of `test_mc_percent_deserialization`, asserting nothing about a removed `progress` field despite its name.
**Detail:** The test body only deserializes `{"print":{"mc_percent":75}}` and asserts `mc_percent == Some(75)` — no assertion about a legacy `progress` field being absent, rejected, or ignored. A reviewer skimming test names would wrongly assume removal is guarded; it isn't.
**Suggested fix:** Delete as a duplicate, or rewrite to actually assert a stray `"progress"` key is silently ignored on deserialize.

(2 additional PLAUSIBLE findings — see the Plausible section below.)

## Plausible, Unverified Findings

None of these were promoted to a `BUG-ID` this sweep — each looked real to its reviewing agent but couldn't be fully verified (unreachable-in-practice call site, ambiguous invariant, or needs real-hardware confirmation). Flagged here for manual triage.

**Unit 1 — src/ams/**
1. `src/ams/mapping.rs:49-57,60-87` — `MaterialSource::StandardAms{ams_id,slot_id}`/`AmsHt{ams_id}` fields are public `u8`s never range-validated in `flat_channel_id()`/`to_mapping2_entry()`, unlike `parser.rs`'s consistent bounds-checking on the inbound side. A hand-built out-of-range entry could reach `validate_external_spool_safety` and be misclassified as "physical," allowing `use_ams:true` on a non-existent channel.
2. `src/ams/mapping.rs:135-136,160-161` — `build_ams_mapping`/`build_ams_mapping2` silently drop any allocation with `filament_id == 0` (0-based-index caller mistake) instead of erroring.
3. `reference/05_materials_ams.md:140` vs `src/ams/parser.rs:12`/`mapping.rs:107` — reference doc's "0 to 103" standard-AMS channel range implies up to 26 AMS units; code's `AMS_MAX_STANDARD_ID = 3` caps at 4. Unclear which is stale without real multi-AMS hardware or an authoritative cross-check.
4. `src/ams/parser.rs:98-102` — `is_type_cleared` treats `tray_type == None` the same as an explicit empty-string clearing signal, broader than its doc comment describes; untested for `state: Some(11)` (Loaded) + `tray_type: None`.

**Unit 2 — src/bin/bambino-cli/**
5. `src/bin/bambino-cli/storage.rs:100-166` — every `FilesAction` arm that uses `?` bypasses `client.disconnect().await` at the bottom of `run()` on error, skipping FTPS's graceful `QUIT`. Low-impact (`disconnect()` is documented best-effort) but not RAII-guarded the way `monitor/mod.rs`'s `TerminalGuard` is.

**Unit 6 — src/client/ (mod, types, print, storage, camera, dummy)**
6. `src/client/mod.rs:213-260` (`from_mqtt`) — hardcodes `ip`/`access_code` to empty strings with no way to set them afterward; nothing stops a caller from later chaining `.with_ftps()`/`.with_camera()`, which then fail opaquely (empty-host `TcpStream::connect`, empty-access-code camera auth that "succeeds" per the documented ambiguous-ack behavior and only fails later on `read_next_frame()`).

**Unit 7 — tests/client_test.rs**
7. `tests/client_test.rs:2717` — `attach_camera()`/`disconnect_camera()` are never exercised by any test despite a comment citing them as prior art.
8. `tests/client_test.rs:371` — `set_fan_speed`'s `speed_percent > 100` clamp path is never tested.

**Unit 9 — src/discovery/**
9. `src/discovery/parser.rs:182,186` — `httparse::parse(buf).ok()?` discards the `Status::Partial` vs `Status::Complete` distinction, so a truncated SSDP packet can still parse successfully.
10. `src/discovery/parser.rs:178` — `is_response`'s status-line check only recognizes exact-case `"HTTP/"`/`"http/"`, inconsistent with this file's otherwise-thorough case-insensitive header handling.
11. `src/discovery/parser.rs:68` — `parse_location`'s port parsing silently coerces any unparseable port string to `80`, indistinguishable from "no port specified."

**Unit 11 — src/ftps/ (client, mod)**
12. `src/ftps/client.rs:521` — `list_directory`'s final-reply check rejects `426` (`FTP_TRANSFER_ABORTED`) unconditionally, unlike `upload_file`/`download_file`, which both tolerate it and recheck independently. No SIZE-equivalent recheck exists for a directory listing, so this may be an intentional gap.
13. `src/ftps/client.rs` — README's "safe despite fail-open" justification for `allow_unverified_tls_1_2` (SIZE recheck / exact-226 requirement) doesn't cover `list_directory`, which has no independent truncation check at all.

**Unit 12 — src/ftps/ (parser, protocol)**
14. `src/ftps/protocol.rs:373-380` — `validate_ftp_path`'s leading-dash check uses `.split(['/','\\']).next_back()`, which returns `""` (not the real last component) for a trailing-slash path, potentially missing a dash-prefixed final directory component. No current call site builds a trailing-slash path, so not confirmed reachable.
15. `src/ftps/protocol.rs:360,239-345` — control-character boundary bytes (`0x20`/`0x7F`) and repeated raw reply-code offsets (`3`/`4`) aren't extracted to named `pub(crate) const`s, unlike the rest of this file's literals.
16. `src/ftps/parser.rs:118-126` — filename reconstruction via `name_tokens.join(" ")` collapses multiple consecutive spaces in a real filename to one, contradicting the struct doc's "preserving single spaces" claim.

**Unit 14 — src/io/ (esp_idf, embassy)**
17. `src/io/esp_idf.rs:84,498,698` — three more `EspIdfTimer::new()` failure sites (UDP pacing, TCP connect, TLS connect timers) discard the real `EspError` with no `log::debug!`, same shape as the CONFIRMED `adopt()` finding (BUG-064). Ties into hardware-only resource-exhaustion territory (`esp32-hw-probe/`), not self-verifiable from source.
18. `src/io/esp_idf.rs:680-712` — `EspIdfTlsConnector::with_connect_timeout(Duration::ZERO)` fails immediately on the first would-block poll rather than disabling the timeout, inconsistent with the crate's other "0 = disabled" convention (`connect_timeout_secs`/`set_command_timeout`). Not confirmed as a written invariant for this specific field.

**Unit 15 — src/mqtt/client/**
19. `src/mqtt/client/mod.rs:311` — `publish_command` unconditionally rearms the 10s write-zombie timer on every call, so frequent command dispatch (well within normal interactive/print-job usage) can mask a dead connection indefinitely, leaving detection to the much longer 60s staleness check.
20. `tests/common/mock_mqtt.rs:52` — `read_packet` is not cancellation-safe yet is raced inside `tokio::select!` in `run_mock_mqtt_broker`; today's tests avoid the race window via an explicit ack handshake, but a future test firing an injection concurrently with a client write (no ordering handshake) could desync the mock's parser silently.

**Unit 16 — src/mqtt/commands/**
21. `src/mqtt/commands/print_job.rs:245` — `subtask_id` is clamped via a direct ad hoc call to the free `clamp_task_id()` function instead of the `ClampedTaskId` newtype every other constructor uses — functionally correct today (and this is the exact call site that produced BUG-001 previously) but has no type-system backstop against a future edit reintroducing the miss.

**Unit 18 — src/quirks/models/**
22. `src/quirks/models/p2.rs` — `supports_auxiliary_right_fan()`/`auxiliary_fan_uses_percentage()` are both `true` for P2S, but the only reference-doc citation for percentage-encoded aux-fan telemetry (`[REF-CLIM-FANS]`) names X2D exclusively; `MODEL_MATRIX.csv` doesn't distinguish the primary vs. secondary aux fan, so it can't independently confirm. If P2S actually uses standard step encoding, a consumer would misread e.g. step `8` as `8%` instead of ~53%.

**Unit 20 — src/types/telemetry/tests.rs**
23. `src/types/telemetry/tests.rs:1786-1826` — no test verifies `bed_temperatures()`/`decode_bed_temperatures()` ignores `device.bed_temp` (confirmed-redundant per BUG-054) rather than falling back to it; a regression reintroducing that fallback would go uncaught.
24. `src/types/telemetry/tests.rs:324-381` — no test exercises a composite-packed (`>500`) `chamber_temper` value through `unpack_temperature()`; only a direct/idle value is covered.

---

`BACKLOG.md` is the status source of truth from here on — the table below is a point-in-time snapshot from this sweep and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | File(s) | One-line |
|---|---|---|---|---|
| BUG-056 | Sev2 | diagnostics | client/connect.rs, diagnostics/kprofile.rs | `k_profile_primed` not reset on `disconnect_mqtt()`, reconnect hangs on priming |
| BUG-057 | Sev3 | bambino-cli | bin/bambino-cli/storage.rs | `run_clock_check` drops cleanup-delete error when listing also fails |
| BUG-058 | Sev3 | bambino-cli | bin/bambino-cli/main.rs | `control ams` usage lines omit `<IP> <SERIAL> [ACCESS_CODE]` |
| BUG-059 | Sev3 | tests | tests/client_test.rs | No test asserts `extrude_cali_flag` wire field |
| BUG-060 | Sev2 | discovery | discovery/parser.rs | `parse_ssdp_payload` accepts non-Bambu SSDP devices |
| BUG-061 | Sev3 | ftps | ftps/parser.rs | `day`/`hour`/`minute` unvalidated in `parse_unix_listing` |
| BUG-062 | Sev3 | ftps | ftps/protocol.rs | `read_response` silently drops non-conformant header line |
| BUG-063 | Sev3 | io | io/embassy.rs | `EmbassyTlsConnector::connect` collapses all TLS errors, no logging |
| BUG-064 | Sev3 | io | io/esp_idf.rs | `EspTls::adopt()` failure discards real `EspError`, no logging |
| BUG-065 | Sev1 | quirks | quirks/mod.rs | `format_z_move_gcode` doesn't reject `NaN`, sends `G0 ZNaN` to hardware |
| BUG-066 | Sev3 | quirks | quirks/mod.rs | Default `z_max()` hardcodes `256.0` instead of a named const |
| BUG-067 | Sev3 | tests | types/telemetry/tests.rs | `test_progress_field_removed` is a duplicate, tests nothing about `progress` |
