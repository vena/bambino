---
name: readme-review
description: Audits README.md for content that doesn't belong there — contributor-only instructions leaking into this consumer-facing doc, duplication with CLAUDE.md/Makefile/other docs, staleness against current public API, and dead cross-references. Use when asked to review, audit, or clean up README.md, or after a batch of public-API-affecting changes to check it's still accurate. Single-pass direct read, not a multi-agent sweep — one file, not the whole crate (use the deep-review skill for that).
---

# README Review (bambino)

**Step 0, mandatory, before any other tool call:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_search,mcp__lean-ctx__ctx_patch")` first and use those tools throughout. Restated here on purpose, redundant with the global lean-ctx rule — a mandate stated once and not repeated near the point of use gets lost across a long task.

## Why this exists

README.md is consumer-facing — for someone using `bambino` as a dependency. `CLAUDE.md` (and its `.claude/rules/`/nested-`CLAUDE.md` companions) is contributor-facing — for someone working on the crate itself. Content drifts across that line over time (found once already: a `make docs`-regeneration instruction sitting in README's Documentation section, duplicating reasoning already on the Makefile's `docs` target, and irrelevant to anyone just consuming the crate). This skill checks for more of that, plus the other ways a large, hand-maintained README goes stale.

## What to check, reading README.md straight through

1. **Audience misplacement.** For each section, ask: would a consumer installing this crate as a dependency need this, or is it a build/test/contribute instruction? Contributor-only content moves to `CLAUDE.md`'s Key Conventions (only if genuinely global — check whether it's actually narrower and belongs in `.claude/rules/` or a nested `<dir>/CLAUDE.md` instead, same routing test `CLAUDE.md` itself uses) or a Makefile target's own comment if that's already the natural home. Don't reflexively default to `CLAUDE.md` — it was trimmed hard once already this project's history; check whether the fact already lives somewhere and README should just stop duplicating it, rather than assuming it needs a new home at all.
2. **Duplication.** Does README restate something that has a more authoritative home elsewhere (a Makefile target's comment, a `CLAUDE.md`/`.claude/rules/` invariant, `reference/`'s protocol docs)? If the other location is genuinely the source of truth, trim README to a pointer instead of restating.
3. **Staleness vs. current code.** Do the code examples, method signatures, and described behavior still match what's actually in `src/`? Check especially against any `BACKLOG.md` `Fixed` rows since the last README review — a fix that changed public API shape or documented behavior is the exact class of change that makes an example silently wrong. Don't assume; grep the actual current signature for anything README shows a code sample for.
4. **Dead cross-references.** Links to `docs/`, `reference/`, or any other file — do they still resolve? A file this project deletes on purpose (a completed `*_PLAN.md`, a fully-resolved `NN-NN-REVIEW.md`) may have been linked from README at some point; confirm nothing still points at it.
5. **Changelog narration.** Grep for `replaced|used to|no longer|unlike the old|previously|formerly|instead of the old`. README states current behavior only — "X replaced Y" or "unlike the old Z" is commit-message content, not API doc. State the current fact and drop the history.
6. **Cross-section consistency.** When the same function/type is described in more than one section, diff the claims against each other, not just against source — contradictions between two true-at-different-times statements survive a source-only check since each half may individually match some version of the code.

Before finishing: re-run checks 1-6 against any line you just edited, including edits made while fixing an unrelated finding — a fix for one item can reintroduce another (e.g. patching a dead cross-reference by inline-summarizing what the deleted file said is exactly how changelog narration gets written back in).

## Reporting

Not a `BACKLOG.md`/`BUG-ID` matter — README issues aren't code bugs, don't file them there. Report directly: what's misplaced (and its target: `CLAUDE.md`/`.claude/rules/`/nested `CLAUDE.md`/Makefile/delete-as-pure-duplication), what's duplicated (and which copy is authoritative), what's stale (with the current actual signature/behavior to correct it to), what's dead (and whether to fix the link or remove the reference). Fix inline as you go if the finding is unambiguous; flag for a decision if it's a judgment call (e.g. "is this actually contributor-only, or do consumers plausibly want to know it too").
