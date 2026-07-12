# Telemetry Test Plan

## Background (read this before starting any phase)

A 2026-07-12 session found and fixed three "wholesale cache replace instead of
field-by-field merge" bugs (BUG-090, BUG-091, BUG-093) by capturing a real P1S's
MQTT traffic across an entire print (start through finish) with a new
`bambino-cli dump --follow` flag (BUG-092 fixed a keep-alive bug in that same
flag). The capture is saved at `tests/mocks/P1S_print_sequence.ndjson` — 342
lines, one raw JSON message per line, in wire order. It is scanned clean of
credentials/serials/IPs (see the BUG-091/092/093 fix commits for the audit).

**Capture limitations, read before drawing conclusions from it:**
- P1S only — a single-nozzle, older-protocol model. It never sends a top-level
  `"device"` object at all (confirmed empirically: `jq 'select(.device != null
  or .print.device != null)'` against the file returns nothing). Any
  investigation that needs `device.*` wire behavior (nozzle/extruder/airduct/ctc
  partial-push shape) has **zero evidence** from this file and must not guess
  from it — either get a capture from an H2/P2/X2 (models that do use
  `device.*`, per `src/types/telemetry/device.rs`'s own doc comments) or mark
  the finding `needs-verification` per the `backlog` skill's severity rubric
  and stop there.
- One clean, successful print — no HMS faults, no `print_error` events, no
  abnormal terminations observed. Phases below that look for fault-path
  coverage may legitimately come back empty. An empty result is a valid
  outcome to report, not a sign the phase was done wrong — do not fabricate a
  fault scenario to have something to show.
- Captured via `push_status` incremental pushes after an initial `pushall`,
  not `pushall` itself. If a phase needs to reason about full-snapshot shape
  specifically, use `tests/mocks/P1S.json` (the older, single-message fixture)
  instead, or the first line(s) of this capture.

**Reference for the bug pattern these phases hunt for:** `AmsStatusReport::merge_from`
(`src/types/telemetry/ams.rs`) and `DeviceTelemetry::merge_from`
(`src/types/telemetry/device.rs`) are the two fixed instances. Both exist
because a `Vec<T>` field with `#[serde(default)]` silently defaults to empty
when its wire key is absent (not explicitly emptied), and naive caching code
that replaces a whole struct on any `Some(_)` push loses that data. Search for
this same shape (`#[serde(default)]` on a `Vec` or nested struct field, paired
with a cache/accumulator that replaces rather than merges) — it is the thing
every phase below is ultimately in service of finding or ruling out.

**Working style:** this file's rules come from `CLAUDE.md`'s Key Conventions —
run `make check-fast` before every commit, one bug fix per commit with
`BUG-ID` filed via the `backlog` skill and referenced in the commit message,
write real commit bodies (the "why," not just the "what"). Each phase below is
independent enough to run in any order except where an ordering dependency is
called out explicitly. Do not batch multiple phases' fixes into one commit
even if convenient — each finding gets its own `BUG-ID` and its own commit,
same discipline as the session that produced this file.

---

## Phase 1 — Systematic gap-finding via consecutive-message diff

**Problem:** BUG-091 and BUG-093 were found by manual inspection (grep for
`#[serde(default)]`, then reason about which cached struct fields could be
affected). That's guesswork with gaps — it only found two instances and
explicitly stopped short of `NozzleCollection`/`ExtruderCollection`/
`AirductCollection`'s own `Vec` fields for lack of evidence. This phase
replaces guessing with a systematic scan of the actual capture.

**What to build:** a script or test (language/location is a **decide-first**
item — see below) that:
1. Parses every line of `P1S_print_sequence.ndjson` as JSON.
2. Walks each message's object tree, and for every message after the first
   full one, records which keys are present vs. which sibling keys were seen
   in a *prior* message at the same tree path but are absent in this one.
3. Cross-references the set of "sometimes-partial" object paths against every
   `#[serde(default)]` `Vec`/`Option<Vec<_>>` field in `src/types/telemetry/`
   (grep `#[serde(default)]` near a `Vec<` field, same technique used to find
   the original two bugs, but exhaustively rather than by manual reasoning).
4. Cross-references *that* list against `TelemetryCache`'s fields
   (`src/client/telemetry.rs`) to find which ones are cached via wholesale
   `Some(x.clone())` replacement rather than a `merge_from`-style call.
5. Outputs a list of "confirmed partial on the wire AND wholesale-replaced in
   cache" — each one is a new `BUG-ID` candidate, filed via the `backlog`
   skill, fixed the same way as BUG-091/093 (add `merge_from`, wire it into
   `update_*_cache`, add a unit test, add a capture-replay assertion if it
   fits Phase 2's harness).

**Decide first:** where does the scan script live?
- *Option A:* a throwaway analysis script (Python, since `jq`/`python3` are
  confirmed available in this environment — see this session's tool use) run
  once, not committed. Fastest, but the analysis isn't repeatable against a
  future capture (e.g. once an H2/P2/X2 capture exists).
- *Option B:* a `#[test]` in `src/types/telemetry/tests/` that asserts *no*
  currently-uncovered partial-push pattern exists against the checked-in
  fixture, so it becomes a standing regression check the moment a new fixture
  is added (mirrors `test_p1s_print_sequence_ams_merge_never_regresses`,
  generalized). More upfront work, more lasting value.

Recommendation if you don't have a strong reason otherwise: Option A to do the
one-time gap analysis fast, but if it finds more than one new gap, promote the
"walk the tree, diff against a prior full message" logic to a small
`pub(crate)` test helper (Option B) rather than writing the same ad hoc script
per bug — three-plus instances of the same shape is exactly the "extract a
strategy" threshold `CLAUDE.md`'s quirks-engine precedent uses elsewhere in
this crate.

**Also revisit while here:** the scope note left in `DeviceTelemetry::merge_from`'s
doc comment and BUG-093's `Detail` column — `NozzleCollection.info`,
`ExtruderCollection.info`, `AirductCollection.parts`/`mode_list` all have the
same `#[serde(default)] Vec` shape as `AmsStatusReport.ams` but no wire
evidence either way (P1S never sends `device.*` at all). This phase's scan
can't resolve that (no `device` data in this capture to scan) — it stays
`needs-verification` until an H2/P2/X2 capture exists. Don't fix it
speculatively; don't close it either. Leave it exactly as filed.

---

## Phase 2 — End-to-end accessor replay across the full sequence

**Problem:** every existing telemetry test (including the ones this session
added) exercises one hand-written or single-real-message fixture at a time.
None of them replay a *sequence* through the actual stateful cache the way a
real `PrinterClient` session does. A bug that only manifests after N
messages of accumulated state (an accessor that's fine on message 1 but wrong
by message 200) has no coverage at all right now.

**Design constraint — read before writing this test:** `TelemetryCache`'s
`update_*_cache` methods (`src/client/telemetry.rs`) are private methods on
the generic `PrinterClient<...>` struct, not on `TelemetryCache` itself. To
replay the capture through the real merge logic without spinning up a full
mock MQTT/TLS/IO stack (the heavy harness `tests/mqtt_test.rs` and
`tests/ftps_test.rs` use), you have two paths:
- *Option A:* build the heavy harness anyway (real precedent:
  `tests/mqtt_test.rs`'s `test_mqtt_client_lifecycle_and_telemetry` already
  drives a `PrinterClient` through mocked I/O end-to-end — feed it this
  capture's raw payloads instead of synthetic ones, in order, and call
  `poll_telemetry()` per message).
- *Option B (decide first):* refactor `update_telemetry_cache` and its
  `update_*_cache` helpers to take `&mut TelemetryCache` and `&dyn
  ModelQuirks` as explicit parameters instead of `&mut self` on the generic
  `PrinterClient`, so they're callable directly from a plain unit test with no
  mock I/O at all. This is a real, non-trivial refactor (touches every
  call site in `client/telemetry.rs`) — worth it only if Phase 1 or this
  phase turns up enough additional merge-logic bugs that testing them keeps
  requiring the heavy harness. If this is the *only* reason to do it, Option A
  is less work for a one-off replay test. State which you picked and why in
  the commit message — this is exactly the kind of judgment call `CLAUDE.md`
  asks to surface, not resolve silently.

**What to assert**, once you have a way to replay the sequence through real
cache-update logic:
1. No accessor call (`print_status()`, `bed_temperatures()`,
   `nozzle_temperatures()`, `print_progress()`, `active_fault()`, `hms()`,
   `ams()`, every public getter on `PrinterClient`'s telemetry surface) panics
   or returns an `Err` on any of the 342 messages.
2. No accessor's return value is physically nonsensical at any point in the
   sequence — temperatures within a plausible range (not negative, not absurdly
   high), percentages in `0..=100`, no `NaN`/overflow. Pick bounds generously;
   this is a sanity net, not a precision spec.
3. Monotonicity/coherence spot checks where the domain supports them —
   e.g. `mc_percent` shouldn't jump backwards except at a known reset point
   (a new print starting). Don't invent state-machine rules that aren't
   already documented somewhere in this crate (`reference/`, doc comments) —
   if you're not sure a transition is illegal, don't assert it's illegal.

**Independent of Phase 1** — can be done in either order, or in parallel by a
different session, since it doesn't depend on Phase 1's findings (though if
Phase 1 already produced new `merge_from` fixes, replaying against the
post-fix code is strictly more useful than pre-fix).

---

## Phase 3 — `gcode_state` coverage against the real state machine

**Problem:** does every distinct `gcode_state` string this real P1S emitted
across a full print map to a known, handled variant in whatever this crate
uses to interpret print status (`PrintStatus` or equivalent — locate it via
`ctx_search`/grep for `gcode_state` consumers in `src/client/` and
`src/types/`), or does any of them silently fall through to a default/unknown
branch?

**What to do:**
1. `jq -r '.print.gcode_state // empty' tests/mocks/P1S_print_sequence.ndjson
   | sort -u` (or equivalent) to get the exhaustive set of distinct values
   this capture contains.
2. Locate the code that maps `gcode_state` strings to `PrintStatus` (or
   whatever the actual type is — don't assume the name, verify it against
   current source).
3. For each state string GraphQL captured, confirm it has an explicit match
   arm, not just a catch-all `_ => Unknown`/default. If any capture value
   *does* fall through, that's a real finding — file it as a `BUG-ID` per the
   `backlog` skill (severity: likely Sev3 unless the fallback produces a
   materially wrong status, e.g. reporting `Idle` when actually `Running` —
   judge severity against the rubric, don't default to Sev3 reflexively).
4. If time/scope permits, extend the same check to other enumerated-string
   fields this capture exercises (`mc_print_stage`, `print_type`) — but
   `gcode_state` is the one explicitly asked for; treat the rest as optional
   stretch, not required scope.

**Independent of Phase 1 and 2** — purely a lookup-and-cross-check, no shared
state or ordering dependency.

---

## Phase 4 — Wire-verified HMS/print_error test upgrades

**Problem:** this crate's existing convention (see `BUG-088`'s P1S hardware
confirmation, `BUG-083`'s Bambuddy cross-check in `BACKLOG.md`) strongly
prefers a real wire-confirmed value over a hand-typed synthetic one wherever
possible. If this capture happens to contain any non-trivial `hms` array
entries or a non-zero `print_error`, that's a real value worth using to
upgrade or add a test for `decode_hms_alert`/`decode_print_error`
(`src/diagnostics/`) instead of (or alongside) whatever synthetic values
those decoders' existing tests use.

**What to do:**
1. `jq -c 'select((.print.hms // [] | length) > 0) | .print.hms'
   tests/mocks/P1S_print_sequence.ndjson` and `jq -c 'select(.print.print_error
   != null and .print.print_error != 0) | .print.print_error'` — check both.
2. **If either query returns nothing** (plausible and expected — this was a
   clean successful print per this file's Background section): report that
   explicitly and stop. Do not fabricate an HMS code or error value to have
   something to upgrade. An empty result here is the correct, expected
   outcome for a fault-free print capture, not a failure of this phase.
3. **If either query returns a real value:** cross-check it against
   `decode_hms_alert`/`decode_print_error`'s existing logic and reference
   docs (`reference/`) for what that code/value is documented to mean, then
   either strengthen an existing test or add a new one using this real value,
   citing the capture as the source (mirror how `BUG-088`/`BUG-083`'s fix
   commits cite their hardware-confirmation source in `BACKLOG.md`).

**Independent of all other phases.**
