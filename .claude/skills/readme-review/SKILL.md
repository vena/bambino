---
name: readme-review
description: Checks every claim in README.md against current src/ and reference/, and flags contributor-only content, duplication and dead links. Use when asked to review or clean up README.md, or after public-API changes. One file only; for the whole crate use deep-review.
---

# README Review (bambino)

**Use the lean-ctx tools throughout, never native `Read`, `Bash`, `sed`, `cat` or `grep`.** Pick the one that fits the claim. For example, `ctx_compose` checks a claim that spans several files in one call, `ctx_graph` (`action=symbol`) finds a definition, `ctx_callgraph` finds callers or callees, `ctx_explore` locates behaviour, and `ctx_execute` handles scripted edits to the scratch ledger. `ctx_read`, `ctx_search` and `ctx_shell` cover the rest.

README is longer than `ctx_read`'s inline limit, so any read of it can stop partway with a truncation notice. That notice is where the tool rule has been broken before. Answer it with another `ctx_read` using `mode="lines:N-M"` that starts at the line where the output stopped, never with a native tool. Read README in windows of about 120 lines; README's lines are long, so a 250-line window still gets cut off. Don't start the ledger until every line has been read.

## Why this exists

README.md is consumer-facing: it's for someone using `bambino` as a dependency. `CLAUDE.md` and its `.claude/rules/` and nested-`CLAUDE.md` companions are contributor-facing. Content drifts across that line, and a large hand-maintained README goes stale against the code it describes.

## Procedure: coverage is tracked, not assumed

A past run spot-checked the code samples, skipped the prose, and reported done. It missed a wrong model list that a ledger row would have caught. So:

1. **Read all of README** in `ctx_read` windows, as above.
2. **Build a ledger in a scratch file** before verifying anything. Give it one heading per `##`/`###` section, and under each, one row per checkable claim:
   - every API a code block calls, constructs or imports;
   - every API named in prose;
   - every model list, numeric limit, range or default;
   - every CLI command or flag;
   - every link.

   Prose claims count exactly as much as code samples.
3. **Verify every row** with whichever ctx tool fits it (see the tool rule above). When you use `ctx_search`, give it an explicit `max_results`, because it caps results silently.
   - APIs: check against `src/`.
   - Model lists: check against the quirks rows and their tests.
   - Limits and defaults: check against the named consts.
   - Firmware and protocol behaviour: check against the matching `reference/` chapter.
   - The CLI usage block: run `cargo run -q --bin bambino-cli --features cli -- --help` through `ctx_shell` with `raw=true` and diff the output against README. Check each subcommand flag README names against that subcommand's `--help`.
4. **Mark every row** as verified, fixed, flagged (a judgment call) or unverifiable (a hardware measurement with no code to check it against). A blank row means the review is unfinished.
5. **Find issues closed since the last review.** Get the date of the last review commit with `git log -1 --format=%ad --date=short --grep='^README review:'`, or of the last commit touching README.md if no review commit exists. Then run `gh issue list --state closed --search "closed:>=<date>" --limit 100`. Run both through `ctx_shell`, and re-check the rows each relevant issue touches. Start the subject of this run's commit with `README review:` so the next run can find it.

## What to check, for every ledger row

1. **Audience misplacement.** Would a consumer installing this crate as a dependency need this, or is it a build, test or contribute instruction? First check whether the fact already lives elsewhere; if it does, README just stops duplicating it. Otherwise route it as `CLAUDE.md` itself routes content: a nested `<dir>/CLAUDE.md` or `.claude/rules/` file when it's narrower, a Makefile target's comment when that's its natural home, and `CLAUDE.md`'s Key Conventions only when it's genuinely global.

   **Kept on purpose; don't re-flag:** in the `bambino-cli` section, the `cargo bambino-cli` alias paragraph and the sentence about keeping the `std` build honest. The maintainer decided on 2026-10-07 to keep both. Raise them only if one has become factually wrong.
2. **Duplication.** Does README restate something that has a more authoritative home elsewhere (a Makefile target's comment, a `CLAUDE.md`/`.claude/rules/` invariant, `reference/`'s protocol docs)? If the other location is genuinely the source of truth, trim README to a pointer instead of restating.
3. **Staleness.** Does the claim still match current code and `reference/`? Signatures, imports, arguments, return types, defaults, model lists and described behaviour all count.
4. **Dead cross-references.** Links to `docs/`, `reference/`, or any other file — do they still resolve? Each in-page `#anchor` link must match a current heading. A `[REF-…]` tag must still exist in `reference/`. A file this project deletes on purpose (a completed `*_PLAN.md`, a fully-resolved `NN-NN-REVIEW.md`) may have been linked from README at some point; confirm nothing still points at it.
5. **Changelog narration.** Run `ctx_search` on README.md with `replaced|used to|no longer|unlike the old|previously|formerly|instead of the old|now that|unchanged|anymore`, then read each match. The pattern is a starting point, not the whole check: also look for history wording while reading each section. README states current behavior only — "X replaced Y" or "unlike the old Z" is commit-message content, not API doc. State the current fact and drop the history.
6. **Cross-section consistency.** When the same function/type is described in more than one section, diff the claims against each other, not just against source — contradictions between two true-at-different-times statements survive a source-only check since each half may individually match some version of the code. To find them, group the ledger rows by API or type name. Compare every group with rows in two or more sections.

Before finishing: re-run checks 1-6 against any line you just edited, including edits made while fixing an unrelated finding — a fix for one item can reintroduce another (e.g. patching a dead cross-reference by inline-summarizing what the deleted file said is exactly how changelog narration gets written back in).

## Reporting

Not a bug-tracker matter — README issues aren't code bugs, don't file them as a GitHub Issue. Report directly: what's misplaced (and its target: `CLAUDE.md`/`.claude/rules/`/nested `CLAUDE.md`/Makefile/delete-as-pure-duplication), what's duplicated (and which copy is authoritative), what's stale (with the current actual signature/behavior to correct it to), what's dead (and whether to fix the link or remove the reference). Fix inline as you go if the finding is unambiguous; flag for a decision if it's a judgment call (e.g. "is this actually contributor-only, or do consumers plausibly want to know it too").

End the report with the ledger's coverage: how many sections and rows were checked, and the unverifiable rows by name. If you stop before every row is marked, list the unchecked sections and don't call the review complete.
