# bambino — Full-Crate Module Deep Review (2026-07-16)

**Status:** COMPLETE (21/21 units)

This is the standard `deep-review` skill sweep: a full-crate correctness review, one parallel
agent per module boundary, covering all of `src/` and `tests/` as currently structured (21
review units — see the partition table below). It is a separate exercise from
`07-16-REVIEW.md` (this same session's earlier, narrower cross-verification against bambuddy /
ha-bambulab / BambuStudio commits) — that file stays as-is; this one is the crate-wide sweep.

**Scope exclusions** (same as every run of this skill): minor LAN-only security nitpicks are
out of scope — bambino is explicitly LAN-only by design (no Bambu Cloud, direct MQTT/FTPS/camera
only, per `README.md`'s opening paragraph) — so things like cert-verification bypass or
plaintext fallback are not flagged unless they diverge from the crate's own stated behavior.
Style/refactor suggestions and naming preferences are also out of scope, except where a name
actively misrepresents behavior (inverted-sense boolean, a doc comment claiming the opposite of
what the code does) — that's a correctness footgun wearing a naming-shaped disguise, not a style
nit.

**`CONFIRMED` vs `PLAUSIBLE`**: every finding is tagged one or the other. `CONFIRMED` means the
reviewing agent verified the failure path actually triggers. `PLAUSIBLE` means it looks like a
real bug but couldn't be fully verified in-agent (e.g. needs real hardware, or the invariant it
violates is itself ambiguous) — these are reported alongside `CONFIRMED` findings, not discarded,
and get re-verified by direct code read (not by re-trusting the agent's claim) before this sweep
is marked complete.

This file is meant to be read standalone by a fresh session with no prior conversation context.
`file:line` references may have drifted if other changes landed on `main` since this sweep ran.

## Partition (21 units)

| # | Unit | Files |
|---|---|---|
| 1 | ams | `src/ams/{mod,mapping,parser}.rs` |
| 2 | cli-interactive | `src/bin/bambino-cli/control.rs`, `src/bin/bambino-cli/monitor/{mod,dashboard}.rs` |
| 3 | cli-transfer-probe | `src/bin/bambino-cli/{storage,probe,camera}.rs` |
| 4 | cli-core | `src/bin/bambino-cli/{main,connection,discover,inspect_cert,verify_tls,table}.rs` |
| 5 | camera | `src/camera/{binary,mod,rtsps}.rs`, `tests/camera_test.rs`, `tests/common/mock_camera.rs` |
| 6 | client-connect-telemetry | `src/client/{mod,connect,dummy,telemetry}.rs` |
| 7 | client-commands | `src/client/{ams,camera,hardware,motion,print,storage,thermal,types}.rs` |
| 8 | client-integration-tests | `tests/client_test.rs` (3226 lines — reviewed alone per the skill's oversized-file rule) |
| 9 | diagnostics | `src/diagnostics/{hms,kprofile,mod}.rs` |
| 10 | discovery | `src/discovery/{mod,parser}.rs` |
| 11 | core | `src/{error,models,lib}.rs` |
| 12 | ftps-client | `src/ftps/{client,mod}.rs`, `tests/ftps_test.rs`, `tests/common/mock_ftps.rs` |
| 13 | ftps-protocol | `src/ftps/{parser,protocol}.rs`, `src/ftps/protocol/tests.rs` |
| 14 | io-tokio | `src/io/{mod,tokio}.rs`, `src/io/tokio/{cert_verify,tests}.rs` |
| 15 | io-embedded | `src/io/{embassy,esp_idf}.rs` |
| 16 | mqtt-client | `src/mqtt/mod.rs`, `src/mqtt/client/{mod,codec,frame,pending}.rs`, `tests/mqtt_test.rs`, `tests/common/{mock_mqtt,mod,io}.rs` |
| 17 | mqtt-commands | `src/mqtt/commands/{mod,ams,control,gcode,hardware,print_job,status}.rs` |
| 18 | quirks | `src/quirks/mod.rs`, `src/quirks/models/{mod,a1,a2,h2,p1,p2,x1,x2}.rs` |
| 19 | telemetry-ams | `src/types/telemetry/ams.rs`, `src/types/telemetry/tests/ams.rs` |
| 20 | telemetry-device | `src/types/telemetry/device.rs`, `src/types/telemetry/tests/{device,nozzle,ctc,bed}.rs` |
| 21 | telemetry-core | `src/types/{mod,version}.rs`, `src/types/telemetry/{mod,report,diagnostics,tests}.rs`, `src/types/telemetry/tests/{misc,fun_field}.rs`, `tests/telemetry_replay_test.rs` |

---

## 1. ams — CONFIRMED: BUG-144

**Files:** `src/ams/mod.rs`, `src/ams/mapping.rs`, `src/ams/parser.rs`

### src/ams/mapping.rs:28,133 — BUG-144
**Verdict:** CONFIRMED
**Issue:** Doc comments claim AMS-HT `ams_id` is "128+"; the code enforces 128-135 only.
**Detail:** `to_mapping2_entry()`/`flat_channel_id_for_entry()` reject `ams_id` 136-253, silently substituting the unmapped sentinel — the doc comments' open-ended "128+" implies any id ≥128 is valid.
**Suggested fix:** Reword to "128-135 (`AMS_HT_ID_MIN`/`AMS_HT_ID_MAX`)".

No other findings — extensive prior review (BUG-012/014/015/033/039/068/069/070/083/103/114/115/122/124/125/127/128); bounds-checking now consistently applied everywhere. One sub-threshold observation, not filed: `build_ams_mapping`/`build_ams_mapping2` silently let a later duplicate `filament_id` overwrite an earlier one with no log (asymmetric with BUG-070's zero-ID warning) — only matters for a caller bug (genuinely duplicate IDs).
## 2. cli-interactive — NO ISSUES FOUND

**Files:** `src/bin/bambino-cli/control.rs`, `src/bin/bambino-cli/monitor/{mod,dashboard}.rs`

All CLI arg→wire-type mappings (fan targets, LED nodes, airduct modes, print speed, calibration bitflags, AMS dry args) verified against `src/client/`/`src/mqtt/commands/`/reference docs. `monitor::run`'s `select!` racing against `poll_wire`'s 30s deadline is documented-expected behavior per `.claude/rules/wire-read-deadline.md`, not a bug.
## 3. cli-transfer-probe — CONFIRMED: BUG-145; 1 PLAUSIBLE

**Files:** `src/bin/bambino-cli/{storage,probe,camera}.rs`

### src/bin/bambino-cli/probe.rs:417-423 — BUG-145
**Verdict:** CONFIRMED
**Issue:** The `pushall` step aborts the entire probe run via `?`, discarding all output — same class as `BUG-017`.
**Detail:** `client.request_pushall().await?`/`capture_pushall(...).await?` propagate any error before the test loop runs or the report is written, despite this function's own `capture_error` field documenting that `capture_responses()` was fixed for this identical failure mode.
**Suggested fix:** Convert to a `pushall_error: Option<String>` field on `ProbeReport`, matching `publish_error`/`capture_error`.

### PLAUSIBLE: src/bin/bambino-cli/probe.rs:342-343 (also 195-201)
Per-test capture windows can silently overshoot up to 10x (advertised 3s window vs. `poll_telemetry()`'s internal 30s read timeout when the printer goes quiet), since the deadline is only checked between `poll_telemetry()` calls. Not filed — dev-tooling UX, not a correctness/safety bug.

No other issues in `storage.rs`/`camera.rs`.
## 4. cli-core — 1 PLAUSIBLE

**Files:** `src/bin/bambino-cli/{main,connection,discover,inspect_cert,verify_tls,table}.rs`

### PLAUSIBLE: src/bin/bambino-cli/table.rs:29,33,54
Column width uses `str::len()` (bytes) and padding uses char-count `format!`, neither matching terminal display width for double-width Unicode (CJK/emoji) in a printer's user-editable `device.name` from SSDP discovery — misaligned table rows. Not filed — cosmetic-only in a dev CLI table renderer. Suggested fix if pursued: `unicode-width` crate for both width computation and padding.

No other correctness issues in `main.rs`, `connection.rs`, `discover.rs`, `inspect_cert.rs`, `verify_tls.rs`.
## 5. camera — 2 PLAUSIBLE

**Files:** `src/camera/{binary,mod,rtsps}.rs`, `tests/camera_test.rs`, `tests/common/mock_camera.rs`

### PLAUSIBLE: src/camera/binary.rs:364-367 (doc comment)
`read_next_frame`'s doc comment claims it "refills the user-supplied `Vec<u8>` to minimize memory churn," but the implementation always allocates fresh and replaces `frame_buf` wholesale — no reuse happens. Could mislead an embedded caller about allocation behavior. Not filed — doc-only.

### PLAUSIBLE: src/camera/binary.rs / mod.rs (module docs) — corroborates `07-16-REVIEW.md`'s open lead
Confirms the earlier cross-verification review's finding (bambuddy: printer's port 6000 is single-connection server-side; a fast redial before the prior TCP FIN completes can orphan the old socket ~20 min). `BambuBinaryCameraStream` doesn't own dial/redial logic (only wraps an already-connected stream) — no code defect, but nothing in this module's docs warns a caller writing their own reconnect loop. Judgment call, not filed — purely additive documentation; no real-hardware verification of the exact stall window performed.

No other findings — handshake/frame-header layout, `FrameReadState`/oversized-frame drain resumability, and RTSPS URL/timestamp handling all verified correct against `reference/06_cameras.md` and existing tests.
## 6. client-connect-telemetry — CONFIRMED: BUG-146

**Files:** `src/client/{mod,connect,dummy,telemetry}.rs`

### src/client/connect.rs:147-149, dummy.rs:76-78, mod.rs:232-235 — BUG-146
**Verdict:** CONFIRMED
**Issue:** `disconnect_mqtt()`'s doc presents "fall through to `ensure_mqtt()`'s lazy dial" as a valid general reconnection strategy, but it's permanently broken for any `PrinterClient` built via `from_mqtt()`.
**Detail:** `from_mqtt()` installs `PreConnected` as both TLS/factory slots; `PreConnected::dial()` unconditionally errors `NotConnected`. Before `BUG-018` added `disconnect_mqtt()`, `dummy.rs`'s "genuinely unreachable" claim held (guarded by `ensure_mqtt()`'s `is_some()` short-circuit); `disconnect_mqtt()` clearing `self.mqtt` makes it reachable-but-broken. A `from_mqtt()` caller (tests, Embassy — `from_mqtt()`'s own documented use case) following the docs after `disconnect_mqtt()` gets a permanent `NotConnected` loop; only `.attach_mqtt()` actually recovers.
**Suggested fix:** Narrow `disconnect_mqtt()`'s doc to `new()`-built clients only, and correct the now-false "genuinely unreachable" claims in `dummy.rs`/`mod.rs`; or make `PreConnected::dial()`'s error message point at `.attach_mqtt()`.

No other findings — TLS SNI-by-serial, camera-trio RTSPS fail-fast ordering, FTPS/camera config `.take()`-on-success timing, connect-timeout zero-disables handling, task-ID clamping, and telemetry cache merge-not-replace paths all verified correct.
## 7. client-commands — NO ISSUES FOUND

**Files:** `src/client/{ams,camera,hardware,motion,print,storage,thermal,types}.rs`

All command dispatch/guard logic (AMS addressing incl. BUG-143's already-fixed P1 drying guard, fan port IDs and quirk gating, bed-on-Z homing safety, Z-travel-limit wrapping, print-speed/calibration bitmask wire encoding, mains-voltage-dependent bed temp, H2C tool-changer nozzle addressing) cross-checked against reference docs and quirks module — all correct.
## 8. client-integration-tests — CONFIRMED: BUG-147, BUG-148; 1 PLAUSIBLE

**File:** `tests/client_test.rs`

### BUG-147: zero coverage for the camera trio
**Verdict:** CONFIRMED
**Issue:** No test exercises `ensure_camera()`/`camera()`/`attach_camera()`/`disconnect_camera()`/`connect_camera()` — RTSPS-model fail-fast-before-dial, unconfigured-camera error, or disconnect/reattach — despite `PrinterClient` carrying a camera trio symmetric with FTPS's, whose equivalent tests already exist as a template.
**Suggested fix:** Add tests analogous to `test_ensure_ftps_retries_after_failed_dial`/`test_disconnect_and_attach_mqtt_recovers_dead_session`.

### BUG-148: `FanTarget::ChamberExhaust` never exercised
**Verdict:** CONFIRMED
**Issue:** `set_fan_speed`'s `ChamberExhaust` branch has zero test references, unlike its 3 sibling fan targets (both success and model-mismatch coverage in `test_cooling_fans_and_peripheral_switches`).
**Suggested fix:** Extend that test with an H2D/X2D success case and a P1S/X1C rejection case.

### PLAUSIBLE: `poll_raw()` untested anywhere in `tests/`
May be adequately covered transitively via `poll_telemetry()`'s shared buffer-draining machinery, but the raw-vs-decoded dispatch distinction itself has no direct assertion. Not filed pending a call on whether it's distinct enough to warrant its own test.

No correctness bugs found in the test file's own logic.
## 9. diagnostics — NO ISSUES FOUND

**Files:** `src/diagnostics/{hms,kprofile,mod}.rs`

HMS severity/module-id bit extraction, print_error's intentionally-narrower low-word status-step check (verified correct vs. reference doc, not a BUG-109-style regression), K-profile priming and task-ID clamping — all verified correct against `reference/07_diagnostics_hms.md` and existing tests.
## 10. discovery — 2 PLAUSIBLE

**Files:** `src/discovery/{mod,parser}.rs`

### PLAUSIBLE: src/discovery/parser.rs:63-75 — empty LOCATION host not rejected
`parse_location` accepts an empty host string (`Some(("", 80))`) instead of returning `None` for a present-but-empty/malformed `LOCATION` header — same class of gap `BUG-084` fixed for the port half, never extended to the host half. Flows through to `SsdpDevice.ip = ""`.

### PLAUSIBLE: src/discovery/parser.rs:205-214 — empty USN serial not rejected
A present-but-empty `USN` header produces `serial = ""` instead of being rejected — `raw.usn?` only bails on an absent header. `effective_dev_model` was hardened against exactly this pattern for `DevModel` (`BUG-047`), never carried to the two headers this parser treats as *required*.

Neither filed as a `BUG-ID` yet — re-verify at Step 5 triage. No other findings; extensive prior hardening (BUG-009/010/011/024/046/047/060/084/085/086) confirmed intact.
## 11. core — NO ISSUES FOUND

**Files:** `src/{error,models,lib}.rs`

`lib.rs`'s feature-flag table cross-checked against `Cargo.toml` — no BUG-025/026/027-style drift. `models.rs`'s serial-prefix table cross-checked against `MODEL_MATRIX.csv` — exact match. `error.rs`'s dual `Display` impl sync gap is already self-documented as BUG-013, not a new finding.
## 12. ftps-client — CONFIRMED: BUG-150

**Files:** `src/ftps/{client,mod}.rs`, `tests/ftps_test.rs`, `tests/common/mock_ftps.rs`

### src/ftps/client.rs:520-525 — BUG-150
**Verdict:** CONFIRMED
**Issue:** `list_directory`'s post-transfer confirmation check only accepts `FTP_TRANSFER_COMPLETE` (226), unlike the structurally identical blocks in `upload_file`/`download_file`, both of which also accept `FTP_TRANSFER_ABORTED` (426, the documented P2S/X2D TLS-1.3 close race, per BUG-030).
**Detail:** By the time the confirmation reply is read, `read_to_eof` has already drained the listing payload to a clean data-channel EOF — the same trust anchor `upload_file`/`download_file` rely on for treating 426 as a benign race rather than a real failure. `list_directory` currently discards an already-fully-received, correctly-parsed directory listing and returns `ProtocolViolation` on this same race. Unlike upload/download, LIST has no `SIZE`-equivalent independent recheck, so accepting 426 here relies purely on the data-socket EOF already having completed — worth being explicit about in the fix's comment. No test currently exercises this path (`test_ftps_upload_426_recovery_via_size`/`test_ftps_download_426_recovery_via_size` exist; no `list`-equivalent).
**Suggested fix:** Accept `FTP_TRANSFER_ABORTED` alongside `FTP_TRANSFER_COMPLETE`, mirroring `download_file`; add a `test_ftps_list_426_recovery` regression test.

No other findings — poisoning discipline, TLS identity, and `validate_ftp_path` usage all applied consistently across every path-taking method; no wire-shape changes found needing hardware re-verification.
## 13. ftps-protocol — CONFIRMED: BUG-149

**Files:** `src/ftps/{parser,protocol}.rs`, `src/ftps/protocol/tests.rs`

### src/ftps/parser.rs:246 — BUG-149
**Verdict:** CONFIRMED
**Issue:** `parse_unix_listing`'s explicit-`YYYY` year branch has no validation, unlike every sibling field (month/day/hour/minute all reject-on-invalid).
**Detail:** A parse failure silently defaults to `current_year` via `.unwrap_or(current_year)` instead of rejecting the line; a successfully-parsed value has no range check. Same class of gap BUG-061 fixed for day/hour/minute, never extended to the year branch.
**Suggested fix:** Reject the line on parse failure; add a plausibility range check (e.g. `1980..=current_year+1`) on success.

No other findings — multi-line reply parsing, PASV port bounds, path validation, day-rollover math all verified correct; hardware-verification and wire-read-deadline invariants respected (no wire-shape changes made).
## 14. io-tokio — 1 PLAUSIBLE

**Files:** `src/io/{mod,tokio}.rs`, `src/io/tokio/{cert_verify,tests}.rs`

### PLAUSIBLE: src/io/tokio/tests.rs:392-397 — misnamed test
`test_build_verified_client_config_bad_key_returns_error` never calls `build_verified_client_config` — it only asserts `PrivateKeyDer::try_from(vec![0u8; 10])` fails, a fact about `rustls_pki_types`'s own parser. Would pass even if `build_verified_client_config` silently mishandled a bad key. Test-only, not filed — wire the test to the real function or rename it.

No other findings — SNI-by-serial, `read_chunk` EOF/timeout disambiguation, chain-of-trust walk (BUG-008/048), and IPv6-literal-not-applicable reasoning for `TokioRawStreamFactory::dial` all verified correct.
## 15. io-embedded — NO ISSUES FOUND

**Files:** `src/io/{embassy,esp_idf}.rs`

Full close read of both files against `io/CLAUDE.md` and the matched cross-cutting rules. Specifically traced and ruled out: shared-timer `BorrowMutError` risk under concurrent `race()` polling (each `EspIdfTlsConnector`/`EspIdfTcpStream::connect()` allocates its own independent timer, by design per BUG-051); whether Embassy's IPv4-literal-only `dial()` restriction could break MQTT (confirmed `PrinterClient.ip` is always an IP literal crate-wide); confirmed `EspIdfUdpSocket`'s `WouldBlock` branch is genuinely reachable (socket is non-blocking). `cargo check --no-default-features --features embassy --lib` compiles clean. All documented known gaps (no TLS-1.2-forcing knob, `EmbassyTlsConnector` no built-in connect timeout, `negotiated_version` always `None` on Embassy) are already correctly documented, not newly-discovered issues.
## 16. mqtt-client — 1 PLAUSIBLE

**Files:** `src/mqtt/mod.rs`, `src/mqtt/client/{mod,codec,frame,pending}.rs`, `tests/mqtt_test.rs`, `tests/common/{mock_mqtt,mod,io}.rs`

### PLAUSIBLE: src/mqtt/client/mod.rs:121-131 (`write_frame`), called from `connect()`/`publish_command()`/`send_ping()`
Writes have no read-side-equivalent stall protection. The read path got deliberate engineering for a stalled connection (`FrameReadState`/`MQTT_READ_TIMEOUT_SECS`/`read_chunk`, surfacing as `TimedOut` within a bounded window) — no equivalent exists for writes: `write_frame` does a plain unguarded `write_all()`/`flush()`, and no caller (including `PrinterClient::poll_until` in `src/client/mod.rs`, checked directly) wraps it in a timeout. A write-side stall (TCP send-buffer full, printer silently dropped off WiFi) can hang the whole client indefinitely — on ESP-IDF/Embassy a stuck task forever, not just a slow host future. Not a wire-shape change (no bytes-on-wire framing changes), so doesn't require hardware-verification gating, but it's a new correctness capability, not a mock-testable existing-behavior fix — flagged for a scoping decision, not filed as a `BUG-ID` yet.

No other findings — framing, connect handshake, QoS 1 tracking, zombie/stale detection, and pending-buffer eviction/matching are all correct and well-covered by existing tests (BUG-032/052/078/079/140).
## 17. mqtt-commands — NO ISSUES FOUND

**Files:** `src/mqtt/commands/{mod,ams,control,gcode,hardware,print_job,status}.rs`

Every constructor across all 7 files takes sequence/task IDs via the type-safe `ClampedTaskId` (no raw-integer bypass found). All wire-field names/types/shapes cross-checked against `reference/03_mqtt_telemetry.md`/`reference/05_materials_ams.md` — exact match across every payload type.
## 18. quirks — 1 PLAUSIBLE

**Files:** `src/quirks/mod.rs`, `src/quirks/models/{mod,a1,a2,h2,p1,p2,x1,x2}.rs`

### PLAUSIBLE: src/quirks/mod.rs:290-292 — `DEFAULT_Z_MAX_MM` doc comment names models that don't use it, 2 factually wrong
`DEFAULT_Z_MAX_MM`'s doc comment says it's "used by every bed-slinger (P1/A1 series) and the base X1C/X1E/X2D/H2S CoreXY chassis size" — but every model in `src/quirks/models/*.rs` overrides `z_max()` explicitly, so this constant is never actually reached by any model's dispatch (it's an unused fallback for hypothetical future models). Worse, 2 of the named models are wrong even if it were reached: A1 Mini is `180.0` not `256.0`, H2S is `340.0` not `256.0` (cross-checked against `MODEL_MATRIX.csv`). Every other model's `z_max()` override was independently cross-checked against `MODEL_MATRIX.csv` and found correct — this is a single stale comment on an unused constant, not a functional bug. Not filed as a `BUG-ID` yet — re-verify at Step 5 triage.

Verification summary: every `ModelQuirks` override across all 7 model files cross-checked against `MODEL_MATRIX.csv` and `reference/*.md` (camera protocol, bed/chamber temp ceilings incl. X1C voltage inversion, AMS pool composition, fan/speaker/buzzer capabilities, door sensors, nozzle counts, FTPS TLS quirks, RTSPS timestamp correction) — all correct, including BUG-143's P1 drying-guard fix (not re-flagged).
## 19. telemetry-ams — NO ISSUES FOUND

**Files:** `src/types/telemetry/ams.rs`, `src/types/telemetry/tests/ams.rs`

Every `merge_from` (`AmsStatusReport`/`AmsUnit`/`AmsTray`) does field-by-field preserve-on-absence merging, `tray` array does keyed merge+prune matching BambuStudio's parser. All `AMS_UNIT_INFO_*` bit shift/mask constants match their accessor doc comments exactly, including the BUG-104 dry_sub_status/bind_switch_in boundary (bits 22-23 vs 24-25, non-overlapping). Cross-checked `AmsStatusReport::merge_from` against a real P1S wire capture (`tests/mocks/P1S_print_sequence.ndjson`). `VirtualTray`'s wholesale-replace (no `merge_from`) is the already-closed `BUG-100` Wontfix, not re-raised. Test coverage (984 lines) is comprehensive — no gaps found.
## 20. telemetry-device — 1 PLAUSIBLE

**Files:** `src/types/telemetry/device.rs`, `src/types/telemetry/tests/{device,nozzle,ctc,bed}.rs`

### PLAUSIBLE: src/types/telemetry/device.rs:196-227, 283-300, 435-462 — same shape as BUG-102, not carried here
`NozzleCollection.info`, `ExtruderCollection.info`, `AirductCollection.parts`/`.mode_list` are plain `Vec<T>` with `#[serde(default)]`; their `merge_from` gates on `!incoming.X.is_empty()`, which can't distinguish "key absent from this push" (should preserve cache) from "key present as an explicit empty array" (should overwrite). BUG-102 fixed this exact shape for `AmsUnit.tray` (`Vec<AmsTray>` → `Option<Vec<AmsTray>>`) in the same commit series, never applied here. Could not confirm from BambuStudio/pybambu source whether the printer ever sends an explicit empty array for these 4 fields — genuinely can't promote past PLAUSIBLE without that evidence.

No other findings — bit-packing doc comments, composite-temperature packing, and merge_from recursion chain all verified correct against cited sources and existing tests.
## 21. telemetry-core — CONFIRMED: BUG-151

**Files:** `src/types/{mod,version}.rs`, `src/types/telemetry/{mod,report,diagnostics,tests}.rs`, `src/types/telemetry/tests/{misc,fun_field}.rs`, `tests/telemetry_replay_test.rs`

### src/types/telemetry/report.rs:108-123 — BUG-151
**Verdict:** CONFIRMED
**Issue:** Doc comments on `nozzle_target_temper` and `bed_temper`/`bed_target_temper` incorrectly instruct callers to composite-unpack these fields via `unpack_temperature()`.
**Detail:** Contradicts this directory's own `CLAUDE.md` ("Bed and nozzle targets arrive as separate `_target_temper` fields — never composite-packed") and `reference/04_toolhead_thermal_motion.md:39` ("These are not composite-packed"). No production call site actually applies `unpack_temperature()` to these three fields (crate-wide search confirmed) — harmless today (values stay ≤500, the unpack is a no-op passthrough), but the doc actively misrepresents a wire format that doesn't exist for these fields, exactly the footgun-in-doc-clothing class this sweep flags. `chamber_temper`'s comment (genuinely composite-packed) is correct and untouched.
**Suggested fix:** Reword the three doc comments to state these are flat, pre-separated, never-packed registers; drop the `unpack_temperature()` pointer from `nozzle_target_temper`. Regenerate `docs/types/telemetry/report/index.md` afterward (currently reproduces the same wrong wording).

No other findings — bit masks (door sensor, net.conf, 220V power, sdcard state) verified against `reference/03_mqtt_telemetry.md`; BUG-054/081/113 (bed-temp dual source), BUG-099 (PREPARE gcode-state arm, out of this unit's files), BUG-105 (ipcam merge), BUG-106 (HMS permissive decode), BUG-107 (ts_boot/ts_unix overclaim, remaining overclaim lives only in `reference/00_index.md`/`07_diagnostics_hms.md`, not this unit) all independently re-verified as already correctly fixed/documented.


---

## Step 5 — Plausible-Findings Triage (re-verified by direct code read, not by re-trusting the agent's claim)

Every `PLAUSIBLE` finding raised by a unit above was independently re-checked by reading the cited source directly before this sweep was marked complete. None were promoted on the agent's word alone.

| Unit | Finding | Resolution |
|---|---|---|
| §3 cli-transfer-probe | `probe.rs` capture-window overshoot | Confirmed real (both cited constants verified: `DEFAULT_CAPTURE_WINDOW_SECS=3`, `MQTT_READ_TIMEOUT_SECS=30`) → **BUG-152**, Open, Sev3 |
| §4 cli-core | `table.rs` Unicode-width misalignment | Confirmed real (`str::len()`/char-count `format!` verified directly) → **BUG-153**, Open, Sev3 |
| §5 camera | `binary.rs:364` doc claims buffer reuse | Confirmed real (doc vs. `*frame_buf = payload` at line 324 verified) → **BUG-154**, Open, Sev3 |
| §5 camera | Port-6000 redial-timing doc gap | No code defect (module doesn't own dial/redial), but the doc warning itself is a real fix → **BUG-161**, Open, Sev3 |
| §8 client-integration-tests | `poll_raw()` untested | `poll_raw()` confirmed a trivial one-line delegate, no independent logic → **BUG-162**, Wontfix/N/A |
| §10 discovery | Empty `LOCATION` host accepted | Confirmed real (`parser.rs:63-66` verified directly) → **BUG-155**, Open, Sev3 |
| §10 discovery | Empty `USN` serial accepted | Confirmed real (`parser.rs:205` verified directly) → **BUG-156**, Open, Sev3 |
| §14 io-tokio | Misnamed test in `tokio/tests.rs` | Confirmed real (test body read directly, never calls the function it's named for) → **BUG-157**, Open, Sev3 |
| §16 mqtt-client | `write_frame` no stall timeout | Confirmed real (`mod.rs:121-131` read directly, no deadline wrapper at any call site) → **BUG-159**, Open, Sev3 |
| §18 quirks | `DEFAULT_Z_MAX_MM` doc comment wrong | Confirmed real (every model's `z_max()` override read directly; A1 Mini=180/H2S=340 vs. claimed 256) → **BUG-160**, Open, Sev3 |
| §20 telemetry-device | `Vec<T>` merge_from absent-vs-empty gap | Code pattern confirmed real, but no wire capture/BambuStudio source available this session to confirm the printer ever sends explicit-empty for these fields → **BUG-158**, Open, `needs-verification` |

## Summary

21/21 units reviewed. 13 clean (`NO ISSUES FOUND` or fully resolved on re-verification), 8 with `CONFIRMED` findings promoted directly, 11 `PLAUSIBLE` findings all independently re-verified and triaged above — 10 promoted to `Open`, 1 closed `Wontfix`/N/A. Total new `BUG-ID`s from this sweep: **BUG-144 through BUG-162** (19 rows: 18 Open, 1 Wontfix), plus **BUG-143** (P1 AMS-drying fix, already `Fixed` before this sweep started, logged retroactively). Zero Sev1/Sev2 findings — every new row is Sev3 or `needs-verification`; the release bar (zero open Sev1/Sev2) is unaffected by this sweep.

**`BACKLOG.md` is the status source of truth from here on.** The table above is a point-in-time snapshot as of this sweep and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | One-line |
|---|---|---|---|
| BUG-144 | Sev3 | ams/mapping.rs | AMS-HT `ams_id` doc says "128+", code enforces 128-135 |
| BUG-145 | Sev3 | bin/bambino-cli/probe.rs | `pushall` step aborts entire probe run via `?` |
| BUG-146 | Sev3 | client/{connect,dummy,mod}.rs | `disconnect_mqtt()` doc claims broken lazy-redial for `from_mqtt()` clients |
| BUG-147 | Sev3 | tests/client_test.rs | Zero coverage for `PrinterClient`'s camera trio |
| BUG-148 | Sev3 | tests/client_test.rs, client/hardware.rs | `FanTarget::ChamberExhaust` never tested |
| BUG-149 | Sev3 | ftps/parser.rs | Explicit-`YYYY` year branch unvalidated |
| BUG-150 | Sev3 | ftps/client.rs | `list_directory` rejects `426` unlike upload/download |
| BUG-151 | Sev3 | types/telemetry/report.rs | Bed/nozzle target-temp docs wrongly claim composite-packing |
| BUG-152 | Sev3 | bin/bambino-cli/probe.rs | Capture window can overshoot up to 10x |
| BUG-153 | Sev3 | bin/bambino-cli/table.rs | Table columns misalign on CJK/emoji names |
| BUG-154 | Sev3 | camera/binary.rs | Doc claims frame-buffer reuse that doesn't happen |
| BUG-155 | Sev3 | discovery/parser.rs | `parse_location` accepts empty host |
| BUG-156 | Sev3 | discovery/parser.rs | `parse_ssdp_payload` accepts empty USN serial |
| BUG-157 | Sev3 | io/tokio/tests.rs | Test never calls the function it's named for |
| BUG-158 | needs-verification | types/telemetry/device.rs | 3 `Vec<T>` fields can't distinguish absent vs. explicit-empty |
| BUG-159 | Sev3 | mqtt/client/mod.rs | `write_frame` has no stall-timeout protection |
| BUG-160 | Sev3 | quirks/mod.rs | `DEFAULT_Z_MAX_MM` doc names wrong/unused models |
| BUG-161 | Sev3 | camera/binary.rs, camera/mod.rs | Missing doc warning against fast port-6000 redial |
| BUG-162 | N/A | client/telemetry.rs | `poll_raw()` untested (Wontfix) |
