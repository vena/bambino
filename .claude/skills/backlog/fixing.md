# Fixing issues

## Batch fixes to save verification cost

Batch bugs that touch the same or tightly-related files into **one commit** and run `make check-fast` **once** (the pre-push hook runs it anyway; the point is not paying it per bug). List every issue it closes, repeating the keyword: `Closes #42, Closes #43`.

Group by what's naturally read and edited together, not by a cap: one file touched by 3 bugs is one batch; two files each touched by one bug is two batches.

**Don't sweep a paused bug into the commit.** If a bug is paused mid-sitting (a decide-first question, waiting on the user), its edits may still be in the working tree — `git add -A` picks them up silently. Stage the batch's files explicitly, or `git reset` the paused bug's files first. This has happened before.

## Docs regen

Before ending a session that closed issues, check whether any change touched the **public API shape** (items added/removed/renamed, signatures changed) **or the prose inside a `///` block** — `make docs` emits `///` bodies since #143, so prose-only edits count. If so, run `make docs` and commit the result on its own, not folded into a fix commit.
