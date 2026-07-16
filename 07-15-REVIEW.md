**Status:** COMPLETE

**Every PLAUSIBLE finding below was independently re-verified by direct code read after the initial sweep** (not just re-stated) — each is now either promoted to a real `BUG-ID` or explicitly resolved as verified-but-not-a-defect, with the reasoning kept below for anyone auditing the triage call. **`BACKLOG.md` is the status source of truth from here on — the table below is a point-in-time snapshot and won't be updated as bugs get fixed.**

| BUG-ID | Sev | Module | File(s) | One-line |
|---|---|---|---|---|
| BUG-127 | Sev2 | ams | src/ams/mapping.rs:146 | `flat_channel_id_for_entry` doesn't bound-check `slot_id`, unlike its inverse `flat_channel_id` |
| BUG-128 | Sev2 | ams | src/ams/mapping.rs:227 | `validate_external_spool_safety` misclassifies out-of-range `AmsMapping2Entry` as physical AMS |
| BUG-129 | Sev2 | bambino-cli | src/bin/bambino-cli/monitor/mod.rs:34-58 | `dump --follow` never calls `tick_zombie_check`, unlike `run()` — silent indefinite hang on a dead connection |
| BUG-130 | Sev3 | bambino-cli | src/bin/bambino-cli/connection.rs:76 | Access-code validation ceiling/charset disagrees with the camera protocol's own constraint |
| BUG-131 | Sev3 | bambino-cli | src/bin/bambino-cli/main.rs:121 | `Control`'s `override_usage` mislabels the AMS dry-cycle duration arg `<TIME>` instead of `<HOURS>` |
| BUG-132 | Sev3 | discovery | src/discovery/parser.rs:35-36 | `raw_model_str` doc comment overstates its provenance (can be NT/ST-derived, not always literal) |
| BUG-133 | Sev3 | ftps | src/ftps/parser.rs:196-199 | `day` bound isn't month-aware, sibling gap to BUG-061 |
| BUG-134 | Sev3 | mqtt | src/mqtt/client/codec.rs:45-96 | Wire-encoding length prefixes silently truncate past `u16::MAX` instead of erroring |
| BUG-135 | Sev3 | client | src/client/telemetry.rs:507-508 | BUG-110-corrected `is_ethernet_active()` is unreachable through `PrinterClient` |
| BUG-136 | Sev3 | docs | src/ftps/CLAUDE.md | Nested CLAUDE.md invariant text stale re: `download_file`'s accepted reply codes (226 vs 226/426) |
| BUG-137 | Sev3 | docs | src/types/telemetry/CLAUDE.md | Nested CLAUDE.md invariant text incompletely lists `unpack_temperature()`'s use sites |

### Verified real but not promoted (no concrete defect found)
- **bambino-cli control+monitor** (`monitor/mod.rs:55` vs `167-172`): confirmed real asymmetry — `dump()` propagates a failed `send_ping()` as fatal via `?`, `run()` logs and continues. Not a defect: `dump()`'s fail-fast is arguably the safer of the two (exits loudly instead of risking a silent hang), and the two functions have no stated contract requiring identical ping-failure handling.
- **discovery** (`mod.rs:30-34`): confirmed `MULTICAST_IP` (str) and `MULTICAST_ADDR` (typed `Ipv4Addr`) are independently-maintained literals for the same address. Currently consistent, no drift exists today — a real defect would require someone to edit one without the other, which hasn't happened. Not promoted; flagged here in case a future editor should double-check both.
- **ftps protocol** (`parser.rs:120-215`, symlink `LIST` entries): confirmed `perms.starts_with('l')` isn't special-cased, so a symlink's `" -> target"` suffix folds into `FtpFile.name` verbatim. Not promoted — Bambu's embedded SD card/vsftpd setup has no realistic path to producing a symlink entry, so there's no concrete failure scenario against real hardware, only a theoretical gap in parsing logic that would trigger on input this device class never produces.
- **core loose files** (`lib.rs:1`, vestigial `no_std` Cargo feature): confirmed the `no_std = []` feature (Cargo.toml:29) is never read anywhere (`lib.rs` gates on `not(feature = "std")`); it exists only as an inert entry pulled in by `embassy`'s feature list. Misleading name, but not promoted — nothing reads it, so there's no behavioral defect, just a name that invites a wrong assumption.
- **mqtt/client** (stall/resume integration coverage): confirmed the resumable-frame-read invariant is unit-tested only against a bare `FrameReadState`, never through a live `BambuMqttClient`'s persistent `self.read_state`. Code inspection shows the wiring is structurally correct today — this is a regression-coverage gap, not a current defect, so not promoted as a bug; worth a future test addition but no `BUG-ID` needed for something that isn't broken.

# bambino Deep Review — 2026-07-15

Full-crate correctness review via parallel subagents, one per module/unit boundary. 18 units, discovered fresh from `src/` and `tests/` structure at sweep time (see partition below). Each unit was reviewed independently by its own agent, given only its file list plus any cross-cutting invariants (`.claude/rules/*.md`, nested `<dir>/CLAUDE.md`) matched to its files.

**Scope exclusions (by design, not oversight):**
- Correctness bugs, invariant violations vs. `CLAUDE.md`/`.claude/rules/`/nested `CLAUDE.md`, missed error handling at real boundaries (network I/O, FFI) — in scope.
- Minor security issues are out of scope — this crate is explicitly LAN-only by design (see `README.md`'s Safety Notice); cert-verification bypass, plaintext fallback, etc. are not flagged unless implemented incorrectly vs. their own stated behavior.
- Style/refactor suggestions and naming preferences are out of scope, except where a name actively misrepresents behavior (inverted boolean sense, function implying the opposite of what it does).

**CONFIRMED vs PLAUSIBLE:** Each agent tagged findings `CONFIRMED` (sure it's a real bug) or `PLAUSIBLE` (looks real but unverifiable — e.g. can't confirm the failure path triggers, or the invariant itself is ambiguous). `CONFIRMED` findings were promoted immediately to `BACKLOG.md`'s Open table with a severity per the `backlog` skill's rubric. `PLAUSIBLE` findings are **not** promoted — they sit in this file's dedicated section below for manual human triage.

This file is meant to be consumed standalone by a fresh session with no other context. **Caveat:** file:line references may have drifted if other changes landed on `main` since this sweep ran.

## Partition

1. `src/ams/` (mapping.rs, mod.rs, parser.rs)
2. `src/bin/bambino-cli/` core (main.rs, connection.rs, discover.rs, table.rs, verify_tls.rs, inspect_cert.rs, camera.rs, storage.rs)
3. `src/bin/bambino-cli/` control+monitor (control.rs, probe.rs, monitor/dashboard.rs, monitor/mod.rs)
4. `src/camera/` (mod.rs, binary.rs, rtsps.rs) + tests/camera_test.rs + tests/common/mock_camera.rs
5. `src/client/` core (mod.rs, connect.rs, dummy.rs, types.rs, telemetry.rs, thermal.rs) + tests/client_test.rs
6. `src/client/` domain (ams.rs, camera.rs, hardware.rs, motion.rs, print.rs, storage.rs)
7. `src/diagnostics/` (hms.rs, kprofile.rs, mod.rs)
8. `src/discovery/` (mod.rs, parser.rs)
9. core loose files (src/error.rs, src/models.rs, src/lib.rs)
10. `src/ftps/` client (client.rs, mod.rs) + tests/ftps_test.rs + tests/common/mock_ftps.rs
11. `src/ftps/` protocol (parser.rs, protocol.rs, protocol/tests.rs)
12. `src/io/` core (mod.rs, tokio.rs, tokio/cert_verify.rs, tokio/tests.rs) + tests/common/io.rs
13. `src/io/` embedded (embassy.rs, esp_idf.rs)
14. `src/mqtt/client/` (codec.rs, frame.rs, mod.rs, pending.rs) + tests/mqtt_test.rs + tests/common/mock_mqtt.rs + tests/common/mod.rs
15. `src/mqtt/commands/` (mod.rs + ams.rs, control.rs, gcode.rs, hardware.rs, print_job.rs, status.rs) + src/mqtt/mod.rs
16. `src/quirks/` (mod.rs + models/{a1,a2,h2,mod,p1,p2,x1,x2}.rs)
17. `src/types/` telemetry-A (types/mod.rs, version.rs, telemetry/mod.rs, ams.rs, diagnostics.rs, telemetry/tests.rs, telemetry/tests/ams.rs)
18. `src/types/` telemetry-B (telemetry/device.rs, report.rs, telemetry/tests/{misc,device,nozzle,bed,ctc,fun_field}.rs) + tests/telemetry_replay_test.rs

## 1. src/ams/

- **BUG-127** (Sev2): `flat_channel_id_for_entry` (mapping.rs:146) doesn't bound-check `slot_id`, unlike its inverse `flat_channel_id` — a garbage slot computes a bogus flat channel instead of falling back to `-1`.
- **BUG-128** (Sev2): `validate_external_spool_safety` (mapping.rs:227) misclassifies out-of-range `ams_id`/`slot_id` combos as physical AMS, reproducing the `07FF_8012` lockup class fixed for id-254 in BUG-039 but for garbage ids generally.
- Both stem from `AmsMapping2Entry`'s public fields being validated only by convention at each call site — a future function accepting `&[AmsMapping2Entry]` is at the same risk.
- No other issues in `mod.rs`/`parser.rs`.

## 2. src/bin/bambino-cli/ (core)

NO CONFIRMED ISSUES. Two PLAUSIBLE findings below.

## 3. src/bin/bambino-cli/ (control+monitor)

- **BUG-129** (Sev2): `dump --follow`'s ping/poll loop (`monitor/mod.rs:34-58`) never calls `tick_zombie_check`, unlike `run()`'s dashboard loop — a silently-dead connection hangs the CLI indefinitely with no error.
- One PLAUSIBLE finding below (ping-failure handling asymmetry between `dump()` and `run()`).
- No issues in `control.rs`/`probe.rs`.

## 4. src/camera/

NO ISSUES FOUND.

## 5. src/client/ (core)

NO ISSUES FOUND (mod.rs, connect.rs, dummy.rs, types.rs, telemetry.rs, thermal.rs, tests/client_test.rs).

## 6. src/client/ (domain)

NO ISSUES FOUND (ams.rs, camera.rs, hardware.rs, motion.rs, print.rs, storage.rs).

## 7. src/diagnostics/

NO ISSUES FOUND.

## 8. src/discovery/

NO CONFIRMED ISSUES. Two low-confidence PLAUSIBLE observations below (doc-precision only, not functional).

## 9. core loose files (error.rs, models.rs, lib.rs)

NO CONFIRMED ISSUES. One PLAUSIBLE finding below (vestigial `no_std` Cargo feature name, cosmetic).

## 10. src/ftps/ (client)

NO CONFIRMED ISSUES. One PLAUSIBLE finding below (stale doc wording in `src/ftps/CLAUDE.md` re: `download_file`'s accepted reply codes, no functional impact).

## 11. src/ftps/ (protocol)

NO CONFIRMED ISSUES. Two PLAUSIBLE findings below (day-of-month not validated against month length; symlink `LIST` entries not special-cased). CR/LF/NUL path-injection defense and data-channel stall-timeout shape both verified correct.

## 12. src/io/ (core)

NO ISSUES FOUND.

## 13. src/io/ (embedded)

NO ISSUES FOUND.

## 14. src/mqtt/client/

NO CONFIRMED ISSUES. Two PLAUSIBLE findings below (persistent-client stall/resume path is unit-tested only on the bare primitive, not through the full `BambuMqttClient`; u16 length-prefix truncation on overlong topic/client_id/username/password with no bounds check). Module split, wire-read-deadline delegation, and poll-telemetry-dispatch drain order all verified correct.

## 15. src/mqtt/commands/

NO ISSUES FOUND. Task-ID clamping confirmed enforced by the `ClampedTaskId` type (BUG-001/BUG-053 already closed this class) across every constructor in all 6 category files.

## 16. src/quirks/

NO ISSUES FOUND. X1C bed-temp voltage inversion, camera-protocol-per-model split, and MODEL_MATRIX.csv cross-checks all verified correct.

## 17. src/types/ (telemetry-A)

NO ISSUES FOUND. AMS `info` bitmask (including the BUG-104 dry_sub_status fix) hand-verified bit-by-bit against a wire fixture; no regression.

## 18. src/types/ (telemetry-B)

NO CONFIRMED ISSUES. One PLAUSIBLE finding below (BUG-110-fixed `is_ethernet_active()` is unreachable through the stateful `PrinterClient`/`TelemetryCache` pipeline — only the less-reliable wifi-signal fallback is). Composite-packing threshold/shift verified as a single shared implementation across chamber/extruder/bed telemetry, no copy-paste drift; `vir_slot`/`vt_tray` kept distinct; device()/fallback order correct.

## Plausible, Unverified Findings

All findings originally reported here by the sweep agents have since been triaged (see the resolution table and "Verified real but not promoted" list at the top of this file) — either promoted to `BUG-127`–`BUG-137` in `BACKLOG.md`, or confirmed real but not a defect with reasoning recorded above. Nothing remains pending in this section.
