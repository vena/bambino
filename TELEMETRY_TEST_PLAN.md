# Telemetry Test Plan

## Background (read this before starting any phase)

A 2026-07-12 session found and fixed five "wholesale cache replace instead of
field-by-field merge" bugs — BUG-090 through BUG-094 — using two different
kinds of evidence: a real P1S wire capture, and cross-referencing two
independent reverse-engineering projects. Both methods are now established
and validated; **Phase 1 below exists to keep applying them systematically**,
not to redo what's already fixed.

**What's already closed** (see `BACKLOG.md` for full detail, don't re-litigate):
- BUG-090 — CLI dashboard (`bambino-cli monitor`) flat-overwrote `print`/`device`
  keys instead of recursively merging. Fixed with a generic `serde_json::Value`
  deep-merge in `dashboard.rs`.
- BUG-091 — `AmsStatusReport` (`src/types/telemetry/ams.rs`) replaced wholesale
  in `TelemetryCache`. Fixed with `AmsStatusReport::merge_from`, confirmed via
  a real P1S wire capture (`tests/mocks/P1S_print_sequence.ndjson`) showing
  `print.ams` arriving as `{"tray_tar":"3"}`-only pushes.
- BUG-092 — `bambino-cli dump --follow` had no MQTT keep-alive ping, dropped
  the connection after ~30s. Fixed.
- BUG-093 — `DeviceTelemetry` replaced wholesale in `TelemetryCache`. Fixed
  with `DeviceTelemetry::merge_from`, at the top level only (its own
  `nozzle`/`extruder`/`airduct`/`ctc`/`bed`/`ext_tool` fields).
- BUG-094 — `DeviceTelemetry::merge_from` didn't recurse into
  `NozzleCollection`/`ExtruderCollection`/`AirductCollection`'s own `Vec`
  fields. Fixed with `merge_from` on all three, confirmed via **cross-project
  source evidence** (see below) rather than a wire capture — P1S can't
  exercise `device.*` at all.

**Established verification policy — read this before treating anything below
as "no evidence, can't proceed":**

We have exactly one physical printer available for wire captures: a P1S. It
does not exercise `device.*`, and by construction never will (it's a
single-nozzle, older-protocol model) — no amount of P1S capturing resolves
questions about H2/P2/X2-specific wire behavior. BUG-094 established the
fallback: **`pybambu`
(`/Users/vena/Documents/Projects/Personal/ha-bambulab/custom_components/bambu_lab/pybambu`)
and `bambuddy`
(`/Users/vena/Documents/Projects/Personal/bambuddy/backend`) are authoritative
for wire-behavior questions this crate cannot verify directly against
hardware.** Both are mature, independently-developed reverse-engineering
projects with real users across the full model range; where they *agree* on
a behavior (especially a defensive "preserve on absence" pattern that only
makes sense if the authors hit the real wire behavior forcing it), treat that
as confirmed, same evidentiary bar this crate already used for BUG-012 pre
this session. Where only one of the two shows a pattern, or they disagree,
that's `needs-verification`, not confirmed — don't round up.

This changes Phase 1's scope from "wait for a hardware capture" to "go read
these two source trees" — most of the original phase text below has been
rewritten accordingly. `pybambu`'s fixture files
(`pybambu/tests/MOCK-*.json`, `H2D.json`) are single full-`pushall` snapshots,
not incremental sequences — useful for confirming field *names*/*shapes*
match `bambino`'s structs, **not** for confirming partial-push behavior
(that needs the source code's own update logic, not its test fixtures).

**Capture limitations** (for the parts of this plan that do use the P1S
capture directly — Phases 2 and 3 below):
- `tests/mocks/P1S_print_sequence.ndjson` — P1S only, 342 lines, one clean
  successful print (no HMS faults, no `print_error` events, no abnormal
  terminations). Phase 4 may legitimately come back empty — that's a valid
  outcome, don't fabricate a fault scenario to have something to show.
- Captured via `push_status` incremental pushes after an initial `pushall`,
  not `pushall` itself. For full-snapshot shape specifically, use
  `tests/mocks/P1S.json` instead.

**Working style:** `CLAUDE.md`'s Key Conventions apply — `make check-fast`
before every commit, one bug fix per commit with a `BUG-ID` filed via the
`backlog` skill and referenced in the commit message, real commit bodies.
Phases are independent unless an ordering dependency is called out. Don't
batch multiple phases' fixes into one commit even if convenient.

---

## Phase 1 — Systematic gap-finding via source cross-reference + capture diff

**Problem:** BUG-091/093/094 were each found by one-off manual reasoning (grep
`#[serde(default)]`, reason about which cached fields could be affected, then
separately go verify). That's ad hoc — it found five instances but there was
no systematic sweep across either `bambino`'s full telemetry surface or the
two reference projects' full `print_update`/message-handling logic. This
phase replaces the ad hoc approach with a systematic one, now that both
verification methods (capture diff, source cross-reference) are proven.

**Known remaining candidates — check these first, don't rediscover them from
scratch:**
- `BedTelemetry`/`BedInfo` (`device.rs`) — `DeviceTelemetry::merge_from`
  replaces `self.bed` wholesale when present; `BedTelemetry.info`/`.state`
  aren't merged independently of each other. `pybambu`'s handling
  (`models.py` ~L525-532, `bed_temp = data.get("device", {}).get("bed",
  {}).get("info", {}).get("temp", None)`, falls back to flat fields when
  absent) is suggestive but was written off in BUG-094 as "not evidence at
  this finer grain" — re-examine with the sibling-field question specifically
  (can `device.bed.state` arrive without `device.bed.info`, or vice versa?)
  rather than the top-level-presence question BUG-094 already answered.
- `CtcTelemetry`/`CtcInfo` (`diagnostics.rs`) — same shape as `BedTelemetry`,
  same open question. `pybambu` ~L542-547 shows the analogous fallback
  pattern for `ctc.info.temp`; `bambuddy` ~L2628-2660 has extensive
  `ctc_info` handling (`explicit_target`, respect-local-target logic) worth
  reading in full for this specific question, not just the excerpt already
  quoted in BUG-094's `BACKLOG.md` entry.
- `ExtToolTelemetry` (`device.rs`) — not examined by BUG-093 or BUG-094 at
  all. Search both reference projects for `ext_tool` handling from scratch.
- `AmsUnit`'s own `#[serde(default)]` fields (`ams.rs` — `humidity_raw`,
  `dry_time`, `dry_sf_reason`) — `AmsStatusReport::merge_from` (BUG-091)
  replaces the *whole* `ams: Vec<AmsUnit>` when the incoming one is
  non-empty; it does not merge individual `AmsUnit` entries field-by-field
  against their previous state. If a wire push resends the array with one
  unit's `dry_sf_reason` newly absent (array present, but that one field
  within one entry omitted), current code takes the new value (`None`),
  losing the old one — narrower and lower-probability than the whole-array
  case BUG-091 fixed, but the same shape. Check both reference projects for
  per-unit incremental update handling before deciding whether this is worth
  fixing or is a `needs-verification`/`Wontfix` (see the `backlog` skill's
  severity rubric — this may reasonably land as `Sev3` even if confirmed,
  since a stale `dry_sf_reason` briefly is much lower-impact than a vanished
  tray array).
- `AmsTray`'s own `#[serde(default)]` field(s) (`ams.rs:339`) — distinct from
  the above: this one already has dedicated stale-clearing logic
  (`clean_stale_tray_data`, BUG-083/BUG-012) that's *state-driven*, not
  presence-driven. Don't touch this without first reading BUG-083's fix
  commit and `clean_stale_tray_data`'s doc comment in full — a naive
  presence-based `merge_from` here would silently reintroduce BUG-083 in a
  different form (see "why not merge everything universally" reasoning
  already captured in this session's conversation history, not repeated here
  since this file must stand alone — the short version: some absences are
  legitimate *clear* signals, not just missed updates, and only
  `clean_stale_tray_data`'s state-based logic can tell the two apart).

**Method for each candidate:**
1. Grep both `pybambu/models.py` and `bambuddy/backend/app/services/bambu_mqtt.py`
   (and `mqtt_relay.py`/`mqtt_bridge.py` if the first doesn't have it — the
   logic may live in a different file than it did for the fields BUG-094
   checked) for the relevant wire path.
2. Confirm *both* projects show the same preserve-on-absence handling before
   treating it as confirmed. One project alone is `needs-verification`.
3. If confirmed: implement `merge_from` the same way as the five existing
   ones, file a `BUG-ID`, fix, test, commit — one bug per commit, same as
   this session.
4. If unconfirmed or the two projects disagree: file as
   `needs-verification` per the `backlog` skill, do not fix speculatively,
   move to the next candidate.

**Separately, for P1S-coverable fields only** (i.e. not `device.*`): the
original consecutive-message-diff approach against
`P1S_print_sequence.ndjson` is still valid and untried — walk the capture,
diff each message's object tree against the last time each key was seen, and
cross-reference against `TelemetryCache` fields not yet covered by a
`merge_from`. This can only find P1S-model gaps (e.g. within `AmsStatusReport`
itself, `VirtualTray`, top-level `PrinterTelemetry` fields) — it cannot find
`device.*` gaps, use the source cross-reference method above for those.

**Decide first, if you build the diff tool:** one-off script (fast, not
repeatable) vs. a checked-in `#[test]` regression helper (more work, becomes
a standing check the moment a new fixture is added). No strong recommendation
either way this time — depends how many more gaps Phase 1 turns up now that
the low-hanging ones are fixed; three-plus new instances is the threshold
this crate's quirks-engine precedent uses to justify extracting a shared
strategy rather than one-off code per case.

---

## Phase 2 — End-to-end accessor replay across the full sequence

**Unchanged from the original plan — not addressed this session.**

**Problem:** every existing telemetry test exercises one hand-written or
single-real-message fixture at a time. None replay a *sequence* through the
actual stateful cache the way a real `PrinterClient` session does. A bug that
only manifests after N messages of accumulated state has no coverage right
now.

**Design constraint — read before writing this test:** `TelemetryCache`'s
`update_*_cache` methods (`src/client/telemetry.rs`) are private methods on
the generic `PrinterClient<...>` struct, not on `TelemetryCache` itself. To
replay the capture through the real merge logic without spinning up a full
mock MQTT/TLS/IO stack, you have two paths:
- *Option A:* build the heavy harness anyway (real precedent:
  `tests/mqtt_test.rs`'s `test_mqtt_client_lifecycle_and_telemetry` already
  drives a `PrinterClient` through mocked I/O end-to-end — feed it this
  capture's raw payloads instead of synthetic ones, in order, and call
  `poll_telemetry()` per message).
- *Option B (decide first):* refactor `update_telemetry_cache` and its
  `update_*_cache` helpers to take `&mut TelemetryCache` and `&dyn
  ModelQuirks` as explicit parameters instead of `&mut self` on the generic
  `PrinterClient`, so they're callable directly from a plain unit test with no
  mock I/O at all. Worth it only if Phase 1 turns up enough additional
  merge-logic bugs that testing them keeps requiring the heavy harness — if
  this is the only reason to do it, Option A is less work for a one-off
  replay test. State which you picked and why in the commit message.

**What to assert**, once you have a way to replay the sequence through real
cache-update logic:
1. No accessor call (`print_status()`, `bed_temperatures()`,
   `nozzle_temperatures()`, `print_progress()`, `active_fault()`, `hms()`,
   `ams()`, every public getter on `PrinterClient`'s telemetry surface) panics
   or returns an `Err` on any of the 342 messages.
2. No accessor's return value is physically nonsensical at any point in the
   sequence — temperatures within a plausible range, percentages in
   `0..=100`, no `NaN`/overflow. Pick bounds generously; this is a sanity
   net, not a precision spec.
3. Monotonicity/coherence spot checks where the domain supports them — e.g.
   `mc_percent` shouldn't jump backwards except at a known reset point. Don't
   invent state-machine rules that aren't already documented somewhere in
   this crate (`reference/`, doc comments).

**Independent of Phase 1** — either order, though replaying against
post-Phase-1-fixes code is strictly more useful than pre-fix.

---

## Phase 3 — `gcode_state` coverage against the real state machine

**Unchanged from the original plan — not addressed this session.**

**Problem:** does every distinct `gcode_state` string this real P1S emitted
across a full print map to a known, handled variant in whatever this crate
uses to interpret print status, or does any of them silently fall through to
a default/unknown branch?

**What to do:**
1. `jq -r '.print.gcode_state // empty' tests/mocks/P1S_print_sequence.ndjson
   | sort -u` for the exhaustive set of distinct values this capture
   contains.
2. Locate the code that maps `gcode_state` strings to a status type — verify
   the actual type/name against current source, don't assume it's still
   called what it might have been called before.
3. For each captured state string, confirm an explicit match arm exists, not
   just a catch-all default. Any fallthrough is a real finding — file via the
   `backlog` skill, judge severity against the rubric rather than defaulting
   to `Sev3`.
4. Optional stretch, not required scope: extend the same check to other
   enumerated-string fields (`mc_print_stage`, `print_type`).

**Independent of Phase 1 and 2.**

---

## Phase 4 — Wire-verified HMS/print_error test upgrades

**Unchanged from the original plan — not addressed this session.**

**Problem:** this crate's convention (BUG-088's P1S hardware confirmation,
BUG-083's Bambuddy cross-check) prefers a real wire-confirmed value over a
hand-typed synthetic one wherever possible. If the capture contains any
non-trivial `hms` array entries or a non-zero `print_error`, that's a real
value worth using to upgrade `decode_hms_alert`/`decode_print_error`'s tests.

**What to do:**
1. `jq -c 'select((.print.hms // [] | length) > 0) | .print.hms'
   tests/mocks/P1S_print_sequence.ndjson` and `jq -c 'select(.print.print_error
   != null and .print.print_error != 0) | .print.print_error'`.
2. **If both return nothing** (plausible, expected — clean successful print):
   report explicitly and stop. Don't fabricate a value to have something to
   upgrade.
3. **If either returns a real value:** cross-check against
   `decode_hms_alert`/`decode_print_error`'s logic and `reference/` docs,
   then strengthen or add a test citing the capture as source, same style as
   `BUG-088`/`BUG-083`'s fix commits.

**Independent of all other phases.**
