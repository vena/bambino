---
name: triage-review
description: Files a deep-review sweep's staged findings (a local MM-DD-REVIEW.md) as self-contained GitHub Issues, then deletes the file. Use for "file the review", "turn the review into issues", "triage the sweep". For a single ad hoc bug, use backlog.
---

# Triage Review — file a deep-review sweep (bambino)

Converts the findings `deep-review` staged into GitHub Issues, using the `backlog` skill's Issue format and Severity rules (load them; don't re-derive). The review file is gitignored, never committed, and deleted when this finishes — **every issue must stand on its own without it.**

**Step 0:** `gh auth status` — stop and tell the user if it fails.

## Which file

`ls *-REVIEW.md` at the repo root (`ls`, not `ctx_glob` — the file is gitignored). None: tell the user. Several: ask which. A file still `IN PROGRESS` can be filed partially; its pending units wait for a later run.

## Filing

Work through findings with Verdict `CONFIRMED` and `Issue: —`. Skip `NOT A BUG` and anything with an issue number. State the count first; if it's over 15, confirm before filing — issues are public.

For each finding:

1. **Re-verify against the current tree.** The sweep may be days old. Read the cited code on `main` and correct the line numbers. If the bug is gone, set `NOT A BUG — fixed since sweep (<sha or reason>)` and move on.
2. **Recheck duplicates** — issues may have been filed since the sweep: `gh issue list --search "<keyword>" --state all --limit 100`. Open match → record `Issue: #N (existing)` and move on. Closed match → note the regression in the body.
3. **Write the body** to a temp file in the session scratchpad. It must let someone fix the bug with nothing but the issue and the repo:
   - Current `file:line` for every site involved, with the offending code quoted in a fenced block.
   - The failure scenario — inputs/state → wrong outcome — and why it happens.
   - A one-sentence fix direction.
   - Any invariant the fix must respect, quoted from `.claude/rules/`, a nested `CLAUDE.md`, or `reference/` (path plus the relevant passage, not a bare link).
   - Related or regressed issue numbers.

   **Never mention** the review file, the sweep, its date, unit or finding IDs (`F3.2`), verdict tags, or subagents — the file is deleted on completion and was never committed, so any such reference is dead on arrival. Leave out how the finding was verified.
4. `gh issue create --title "<states the problem>" --body-file <temp file> --label <priority> --label <kind>`, using the priority and kind recorded in the finding. Kind is `bug` or `enhancement`, never both, per `backlog`.
5. Immediately record `Issue: #N` in the finding with `ctx_patch`. That is what makes an interrupted run resumable.

## Cleanup

When the status is `COMPLETE` and every finding has an issue number or `NOT A BUG`, `rm` the file — there is nothing to commit. Report the issue numbers filed. Until then, leave the file in place; it is the resume state.
