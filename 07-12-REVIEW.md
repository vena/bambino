**Status:** COMPLETE (8/8 units)

# BambuStudio/OrcaSlicer MQTT Surface Cross-Reference Sweep

Systematic cross-reference of bambino's MQTT telemetry-decode and
outbound-command-construction surface against BambuStudio (the official
first-party client), OrcaSlicer (fork, divergence check), and the
`bambuddy`/`pybambu` reverse-engineering projects. Methodology and
per-unit partition come from `BAMBUSTUDIO_MQTT_CROSSREF_PLAN.md` (see that
file for full detail); orchestration mechanics (parallel per-unit agents,
incremental persistence, promotion to `BACKLOG.md`) reuse the `deep-review`
skill.

**Scope exclusions** (same as `deep-review`'s default): minor security
issues in this LAN-only-by-design crate are out of scope unless they
violate the crate's own stated behavior; style/refactor/naming
suggestions are out of scope except where a name actively misrepresents
behavior.

**Confidence tiers** (this plan's three-tier scheme, wider than
`deep-review`'s default two-tier):
- `CONFIRMED` — BambuStudio evidence plus agreement from at least one of
  bambuddy/pybambu, or a source disagreement resolved via git-history
  check.
- `PLAUSIBLE` — looks like a real gap but only one source, or resolution
  didn't fully settle it.
- `NEEDS-VERIFICATION` — genuine irreconcilable disagreement between
  sources, or something only real H2/P2/X2 hardware (unavailable to this
  crate, P1S-only) can settle.

This file is meant to be consumed standalone by a fresh session. File:line
references may have drifted if other changes landed on `main` since this
sweep.

## Units

## 1. types/telemetry/{ams.rs, tests/ams.rs} — 1 CONFIRMED, 4 PLAUSIBLE

### `AmsUnit::dry_sub_status()` reads bits 22-25 (4 bits), real field is bits 22-23 (2 bits) — BUG-104

**Verdict:** CONFIRMED (promoted to BACKLOG.md as BUG-104, Sev2). Bits 24-25 belong to the separate `bind_switch_in` field (`DevFilaSystem.cpp:599`), so whenever it's nonzero, `dry_sub_status()` returns a value (up to 15) outside `DrySubStatus`'s real 0-2 range. Fix: `AMS_UNIT_INFO_DRY_SUB_STATUS_MASK` `0xF` → `0x3`; also correct `types/telemetry/CLAUDE.md`'s "bits 22-25" bullet.

### Plausible gaps (not promoted — single-sourced to BambuStudio, no bambuddy/pybambu corroboration)

- **Missing `dry_fan1_status`/`dry_fan2_status` accessors** — BambuStudio decodes bits 18-19/20-21 of the same `info` mask (`DevFilaSystem.cpp:696-697`); `AmsUnit` has no accessor for either. Low priority.
- **Missing `AmsTray.remain_g`** — weight-based remaining-filament field (`DevFilaSystem.cpp:800`, `ParseVal(..., -1)`, reset-on-absence like `remain`). No P1S corroboration; possibly X2D/newer-hardware only.
- **Missing `AmsTray.setting_id`** (maps to BambuStudio's `filament_setting_id`, distinct from `tray_info_idx`) — `DevFilaSystem.cpp:801`, genuinely preserve-on-absence (no-default `ParseVal` overload), unlike this file's deliberate `remain`/`tag_uid`/`tray_uuid` divergence.
- **Missing `AmsStatusReport.calibrate_remain_flag`** (trivial, mirrors existing `insert_flag`/`power_on_flag` `.contains()` pattern) **and `.cfs`** (structured filament-change-step array, explicit-reset-on-absence per `DevFilaSystem.cpp:507-511`; confirmed present-on-wire via pybambu's `MOCK-X2D.json` fixture but unparsed there too — scope likely belongs with print-job/toolchange state, not this AMS unit/tray unit; flag for `report.rs`/`mqtt/commands` review if this sweep reaches it).

### Cross-unit note: `VirtualTray` merge semantics — already resolved, no action needed

Unit 1's agent independently flagged that BambuStudio's `parse_vt_tray` rebuilds `vt_tray`/`vir_slot` from scratch each push (reset-on-absence), a different policy from regular `AmsTray`'s preserve-on-absence — this exact question was already investigated and closed as **BUG-100** (Wontfix, N/A: current bambino wholesale-replace for `VirtualTray` is correct as-is). No new finding.

## 2. types/telemetry/{device.rs, tests/device.rs} — 1 CONFIRMED, 1 NEEDS-VERIFICATION

### `ExtruderInfo.snow`/`spre`/`star` doc comment wrong bit layout — BUG-112 (Sev3, doc-only)

Claims 4-bit tray-index/upper-bits-AMS-index; real layout (BambuStudio + bambuddy agree) is low 8 bits = slot, next 8 bits (8-15) = AMS unit. No decode helper exists yet for these fields, so no runtime behavior changes — the doc comment is the only spec and was wrong.

### Bed-temp value-source three-way disagreement — BUG-113 (needs-verification, reopens BUG-054/081)

BambuStudio/OrcaSlicer's `DevBed::ParseV2_0` reads the flat `device.bed_temp` composite field exclusively; pybambu reads `device.bed.info.temp` primary; bambuddy reads neither for values (old-gen `bed_temper`/`bed_target_temper`). No two sources agree — this crate's BambuStudio-only access this session reopens a question BUG-054/081 closed using only pybambu/bambuddy. P1S-only hardware here can't settle it; needs an H2/P2/X2 wire capture.

### Re-verified, no new issues

`NozzleCollection`/`ExtruderCollection`/`AirductCollection` array-replace-on-presence semantics (confirmed against `json_diff::restore_objects`'s object-only recursion — arrays are wholesale by design, matching bambino), `ExtruderCollection`'s `state` bit math, `ExtruderInfo.temp` composite-unpack heuristic, and `ExtToolTelemetry`/`CtcTelemetry` merge behavior all check out. `.claude/rules/bed-temp-voltage.md` not relevant to this unit (governs `quirks/models/x1.rs` clamping, not decode-source selection).

## 3. types/telemetry/{diagnostics.rs, tests/ctc.rs, tests/bed.rs} — 2 CONFIRMED, 1 NEEDS-VERIFICATION

### `IpcamTelemetry` has no `merge_from`/cache field — BUG-105 (Sev3)

BambuStudio gates every `ipcam` field behind `.contains()` (preserve-on-absence), matching the pattern already applied to `CtcTelemetry`/`BedTelemetry`/`ExtToolTelemetry` (BUG-096/095/097). `TelemetryCache` has no `last_ipcam` field at all — camera/recording state is only visible on the exact push it arrives in, never persisted. Fix spans this unit's `diagnostics.rs` and unit 5's `client/telemetry.rs` — coordinate if unit 5 hasn't landed yet.

### `HmsEntry.attr`/`.code` strict `u32` breaks whole-message parse on malformed entry — BUG-106 (Sev2)

A single HMS entry with a missing key or hex-string value (BambuStudio + bambuddy both handle this defensively) fails deserialization of the *entire* `TelemetryReport`, not just the `hms` array — `poll_telemetry` silently drops every other field (device/ams/temps/state) in that push via its `Err(_) => Ok(TelemetryEvent::Unknown(msg))` fallback. BambuStudio's `ParseHMSItems` pushes a default-zeroed item instead of aborting; bambuddy additionally string-hex-tolerates both fields.

### `HmsEntry.ts_boot`/`.ts_unix` — unconfirmed doc claim — BUG-107 (needs-verification)

Doc comments + `reference/07_diagnostics_hms.md` claim these fields are "present on X2/H2/P2" but zero of the four cross-reference sources model them at all, and `DevHMSItem`'s real field set has no timestamp fields on any generation. Traced to bulk-expansion commit `22b81ec` with no cited source. Already `#[serde(default)]`-guarded (safe as shipped) — only the confident wording is unverified; needs real X2/H2/P2 hardware or a capture to settle.

### Re-verified, no new issues

- `CtcTelemetry::merge_from`/`CtcInfo::merge_from` (BUG-096/101) re-checked against current BambuStudio `DevChamber.cpp` and OrcaSlicer's inline `DeviceManager.cpp` equivalent — unchanged, still correct in both forks.
- `tests/bed.rs`'s `bed_temperatures()` preference (`device.bed.info.temp` over `device.bed_temp`, BUG-081/054) re-examined: BambuStudio's own `DevBed::ParseV2_0` actually reads the *flat* `bed_temp` field, not the nested one bambino prefers — looked like it might reopen BUG-081/054, but pybambu's code comment confirms both fields carry the identical composite value in the same push (genuine redundant duplication, not competing sources), so bambino's existing choice still produces correct results. No action, but noted since it came close to a real finding.
- `last_hms` wholesale-replace caching (`client/telemetry.rs`) confirmed correct against `DevHMS::ParseHMSItems`'s own `m_hms_list.clear()` — official client also rebuilds the whole list every push, same "always-fully-populated array" class as `vt_tray` (BUG-100).

## 4. types/telemetry/{report.rs, mod.rs, tests/misc.rs, tests/fun_field.rs, tests/nozzle.rs} — 1 CONFIRMED, 2 NEEDS-VERIFICATION

### `is_ethernet_active()`'s bit-18 heuristic confirmed wrong — BUG-110 (Sev3)

Both BambuStudio and OrcaSlicer decode `home_flag` bit 18 as prompt-sound-detection support, not ethernet — this is stronger than the existing "disputed" doc comment suggests (active contradiction, not silence). Both also derive real wired-ethernet from `print.net.conf` bit 0, a wire field `PrinterTelemetry` doesn't model at all today.

### IDEX-detection heuristic vs. nozzle racks — BUG-111 (needs-verification)

`nozzle.info.len() >= 2` could misfire on a single-extruder printer with a populated nozzle-changer rack (BambuStudio separates rack-stored spares from installed nozzles via a dedicated bit; OrcaSlicer has no rack modeling at all — newer-hardware-only). Only reached when `device.extruder` is also absent; no rack-equipped hardware available to confirm real-world impact.

### `deserialize_permissive_bool`'s `sdcard` decoder — PLAUSIBLE, not promoted

Only maps exact string `"HAS_SDCARD_NORMAL"` to `true`; BambuStudio's `DevStorage` enum confirms `HAS_SDCARD_ABNORMAL`/`HAS_SDCARD_READONLY` are real distinct-from-absent states, and bambuddy treats all three as "card present." Single-sourced (bambuddy only) for the exact string-match gap — under-reports on two degraded states, never over-reports. Consider broadening to `.contains("HAS_SDCARD")` or decoding `home_flag` bits 8-9 properly.

### Re-verified, no new issues

Temperature composite-packing threshold, `is_220v_power`, door-open bit-23 handling, `total_layer_num` alias, `fun` bit-29 developer-mode heuristic, and `device()`/`fun()` fallback ordering all checked out clean against BambuStudio/OrcaSlicer/bambuddy/pybambu.

## 5. client/telemetry.rs, client/ams.rs, diagnostics/hms.rs, diagnostics/kprofile.rs — 1 CONFIRMED, 1 NEEDS-VERIFICATION

### `HmsSeverity::from_attr` reads the wrong field entirely — BUG-108 (Sev2)

Severity is derived from `attr` (`(attr >> 8) & 0x0F`) but the printer's real severity/message-level value lives in `code >> 16`. Two independent, unchanged sources (BambuStudio's `parse_hms_info`, pybambu's `get_HMS_severity`) both use `code`; bambuddy's `attr`-based formula was traced via its own git history to a same-day refactor regression (`dd02acd`), not an independent wire-behavior claim. `test_real_x2d_hms_entry`'s real captured X2D data currently locks in the wrong answer (`Unknown` instead of `Serious`). `reference/07_diagnostics_hms.md`'s severity formula needs the same correction — it was evidently written from bambuddy's post-regression behavior.

### `is_status_step`'s fault-magnitude threshold — BUG-109 (needs-verification)

`code_low < HMS_FAULT_THRESHOLD` (low 16 bits) vs. bambuddy's `code < 0x4000` (full 32-bit value) — these diverge whenever `code`'s upper 16 bits are nonzero. BambuStudio has no local equivalent to arbitrate with (relies on an external message catalog for relevance instead). Needs a live wire capture of a genuine low-`code_low`/high-`code` fault to settle; P1S-only hardware here can't produce that case.

### No other findings

`client/ams.rs`'s AMS-HT/standard dry-temp clamps (cite Bambu's own wiki directly, already strongest available source), IDEX `ams_id` 254/255 addressing (re-confirmed against `DevDefs.h`), and `kprofile.rs`'s outbound-only construction/client-side `setting_id` validation all check out with no BambuStudio contradiction. `client/telemetry.rs`'s `sanitized_ams()` stale-tray design and `is_ethernet_active_via_wifi_signal()`'s existing "disputed" caveat were both re-verified as already accurate — the latter corroborated further by `SideTools.cpp`/`DeviceManager.cpp:3053` showing BambuStudio derives wired-Ethernet from an unrelated `net.conf` bit, not a `wifi_signal` sentinel, but that's out of this unit's scope (lives in `report.rs`, unit 4) so not filed here.

## 6. ams/{mapping.rs, parser.rs, mod.rs} — 1 CONFIRMED, 1 PLAUSIBLE (cross-unit); AMS-HT bit-offset question closed post-sweep, see addendum

Context check: `AMS_TRAY_MERGE_PLAN.md` is deleted (commit `bb4fbd3`) — both phases already shipped (BUG-102/103), neither touched this unit's files directly. `clean_stale_tray_data`'s same-day BUG-083 fix and `build_ams_mapping[2]`'s BUG-070 fix already reflected in current source, not re-litigated.

### `evaluate_spool_presence` AMS-HT doc/behavior wrong — BUG-114 (Sev2, reopens BUG-015)

See BACKLOG.md row — BambuStudio's `GetTrayId`'s N3S branch shows AMS-HT does have a `tray_exist_bits` bit (`16 + (ams_id-128) + slot_id`), confirmed independently in OrcaSlicer. bambuddy's skip of `ams_id >= 128` traced via its own git history to "never wired up," not a competing wire-behavior claim.

### AMS-HT bit-offset collision risk — CLOSED, not a bug (BUG-115, moved to Wontfix)

Fully resolved post-sweep, not merely narrowed as first recorded here. User-supplied official Bambu Lab documentation confirms standard (non-HT) AMS units cap at 4 on every product line (H2/X2D, P2S, X1/P1, A1, A2L) — no exceptions. BambuStudio's own `DevAms::GetTrayId` hardcodes AMS-HT's base offset at `16`, which is only correct if standard units never exceed id 3 — i.e. BambuStudio's own protocol implementation assumes the same 4-unit cap. Collision was never possible.

This closure directly reopened **BUG-068** (previously Fixed, widened `AMS_MAX_STANDARD_ID` 3→7) as **BUG-125** (Open): re-checking bambuddy's actual commit history showed the `0-7` range predates the issue BUG-068 cited as justification by a month, with no observed evidence for a standard unit above id 3 — that issue (#1274) only confirms `ams_id=128` (AMS-HT). `AMS_MAX_STANDARD_ID` should revert to `3`. Not a `ModelQuirks` case (the ID boundary is protocol-wide) — see BUG-122 for the separate, genuinely model-dependent concern (outbound config validation against each model's actual AMS/AMS-HT pool structure).

### `ams_extruder_map` construction — PLAUSIBLE, cross-unit, not filed

`resolve_printing_global_id`'s `ams_extruder_map: &[u8]` parameter assumes an extruder-indexed array, but the wire mechanism (`ams.info` bitmask, bits 8-11) is `ams_id → extruder_id`, the inverse direction, and construction of any inverted array happens outside this unit's files (`types/telemetry/ams.rs` / `client/ams.rs`, units 1/5). Flagging for whoever next touches those units — not confirmable as a bug from this unit alone.

### Re-verified, no new issues

`MaterialSource::flat_channel_id`/`to_mapping2_entry` for `StandardAms`/`AmsHt`, external-spool ID assignments (254/255), `validate_external_spool_safety`'s single-nozzle override, and `resolve_global_tray_id`'s channel-ID scheme (distinct from the `tray_exist_bits` bit-index scheme above) all check out. `AMS_LITE_MIXED`'s tray-index scheme is entirely unmodeled — noted as a completeness gap only, not a bug (not P1S-relevant hardware).

## 7. mqtt/commands/{ams.rs, print_job.rs} — 4 CONFIRMED

Both files correctly follow the Payload+Request pattern and clamp every task-ID via `ClampedTaskId` — no `task-id-clamping.md` violations. Different audit shape than telemetry-decode units: bambino is *constructing* outbound JSON, so the comparison is field-name/type/value-formula parity with BambuStudio's own command construction, not merge-on-absence semantics.

### `AmsChangeFilamentPayload::target` semantics wrong — BUG-116 (Sev2)

Doc comment claims `target` mirrors `slot_id`; BambuStudio actually sends the global tray ID (`ams_id*4+slot_id`) for any standard unit other than 0. `client/ams.rs`'s `target_valid` check rejects the real value for non-zero units and accepts wrong ones instead — real misconfiguration risk, not just a doc gap.

### `AmsFilamentSettingRequest` IDEX `tray_id` wrong — BUG-117 (Sev2)

Doc comment claims `tray_id:0` for both Ext-L/Ext-R; BambuStudio always sends `254` (`VIRTUAL_TRAY_DEPUTY_ID`) for either virtual `ams_id`. Same wrong claim duplicated in two reference docs (`05_materials_ams.md:182`, `03_mqtt_telemetry.md:436`), both need correcting alongside the code doc comment.

### `AmsFilamentDryingPayload` entirely wrong field schema — BUG-118 (Sev2)

`dry_temp`/`dry_time` (minutes) vs. BambuStudio's and bambuddy's `temp`/`duration` (hours) plus missing `humidity`/`cooling_temp`/`close_power_conflict` — no field name in common with either source. bambuddy's own comment cites a real silent-rejection production incident (#1447) this exact mismatch shape would cause. Untested at the field level. Minutes→hours is a breaking API change to `PrintJobConfig`/`start_drying` — decide migration approach before fixing.

### `ProjectFilePayload` missing calibration/identity fields — BUG-119 (Sev2)

Missing `flow_cali`, `profile_id`, `project_id`, `task_id`, all sent by both bambuddy and pybambu independently. bambuddy cites two real production incidents (#1478 calibration-skip, #1042/#1011 task-continuation) tied to these exact fields. BambuStudio's own construction isn't visible in the open-source tree (closed-source network module) — corroboration is bambuddy+pybambu, not first-party. Decide-first: whether `project_id`/`task_id` reuse `subtask_id` or need independent minting.

## 8. mqtt/commands/{control.rs, gcode.rs, hardware.rs, status.rs, mod.rs} — NO ISSUES FOUND

All payload/field checks against BambuStudio (`DeviceManager.cpp`, `DevLampCtrl.cpp`, `DevFan.cpp`, `DevPrintOptions.cpp`, `DevAxisCtrl.cpp`) confirmed correct or intentionally minimal (corroborated by bambuddy/pybambu where BambuStudio's shape was richer — `StandardControlPayload`, `PushAllPayload`, `CleanPrintErrorPayload`, `AirductPayload.submode`). Task-ID clamping (`.claude/rules/task-id-clamping.md`) and the Payload+Request pattern (Key Invariant #3) both verified uniform across all 24 constructors in `src/mqtt/commands/`.

## Post-Sweep Verification Addendum (2026-07-12)

All 3 PLAUSIBLE findings and all 5 needs-verification findings from the sweep above were re-verified by a second parallel pass, digging past the first pass's sources (full git history, not just grep; bundled BambuStudio resource files outside `DeviceCore/`; a third independent reverse-engineering project, `bambu-printer-manager`; bambino's own test fixtures). Outcomes:

- **BUG-107** (`ts_boot`/`ts_unix`): CONFIRMED for X2 — pybambu's `MOCK-X2D.json` fixture (a sanitized real capture) has both fields; bambino's own reference-doc example value was silently copied from it. H2/P2 still unconfirmed — downgraded from a blanket claim to X2-only, doc-fix Sev3.
- **BUG-109** (`is_status_step` threshold): CONFIRMED real bug — BambuStudio's own bundled HMS fault catalog (`resources/hms/hms_en_093.json`, not checked in the first pass) shows 4591/4592 real cataloged faults have `code_low < 0x4000`, meaning bambino's `decode_hms_alert` check misclassifies nearly every real fault as a non-fault. Fix: compare full `code`, not `code_low` — the analogous `decode_print_error` check is separately confirmed correct as-is.
- **BUG-111** (nozzle-rack IDEX heuristic): CONFIRMED and elevated to Sev2 — BambuStudio's `DevNozzleSystem.cpp` confirms rack-stored spares share `nozzle.info` with installed nozzles; H2C is a currently-modeled printer in `MODEL_MATRIX.csv` with existing rack-aware code elsewhere in bambino (`client/thermal.rs:75-99`), so this is reachable on real supported hardware, not hypothetical.
- **BUG-113** (bed-temp value source): re-closed as **not a bug** — pybambu's own source contains a literal captured real payload with `device.bed_temp` and `device.bed.info.temp` both present with identical values; BambuStudio's `DevBed.cpp` (read in full) has no fallback path reading the nested field. Re-confirms BUG-054/081's original conclusion with primary-source evidence instead of inference. Moved to `Wontfix`.
- **BUG-115** (AMS-HT bit-offset collision): fully CLOSED as not-a-bug (moved to Wontfix), and its own closing evidence directly reopened **BUG-068** (Fixed, `AMS_MAX_STANDARD_ID` 3→7) as **BUG-125** (Open, revert to 3) — see the unit 6 section above and BACKLOG.md for the full chain. **BUG-122** was independently reframed from "widen `AMS_HT_ID_MAX`" (that constant was already correct) to the real gap: outbound config construction doesn't validate against each model's actual pool structure.
- **Unit 1 plausible findings**: `dry_fan1_status`/`dry_fan2_status` and `calibrate_remain_flag`/`.cfs` all CONFIRMED via a second independent source (`bambu-printer-manager`, plus `OpenBambuAPI`'s community protocol spec for `calibrate_remain_flag`) — promoted to BUG-120/BUG-121. `remain_g`/`setting_id` remain genuinely single-sourced (BambuStudio only) — checked bambino's own test fixtures, no corroborating capture found; stays PLAUSIBLE, unresolved without a real capture from a Bambu-Cloud-linked spool.
- **Unit 4's `sdcard` plausible finding**: original theory (string-form decoder gap) was wrong — BambuStudio's `DevStorage::ParseV1_0` parses `sdcard` strictly as bool, never a string. But surfaced a real, differently-shaped bug: bambino has no accessor for `home_flag` bits 8-9 (the actual first-party mechanism for degraded SD-card states), confirmed by BambuStudio + pybambu. Promoted to BUG-123.
- **Unit 6's `ams_extruder_map` plausible finding**: CONFIRMED and reframed — `resolve_printing_global_id` has zero callers anywhere in the crate (dead code), and the preferred resolution fields it should consult (`ExtruderInfo::snow`/`spre`/`star`) have no decoder at all. A half-finished feature, not a false alarm. Promoted to BUG-124.
- **New finding surfaced during BUG-115 re-verification**: bambuddy's own `ck_ams_id_range` CHECK constraint documents AMS-HT's real `ams_id` range as 128-191 (64 units), wider than bambino's `AMS_HT_ID_MAX=135` (8 units) — same shape as the already-fixed BUG-068. Filed as BUG-122.

### Remaining genuinely unresolved (hardware/capture-blocked, no further local verification possible)

- (none remaining in this category — the AMS-HT collision question above closed with a definitive answer, not a hardware-blocked one; see BUG-115/BUG-125.)
- **`AmsTray.remain_g`/`setting_id`** (unit 1, still PLAUSIBLE, no BUG-ID) — single-sourced to BambuStudio, no second source or capture found anywhere checked.


---

**`BACKLOG.md` is the status source of truth from here on** — this table is a point-in-time snapshot at sweep completion and won't be updated as bugs get fixed.

| BUG-ID | Sev | Module | File(s) | One-line |
|---|---|---|---|---|
| BUG-104 | Sev2 | types/telemetry/ams.rs | ams.rs | `dry_sub_status()` reads 4-bit mask, real field is 2 bits |
| BUG-105 | Sev3 | types/telemetry/diagnostics.rs, client/telemetry.rs | diagnostics.rs, client/telemetry.rs | `IpcamTelemetry` has no merge/cache |
| BUG-106 | Sev2 | types/telemetry/diagnostics.rs | diagnostics.rs | `HmsEntry.attr`/`.code` required, one bad entry fails whole message |
| BUG-107 | Sev3 | types/telemetry/diagnostics.rs | diagnostics.rs | `ts_boot`/`ts_unix` confirmed real on X2; H2/P2 unconfirmed |
| BUG-108 | Sev2 | diagnostics/hms.rs | hms.rs | `HmsSeverity::from_attr` reads wrong field (`attr` not `code`) |
| BUG-109 | Sev2 | diagnostics/hms.rs | hms.rs | `is_status_step` uses wrong field (`code_low` not `code`) |
| BUG-110 | Sev3 | types/telemetry/report.rs | report.rs | `is_ethernet_active()` bit-18 heuristic confirmed wrong |
| BUG-111 | Sev2 | types/telemetry/mod.rs | mod.rs | IDEX heuristic corrupts nozzle temps on H2C w/ rack nozzle |
| BUG-112 | Sev3 | types/telemetry/device.rs | device.rs | `snow`/`spre`/`star` doc comment wrong bit layout (doc-only) |
| BUG-114 | Sev2 | ams/parser.rs | parser.rs | AMS-HT presence doc/behavior wrong, reopens BUG-015 |
| BUG-115 | needs-verification | ams/parser.rs | parser.rs | AMS-HT presence bit-offset formula unresolved (narrowed, still capture-blocked) |
| BUG-116 | Sev2 | mqtt/commands/ams.rs | ams.rs | `AmsChangeFilamentPayload::target` semantics wrong |
| BUG-117 | Sev2 | mqtt/commands/ams.rs | ams.rs | IDEX external-spool `tray_id` wrong (0 vs 254) |
| BUG-118 | Sev2 | mqtt/commands/ams.rs | ams.rs | `AmsFilamentDryingPayload` entirely wrong field schema |
| BUG-119 | Sev2 | mqtt/commands/print_job.rs | print_job.rs | `ProjectFilePayload` missing calibration/identity fields |
| BUG-120 | Sev3 | types/telemetry/ams.rs | ams.rs | Missing `dry_fan1_status`/`dry_fan2_status` accessors |
| BUG-121 | Sev3 | types/telemetry/ams.rs | ams.rs | Missing `calibrate_remain_flag`/`.cfs` |
| BUG-122 | Sev3 | ams/mapping.rs | mapping.rs | Outbound AMS config not validated against per-model pool structure |
| BUG-123 | Sev3 | types/telemetry/mod.rs | mod.rs | No accessor for `home_flag` bits 8-9 (SD-card degraded states) |
| BUG-124 | Sev3 | ams/parser.rs, types/telemetry/device.rs | parser.rs, device.rs | `resolve_printing_global_id` dead code; `snow`/`spre`/`star` undecoded |
| BUG-125 | Sev2 | ams/parser.rs, ams/mapping.rs | parser.rs, mapping.rs | `AMS_MAX_STANDARD_ID=7` wrong, reopens/reverses BUG-068, revert to 3 |
