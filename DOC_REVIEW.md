# Doc Review Plan

## Background

We generated an LLM-facing API reference for this crate using `cargo-doc-md`
(`make docs` → `docs/*.md`, one file per top-level module, flattened —
see the Makefile `docs` target comment for why). While building it we ran
rustdoc's `missing_docs` lint against the crate (`cargo rustdoc --lib --
-W missing_docs`, no source changes required to run this check) and found
**210 undocumented public items**. This plan fixes those, then does a
correctness pass on the docs we already have, then locks the gate in place
so the gap doesn't reopen.

Do not hand-wave doc content. A doc comment that just repeats the field/const
name in prose ("id: the id") is worse than useless — it satisfies the lint
while adding nothing. Every doc comment written in this plan should say
something a reader couldn't already infer from the identifier: wire meaning,
units, valid range, which model(s) it applies to, or a pointer to the
`reference/*.md` section that defines it.

Read the project README.md cover-to-cover first to get a sense of the
project identity and goals before completing each phase.

Re-run `cargo rustdoc --lib -- -W missing_docs 2>&1 | grep -c "warning: missing documentation"`
at the start of each phase below to get current numbers — the counts in this
plan are a snapshot and will drift as phases land.

## Phase 1 — Document quirks model constants (highest priority)

**Scope**: `src/quirks/models/{a1,a2,h2,p1,p2,x1,x2}.rs` — ~39 undocumented
constants (bed temp ceilings, fan mappings, and similar per-model magic
numbers).

**Why this tier first**: these are exactly the kind of undocumented magic
number CLAUDE.md's existing conventions (`MODEL_MATRIX.csv`, "verify against
reference docs", noting verification sources) are meant to prevent drifting
on. A constant like a bed-temp ceiling with no doc comment has no trace back
to the spec sheet or wire capture that justified the number — that's a real
gap, not a lint nitpick.

**How to approach**: for each constant, find its justification — either an
existing comment elsewhere in the file, `MODEL_MATRIX.csv`, or a
`reference/*.md` section — and write a one-line doc comment citing it. If a
constant has no traceable justification in the repo, flag it in the PR/commit
message rather than inventing a plausible-sounding source. The 7 files are
independent of each other and of every other phase in this plan — safe to
split across parallel sessions.

## Phase 2 — Document MQTT command struct fields

**Scope**: `src/mqtt/commands/{ams,print_job,hardware,control,status,gcode}.rs`
and `src/diagnostics/kprofile.rs` — ~148 undocumented struct fields (the bulk
of the 210), following the Payload+Request pattern described in CLAUDE.md.

**Why lower priority than Phase 1**: most of these field names are already
fairly self-explanatory (`tray_id`, `ams_id`), so the lint is flagging volume,
not necessarily a real comprehension gap. Still worth doing — an LLM (or
human) skimming `docs/mqtt/commands/*.md` benefits from field-level units and
wire-type notes (e.g. "0-indexed", "percentage, 0-100", "wire sends string not
int — see `AmsTray.id`-style note in CLAUDE.md for why").

**How to approach**: cross-check field meaning against the relevant
`reference/*.md` file before writing the doc comment — don't guess from the
field name alone. Where a field mirrors a documented non-obvious type
decision already in CLAUDE.md's "Non-Obvious Type Decisions" section, the doc
comment should reference it rather than re-explain it. These 7 files are
independent of each other and of Phase 1 — safe to split across sessions.

## Phase 3 — Document remaining scattered items

**Scope**: `src/client/types.rs` (6), `src/ams/mapping.rs` (3),
`src/mqtt/client/mod.rs` (2), `src/io/mod.rs` (2), `src/io/tokio.rs` (1) — 14
items total, mixed kinds (functions, structs, constants).

**How to approach**: small enough to do in one pass. Same bar as Phases 1-2 —
no restating the identifier as prose.

## Phase 4 — Correctness pass on existing docs

**Scope**: doc comments that already exist (i.e. didn't trip `missing_docs`)
but may be stale, wrong, or contradict `reference/*.md` or the actual
implementation. This is a semantic review, not a lint-driven checklist — the
`missing_docs` count won't help here.

**Ordering**: do this after Phases 1-3, so the review covers the complete
doc set including newly-written comments (which need the same scrutiny as
pre-existing ones).

**How to approach**: go module-by-module through `docs/*.md` (regenerate
first via `make docs`), comparing each doc comment's claims against (a) the
`reference/*.md` file covering that protocol area, and (b) the actual
implementation it documents. Fix mismatches in the *source* doc comments
(`src/**/*.rs`), never in the generated `docs/` output directly — it's
regenerated, not hand-edited. Flag anything you can't resolve confidently
(e.g. a doc comment asserting behavior that isn't verified against real
hardware) rather than guessing.

## Phase 5 — Enforce the gate

**Scope**: add `#![warn(missing_docs)]` (or `#![deny(missing_docs)]`) to
`src/lib.rs`.

**Decide first**: `warn` vs `deny`. `deny` fails the build on any future
undocumented public item — stricter, mechanically enforces documentation
discipline going forward, fits this crate's "verify, don't assume" ethos, but
adds friction to every future public API addition. `warn` is a weaker nudge
that `make check-fast`'s `cargo clippy` step won't fail on by default. Lean
towards `deny` given the project already treats undocumented magic numbers as
a real defect (see Phase 1), but this is a call for whoever lands this phase
to make, not a default to accept blindly.

**Ordering**: must be last — requires Phases 1-3 to have already driven the
`missing_docs` count to 0, or the build breaks immediately if `deny` is
chosen (or a wall of warnings appears if `warn` is chosen). Re-run the lint
check from Background before starting this phase to confirm the count is
actually 0.

## Phase 6 — Regenerate

**Scope**: run `make docs` to refresh `docs/*.md` with everything written in
Phases 1-4.

**Ordering**: trails every content phase — run it once after Phases 1-3 to
verify the new doc comments render as expected, and again after Phase 4's
corrections. Not a one-time step tied to a specific phase number; re-run
whenever source doc comments change.
