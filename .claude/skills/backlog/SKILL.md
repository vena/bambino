---
name: backlog
description: Rules for filing, triaging, and closing this repo's bug/finding tracker on GitHub Issues — issue format, label schema (P-critical/P-high/P-low/needs-verification/bug), the release bar, and the commit-closes-issue convention. Use whenever opening a new bug/finding issue, closing or triaging an existing one, reassigning a needs-verification issue to a real priority, checking whether the crate meets its release bar, or deciding how a fix commit should reference the issue it closes. Invoked by `triage-review` when filing a deep-review sweep's findings — don't duplicate these rules elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

The tracker is GitHub Issues, not a file in this repo. `gh issue` is the interface. There is no local BACKLOG.md — don't recreate one; a bulk-imported history of hundreds of closed nits reads as noise to anyone landing on the repo, which is exactly why it was retired (see git history for `BACKLOG.md` if the old rationale is ever needed).

**Step 0, every invocation:** run `gh auth status` first. If it fails, stop and tell the user — don't silently fall back to guessing or to a local file.

## Entry point

Rules, not a self-driving procedure. Invoked bare: `gh issue list --state open --limit 100`, summarize, ask what to do. Adding a new bug: dedupe first — `gh issue list --search "<keyword>" --state all --limit 100` (a finding resurfacing from a closed issue is a regression, not a new bug — say so in the new issue). Closing/triaging a specific issue: `gh issue view <N>`, no need to list everything else. Checking the release bar: `gh issue list --state open --label P-critical` and `--label P-high` separately (`--label` ANDs, not ORs — two calls). "Fix everything open": `gh issue list --state open --limit 200`, work `P-critical` → `P-high` → `P-low` so an interruption leaves the least release-blocking bugs behind. Invoked by `triage-review` while filing a batch: it already supplies the finding and has already deduped — no separate lookup here.

## What counts as an issue

Not every finding gets one. A confirmed real bug or an outstanding needs-verification item gets an issue. **A finding that turns out not to be a bug does not** — there's no equivalent of an old `Wontfix` row; a non-bug doesn't belong in a public tracker. Note it inline in whatever review file triaged it and move on. Deliberate asymmetry, not an oversight — the whole point of moving off a build-log model.

## Issue format

1. **Title**: one line, states the problem, not "bug in X."
2. **Body**: file:line, one-sentence problem, one-sentence fix direction. Longer investigative detail goes in a dated review file (or a `*_PLAN.md`), linked from the issue body, not pasted into it.
3. **Labels**: exactly one priority label (`P-critical`/`P-high`/`P-low`/`needs-verification`, see Severity below) plus GitHub's default `bug` label — the latter distinguishes this from a feature request or question landing in the same tracker later. No separate status label — issue `open`/`closed` state is the status.
4. **No manual numbering.** GitHub assigns the issue number.

## Closing

**The commit that fixes a bug closes its issue in the same commit's message** (`Closes #42`) — GitHub auto-closes on push to the default branch when the message contains that keyword. No separate "update the tracker" follow-up; that's exactly how the old file went stale, and an issue left open after its fix landed is worse than a file row, since it's publicly visible. Referencing the issue number in the commit message is the one direction `git blame` doesn't cover for free: blame on the *fixed source line* doesn't find the issue that tracked it unless the message says so.

**Reassigning `needs-verification`:** when hardware evidence lands, swap the label to a real priority tier (or close as not-a-bug — see "What counts as an issue" above; if it turns out not to be a bug, close it with a one-line comment stating why, `gh issue close <N> --comment "..."`, rather than leaving it open indefinitely). State what resolved it (wire capture, cross-reference to a known-good source) in the closing comment.

**If applying these rules hits a genuine conflict or an undefined case, stop and flag it — don't resolve it silently and move on.** Same standing as any other design tradeoff on the actual code.

**Re-verify, don't assume settled.** A closed issue reflects what the commit changed at the time, not a permanent guarantee — re-open (or file a new issue referencing the old one) if a stronger source later contradicts a prior fix. Cheap to recheck, costly to carry forward stale.

## Batching fixes to save verification cost

When fixing multiple open issues in one sitting, batch bugs that touch the same file or tightly-related files into **one commit** running `make check-fast` **once**, instead of one commit-and-verify cycle per bug — `check-fast` is expensive (multi-target build/test/clippy) and mostly redundant between two tiny adjacent fixes in the same file. The commit message lists every issue it closes (`Closes #42, Closes #43`).

Don't fold in a bug that's paused mid-sitting (blocked on a decide-first question, or waiting on user input) just because its edits happen to be sitting in the working tree at commit time — `git add -A` will silently sweep up unrelated in-progress changes. Stage only the batch's own files explicitly, or `git reset` the paused bug's files first. Treat this as a real footgun, not a hypothetical — it's happened once already under the old system and the risk is identical here.

Group by what's naturally already being read/edited together, not by an artificial cap — one file touched by 3 unrelated bugs is one batch; two files each touched by one bug you happen to be doing back-to-back is two batches.

## Severity

Labels use rust-lang/rust's `P-` priority convention rather than an invented "sev" scheme — idiomatic to anyone who's filed a Rust issue before, and each tier here is narrowly enough scoped that no separate impact axis (rust-lang's `I-unsound`/`I-crash`, etc.) is needed on top.

- **`P-critical`** — can cause unsafe physical behavior (temp overshoot past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard) — and only that; nothing else lives in this tier. Blocks release.
- **`P-high`** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible/common condition. Blocks release.
- **`P-low`** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps. Tracked, non-blocking.
- **`needs-verification`** — can't be triaged into the above without something only real hardware can confirm (a wire capture, physical behavior on a specific model). Not a priority tier in itself — means "outstanding, blocked on evidence." Resolve per "Reassigning `needs-verification`" above once evidence lands.

No `A-`/`T-`/area or team labels — those exist in large multi-team projects for routing and faceted search across thousands of issues; free-text title/body plus GitHub's own search covers a crate this size. Revisit only if issue volume genuinely grows past what search handles well.

## Release bar

Zero open `P-critical`, zero open `P-high`. `P-low` doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Docs regen

Before ending a session that closed one or more issues, check whether any touched public API or doc comments. If so, run `make docs` and commit any changes in their own commit — not folded into a fix commit, same batching-cost reasoning as above.

## Relationship to a future `CHANGELOG.md`

Different audience — issue tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists, and have its entries cite issue numbers for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
