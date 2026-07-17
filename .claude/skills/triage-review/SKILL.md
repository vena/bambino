---
name: triage-review
description: Converts a deep-review sweep's staged findings into GitHub Issues — reads a dated MM-DD-REVIEW.md, files each un-filed finding via the backlog skill's issue format/label rules, records the issue number back into the file, and deletes the file once every finding is filed or discarded. Use when asked to "file the review", "turn the review into issues", "triage the sweep", or to convert a *-REVIEW.md's findings after reading it. Not for filing a single ad hoc bug — invoke backlog directly for that.
---

# Triage Review — file a deep-review sweep's findings (bambino)

Sits between `deep-review` (discovers + stages, never files) and `backlog` (owns live-issue rules) — this skill's only job is the conversion step, done on its own schedule, in its own session, so a completed review file can sit for however long before anyone decides to act on it without polluting GitHub mid-sweep.

**Step 0:** `gh auth status` first — stop and tell the user if it fails.

## Which file

No file named: find the most recently modified `*-REVIEW.md`. More than one plausible candidate: ask which one. A file still `IN PROGRESS` (deep-review's own status marker) can still be triaged partially — file what's already staged, leave the rest for a later run once the sweep itself finishes.

## Filing

For each staged finding in the file that doesn't yet have an issue number recorded:
1. Already noted as triaged-not-a-bug — skip, nothing to file (see `backlog`'s "What counts as an issue").
2. Otherwise — `gh issue create` per the `backlog` skill's Issue format and label scheme (`P-critical`/`P-high`/`P-low`/`needs-verification` + `bug`). Don't re-derive those rules here, invoke them directly.
3. Record the new issue number back into that finding's line in the review file (`ctx_patch`, not a full rewrite) — this is what makes the file resumable if filing gets interrupted partway: a finding with an issue # recorded is done, one without isn't, regardless of session boundaries.

There's no bulk-file `gh` subcommand — each issue is its own `gh issue create` call. Don't ask for confirmation per issue; do surface the total count before starting if it's large (say, >15) — filing is a visible, public action, same spirit as this project's standing git-safety confirm-before-bulk-action rule.

## Cleanup

Once every finding in the file has either an issue number or a not-a-bug note, and the file's own `Status` line reads `COMPLETE` (not `IN PROGRESS`): delete the review file in its own commit (message: which sweep it was, how many issues it produced). Don't delete while anything's still unresolved or the sweep itself is still running — a partially-triaged file left in the tree is the resumable state, not a bug.
