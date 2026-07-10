---
name: deep-review
description: Runs a parallel, module-by-module deep code review sweep across the whole bambino crate — discovers the current src/ structure, spawns one review agent per module boundary, and compiles findings into a dated review file plus new BUG-ID rows in BACKLOG.md. Use when asked for a "full review", "deep review", "review sweep", to audit the whole crate/codebase, or to check for accumulated bugs before a release milestone. Not for reviewing a single diff/PR/recent change — use the code-review skill for that instead.
---

# Deep Review — bambino module sweep

Full-crate correctness review via parallel subagents, one per module. Designed to be re-run as the crate grows — never hardcode today's module list or file count; rediscover both every time this runs.

**Step 0, mandatory, before any other tool call:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_shell,mcp__lean-ctx__ctx_search,mcp__lean-ctx__ctx_tree,mcp__lean-ctx__ctx_patch,mcp__lean-ctx__ctx_compose,mcp__lean-ctx__ctx_explore,mcp__lean-ctx__ctx_call")` first and use those tools throughout — for this session's own orchestration work (discovery, `BACKLOG.md` edits) _and_ as a mandatory Step 0 inside every spawned agent's prompt (see template below). This is restated here on purpose, redundant with the global lean-ctx rule — a mandate stated once at session start and not repeated near the point of use gets lost across a long multi-step task; this happened in the session that built this skill.

Also read `CLAUDE.md` and `README.md` in full before starting — the module-boundary and scope decisions below depend on understanding this crate's actual architecture, not just its file layout.

## Step 1 — Discover current structure

Don't reuse a module list from a prior run of this skill. Walk the tree fresh:

```
ctx_tree(path="src", depth=3)
ctx_tree(path="tests", depth=2)
find src tests -name '*.rs' 2>/dev/null | xargs wc -l | sort -rn
```

Note: `src/bin/*/` (CLI binaries, if any — currently `bambino-cli`), loose top-level files (`error.rs`, `models.rs`, `lib.rs`), `tests/` (integration tests + shared mock infrastructure — see Step 2 for why this walk matters and how these fold into the partition), and whether `docs/` exists and mirrors `src/`'s layout (generated via `make docs` per `CLAUDE.md`'s Documentation section — if `docs/` looks present but stale relative to recent `src/` changes, note that in each agent's prompt as "cross-check but don't over-trust").

## Step 2 — Partition into review units

Heuristic, not a fixed list:

- Each top-level `src/` subdirectory is a candidate unit on its own.
- Split a subdirectory into 2+ units if it's large (rough guide: >8 files) or has clearly separable concerns living in its own subdirectories (e.g. a `foo/` with both `foo/client/` and `foo/commands/` as distinct wire-protocol-vs-payload-builder concerns is two units, not one).
- Merge 2–3 thin subdirectories (rough guide: ≤3 files each, related domain) into a single unit rather than spawning a trivially small agent for each.
- Bundle loose top-level files (`error.rs`, `models.rs`, `lib.rs`, etc.) into one "core" unit.
- Any `src/bin/*/` binary is its own unit.
- Fold each `tests/*_test.rs` integration test file into the same unit as the `src/` code it exercises (e.g. `tests/ftps_test.rs` joins the `ftps` unit) — judging mock fidelity needs the mock and the real implementation in the same agent's view. `tests/common/*` (shared mock infrastructure used across multiple units) doesn't belong to just one — give it its own small unit, or fold it into whichever unit relies on it most this run; decide fresh, don't hardcode which.
- Target 3–12 files per unit, but weight by size, not just count. A single file over ~800 lines (or clearly larger than its siblings, e.g. 3x the unit's average) counts as 2–3 file-slots against that target, or gets split into its own unit outright if it's large enough to dominate the agent's attention on its own — the file-count target alone doesn't catch a unit that's technically 3 files but one of them is huge. Too few (by count or effective weighted count) wastes an agent spawn on triviality; too many (by either measure) means the agent can't actually deeply read everything.

Record the resulting partition (unit name → file list) — this is the actual worklist, and it will differ from any previous run once the crate's structure changes.

## Step 3 — Build each agent's prompt

Every spawned agent needs, at minimum:

1. **Step 0 mandate** (verbatim, adapted): run the lean-ctx ToolSearch bootstrap before any other tool call, per this skill's own Step 0 above.
2. **Read `CLAUDE.md` and `README.md` in full first** — a fresh agent has no context beyond what it reads; these define the architectural invariants that make a "bug" actually a bug and not intentional design.
3. **Its file list from Step 2**, and instruction to review _only_ those files — this is what keeps the sweep parallelizable without agents duplicating or stepping on each other.
4. **Any invariants you (the orchestrator) can identify as relevant to this specific unit** — most invariant detail now lives outside root `CLAUDE.md` (see its own "Where Other Invariants Live" section): grep `.claude/rules/*.md` for `paths:` globs matching this unit's files, and check for a nested `<dir>/CLAUDE.md` in any directory the unit covers, in addition to skimming root `CLAUDE.md`'s own remaining (now much shorter) content. Call out whatever's relevant in the prompt. Judgment call made fresh each run, not a fixed mapping to hardcode — all three locations' content will change as the crate does.
5. **Scope rules** (bambino-specific policy, keep these unless the project's stated design changes):
   - Correctness bugs, invariant violations vs. `CLAUDE.md`/`.claude/rules/`/nested `CLAUDE.md` (see item 4 above), missed error handling at real boundaries (network I/O, FFI) — not hypothetical internal-invariant validation.
   - Skip minor security issues — this crate is explicitly LAN-only by design (see `README.md`'s Safety Notice); don't flag cert-verification bypass, plaintext fallback, etc. unless implemented incorrectly vs. its _own_ stated behavior.
   - Skip style/refactor suggestions and naming preferences entirely — except where a name actively misrepresents behavior (e.g. implies the opposite of what the function does, or a boolean whose sense is inverted from what its name suggests). That's a correctness/footgun risk wearing a naming-shaped disguise, not a style preference — same class as a doc comment that says one thing while the code does another. (Abstraction suggestions: see the next bullet for the one case they're in scope — not skipped entirely.)
   - When a correctness bug you already found (per the first bullet) exists because an invariant is enforced only by convention across multiple similar call sites — nothing at the type level or a shared code path prevents a miss — name that pattern in the finding too, not just the one instance (`BUG-001`'s six constructors each manually calling `clamp_task_id()` is exactly this shape: fixing today's six doesn't stop a seventh from making the same mistake tomorrow). This is root-cause context on a bug you already have, not a refactor suggestion — don't go looking for architecture smells with no concrete violation behind them, that's still hypothetical internal-invariant validation.
   - Be skeptical of your own findings — only report what you're confident is a real bug or a real inconsistency with documented design.
6. **Output contract**: for each real issue, `### <file>:<line>` / **Issue** (one line) / **Detail** (concrete failure scenario) / **Suggested fix** (brief). If the unit has no real issues, say so explicitly (`NO ISSUES FOUND in <unit>`) rather than staying silent — a silent report is indistinguishable from a forgotten one.
7. **Self-contained framing**: the report will be read by a fresh session with none of this conversation's context — file paths and line numbers must be exact and unambiguous on their own.

## Step 4 — Spawn

All units' agents in parallel — single message, multiple `Agent` tool calls, `subagent_type: general-purpose`, background (default). Use `TodoWrite` with one entry per unit plus a final "compile findings" entry; mark each in-progress → completed as results land, don't batch.

## Step 5 — Triage and compile

For each real finding reported back:

1. **Dedupe first** — check `BACKLOG.md`'s existing rows for the same file/topic before treating it as new (a finding that resurfaces from a prior sweep is a regression, not a new bug — note that distinction in the review file rather than silently double-counting).
2. **Assign severity** per the `backlog` skill's rubric (Sev1/Sev2/Sev3/needs-verification) — don't re-derive or duplicate those definitions here, invoke that skill's rules directly.
3. Write the full writeup to a new dated review file, `MM-DD-REVIEW.md`, at repo root. Required framing (a fresh session with zero context needs this, not just the findings): an opening paragraph stating what was reviewed and how (crate description, parallel-agent methodology, unit count), the scope exclusions from Step 3 item 5 restated for the reader (LAN-only-security minor issues and style/refactor suggestions are out of scope, not overlooked), an explicit note that the file is meant to be consumed standalone by a fresh session, and a caveat that file:line references may have drifted if other changes landed on `main` since this sweep. Structure: one section per unit (`## N. <unit path(s)> — <one-line summary>`, using the same **Issue**/**Detail**/**Suggested fix** format from Step 3's output contract for each finding), a one-line "Modules reviewed with no issues" list near the top for units that came back clean, and a closing summary table (`BUG-ID | Sev | Module | File(s) | One-line`) mapping every finding to its assigned `BUG-ID` and severity, with a note directly above the table that `BACKLOG.md` is the status source of truth once this file exists — the table itself is a point-in-time snapshot and won't be updated as bugs get fixed. Don't rely on a prior sweep's review file surviving as a template — per the `backlog` skill's review-file lifecycle rule, a fully-resolved one gets deleted; `git log --all -- 'NN-NN-REVIEW.md'` finds a deleted one's content if a concrete example is wanted.
4. Append new rows to `BACKLOG.md`'s `Open` table using the `backlog` skill's entry-format and next-BUG-ID rules — link each row's `Detail` column back to the matching section of the new dated review file.

## Step 6 — Report

Summarize to the user: unit count, clean vs. flagged, new `BUG-ID`s and their severities, and point at the two artifacts (`MM-DD-REVIEW.md`, updated `BACKLOG.md`) rather than dumping full findings inline into the conversation.
