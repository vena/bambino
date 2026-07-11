**Status:** COMPLETE

# bambino Deep Review — 2026-07-11

Full-crate correctness sweep of the `bambino` async Rust printer-control library (host/ESP-IDF/Embassy targets). Methodology: the crate's `src/` and `tests/` trees were partitioned into 20 review units along module/concern boundaries (splitting large modules like `ftps`, `io`, `client`, `quirks`, `types/telemetry` in two where a single agent couldn't deeply read everything; merging small/thin directories together). One `general-purpose` subagent was spawned per unit, in parallel, each with only its own file list, the relevant `.claude/rules/*.md` invariants and nested `CLAUDE.md` files, and root `CLAUDE.md`/`README.md` in full.

**Scope exclusions (per the `deep-review` skill's standing policy for this crate):**
- Minor security issues are out of scope — this crate is explicitly LAN-only by design (see `README.md`'s Safety Notice). Cert-verification bypass, plaintext fallback, etc. are not flagged unless implemented incorrectly vs. their *own* stated behavior.
- Style/refactor suggestions and naming preferences are out of scope, except where a name actively misrepresents behavior (inverted boolean sense, a function that does the opposite of what it claims) — that's a correctness/footgun risk, not style.
- Abstraction/pattern-duplication commentary is only in scope when it's root-cause context on a `CONFIRMED` bug already found (an invariant enforced only by convention across similar call sites, where a bug already exists because of it) — not as standalone architecture criticism.

**Confidence tiers:** each finding is tagged `CONFIRMED` (agent is sure it's a real bug) or `PLAUSIBLE` (looks real but couldn't be fully verified — e.g. can't confirm the failure path triggers, or the invariant it'd violate is itself ambiguous). `CONFIRMED` findings were promoted to a `BUG-ID` in `BACKLOG.md` during the original sweep; `PLAUSIBLE` findings were collected in their own section below. A follow-up triage pass (2026-07-11, see that section's own note) subsequently promoted 15 more of those `PLAUSIBLE` findings to `BUG-ID`s after cross-checking them against source where possible — the per-unit counts below reflect the original sweep's tally, not that later triage; the Plausible section itself is the current source of truth for what's still actually open.

This file is meant to be read standalone by a fresh session with no other context from the sweep that produced it. **Caveat:** file:line references may have drifted if other commits landed on `main` since this sweep ran — verify against current source before acting on a finding.

## 1. src/ams/ — NO CONFIRMED ISSUES AT SWEEP TIME (4 PLAUSIBLE found; follow-up triage later promoted 3 of them to BUG-068/069/070 — see Plausible section)

## 2. src/bin/bambino-cli/ (control, storage, probe, monitor)

### src/bin/bambino-cli/storage.rs:200-227 — BUG-057 (Sev3)
**Issue:** `run_clock_check`'s cleanup-delete error is silently dropped when the preceding directory listing also fails.
**Detail:** `delete_result` (line ~204) is computed unconditionally, but if `listing` (line ~200) errors, `listing?` (line ~206) returns before `delete_result?` (line ~226) is ever checked — despite an in-code comment promising the probe file is always cleaned up. A failed cleanup on this path leaves `/bambino_clock_probe.txt` orphaned on the printer's SD card with no diagnostic.
**Suggested fix:** Check `delete_result` too when `listing` errors and fold both failures into the reported error.

(1 additional PLAUSIBLE finding at sweep time, since promoted to BUG-071 — see the Plausible section below.)

## 3. src/bin/bambino-cli/ (main, connection, discover, camera, table, cert/tls)

### src/bin/bambino-cli/main.rs:116 — BUG-058 (Sev3)
**Issue:** `control`'s `override_usage` text drops `<IP> <SERIAL> [ACCESS_CODE]` on all three `ams` sub-subcommand lines, same class as BUG-016/commit 68f380f.
**Detail:** Every other line in this usage string correctly shows `bambino-cli control <IP> <SERIAL> [ACCESS_CODE] <action>...`, but the three `ams dry`/`ams dry-stop`/`ams help` lines omit the prefix, so a user copying the shown invocation gets a clap parse error.
**Suggested fix:** Prefix all three `ams` lines with `bambino-cli control <IP> <SERIAL> [ACCESS_CODE] `.

No other findings (CONFIRMED or PLAUSIBLE) in this unit — `connection.rs`, `discover.rs`, `inspect_cert.rs`, `verify_tls.rs`, `camera.rs`, `table.rs` all checked clean, including no access-code logging anywhere in `src/bin/bambino-cli/`.

## 4. src/camera/ + tests/camera_test.rs — NO ISSUES FOUND

## 5. src/client/ (connect, telemetry, ams, motion, hardware, thermal) — NO ISSUES FOUND

## 6. src/client/ (mod, types, print, storage, camera, dummy) — NO CONFIRMED ISSUES AT SWEEP TIME (1 PLAUSIBLE found, since promoted to BUG-072 — see Plausible section)

## 7. tests/client_test.rs

### tests/client_test.rs:783 — BUG-059 (Sev3)
**Issue:** No test asserts the `extrude_cali_flag` (flow calibration) wire field.
**Detail:** README documents flow calibration as a `PrintJobConfig` default that runs automatically. `test_start_print_wire_payload` and every other print-job test assert `bed_leveling`/`vibration_cali` but never `extrude_cali_flag`, and no test calls `.flow_calibration(false)` to check the field flips to `0`. A regression in `ProjectFileRequest::from_config` (src/mqtt/commands/print_job.rs:251) would ship undetected.
**Suggested fix:** Add `assert_eq!(json["print"]["extrude_cali_flag"], 1)` to `test_start_print_wire_payload`, plus a dedicated test for `.flow_calibration(false)` asserting `0`.

(2 additional PLAUSIBLE findings at sweep time, both since promoted to BUG-073/074 — see the Plausible section below.)

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

(3 additional PLAUSIBLE findings at sweep time: 1 since promoted to BUG-075, 2 given other dispositions on follow-up triage — see the Plausible section below.)

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

All other invariants verified clean: `negotiated_version()` on both connectors (ESP-IDF real-read, Embassy honest `None`), single-process-wide `mbedtls_rs::Tls` constraint respected, `EmbassyUdpSocket` correctly omits `BindableUdpSocket`, `EspIdfUdpSocket`'s 15ms pacing sleep is real. (2 additional PLAUSIBLE findings at sweep time, both since promoted to BUG-076/077 — see the Plausible section below.)

## 15. src/mqtt/client/ + tests/mqtt_test.rs + tests/common/mock_mqtt.rs — NO CONFIRMED ISSUES AT SWEEP TIME (2 PLAUSIBLE found, both since promoted to BUG-078/079 — see Plausible section)

`FrameReadState`/`read_exact_packet` resumption, MQTT packet encoding, pending-buffer eviction/matching, and `tick_zombie_check`'s 10s/60s boundary arithmetic all checked correct against MQTT v3.1.1 and this crate's documented invariants.

## 16. src/mqtt/commands/ + src/mqtt/mod.rs — NO CONFIRMED ISSUES AT SWEEP TIME (1 PLAUSIBLE found, since promoted to BUG-080 — see Plausible section)

All 18 constructors across `mod.rs`/`ams.rs`/`control.rs`/`gcode.rs`/`hardware.rs`/`status.rs` type-enforce task-ID clamping via `ClampedTaskId`; Payload+Request pattern (Key Invariant #3) followed consistently everywhere; `serde_json::to_vec` (never `to_string`) used at the one real wire-transmission call site; field names cross-checked against `reference/05_materials_ams.md`/`reference/03_mqtt_telemetry.md` and match.

## 17. src/quirks/mod.rs

### src/quirks/mod.rs:260-268 — BUG-065 (Sev1)
**Issue:** `format_z_move_gcode` does not reject `NaN` distance, so a `NaN` Z-move bypasses the travel-limit safety check and gets sent to hardware as literal G-code.
**Detail:** The guard is `distance == 0.0 || distance.abs() > z_max`; for `NaN` both comparisons are `false` (any comparison against `NaN` is `false`), so the function falls through to `format!(...)` and returns a command containing `G0 ZNaN`. Reachable from the public API: `src/client/motion.rs:150` (`move_relative`) only special-cases `distance == 0.0` before calling `relative_z_move_gcode(distance, feedrate)` → `format_z_move_gcode`. A `NaN` distance (bad upstream computation, deserialized float) sails past both guards and gets written straight to `send_gcode_raw`, transmitting a malformed command to the physical printer instead of the intended `ModelMismatch` error.
**Suggested fix:** Guard with `distance.is_nan() || distance == 0.0 || distance.abs() > z_max`.

### src/quirks/mod.rs:101-103 — BUG-066 (Sev3)
**Issue:** Default `z_max()` hardcodes `256.0` instead of a named `pub(crate) const`, inconsistent with this same file's own `FAN_STEP_MAX`/`FAN_ROUNDING_OFFSET` convention for exactly this class of literal.
**Suggested fix:** Add `pub(crate) const DEFAULT_Z_MAX_MM: f32 = 256.0;` near `FAN_STEP_MAX` and reference it from the default `z_max()` impl.

(2 additional PLAUSIBLE findings at sweep time — both remain open, see the Plausible section below.)

## 18. src/quirks/models/ — NO CONFIRMED ISSUES AT SWEEP TIME (1 PLAUSIBLE found, resolved not-a-bug on follow-up triage — see Plausible section)

All temperature limits, chamber-heater/sensor flags, nozzle counts, door-sensor routing, camera protocols, and FTPS TLS overrides cross-checked against `MODEL_MATRIX.csv` and `reference/04_toolhead_thermal_motion.md`/`reference/03_mqtt_telemetry.md` with no copy-paste mismatches. X1C's voltage-inverted bed ceiling matches `.claude/rules/bed-temp-voltage.md` exactly.

## 19. src/types/ + src/types/telemetry/ (ams, device, diagnostics, mod, report) — NO ISSUES FOUND

## 20. src/types/telemetry/tests.rs

### src/types/telemetry/tests.rs:1531-1539 — BUG-067 (Sev3)
**Issue:** `test_progress_field_removed` is a verbatim duplicate of `test_mc_percent_deserialization`, asserting nothing about a removed `progress` field despite its name.
**Detail:** The test body only deserializes `{"print":{"mc_percent":75}}` and asserts `mc_percent == Some(75)` — no assertion about a legacy `progress` field being absent, rejected, or ignored. A reviewer skimming test names would wrongly assume removal is guarded; it isn't.
**Suggested fix:** Delete as a duplicate, or rewrite to actually assert a stray `"progress"` key is silently ignored on deserialize.

(2 additional PLAUSIBLE findings at sweep time, both since promoted to BUG-081/082 — see the Plausible section below.)

## Plausible, Unverified Findings

None of these were promoted to a `BUG-ID` during the original sweep — each looked real to its reviewing agent but couldn't be fully verified from source alone (unreachable-in-practice call site, ambiguous invariant, or needs real-hardware confirmation). A follow-up triage pass (2026-07-11) then cross-checked the wire-protocol-relevant subset against `pybambu` and `bambuddy` (independent Bambu Lab printer implementations) and re-read bambino's own source for the rest: 15 were promoted to `BUG-069`-`BUG-082` (struck through below, left in place for their reasoning), 1 was resolved as not-a-bug (`p2.rs`'s aux fan, #22), and 8 remain genuinely unverifiable — no available source settles them either way, still open for manual triage (marked **Unresolved** below).

**Unit 1 — src/ams/**
1. ~~`src/ams/mapping.rs:49-57,60-87` — `MaterialSource::StandardAms{ams_id,slot_id}`/`AmsHt{ams_id}` unvalidated.~~ **PROMOTED to BUG-069 (Sev3).** Confirmed by direct read: `MaterialSource` is `pub enum` and `build_ams_mapping`/`build_ams_mapping2` are `pub fn` — genuinely reachable public API, not internal-only. Real footgun for a misbehaving external caller.
2. ~~`src/ams/mapping.rs:135-136,160-161` — `build_ams_mapping`/`build_ams_mapping2` silently drop `filament_id == 0`.~~ **PROMOTED to BUG-070 (Sev3).** Same reachability confirmation as #1 — both functions are `pub`.
3. ~~`reference/05_materials_ams.md:140` vs `src/ams/parser.rs:12`/`mapping.rs:107` — `AMS_MAX_STANDARD_ID` doc/code mismatch.~~ **PROMOTED to BUG-068 (Sev2).** 3rd-party cross-check (2026-07-11) against `bambuddy/backend/app/models/spoolman_slot_assignment.py`'s `ck_ams_id_range` CHECK constraint: standard AMS spans `ams_id` 0-7 (8 units), widened from 0-3 in bambuddy's own issue #1274 specifically because real H2C/H2D hardware exceeded the old cap. `pybambu`'s `tray_now >> 2` decode (models.py) derives the AMS index dynamically with no hardcoded cap either. bambino's `AMS_MAX_STANDARD_ID: u8 = 3` (4 units) is confirmed too low — real multi-AMS setups beyond 4 units get silently misclassified as non-standard/external by `evaluate_spool_presence`/`resolve_global_tray_id`/`flat_channel_id_for_entry`. Fix: raise to at least `7` per bambuddy's verified value (neither source confirms the reference doc's own "0 to 103"/26-unit claim, so that doc still needs its own correction pass separately, but a documented `AMS_MAX_STANDARD_ID` of `3` is now confirmed wrong).
4. `src/ams/parser.rs:98-102` — `is_type_cleared` treats `tray_type == None` the same as an explicit empty-string clearing signal, broader than its doc comment describes; untested for `state: Some(11)` (Loaded) + `tray_type: None`.

**Unit 2 — src/bin/bambino-cli/**
5. ~~`src/bin/bambino-cli/storage.rs:100-166` — FTPS disconnect skipped on early-return error paths.~~ **PROMOTED to BUG-071 (Sev3).** Confirmed by direct read of `run()`: every `FilesAction` arm's `?` returns before line 173's `client.disconnect().await`.

**Unit 6 — src/client/ (mod, types, print, storage, camera, dummy)**
6. ~~`src/client/mod.rs:213-260` (`from_mqtt`) — empty ip/access_code reachable via later `.with_ftps()`/`.with_camera()`.~~ **PROMOTED to BUG-072 (Sev3).** Confirmed by direct read: `with_ftps()`/`with_camera()` are unconstrained generic builder methods on `PrinterClient<...>`, no type-level distinction for `from_mqtt()`-constructed instances.

**Unit 7 — tests/client_test.rs**
7. ~~`tests/client_test.rs:2717` — `attach_camera()`/`disconnect_camera()` never tested.~~ **PROMOTED to BUG-073 (Sev3).** Confirmed by grep: zero call sites in `client_test.rs`/`camera_test.rs`, only a comment citing them as prior art.
8. ~~`tests/client_test.rs:371` — `set_fan_speed`'s `>100` clamp path never tested.~~ **PROMOTED to BUG-074 (Sev3).** Confirmed by grep: all `set_fan_speed` calls in the test file use values `<=100`.

**Unit 9 — src/discovery/**
9. `src/discovery/parser.rs:182,186` — `httparse::parse(buf).ok()?` discards the `Status::Partial` vs `Status::Complete` distinction, so a truncated SSDP packet can still parse successfully. **Unresolved** — neither `pybambu` nor `bambuddy` implement a comparable two-phase HTTP-style parser to cross-check against; still open for manual triage.
10. `src/discovery/parser.rs:178` — `is_response`'s status-line check only recognizes exact-case `"HTTP/"`/`"http/"`, inconsistent with this file's otherwise-thorough case-insensitive header handling. **Unresolved** — neither 3rd-party source records having seen non-canonical-case status lines from real firmware; no signal either way.
11. `src/discovery/parser.rs:68` — `parse_location`'s port parsing silently coerces any unparseable port string to `80`, indistinguishable from "no port specified." **Unresolved** — checked `bambuddy/backend/app/services/discovery.py`: it never parses `LOCATION`'s port at all (verifies printers via fixed known ports 990/8883 instead), so it offers no comparable logic to confirm or refute against.

**Unit 11 — src/ftps/ (client, mod)**
12. `src/ftps/client.rs:521` — `list_directory`'s final-reply check rejects `426` unconditionally, unlike `upload_file`/`download_file`. **Unresolved** — checked `bambuddy/backend/app/services/bambu_ftp.py`: its only documented 426-tolerance comment is for `STOR` (upload), silent on `LIST`; inconclusive either way without a real P2S/X2D wire capture.
13. `src/ftps/client.rs` — README's "safe despite fail-open" justification doesn't cover `list_directory`. **Unresolved**, same reason as #12 — no independent evidence either way from either 3rd-party source.

**Unit 12 — src/ftps/ (parser, protocol)**
14. ~~`src/ftps/protocol.rs:373-380` — `validate_ftp_path`'s leading-dash check bypassed by a trailing-slash path.~~ **PROMOTED to BUG-075 (Sev3).** Confirmed by direct read: `.split(['/','\\']).next_back()` genuinely returns `""` for a trailing-slash path; `create_directory`/`remove_directory` are public API, so the path is externally reachable.
15. `src/ftps/protocol.rs:360,239-345` — control-character boundary bytes (`0x20`/`0x7F`) and repeated raw reply-code offsets (`3`/`4`) aren't extracted to named `pub(crate) const`s, unlike the rest of this file's literals.
16. `src/ftps/parser.rs:118-126` — filename reconstruction via `name_tokens.join(" ")` collapses multiple consecutive spaces in a real filename to one, contradicting the struct doc's "preserving single spaces" claim.

**Unit 14 — src/io/ (esp_idf, embassy)**
17. ~~`src/io/esp_idf.rs:84,498,698` — 3 more `EspIdfTimer::new()` sites discard the real `EspError`.~~ **PROMOTED to BUG-076 (Sev3).** Confirmed by direct read of lines 83-85 (UDP pacing) — matches the described pattern exactly; the resource-exhaustion-under-load question itself stays hardware-only (`esp32-hw-probe/`), but the missing-log-line defect is confirmed from source alone.
18. ~~`src/io/esp_idf.rs:680-712` — `with_connect_timeout(Duration::ZERO)` fails immediately.~~ **PROMOTED to BUG-077 (Sev3).** Confirmed by direct read: the elapsed check (`saturating_sub(start) >= self.connect_timeout.as_millis() as u64`) is true after the very first would-block poll when the budget is `0`. Same root-cause class as already-fixed BUG-007 (0 should mean "disabled", not "instant") — treating as confirmed by analogy to that precedent.

**Unit 15 — src/mqtt/client/**
19. ~~`src/mqtt/client/mod.rs:311` — `publish_command` unconditionally rearms the zombie timer.~~ **PROMOTED to BUG-078 (Sev3).** Confirmed by direct read: `self.write_pending_secs = Some(0);` runs unconditionally on every call with no check against already-in-flight commands.
20. ~~`tests/common/mock_mqtt.rs:52` — `read_packet` not cancellation-safe when raced in `select!`.~~ **PROMOTED to BUG-079 (Sev3).** Confirmed by re-reading `read_packet`'s sequential multi-await structure against `run_mock_mqtt_broker`'s `select!` usage — genuinely not cancellation-safe, test-infra fragility risk stands.

**Unit 16 — src/mqtt/commands/**
21. ~~`src/mqtt/commands/print_job.rs:245` — `subtask_id` clamped via ad hoc `clamp_task_id()` instead of `ClampedTaskId`.~~ **PROMOTED to BUG-080 (Sev3).** Confirmed by direct read: line 245 calls the free function while `sequence_id` (line 209, same constructor) uses `impl Into<ClampedTaskId>` — genuine type-safety inconsistency left over from the BUG-053 migration.

**Unit 18 — src/quirks/models/**
22. ~~`src/quirks/models/p2.rs` — `supports_auxiliary_right_fan()`/`auxiliary_fan_uses_percentage()` possibly X2D-only.~~ **RESOLVED, not a bug.** 3rd-party cross-check (2026-07-11) against `pybambu/models.py`: `Features.SECONDARY_AUX_FAN` explicitly returns `model in (p2_printers | x2_printers)` (line ~302), i.e. pybambu treats P2S and X2D identically for the percentage-encoded `airduct.parts id:160` secondary aux fan — the same decode path, no P2S-specific branch. bambino's `p2.rs` quirks (`true`/`true`) match this independently-maintained, actively-used source. `reference/04_toolhead_thermal_motion.md`'s `[REF-CLIM-FANS]` section is the one that's incomplete (names X2D only) — worth a doc-only correction to add P2S, but the code itself is correct.

**Unit 20 — src/types/telemetry/tests.rs**
23. ~~`src/types/telemetry/tests.rs:1786-1826` — no test verifies `bed_temperatures()` ignores `device.bed_temp`.~~ **PROMOTED to BUG-081 (Sev3).** Production behavior already confirmed correct against `pybambu` (reads only `bed.info.temp`, never `bed_temp`) — this is now a pure test-coverage gap, not a correctness question.
24. ~~`src/types/telemetry/tests.rs:324-381` — no test exercises composite-packed `chamber_temper`.~~ **PROMOTED to BUG-082 (Sev3).** Pure test-coverage gap; `unpack_temperature()`'s decode logic itself is already generically tested and unaffected.

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
| BUG-068 | Sev2 | ams | ams/parser.rs, ams/mapping.rs | `AMS_MAX_STANDARD_ID = 3` too low, 3rd-party-confirmed against bambuddy/pybambu |
| BUG-069 | Sev3 | ams | ams/mapping.rs | `MaterialSource` fields unvalidated in `flat_channel_id()`/`to_mapping2_entry()`, public API |
| BUG-070 | Sev3 | ams | ams/mapping.rs | `build_ams_mapping`/`build_ams_mapping2` silently drop `filament_id == 0` |
| BUG-071 | Sev3 | bambino-cli | bin/bambino-cli/storage.rs | FTPS `disconnect()` skipped on every early-return error path in `run()` |
| BUG-072 | Sev3 | client | client/mod.rs | `from_mqtt()`'s empty ip/access_code reachable via later `.with_ftps()`/`.with_camera()` |
| BUG-073 | Sev3 | tests | tests/client_test.rs | `attach_camera()`/`disconnect_camera()` never tested |
| BUG-074 | Sev3 | tests | tests/client_test.rs | `set_fan_speed`'s `>100` clamp path never tested |
| BUG-075 | Sev3 | ftps | ftps/protocol.rs | `validate_ftp_path`'s dash check bypassed by a trailing-slash path |
| BUG-076 | Sev3 | io | io/esp_idf.rs | 3 more `EspIdfTimer::new()` sites discard `EspError`, no logging (companion to BUG-064) |
| BUG-077 | Sev3 | io | io/esp_idf.rs | `EspIdfTlsConnector` `connect_timeout=ZERO` fails instantly instead of disabling (same class as BUG-007) |
| BUG-078 | Sev3 | mqtt | mqtt/client/mod.rs | `publish_command` unconditionally rearms zombie timer every call |
| BUG-079 | Sev3 | tests | tests/common/mock_mqtt.rs | `read_packet` not cancellation-safe when raced in `select!` |
| BUG-080 | Sev3 | mqtt | mqtt/commands/print_job.rs | `subtask_id` clamped ad hoc instead of via `ClampedTaskId` |
| BUG-081 | Sev3 | tests | types/telemetry/tests.rs | No test verifies `bed_temperatures()` ignores `device.bed_temp` |
| BUG-082 | Sev3 | tests | types/telemetry/tests.rs | No test exercises composite-packed `chamber_temper` |
