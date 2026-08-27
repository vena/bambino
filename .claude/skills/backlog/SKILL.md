---
name: backlog
description: Rules for filing, triaging, and closing this repo's bug/finding tracker on GitHub Issues — issue format, label schema (P-critical/P-high/P-low/needs-verification plus a bug/enhancement kind label), the release bar, and the commit-closes-issue convention. Use whenever opening a new bug, finding, or enhancement issue, closing or triaging an existing one, reassigning a needs-verification issue to a real priority, checking whether the crate meets its release bar, or deciding how a fix commit should reference the issue it closes. Invoked by `triage-review` when filing a deep-review sweep's findings — don't duplicate these rules elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

The tracker is GitHub Issues, not a file in this repo. `gh issue` is the interface. There is no local BACKLOG.md — don't recreate one; a bulk-imported history of hundreds of closed nits reads as noise to anyone landing on the repo, which is exactly why it was retired (see git history for `BACKLOG.md` if the old rationale is ever needed).

**Step 0, every invocation:** run `gh auth status` first. If it fails, stop and tell the user — don't silently fall back to guessing or to a local file.

## Entry point

Rules, not a self-driving procedure. Invoked bare: `gh issue list --state open --limit 100`, summarize, ask what to do. Adding a new bug: dedupe first — `gh issue list --search "<keyword>" --state all --limit 100` (a finding resurfacing from a closed issue is a regression, not a new bug — say so in the new issue). Closing/triaging a specific issue: `gh issue view <N>`, no need to list everything else. Checking the release bar: `gh issue list --state open --label P-critical` and `--label P-high` separately (`--label` ANDs, not ORs — two calls). "Fix everything open": `gh issue list --state open --limit 200`, work `P-critical` → `P-high` → `P-low` so an interruption leaves the least release-blocking bugs behind. Invoked by `triage-review` while filing a batch: it already supplies the finding and has already deduped — no separate lookup here.

## What counts as an issue

Not every finding gets one. A confirmed real bug or an outstanding needs-verification item gets an issue. **A finding that turns out not to be a bug does not** — there's no equivalent of an old `Wontfix` row; a non-bug doesn't belong in a public tracker. Note it inline in whatever review file triaged it and move on. Deliberate asymmetry, not an oversight — the whole point of moving off a build-log model.

**Enhancements are in scope too, and they are not the same as a not-a-bug finding.** An enhancement is work worth doing where nothing is currently broken: a missing capability, a diagnostic the crate cannot express, an API a consumer can't build on. Issue #157 is the worked example — every certificate-verification failure correctly failed closed, so nothing was broken, but a consumer could not tell an untrusted anchor from a name mismatch and therefore couldn't build trust-on-first-use on top. That's an enhancement, and it belongs in the tracker. The test isn't "did something misbehave", it's "is there work here someone should be able to find later". A finding with no work attached still gets no issue.

## Issue format

1. **Title**: one line, states the problem, not "bug in X."
2. **Body**: self-contained. `file:line`, the failure mechanism, and a one-sentence fix direction, all pasted in — plus whatever code, `reference/` docs, `.claude/rules/` files, or related issue numbers the reader needs. **Never point the body at a review file, a `*_PLAN.md`, or a commit SHA for the substance.** A `*-REVIEW.md` is deleted by `triage-review` the moment its findings are filed, so a link to one is dead on arrival and costs whoever picks the issue up a wasted lookup; a plan file has the same fate. Length is not the constraint — an issue nobody can act on without fetching a second document is the thing to avoid. Keep the investigative narrative (what was ruled out, how it was verified, which agent found it) out of the issue; that's the review file's job while it exists, and it isn't needed to fix the bug.
3. **Labels**: exactly one priority label (`P-critical`/`P-high`/`P-low`/`needs-verification`, see Severity below) plus exactly one *kind* label — `bug` when something misbehaves, `enhancement` when nothing is broken but work is wanted (see "What counts as an issue"). Never both: the kind label is what tells a reader whether the issue describes a defect or an addition, and an issue carrying both answers neither. GitHub's stock `question`/`documentation`/`duplicate`/`wontfix` labels exist in the repo but aren't part of this schema — don't reach for them to avoid deciding between `bug` and `enhancement`. No separate status label — issue `open`/`closed` state is the status.
4. **No manual numbering.** GitHub assigns the issue number.

## Closing

**The commit that fixes a bug — or lands an enhancement — closes its issue in the same commit's message** (`Closes #42`) — GitHub auto-closes on push to the default branch when the message contains that keyword. No separate "update the tracker" follow-up; that's exactly how the old file went stale, and an issue left open after its fix landed is worse than a file row, since it's publicly visible. Referencing the issue number in the commit message is the one direction `git blame` doesn't cover for free: blame on the *fixed source line* doesn't find the issue that tracked it unless the message says so.

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

**Enhancements are `P-low`, always.** The tiers above are written in defect terms on purpose — `P-critical` and `P-high` describe things going *wrong*, and an enhancement by definition has nothing going wrong yet, so it cannot reach either. If an issue seems to demand a blocking tier while wearing an `enhancement` label, that's the signal it was mislabelled: something *is* broken and it's a bug. Re-decide the kind label rather than promoting the priority. (`needs-verification` still applies to an enhancement whose shape depends on hardware evidence — it means "blocked on evidence", not "a defect we can't rank yet".)

No `A-`/`T-`/area or team labels — those exist in large multi-team projects for routing and faceted search across thousands of issues; free-text title/body plus GitHub's own search covers a crate this size. Revisit only if issue volume genuinely grows past what search handles well.

## Release bar

Zero open `P-critical`, zero open `P-high`. `P-low` doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Docs regen

Before ending a session that closed one or more issues, check whether any changed the **shape of the public API** — items added, removed, or renamed, or signatures changed — **or the prose inside a `///` block**. If so, run `make docs` and commit the result in its own commit, not folded into a fix commit (same batching-cost reasoning as above).

Doc-comment prose counts as of #143: `make docs` now emits `///` bodies, so a prose-only edit does change `docs/` and skipping the regen leaves the generated reference stale. Before that fix the pipeline emitted signatures and type structure only, and this rule told sessions to skip the multi-minute Docker pass for prose edits — it no longer does.

## Relationship to a future `CHANGELOG.md`

Different audience — issue tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists, and have its entries cite issue numbers for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
