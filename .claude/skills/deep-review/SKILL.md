---
name: deep-review
description: Full-crate correctness review sweep of bambino — partitions src/ and tests/ into units, reviews each with a batched subagent, and stages verified, prioritized findings in a local MM-DD-REVIEW.md. Use for a "full review", "deep review", "review sweep", auditing the whole crate, or a pre-release bug check. Not for a single diff/PR — use code-review.
---

# Deep Review — bambino module sweep

Discovers and stages findings only. It never fixes code, never files issues (`triage-review` does that), and never commits the review file: `MM-DD-REVIEW.md` is a local, gitignored working file at the repo root. It is written unit by unit as agents report, so an interrupted sweep resumes from the file rather than from conversation memory.

Rediscover the layout every run — never reuse a previous run's partition.

## Step 0 — Setup

1. Load lean-ctx and use it throughout, including `ctx_graph`/`ctx_callgraph` for cross-cutting invariant checks: `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_shell,mcp__lean-ctx__ctx_search,mcp__lean-ctx__ctx_tree,mcp__lean-ctx__ctx_patch,mcp__lean-ctx__ctx_compose,mcp__lean-ctx__ctx_explore,mcp__lean-ctx__ctx_call,mcp__lean-ctx__ctx_graph,mcp__lean-ctx__ctx_callgraph")`.
2. `gh auth status` — stop and tell the user if it fails (Step 4 dedupes against issues).
3. `date +%m-%d` and `git rev-parse --short HEAD` — don't infer either from context.
4. `ctx_read(README.md, mode="full")` — `map` returns only headings on prose. Root `CLAUDE.md` is already loaded.

## Step 1 — Resume or discover

`ls *-REVIEW.md` at the repo root (use `ls`, not `ctx_glob` — the file is gitignored). If any has a status line reading `IN PROGRESS`, whatever its date, keep that file and its partition and go to Step 3, spawning only its `PENDING` units. Otherwise start fresh:

```
ctx_tree(path="src", depth=3)
ctx_tree(path="tests", depth=2)
find src tests -name '*.rs' | xargs wc -l | sort -rn
```

Out of scope: `embassy-hw-probe/` and `esp32-hw-probe/` (separate crates, excluded from the workspace) and `docs/` (generated — stale docs are a `make docs` pass).

## Step 2 — Partition and match invariants

Heuristics, not a fixed list:

- Each top-level `src/` subdirectory is a unit. Split one with >8 files or separable subdirectories (e.g. `mqtt/client/` vs `mqtt/commands/`); merge thin related ones (≤3 files each).
- Loose top-level files (`lib.rs`, `error.rs`, `models.rs`, …) form one "core" unit. Each `src/bin/*/` binary is its own unit (split if large).
- Put each `tests/integration/*_test.rs` in the unit whose code it exercises, so one agent sees mock and implementation together. They are modules of one test binary rooted at `tests/integration/main.rs` (#245). `tests/integration/common/` goes in its own small unit or with its heaviest user; `tests/embassy_tls_version_test.rs` goes with `io`.
- Aim for 3–12 files per unit, weighted by size: a file over ~800 lines counts as 2–3, or becomes its own unit.

Then match invariants once for all units: `ctx_search` `.claude/rules/*.md` for `paths:` globs, and check each unit's directories for a nested `CLAUDE.md`. Record unit → files → matched invariant excerpts.

## Step 3 — Choose the model, write the skeleton

Ask via `AskUserQuestion` which model the unit agents run as, stating the unit count: **Sonnet (Recommended)** — bounded reading against pre-matched invariants, a fraction of the cost — or **Opus** — deeper on subtle invariant violations, several times the spend. On resume, ask again with the recorded model as the default.

Fresh run: before spawning anything, write `<MM-DD>-REVIEW.md` at the repo root:

```markdown
**Status:** IN PROGRESS (0/N units complete)
**Agent model:** <model> · **Commit:** <short sha>

Deep-review sweep of bambino: N units, one review subagent each. Local working file, never committed; `triage-review` files its findings as GitHub Issues and then deletes it. file:line references are as of the commit above.

Out of scope by design: minor security issues that follow from LAN-only operation, and style/refactor suggestions.

## 1. <unit> (<paths>) — PENDING
…
```

## Step 4 — Spawn in batches

At most 4 agents at once (2–3 for Opus): `Agent`, `subagent_type: general-purpose`, the chosen `model`, background. Each uses roughly 70–190k tokens; a 20-unit all-at-once spawn once exhausted the session limit and lost every in-flight agent. Wait for a batch, write each of its units, then spawn the next. If a batch came back unusually expensive, or several units returned nothing, check with the user before continuing.

Agent prompt — fill in the `<…>` parts:

```
Step 0: before any other tool call, run ToolSearch("<the select: string from the skill's Step 0>") and use ctx_* tools instead of native ones throughout.

Review ONLY these bambino files for correctness; other agents cover the rest: <file list>

Architecture (from README.md): <opening paragraph — LAN-only, no cloud> <unit-specific passage, if any>. If a finding seems to contradict this, ctx_read README.md with mode="full".

Invariants for these files: <matched excerpts>. If a finding looks like it collides with an unstated cross-cutting convention, ctx_search(pattern="paths:", path=".claude/rules") for your files in case a rule was missed.

In scope: correctness bugs; violations of CLAUDE.md, .claude/rules/, or nested CLAUDE.md invariants; missing error handling at real boundaries (network I/O, FFI); names or doc comments that state the opposite of what the code does.
Out of scope: hypothetical internal-invariant validation; minor security issues that follow from the crate being LAN-only (cert-verification bypass, plaintext fallback) unless they contradict the crate's own stated behavior; style, naming and refactor suggestions.
If a bug exists because an invariant is enforced only by convention across several similar call sites, list every such site, not just the one you hit. (Example of the shape: wire constructors once each had to remember clamp_task_id(); that is now type-enforced by ClampedTaskId — don't audit for it.)

Tag each finding CONFIRMED (sure it's real) or PLAUSIBLE (looks real, but you can't verify the failure path triggers). Report both.

Per finding:
### <file>:<line> — <one-line problem>
- Verdict: CONFIRMED | PLAUSIBLE
- Detail: concrete failure scenario (inputs/state → wrong outcome) and why
- Code: the offending lines, quoted
- Suggested fix: brief
- Related: other call sites, .claude/rules/ files, reference/ docs involved

If nothing: NO ISSUES FOUND in <unit>. A session with none of this context will act on your report, so paths and lines must be exact.
```

As each agent reports, handle that unit fully before the next (even when several land at once):

1. **Dedupe** each finding: `gh issue list --search "<keyword>" --state all --limit 100`. Open match → `Issue: #N (existing)`. Closed match → keep it, add `Regression of: #N`.
2. **Classify** each `CONFIRMED` finding: priority and kind (`bug`/`enhancement`) per the `backlog` skill's Severity and Issue format rules.
3. **Write** the unit's section with `ctx_patch`, replacing `PENDING` with `COMPLETE`, and update the status count. Each finding:

```markdown
### F<unit>.<n> `src/path.rs:123` — <one-line problem>
- **Verdict:** CONFIRMED | PLAUSIBLE | NOT A BUG — <reason>
- **Priority:** P-… · **Kind:** bug|enhancement   (CONFIRMED only)
- **Issue:** —
- **Detail:** …
- **Code:** …
- **Suggested fix:** …
- **Related:** …
```

A clean unit gets `NO ISSUES FOUND` under its heading.

## Step 5 — Finalize

Once no unit is `PENDING`, re-verify every `PLAUSIBLE` finding by reading the code yourself (not by restating the agent's claim). Change each one to `CONFIRMED` with priority and kind, or to `NOT A BUG — <reason>`. None stay `PLAUSIBLE`. Set the status to `COMPLETE`.

## Step 6 — Report

Give the user: unit count, clean vs. flagged units, `CONFIRMED` findings by priority, and the review file path. Don't paste the findings inline and don't fix anything. Point them at `triage-review` to file the findings. The file stays on disk until then.
