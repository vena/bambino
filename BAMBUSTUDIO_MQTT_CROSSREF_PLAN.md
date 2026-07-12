# BambuStudio/OrcaSlicer MQTT Surface Cross-Reference Plan

## Background (read this before starting)

A 2026-07-12 session found and fixed six telemetry merge-logic bugs
(BUG-095/096/097/098/100/101 — see `BACKLOG.md`'s `Fixed` table) by
cross-referencing bambino's `merge_from` implementations against
BambuStudio's actual parsing source
(`/Users/vena/Documents/Projects/Personal/BambuStudio`, confirmed identical
in OrcaSlicer at `/Users/vena/Documents/Projects/Personal/OrcaSlicer` for
everything checked so far) — a materially stronger evidence source than the
`pybambu`/`bambuddy` reverse-engineering projects this crate had relied on
exclusively before that session, because BambuStudio is the actual
official first-party client, not a third-party reimplementation.

That session covered exactly the structs it happened to be auditing for
merge-on-partial-push correctness (`device.*`, `ctc`, `ext_tool`, `bed`,
`ams` at the unit level). It was not a systematic sweep — most of bambino's
telemetry-decode and command-construction surface has never been
cross-checked against BambuStudio at all. This plan scopes that sweep.

**This plan reuses the `deep-review` skill's orchestration mechanics**
(parallel per-unit agents, incremental on-disk persistence to a dated
`MM-DD-REVIEW.md`, `CONFIRMED`/`PLAUSIBLE` tiering, promotion to
`BACKLOG.md`) — read that skill in full before starting, since Steps 4-6
below are "do exactly what `deep-review` does," not reproduced here. What
this plan overrides is `deep-review`'s Step 1 (discovery), Step 2
(partition), and the audit methodology inside Step 3's agent prompts —
`deep-review`'s default methodology is pure-internal correctness against
`CLAUDE.md`/`.claude/rules/`; this plan's methodology is cross-referencing
against three external source trees instead (or in addition).

## Scope

**In scope** — the part of bambino that BambuStudio has a genuine analog
for: MQTT telemetry decoding (`src/types/telemetry/`), the telemetry cache
and its accessors (`src/client/telemetry.rs`, `src/client/ams.rs`),
diagnostics decode (`src/diagnostics/`), AMS channel/mapping logic
(`src/ams/`), and outbound MQTT command payload construction
(`src/mqtt/commands/`).

**Out of scope** — no BambuStudio analog exists, cross-referencing would be
wasted effort: `src/ftps/` (file transfer, BambuStudio uses a different
mechanism entirely), `src/camera/` (RTSP/binary JPEG streaming, no MQTT
involvement), `src/discovery/` (SSDP, unrelated to MQTT), `src/io/`
(platform I/O abstraction, no wire-format content), `src/mqtt/client/`
(low-level MQTT v3.1.1 packet framing — BambuStudio uses its own MQTT
library at a different abstraction level; the useful comparison ground is
the JSON payload semantics one layer up, in `src/mqtt/commands/` and
`src/types/telemetry/`, not the packet framing itself).

## Methodology — the part `deep-review`'s default doesn't cover

Each unit's agent needs a different Step 3 audit approach than
`deep-review`'s default "check against `CLAUDE.md`/`.claude/rules/`" —
instead, for every struct field / decode function / command payload field
in the unit's file list:

1. **Locate BambuStudio's parsing or construction code for the same wire
   data.** Telemetry decode lives under
   `BambuStudio/src/slic3r/GUI/DeviceCore/*.cpp` (per-domain files —
   `DevChamber.cpp`, `DevBed.cpp`, `DevFilaSystem.cpp`,
   `DevExtensionTool.cpp`, `DevNozzleSystem.cpp`, `DevExtruderSystem.cpp`,
   `DevHMS.cpp`, `DevFan.cpp`, etc. — `ctx_tree` that directory fresh,
   don't assume this list is exhaustive) and the generic reconstruction
   layer at `BambuStudio/src/slic3r/Utils/json_diff.cpp`
   (`json_diff::restore_objects`/`diff2all`, wired into
   `MachineObject::parse_json` in `DeviceManager.cpp` for any message
   tagged `print.msg == 1` — see BUG-095's `Fixed` row in `BACKLOG.md` for
   the full trace if unfamiliar with this mechanism, then re-verify it
   yourself rather than trusting that summary). Outbound command
   construction lives under `BambuStudio/src/slic3r/GUI/DeviceManager.cpp`
   (`command_*` methods) and wherever the UI action that triggers each
   command lives (varies per command, search by the MQTT `command:` string
   literal bambino's own payload sends).
2. **Check the same code in OrcaSlicer.** So far every file checked this
   session was byte-identical or near-identical between the two (not
   diverged) — but don't assume that holds everywhere; OrcaSlicer is an
   independently-developed fork and could have genuinely diverged on
   newer/different hardware support. Note explicitly, per finding, whether
   you checked both or only one and why.
3. **Determine what tier of evidence you have, per field:**
   - **Confirmed field-preserve/absence pattern**: BambuStudio's parser
     uses `.contains()`/`ParseVal`'s no-default overload against a
     persistent object (preserve-on-absence), or explicitly resets
     (absence = clear) — either way, that's real, load-bearing evidence
     for how bambino's `merge_from` should treat that field. Compare
     against bambino's actual current behavior for that field, not what
     you assume it does — re-read the current source, this crate has
     changed several of these already this session.
   - **BambuStudio doesn't model the field/struct at all.** Not itself
     evidence of anything — check `bambuddy`
     (`/Users/vena/Documents/Projects/Personal/bambuddy/backend`) and
     `pybambu`
     (`/Users/vena/Documents/Projects/Personal/ha-bambulab/custom_components/bambu_lab/pybambu`)
     next, same as the BUG-095 (bed) and BUG-097 (`ext_tool`'s
     `mount`/`low_prec`/`th_temp`) precedent — those stayed
     under-evidenced in different ways (BUG-095 genuinely unconfirmed,
     BUG-097's unmodeled fields extended anyway for internal consistency
     rather than left inconsistent with their siblings). Match whichever
     precedent actually fits, don't default to one mechanically.
   - **BambuStudio confirms one behavior, `bambuddy`/`pybambu` show
     another (a real disagreement, not just "one source is silent").**
     This is the carve-out case the user asked about directly — **don't
     assume BambuStudio wins by default.** `bambuddy` has had substantially
     more recent development against newer H2/P2/X2-generation hardware
     than this session's BambuStudio checkout necessarily reflects, and
     has fixed real production incidents BambuStudio may not have hit yet
     (or may have fixed differently, or not at all). Check `bambuddy`'s
     (and `pybambu`'s) own git history for the disagreeing line before
     concluding anything — `git log -L <start>,<end>:<path>` or `git log
     --oneline -L`, same technique that resolved BUG-098's apparent
     pybambu/bambuddy disagreement over `dry_time` (pybambu's own commit
     history showed its outlier behavior was an expedient crash-fix, not a
     competing wire-behavior claim — that reframed a "two sources
     disagree" situation into "three sources actually agree once you
     understand why one looks different"). A disagreement that survives
     this check (both sides show real, deliberate, still-current logic,
     for what looks like the same wire behavior) stays genuinely
     unresolved — file it as `needs-verification`, don't pick a side by
     authority alone.
4. **Confidence tags, adapted from `deep-review`'s default:**
   `CONFIRMED` (BambuStudio + at least one of bambuddy/pybambu agree, or a
   disagreement resolved via the git-history check above), `PLAUSIBLE`
   (looks like a real gap but only one source, or the resolution above
   didn't fully settle it), `NEEDS-VERIFICATION` (genuine, irreconcilable
   disagreement between sources, or something only real H2/P2/X2 hardware
   can settle — this crate has only P1S available, same constraint
   `TELEMETRY_TEST_PLAN.md`'s session operated under). This third tier
   doesn't exist in `deep-review`'s default two-tier scheme — use it, and
   when promoting to `BACKLOG.md`, file `NEEDS-VERIFICATION` findings with
   `Sev` column `needs-verification` per the `backlog` skill's existing
   convention (see `BUG-095`/`BUG-102`'s rows for the format), not folded
   into `CONFIRMED`.
5. **This sweep finds and files. It does not fix in the same pass** — same
   as `deep-review`'s own Step 4 (promote to `BACKLOG.md`'s `Open` table,
   don't auto-fix). Several of this session's findings turned out to need
   a "decide first" design discussion before any code changed (BUG-098's
   array-keying semantics, the sanitizer-wiring question now in
   `AMS_TRAY_MERGE_PLAN.md`) — expect the same here. Filing cleanly now
   is more valuable than a rushed fix that reopens the same
   design question later.

## Step 1/2 override — discovery and partition

Don't reuse `deep-review`'s generic "walk `src/` fresh" — the relevant
discovery here spans four repos, not one. Confirm current file layout in
all of:
- `bambino/src/{types/telemetry,client,diagnostics,ams,mqtt/commands}/`
  (`ctx_tree`, depth 2-3)
- `BambuStudio/src/slic3r/GUI/DeviceCore/` and
  `BambuStudio/src/slic3r/GUI/DeviceManager.cpp` (the file list already
  found this session — `DevBed`, `DevChamber`, `DevExtensionTool`,
  `DevFilaSystem`, `DevNozzleSystem`, `DevExtruderSystem`, `DevFan`,
  `DevHMS`, etc. — will very likely have grown since; re-list, don't trust
  this document's names as exhaustive)
- `OrcaSlicer/src/slic3r/GUI/DeviceCore/` (same, for the divergence check)
- `bambuddy/backend/app/services/bambu_mqtt.py` and
  `ha-bambulab/custom_components/bambu_lab/pybambu/models.py` (the two
  files this session's cross-referencing lived in almost entirely — verify
  the logic hasn't moved to a different file since)

**Candidate partition** (weight/rebalance per `deep-review`'s Step 2 rules
against actual current file sizes — these are 2026-07-12 line counts, will
have drifted):

1. `types/telemetry/{ams.rs, tests/ams.rs}` (~1100 lines combined) — AMS
   unit/tray/dry-setting decode and merge. Natural continuation of
   `AMS_TRAY_MERGE_PLAN.md`'s Phase 1 if that hasn't landed yet by the
   time this runs — coordinate rather than duplicate if so.
2. `types/telemetry/{device.rs, tests/device.rs}` (~1100 lines) — nozzle/
   extruder/airduct/bed/ctc/ext_tool, largely audited already this session
   (BUG-095/096/097/101) but not exhaustively — re-check every field, not
   just the ones already fixed, since this session's own audit was itself
   prompted by going "one level deeper" repeatedly and still didn't reach
   every leaf (see `AMS_TRAY_MERGE_PLAN.md`'s Finding 1/2 as the example of
   what "one level deeper still" turned up).
3. `types/telemetry/{diagnostics.rs, tests/ctc.rs, tests/bed.rs}` (~230
   lines) — small, HMS entry shape specifically (not `decode_hms_alert`
   itself, that's unit 5 below) plus what's left of ctc/bed after unit 2.
   Consider merging into unit 2 if it's too thin on its own by the time
   this runs, per `deep-review`'s merge-thin-units rule.
4. `types/telemetry/{report.rs, mod.rs, tests/misc.rs, tests/fun_field.rs,
   tests/nozzle.rs}` (~1250 lines) — top-level `PrinterTelemetry`/
   `TelemetryReport` fields, `decode_bed_temperatures`/
   `decode_nozzle_temperatures`/`unpack_temperature` and siblings.
5. `client/telemetry.rs`, `client/ams.rs`, `diagnostics/hms.rs`,
   `diagnostics/kprofile.rs` (~1370 lines) — the cache/accessor layer and
   HMS/k-profile decode. Cross-reference against BambuStudio's derived
   accessors (e.g. `DevAms`'s `dry_status`/`dry_sub_status`/
   `ams_extruder_map` bit-derivation logic, `DevHMS.cpp`) for decode logic
   bambino might be missing entirely, not just merge-semantics gaps.
6. `ams/{mapping.rs, parser.rs}` (~990 lines) — channel-ID/mapping math and
   the `evaluate_spool_presence`/`clean_stale_tray_data`/
   `resolve_global_tray_id` toolkit. Directly overlaps
   `AMS_TRAY_MERGE_PLAN.md`'s Finding 2 — if that plan's Phase 2 already
   landed by the time this runs, treat its outcome as settled context, not
   something to re-litigate; if it hasn't, this unit's findings should
   feed into that plan's still-open decision rather than filing a
   competing one.
7. `mqtt/commands/{ams.rs, print_job.rs}` (~535 lines) — outbound AMS
   control and print-job-start payloads. Different audit shape than units
   1-6: these are bambino *constructing* JSON to send, so the comparison
   is "does BambuStudio's own command-sending code populate the same
   fields the same way," not merge-on-absence semantics.
8. `mqtt/commands/{control.rs, gcode.rs, hardware.rs, status.rs, mod.rs}`
   (~840 lines) — remaining outbound commands (fan/temperature/motion/
   calibration/lighting), same construction-comparison shape as unit 7.

Rebalance if actual current sizes/complexity don't match this snapshot —
this is a starting point, not a mandate.

## Steps 3-6 — as `deep-review`, with this plan's methodology substituted into Step 3

Follow `deep-review`'s Step 3 requirements (Step 0 lean-ctx mandate, read
`CLAUDE.md`/`README.md`, unit's file list, relevant `.claude/rules/*.md`/
nested `CLAUDE.md` invariants, self-contained framing) verbatim, but
replace its default "Scope rules" audit methodology (item 5 in its Step 3)
with this plan's Methodology section above, and its output contract's
`CONFIRMED`/`PLAUSIBLE` tiering with this plan's three-tier scheme
(`CONFIRMED`/`PLAUSIBLE`/`NEEDS-VERIFICATION`). Follow `deep-review`'s
Steps 4 (spawn + incremental persistence + promote `CONFIRMED`/
`NEEDS-VERIFICATION` to `BACKLOG.md`), 5 (finalize), and 6 (report) exactly
as written — no changes needed there, the mechanics are methodology-agnostic.

One addition to Step 4's promotion step: when a finding's `Detail` cites
external source evidence (BambuStudio/OrcaSlicer/bambuddy/pybambu file:line
references), write the citation the same way this session's BUG-095/096/
097/098/100/101/102 entries did in `BACKLOG.md` — specific enough that a
future session can re-locate and re-verify without re-deriving the search
from scratch, since (per this session's own experience re-litigating
BUG-102's assumptions) a citation without enough detail to re-check
invites exactly the "trust the fix commit forever" failure mode this
plan's own Methodology section is trying to avoid.
