# AMS Tray Merge & Sanitization Plan

## Background (read this before starting either phase)

A 2026-07-12 session fixed six telemetry "wholesale replace instead of
field-by-field merge" bugs at the `device.*`/`ctc`/`ext_tool`/`bed` level and
the AMS *unit* level (BUG-095, 096, 097, 098, 100, 101 — see `BACKLOG.md`'s
`Fixed` table for the full detail, don't re-litigate). BUG-098 specifically
gave `AmsStatusReport.ams: Vec<AmsUnit>` a **keyed per-unit** merge
(`AmsStatusReport::merge_from` in `src/types/telemetry/ams.rs`): a unit not
mentioned in an incoming push's array stays cached, a mentioned unit's own
fields merge via `AmsUnit::merge_from` instead of the whole `AmsUnit`
getting cloned wholesale.

While auditing BUG-098's evidence source (BambuStudio's
`src/slic3r/GUI/DeviceCore/DevFilaSystem.cpp`, `ParseAmsInfo`/
`ParseAmsTrayInfo`, confirmed identical in OrcaSlicer) one level deeper, two
separate findings turned up that don't fit cleanly into that session's
scope. This plan exists to resolve both, carefully, since they sit directly
in territory an earlier bug (BUG-083) already burned this crate on once.

**Read `BambuStudio/src/slic3r/GUI/DeviceCore/DevFilaSystem.cpp` lines
~715-849 (`ParseAmsInfo`'s tray handling + `ParseAmsTrayInfo`) yourself
before writing any code** — this document summarizes what that source
showed as of this writing, but re-verify against current source rather than
trusting this summary blind; re-verification is cheap, a wrong foundational
assumption here is not.

### Finding 1 — `AmsUnit.tray`'s own array isn't keyed-merged either

`AmsUnit::merge_from` (added this session, `src/types/telemetry/ams.rs`)
currently does:

```rust
if !incoming.tray.is_empty() {
    self.tray = incoming.tray.clone();
}
```

i.e. the same "whole-array-replace-if-nonempty" shape BUG-098 fixed one
level up for `AmsStatusReport.ams`. BambuStudio's `ParseAmsInfo` handles
`j_ams["tray"]` differently — for each `tray_item` in *this push's* array it
does a keyed lookup (`curr_ams->GetTray(tray_id)`, create-or-reuse) and
merges fields into that persistent `DevAmsTray` via
`DevJsonValParser::ParseVal` (preserve-on-absence, confirmed exhaustively —
`tag_uid`, `tray_info_idx`+`tray_type`, `tray_sub_brands`, `tray_weight`,
`tray_diameter`, `tray_temp`, `tray_time`, `bed_temp_type`, `bed_temp`,
`nozzle_temp_max`, `nozzle_temp_min`, `xcam_info`, `tray_uuid`,
`tray_id_name`, `remain`, `k`/`n` (unless a local calibration write is still
in its hold window — see "Explicitly out of scope" below), `cali_idx`,
`tray_color`/`cols`/`ctype`). **Then**, after processing every tray in the
push's array, it removes any previously-cached `tray_id` that wasn't
present in *this specific push's* array (`existing_tray_set`-gated erase
loop).

That's two behaviors bambino's current whole-array-clone doesn't reproduce:
1. A tray_id present in both the cached and incoming arrays should merge
   fields (preserve-on-absence), not get its entire `AmsTray` struct
   replaced by whatever subset the incoming push happened to repeat.
2. A tray_id in the incoming array that bambino hasn't seen before should
   be added; a previously-cached tray_id **absent from a non-empty
   incoming array** should be removed from the cache (opposite of BUG-098's
   unit-level "absent = preserve" — trays within a unit get pruned to
   exactly what's present, units across an `ams` push don't).

**Type-level wrinkle to resolve, not ignore:** `AmsUnit.tray: Vec<AmsTray>`
carries `#[serde(default)]`, so "tray key absent from this push" and "tray
key present as `[]`" both deserialize to an empty `Vec` — bambino's Rust
types currently cannot tell them apart, but BambuStudio's behavior implies
they're semantically different (absent key ⇒ don't touch trays this push;
present-empty-array ⇒ prune every cached tray for this unit). Decide
whether this distinction is worth modeling (e.g. `tray:
Option<Vec<AmsTray>>`) or whether it's fine to keep conflating them — this
is a real design fork, not a detail to paper over silently. Consider
whether `tests/mocks/P1S_print_sequence.ndjson` (P1S only has one AMS
unit's worth of real traffic to check) ever shows an empty-but-present
`tray` array, and whether the answer changes the recommendation.

### Finding 2 — the entire `ams::parser` sanitizer toolkit is never called from bambino's own pipeline

`src/ams/parser.rs` exports three `pub` functions: `clean_stale_tray_data`,
`evaluate_spool_presence`, `resolve_global_tray_id`. Grepping the whole
crate (`ctx_search` for each function name across `src/`) turns up **zero
callers outside their own unit tests** — not in `AmsUnit::merge_from`, not
in `AmsStatusReport::merge_from`, not in `client/telemetry.rs`, not in
`src/bin/bambino-cli/`. `client.ams()` (`src/client/telemetry.rs`) returns
the cached `AmsStatusReport` straight through, with none of these three
functions ever applied to it.

This means BUG-012 and BUG-083 (both real, tested fixes to
`clean_stale_tray_data`'s logic) currently have **zero effect on what a
consumer of this crate actually observes** through `client.ams()` — nothing
in the library calls the function they fixed. Note the deliberate wording:
"fixed", not "correct" — a fix commit landing, and being tested, doesn't
retroactively certify the underlying *design* as right forever, especially
once new evidence surfaces that wasn't available when the fix landed. Both
BUG-012 and BUG-083 were verified against pybambu/bambuddy only; BambuStudio
wasn't consulted for either at the time, because this crate's BambuStudio
cross-referencing only started this session (BUG-095 onward). **Re-verify
`clean_stale_tray_data`'s own clearing logic against BambuStudio now that
it's available as a source, don't just carry the "already fixed" status
forward unexamined.**

Evidence gathered *this session*, after BUG-012/083 landed, cuts more
sharply than "no clearing branch" suggests — go further than re-reading
`ParseAmsTrayInfo`:
- `DevAmsTray::reset()` (`DevFilaSystem.cpp:50`) exists and clears every
  material field (`color`, `m_fila_type`, `weight`, `remain`, etc.) to
  empty/zero — the direct structural analog of
  `clean_stale_tray_data`. Grepping the *entire* BambuStudio source tree for
  any call site (`tray.reset()`, `tray->reset()`, `Tray...reset()`) finds
  **zero** — it's dead code in BambuStudio's own current codebase, same
  shape as the finding for `clean_stale_tray_data` in bambino.
- What BambuStudio's UI actually does instead, confirmed by grepping real
  call sites (`AMSDryControl.cpp`, `AmsMappingPopupUpdate.cpp`,
  `DevFilaBlackList.cpp`, `DevMapping.cpp`, the web filament manager):
  every one of them gates on `tray->is_exists` (derived from
  `tray_exist_bits`, `DevFilaSystem.cpp:833`) or
  `tray->is_tray_info_ready()` (`color`/`m_fila_type` both non-empty,
  `DevFilaSystem.cpp:77-83`) **before** trusting any material field. This
  is a real, actively-used, multi-call-site pattern — not incidental
  architecture. The design is: keep stale fields in the raw cache
  indefinitely, gate *consumption* on a presence check, never proactively
  scrub.
- Bambino already has both BambuStudio-equivalent tools available:
  `AmsTray::get_state()` (defaults to `AMS_TRAY_STATE_EMPTY` when `None`)
  and `evaluate_spool_presence` (`tray_exist_bits`-driven). The open
  question this reframes isn't just "is `clean_stale_tray_data` wired in,"
  it's **"is `clean_stale_tray_data`'s proactive-clearing model even the
  right design, given the officially-shipped client doesn't do that and
  instead relies entirely on check-before-trust?"** Re-examine BUG-012 and
  BUG-083's original fixes with this framing — it's possible the *correct*
  fix all along was "consumers must check `state == 9/10` before trusting
  material fields" (which bambino already supports via `get_state()`) and
  `clean_stale_tray_data` is solving a problem BambuStudio's own design
  shows doesn't need solving this way. It's also possible
  `clean_stale_tray_data`'s proactive model is still the better choice for
  a library (vs. a GUI app) precisely *because* it doesn't require every
  consumer to remember the check-before-trust discipline. Don't assume
  either answer — investigate and decide, and say which, with reasons, in
  whatever commit resolves this.

There's also a direct precedent already in this crate for exactly this
raw-vs-decoded accessor split: `hms()` returns the raw cached `HmsEntry`
array; `active_hms_alerts()` separately decodes *and filters to genuine
faults* on top of the same cache, both as public accessors, neither
mutating what the other returns (`src/client/telemetry.rs`). If Finding 2
needs a fix at all, mirroring that pattern (an `ams()`-adjacent accessor
that returns sanitized trays, `ams()` itself staying raw) is a strong
candidate — cleaner than either wiring `clean_stale_tray_data` into the
mutable cache (which would make bambino's cache *less* faithful to
BambuStudio's own internal state than it is today) or leaving the
disconnect completely undocumented.

**Decide first, and this is now a two-part decision, not one:**
1. Is `clean_stale_tray_data`'s proactive-clearing *design* still the right
   one, given BambuStudio ships check-before-trust instead and its own
   `reset()` equivalent is dead code? Re-verify against BambuStudio
   yourself (don't trust this document's summary), and be willing to
   conclude BUG-012/BUG-083 need revisiting, not just re-invoking as-is.
2. Given whatever (1) concludes, is the result a real gap needing a code
   fix (a new sanitized accessor, `hms()`/`active_hms_alerts()`-shaped, or
   a documented check-before-trust contract using `get_state()`/
   `evaluate_spool_presence`), or a documentation gap only (state plainly,
   in `ams()`'s doc comment and `README.md`, what the actual contract is)?

Multiple outcomes are defensible from the evidence gathered so far — this
document doesn't pick for you, because the call depends on your own
re-verification, not on repeating what's written here. If undecided after
investigating, ask rather than guessing quietly — this is exactly the kind
of judgment call `.claude/skills/backlog/SKILL.md` rule 7 says to surface,
not resolve silently.

### Explicitly out of scope for this plan

- **`k`/`n` calibration hold-window** (`ParseAmsTrayInfo` lines ~838-845,
  `extrusion_cali_set_hold_start`/`extrusion_cali_set_tray_id`): BambuStudio
  suppresses re-parsing `k`/`n` from telemetry for a short window after the
  *local UI* just wrote a calibration value, so the client's own optimistic
  write doesn't get immediately clobbered by an echo of the pre-write wire
  value. This is a GUI-application concern (local edit vs. server echo
  race) with no analog in bambino, which is a headless client library with
  no local optimistic-write cache to protect. Do not port this — flagging
  it so it isn't mistaken for an overlooked field during the audit.
- `BedTelemetry`/`CtcTelemetry`/`ExtToolTelemetry`/etc. — all already fixed
  this session (BUG-095/096/097/101), not in scope here.

## Phase 1 — Keyed per-tray field merge in `AmsUnit.tray`

**Problem:** implement the merge Finding 1 describes, so a partial per-tray
push (a real, confirmed-shape wire event per BambuStudio's own field-level
`ParseVal` gating) doesn't wholesale-clobber a matched tray's other fields.

**Design constraint:** this is a *pure* field-level preserve-on-absence
merge, matching `ParseVal`'s exact behavior — **do not** fold in any
state-driven clearing logic here (that's `clean_stale_tray_data`'s job,
Phase 2 decides whether/where it runs). Keep this phase's `AmsTray`-level
merge symmetrically dumb, the same way `AmsUnit::merge_from` (BUG-098) and
every other `merge_from` this session added is dumb — presence-gated
per-field assignment, nothing smarter.

**What to build**, in `src/types/telemetry/ams.rs`:
1. `AmsTray::merge_from(&mut self, incoming: &AmsTray)` — every `Option<T>`
   field preserves on absence (mirror the confirmed `ParseAmsTrayInfo`
   field list above exactly; if a bambino field has no confirmed BambuStudio
   counterpart, default to preserve-on-absence for consistency with every
   other `merge_from` in this codebase, same precedent BUG-097 used for
   `mount`/`low_prec`/`th_temp`). `id` and `state` need their own judgment
   call — `state` is not `#[serde(default)]` in `AmsTray` currently
   (check: is it actually optional on the wire? `AmsTray.state: Option<u8>`
   — yes it's already `Option`), so decide whether it preserves-on-absence
   like everything else or always takes the incoming value (re-check
   `ParseAmsTrayInfo`'s handling — it wasn't covered by the excerpt this
   plan quotes above, if it exists at all it's outside the ~715-849 range
   already read; find it before guessing).
2. Update `AmsUnit::merge_from`'s `tray` handling from whole-clone to a
   keyed merge, same shape as BUG-098's `AmsStatusReport::merge_from` ams
   loop:
   ```rust
   for incoming_tray in &incoming.tray {
       match self.tray.iter_mut().find(|t| t.id == incoming_tray.id) {
           Some(cached_tray) => cached_tray.merge_from(incoming_tray),
           None => self.tray.push(incoming_tray.clone()),
       }
   }
   ```
   **but** decide first whether to also add BambuStudio's pruning step
   (remove any cached `tray_id` absent from a non-empty incoming array) —
   this is the array-membership question Finding 1 raises, and skipping it
   silently would leave bambino's tray set only ever growing, never
   shrinking, which is its own kind of staleness bug. If implementing
   pruning, decide the `Vec` vs. `Option<Vec>` type question from Finding 1
   first, since pruning-on-present-empty-array vs.
   preserve-on-absent-array need to be distinguishable to implement
   correctly.
3. Tests, mirroring this session's style
   (`src/types/telemetry/tests/ams.rs`): a
   `test_ams_tray_merge_from_preserves_fields_on_absence` (field-level, one
   tray, partial push omits several fields), and a
   `test_ams_unit_merge_from_preserves_and_prunes_trays` (or whatever the
   pruning decision lands on — could equally be
   `..._never_prunes_trays` if that's the decision) exercising the
   array-membership behavior specifically.
4. Re-run `test_p1s_print_sequence_ams_merge_never_regresses`
   (`src/types/telemetry/tests/ams.rs`) and
   `tests/telemetry_replay_test.rs` after the change — both already pass
   with the current whole-array-clone behavior; confirm they still pass
   (or, if the array shape assumptions changed, that any failure is
   understood and either the fixture or the assertion is wrong, not the
   new code).

**File the `BUG-ID` via the `backlog` skill at fix time** — don't
pre-assign one here, other sessions may land fixes between now and when
this runs (as of this writing, `BUG-102` is the highest ID; the real next
one may be higher, `backlog`'s own next-ID logic handles this).

## Phase 2 — Resolve Finding 2 (sanitizer wiring or documentation)

**Depends on Phase 1 being done first** if the "decide first" above lands
on wiring `clean_stale_tray_data` into any pipeline path — it needs
correct, already-merged tray state to make a correct clear-vs-preserve call
on, and Phase 1's keyed merge is what makes that state trustworthy per-tray
rather than whatever subset of fields the last push happened to repeat.
Independent of Phase 1 if the decision lands on documentation-only.

**What to do**, depending on the Phase-1-adjacent decision above:
- **If documentation-only:** update `ams()`'s doc comment
  (`src/client/telemetry.rs`) and `README.md` to state plainly that
  `client.ams()` returns the raw merged cache, and
  `clean_stale_tray_data()`/`evaluate_spool_presence()` are opt-in
  utilities a consumer calls themselves per-tray if they want scrubbed
  output — cite this crate's own `hms()`/`active_hms_alerts()` split as the
  existing precedent for "raw + opt-in-decoded coexist as separate
  accessors" so the design reads as deliberate, not accidental.
- **If a new accessor:** add something like `sanitized_ams()` or
  per-`AmsUnit` sanitized tray access, internally cloning the cached
  `AmsStatusReport`/`AmsUnit`/`AmsTray` and running `clean_stale_tray_data`
  over each tray before returning — mirror `active_hms_alerts()`'s doc
  comment style and placement in `src/client/telemetry.rs` closely. Add a
  test exercising a tray that's gone stale (state 9/10) still carrying old
  `tray_color`/`remain` in the raw cache, confirming the new accessor
  clears it while `ams()` itself still shows the raw (uncleaned) value —
  this is the one behavior difference a test needs to prove exists.
- Either way, decide whether `evaluate_spool_presence` gets the same
  treatment (it's in the identical "correct, tested, never invoked"
  category) or is genuinely fine left as a pure standalone utility (its
  inputs — `tray_exist_bits`, `ams_id`, `tray_id`, `power_on_flag` — are
  all things a consumer already has via other accessors, so "call it
  yourself" is a much smaller ask than for `clean_stale_tray_data`, which
  needs a whole `&mut AmsTray` the consumer would otherwise have to clone
  out of the cache manually). These two functions don't need the same
  answer.

**Independent of Phase 1** only in the documentation-only branch; in the
new-accessor branch, sequence after Phase 1 as noted above.
