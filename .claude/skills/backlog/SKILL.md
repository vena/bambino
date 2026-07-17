---
name: backlog
description: Rules for filing, triaging, and closing this repo's bug/finding tracker on GitHub Issues — issue format, label schema (sev1/sev2/sev3/needs-verification), the release bar, dedupe discipline, and the commit-closes-issue convention. Use whenever opening a new bug/finding issue, closing or triaging an existing one, reassigning a needs-verification issue to a real severity, checking whether the crate meets its release bar, or deciding how a fix commit should reference the issue it closes. Invoked by the deep-review skill for its severity rubric — don't duplicate these definitions elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

The tracker is GitHub Issues, not a file in this repo. `gh issue` is the interface. There is no local BACKLOG.md — don't recreate one; a bulk-imported history of hundreds of closed nits reads as noise to anyone landing on the repo, which is exactly why it was retired (see git history for `BACKLOG.md` if the old rationale is ever needed).

**Step 0, every invocation:** run `gh auth status` before anything else. If it fails, stop and tell the user — don't silently fall back to guessing or to a local file.

## Entry point

This file is rules, not a self-driving procedure — unlike `deep-review`, it doesn't discover its own task. Invoked bare (no task given): run `gh issue list --state open --limit 100` and summarize before asking the user what to do — don't guess a task from silence. Invoked to add a new bug (no issue yet): dedupe first — `gh issue list --search "<file/topic keyword>" --state all --limit 100` across open and closed (a finding that resurfaces from a prior closed issue is a regression, not a new bug — note that distinction in the new issue rather than silently double-filing). Invoked with a task naming a specific existing issue number (close/triage/reassign severity): `gh issue view <N>` for its current body/labels, no need to list everything else. Invoked to check the release bar: `gh issue list --state open --label sev1` and `--label sev2` separately (gh's `--label` flags AND together, not OR — two calls, not one). Invoked with "fix everything open": `gh issue list --state open --limit 200`, every result in scope. Invoked by `deep-review` mid-sweep: that skill already supplies the specific finding and has already deduped via `gh issue list --search` itself — no separate lookup needed here. Work severity order (sev1, sev2, sev3, per the release bar below) so an interruption leaves the least release-blocking bugs behind, not the reverse.

## What counts as an issue

Not every finding gets one. A confirmed real bug or an outstanding needs-verification item gets an issue. **A finding that turns out not to be a bug does not** — there's no GitHub equivalent of the old `Wontfix` table; a non-bug doesn't belong in a public tracker at all. Note it inline in whatever review file triaged it (see `deep-review`'s Step 5) and move on. This is a deliberate asymmetry from the old file-based system, not an oversight — it's the whole point of moving off a build-log model.

## Issue format

1. **Title**: one line, states the problem (not "bug in X" — the actual defect).
2. **Body**: file:line, one-sentence problem, one-sentence fix direction — same budget the old system enforced via its 3-line rule. Longer investigative detail goes in a dated review file (or a `*_PLAN.md`), linked from the issue body, not pasted into it.
3. **Labels**: exactly one severity label (`sev1`/`sev2`/`sev3`/`needs-verification`) per the Severity section below. No separate status label — issue `open`/`closed` state is the status.
4. **No manual numbering.** GitHub assigns the issue number; nothing here tracks "next ID."

## Closing

**The commit that fixes a bug closes its issue in the same commit's message** (`Closes #42`) — GitHub auto-closes on push to the default branch when the message contains that keyword. No separate "update the tracker" follow-up; that's exactly how the old file went stale, and an issue left open after its fix landed is worse than a file row, since it's publicly visible. Referencing the issue number in the commit message is the one direction `git blame` doesn't cover for free: blame on the *fixed source line* doesn't find the issue that tracked it unless the message says so.

**Reassigning `needs-verification`:** when hardware evidence lands, swap the label to a real severity (or close as not-a-bug — see "What counts as an issue" above; if it turns out not to be a bug, close it with a one-line comment stating why, `gh issue close <N> --comment "..."`, rather than leaving it open indefinitely). State what resolved it (wire capture, cross-reference to a known-good source) in the closing comment.

**If applying these rules hits a genuine conflict or an undefined case, stop and flag it — don't resolve it silently and move on.** Same standing as any other design tradeoff on the actual code.

**Re-verify, don't assume settled.** A closed issue reflects what the commit changed at the time, not a permanent guarantee — re-open (or file a new issue referencing the old one) if a stronger source later contradicts a prior fix. Cheap to recheck, costly to carry forward stale.

## Batching fixes to save verification cost

When fixing multiple open issues in one sitting, batch bugs that touch the same file or tightly-related files into **one commit** running `make check-fast` **once**, instead of one commit-and-verify cycle per bug — `check-fast` is expensive (multi-target build/test/clippy) and mostly redundant between two tiny adjacent fixes in the same file. The commit message lists every issue it closes (`Closes #42, Closes #43`).

Don't fold in a bug that's paused mid-sitting (blocked on a decide-first question, or waiting on user input) just because its edits happen to be sitting in the working tree at commit time — `git add -A` will silently sweep up unrelated in-progress changes. Stage only the batch's own files explicitly, or `git reset` the paused bug's files first. Treat this as a real footgun, not a hypothetical — it's happened once already under the old system and the risk is identical here.

Group by what's naturally already being read/edited together, not by an artificial cap — one file touched by 3 unrelated bugs is one batch; two files each touched by one bug you happen to be doing back-to-back is two batches.

## Severity

- **sev1** — can cause unsafe physical behavior (temp overshoot past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard). Blocks release.
- **sev2** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible/common condition. Blocks release.
- **sev3** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps. Tracked, non-blocking.
- **needs-verification** — can't be triaged into the above without something only real hardware can confirm (a wire capture, physical behavior on a specific model). Not a severity in itself — a label meaning "outstanding, blocked on evidence." Resolve per "Reassigning `needs-verification`" above once evidence lands.

## Release bar

Zero open `sev1`, zero open `sev2`. `sev3` doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Docs regen

Before ending a session that closed one or more issues, check whether any touched public API or doc comments. If so, run `make docs` and commit any changes in their own commit — not folded into a fix commit, same batching-cost reasoning as above.

## Relationship to a future `CHANGELOG.md`

Different audience — issue tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists, and have its entries cite issue numbers for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
