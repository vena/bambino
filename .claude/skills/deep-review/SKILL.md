---
name: deep-review
description: Runs a parallel, module-by-module deep code review sweep across the whole bambino crate — discovers the current src/ structure, spawns one review agent per module boundary, and compiles findings into a dated review file plus new BUG-ID rows in BACKLOG.md. Use when asked for a "full review", "deep review", "review sweep", to audit the whole crate/codebase, or to check for accumulated bugs before a release milestone. Not for reviewing a single diff/PR/recent change — use the code-review skill for that instead.
---

# Deep Review — bambino module sweep

Full-crate correctness review via parallel subagents, one per module. Designed to be re-run as the crate grows — never hardcode today's module list or file count; rediscover both every time this runs.

**Step 0, mandatory, before any other tool call:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_shell,mcp__lean-ctx__ctx_search,mcp__lean-ctx__ctx_tree,mcp__lean-ctx__ctx_patch,mcp__lean-ctx__ctx_compose,mcp__lean-ctx__ctx_explore,mcp__lean-ctx__ctx_call")` first and use those tools throughout — for this session's own orchestration work (discovery, `BACKLOG.md` edits) *and* as a mandatory Step 0 inside every spawned agent's prompt (see template below). This is restated here on purpose, redundant with the global lean-ctx rule — a mandate stated once at session start and not repeated near the point of use gets lost across a long multi-step task; this happened in the session that built this skill.

Also read `CLAUDE.md` and `README.md` in full before starting — the module-boundary and scope decisions below depend on understanding this crate's actual architecture, not just its file layout.

## Step 1 — Discover current structure

Don't reuse a module list from a prior run of this skill. Walk the tree fresh:

```
ctx_tree(path="src", depth=3)
```

Note: `src/bin/*/` (CLI binaries, if any — currently `bambino-cli`), loose top-level files (`error.rs`, `models.rs`, `lib.rs`), and whether `docs/` exists and mirrors `src/`'s layout (generated via `make docs` per `CLAUDE.md`'s Documentation section — if `docs/` looks present but stale relative to recent `src/` changes, note that in each agent's prompt as "cross-check but don't over-trust").

## Step 2 — Partition into review units

Heuristic, not a fixed list:

- Each top-level `src/` subdirectory is a candidate unit on its own.
- Split a subdirectory into 2+ units if it's large (rough guide: >8 files) or has clearly separable concerns living in its own subdirectories (e.g. a `foo/` with both `foo/client/` and `foo/commands/` as distinct wire-protocol-vs-payload-builder concerns is two units, not one).
- Merge 2–3 thin subdirectories (rough guide: ≤3 files each, related domain) into a single unit rather than spawning a trivially small agent for each.
- Bundle loose top-level files (`error.rs`, `models.rs`, `lib.rs`, etc.) into one "core" unit.
- Any `src/bin/*/` binary is its own unit.
- Target 3–12 files per unit. Too few wastes an agent spawn on triviality; too many means the agent can't actually deeply read everything.

Record the resulting partition (unit name → file list) — this is the actual worklist, and it will differ from any previous run once the crate's structure changes.

## Step 3 — Build each agent's prompt

Every spawned agent needs, at minimum:

1. **Step 0 mandate** (verbatim, adapted): run the lean-ctx ToolSearch bootstrap before any other tool call, per this skill's own Step 0 above.
2. **Read `CLAUDE.md` and `README.md` in full first** — a fresh agent has no context beyond what it reads; these define the architectural invariants that make a "bug" actually a bug and not intentional design.
3. **Its file list from Step 2**, and instruction to review *only* those files — this is what keeps the sweep parallelizable without agents duplicating or stepping on each other.
4. **Any CLAUDE.md invariants you (the orchestrator) can identify as relevant to this specific unit** — skim CLAUDE.md's own bullets before writing each prompt and call out the ones that plausibly bear on this unit's files. This is a judgment call made fresh each run, not a fixed mapping to hardcode — CLAUDE.md's content will change as the crate does.
5. **Scope rules** (bambino-specific policy, keep these unless the project's stated design changes):
   - Correctness bugs, invariant violations vs. `CLAUDE.md`, missed error handling at real boundaries (network I/O, FFI) — not hypothetical internal-invariant validation.
   - Skip minor security issues — this crate is explicitly LAN-only by design (see `README.md`'s Safety Notice); don't flag cert-verification bypass, plaintext fallback, etc. unless implemented incorrectly vs. its *own* stated behavior.
   - Skip style/naming/refactor/abstraction suggestions entirely — out of scope for this sweep.
   - Be skeptical of your own findings — only report what you're confident is a real bug or a real inconsistency with documented design.
6. **Output contract**: for each real issue, `### <file>:<line>` / **Issue** (one line) / **Detail** (concrete failure scenario) / **Suggested fix** (brief). If the unit has no real issues, say so explicitly (`NO ISSUES FOUND in <unit>`) rather than staying silent — a silent report is indistinguishable from a forgotten one.
7. **Self-contained framing**: the report will be read by a fresh session with none of this conversation's context — file paths and line numbers must be exact and unambiguous on their own.

## Step 4 — Spawn

All units' agents in parallel — single message, multiple `Agent` tool calls, `subagent_type: general-purpose`, background (default). Use `TodoWrite` with one entry per unit plus a final "compile findings" entry; mark each in-progress → completed as results land, don't batch.

## Step 5 — Triage and compile

For each real finding reported back:

1. **Dedupe first** — check `BACKLOG.md`'s existing rows for the same file/topic before treating it as new (a finding that resurfaces from a prior sweep is a regression, not a new bug — note that distinction in the review file rather than silently double-counting).
2. **Assign severity** per the `backlog` skill's rubric (Sev1/Sev2/Sev3/needs-verification) — don't re-derive or duplicate those definitions here, invoke that skill's rules directly.
3. Write the full writeup to a new dated review file, `MM-DD-REVIEW.md`, at repo root. Structure: one section per unit (`## N. <unit path(s)> — <one-line summary>`, using the same **Issue**/**Detail**/**Suggested fix** format from Step 3's output contract for each finding), a one-line "Modules reviewed with no issues" list near the top for units that came back clean, and a closing summary table (`BUG-ID | Sev | Module | File(s) | One-line`) mapping every finding to its assigned `BUG-ID` and severity. Don't rely on a prior sweep's review file surviving as a template — per the `backlog` skill's review-file lifecycle rule, a fully-resolved one gets deleted.
4. Append new rows to `BACKLOG.md`'s `Open` table using the `backlog` skill's entry-format and next-BUG-ID rules — link each row's `Detail` column back to the matching section of the new dated review file.

## Step 6 — Report

Summarize to the user: unit count, clean vs. flagged, new `BUG-ID`s and their severities, and point at the two artifacts (`MM-DD-REVIEW.md`, updated `BACKLOG.md`) rather than dumping full findings inline into the conversation.
