# Fixing issues

## Batch fixes to save verification cost

Batch bugs that touch the same or tightly-related files into **one commit** and run `make check-fast` **once** (the pre-push hook runs it anyway; the point is not paying it per bug). List every issue it closes, repeating the keyword: `Closes #42, Closes #43`.

Group by what's naturally read and edited together, not by a cap: one file touched by 3 bugs is one batch; two files each touched by one bug is two batches.

**Don't sweep a paused bug into the commit.** If a bug is paused mid-sitting (a decide-first question, waiting on the user), its edits may still be in the working tree — `git add -A` picks them up silently. Stage the batch's files explicitly, or `git reset` the paused bug's files first.

## Delegating a batch to Sonnet

Sonnet uses session quota at a much lower rate than Opus. When the session runs on Opus, a batch's implementation can go to the `fixer` subagent (`.claude/agents/fixer.md`, pinned to Sonnet) while planning, review and committing stay in the Opus session.

**Decide at the end of planning, not before.** Opus reads the relevant files and settles the fix either way, so planning costs the same whether you delegate or not. What differs is what comes after it:

- **Inline:** Opus's edit/build/test turns. Each turn re-sends the whole Opus context, which by then includes everything read during planning, and every line of new code is Opus output.
- **Delegated:** Opus writes the handoff, reads the report and reviews the diff. On top of that, at Sonnet's rate: the fixer's cold start (about 59k tokens for a trivial 5-call probe on 2026-10-19), its re-reading of the files Opus already read, and its own edit/build/test turns.

Delegate when the remaining work is long and mechanical: new functions or tests to write, several files to change, or more than one build/test round expected. Do it inline when the plan already amounts to the diff (a few lines in a file or two) or the review would take about as long as the edit. No break-even has been measured. When you delegate, note the Agent result's `subagent_tokens` against the batch's size so the threshold can be set from data.

1. **Plan in the Opus session.** Group the batches (above). Settle every decide-first question with the user *before* the handoff. The fixer stops on a decision rather than making one, so an unsettled question costs a round trip.
2. **Hand off.** Each `fixer` call gets the issue numbers, the files it may touch (tests included), any decision already made, any constraint not in the issue bodies, and the fix plan from planning with `file:line` locations, so the fixer doesn't re-derive what Opus already worked out. Don't paste issue bodies; it runs `gh issue view` itself. It loads the root `CLAUDE.md` on its own, and the nested `CLAUDE.md` files and rules for its files as that file's "Where Other Invariants Live" describes.
3. **One fixer at a time** in the main working tree. Parallel fixers in one tree mix their edits, and that is the paused-bug staging problem above. Parallel fixers need `isolation: "worktree"`, and each worktree builds `target/` from scratch, so use it only for unrelated batches that are each large. The fixer doesn't commit, so its edits stay uncommitted in that worktree; bring them into the main tree with `git -C <worktree> diff | git apply`, plus any new files it created, before reviewing.
4. **Review the diff, not the report.** `git status` for new files (untracked, so `git diff` won't show them), `git diff` the batch, and check each hunk against its issue: the assertion actually tests the failure mechanism, literals use the module's existing constants, nothing outside the listed files changed. A fixer's "tests pass" covers only what it chose to run.
5. **Finish in the Opus session:** `make check-fast` once, stage the batch's files explicitly, commit with `Closes #N, Closes #M`, and post decision comments as described in SKILL.md.

If the review finds a problem, fix small ones directly. For a larger one, continue the same fixer with `SendMessage` (its context is still loaded) rather than starting a new one.

## Docs regen

Before ending a session that closed issues, check whether any change touched the **public API shape** (items added/removed/renamed, signatures changed) **or the prose inside a `///` block** — `make docs` emits `///` bodies, so prose-only edits count. If so, run `make docs` and commit the result on its own, not folded into a fix commit.
