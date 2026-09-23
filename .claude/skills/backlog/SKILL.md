---
name: backlog
description: Rules for this repo's GitHub Issues tracker — issue format, labels (P-critical/P-high/P-low/needs-verification plus bug/enhancement), release bar, and closing via commit. Use when opening, triaging, closing, reprioritizing, or fixing issues, or checking the release bar.
---

# Backlog rules (bambino)

The tracker is GitHub Issues; `gh issue` is the interface. There is no local `BACKLOG.md` — don't recreate one.

**Step 0:** `gh auth status`. If it fails, stop and tell the user.

Supporting files — read only when the task needs them:
- [evidence.md](evidence.md) — what counts as confirmation of a protocol/wire claim. Read before choosing `needs-verification` vs. a real tier, or before resolving one.
- [fixing.md](fixing.md) — batching fixes into commits and the docs-regen check. Read before fixing issues.

## Entry points

- **Bare invocation:** `gh issue list --state open --limit 100`, summarize, ask what to do.
- **New issue:** dedupe first — `gh issue list --search "<keyword>" --state all --limit 100`. A match to a closed issue is a regression; say so in the new issue.
- **One specific issue:** `gh issue view <N>`.
- **Release bar check:** `gh issue list --state open --label P-critical`, then again with `--label P-high` (`--label` ANDs, so two calls).
- **"Fix everything open":** `gh issue list --state open --limit 200 --json number,title,labels,blockedBy`. Work `P-critical` → `P-high` → `P-low`, skipping any issue with an open blocker (`.blockedBy.nodes|map(.number)`) until the blocker lands. A blocker is recorded when doing the dependent fix first would be *wrong* (code written to be deleted), not just inconvenient. Then read `fixing.md`.

## What counts as an issue

- A confirmed bug or an outstanding `needs-verification` item gets an issue.
- **An enhancement** gets one too: work worth doing where nothing is broken — a missing capability, a diagnostic the crate can't express, an API a consumer can't build on (#157: verification failures correctly failed closed, but a consumer couldn't tell an untrusted anchor from a name mismatch). The test is "is there work someone should be able to find later".
- **A finding that turns out not to be a bug gets no issue.** Note it in whatever review file triaged it and move on.

## Issue format

1. **Title:** one line stating the problem, not "bug in X".
2. **Body — self-contained.** Current `file:line`, the offending code quoted, the failure mechanism, a one-sentence fix direction, plus any code, `reference/` or `.claude/rules/` passages (quoted, not just linked) and related issue numbers the fixer needs. **Never point at a `*-REVIEW.md`, a `*_PLAN.md`, or a commit SHA for the substance, and don't mention review sweeps, finding IDs, or agents** — review and plan files are deleted (review files are never even committed), so the reference is dead on arrival. Leave out the investigative narrative. Length is not the constraint; needing a second document to act is.
3. **Labels:** exactly one priority (`P-critical`/`P-high`/`P-low`/`needs-verification`) plus exactly one kind — `bug` if something misbehaves, `enhancement` if nothing is broken. Never both. Don't use the stock `question`/`documentation`/`duplicate`/`wontfix` labels to dodge the choice. No status labels — open/closed is the status. No area/team labels.
4. **Numbering:** GitHub assigns it.

## Severity

Labels follow rust-lang's `P-` convention.

- **`P-critical`** — can cause unsafe physical behavior (temperature past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard). Only that. Blocks release.
- **`P-high`** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible condition. Blocks release.
- **`P-low`** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps.
- **`needs-verification`** — can't be placed above without evidence only hardware can give. Means "blocked on evidence", not a tier. See `evidence.md` for what closes it.

**Enhancements are always `P-low`** (or `needs-verification` if their shape depends on hardware evidence). An enhancement that seems to need a blocking tier is really a bug — change the kind label, not the priority.

## Closing

The commit that fixes a bug or lands an enhancement closes its issue in its message (`Closes #42`). For several issues, repeat the keyword: `Closes #42, Closes #43` — `Closes #42, #43` closes only #42. No separate tracker-update step. The message is also the only link from `git blame` on the fixed line back to the issue.

**Record the decision in the issue.** When an issue was resolved by a choice — a decide-first option the user picked, or a judgment call the fixer made between fix directions (e.g. reject vs. clamp, a behavior kept or dropped, a doc reworded instead of code changed) — comment on the issue with the option chosen, what it concretely does, what it deliberately does not do, and the landing commit. Post it once the commit is pushed. The commit message alone isn't enough: the issue is where a later reader checking "why this way?" lands.

Resolving `needs-verification`: swap in a real priority label, or close as not-a-bug with `gh issue close <N> --comment "<why>"`. Either way, state what resolved it (wire capture, upstream source).

Re-verify, don't assume settled: reopen, or file a new issue referencing the old one, if a stronger source later contradicts a prior fix.

If these rules hit a genuine conflict or an undefined case, stop and flag it — don't resolve it silently.

## Release bar

Zero open `P-critical`, zero open `P-high`. `P-low` doesn't block. (If this changes, edit this line only.)

`CHANGELOG.md` doesn't exist yet. When it does, give it its own skill, with entries citing issue numbers — don't generate one from the other.
