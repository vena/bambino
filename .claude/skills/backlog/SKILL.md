---
name: backlog
description: Rules for adding, updating, and triaging entries in this repo's BACKLOG.md findings tracker — entry format, Fixed/Wontfix row schema, severity definitions (Sev1/Sev2/Sev3), the release bar, next-BUG-ID logic, status-update discipline, and when to delete a fully-closed dated review file. Use whenever adding a new bug/finding to BACKLOG.md, closing or triaging an existing BUG-ID, reassigning a needs-verification entry to a real severity, checking whether the crate meets its release bar, or deciding how a fix commit should update tracker status. Also covers the relationship to a future CHANGELOG.md. Invoked by the deep-review skill for its severity rubric — don't duplicate these definitions elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

**Step 0, mandatory, before touching `BACKLOG.md`:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_patch,mcp__lean-ctx__ctx_shell")` first and use `ctx_read(mode="anchored")` → `ctx_patch` for edits, not native Read/Edit. Use `ctx_shell`, not native Bash. This is restated here on purpose, redundant with the global lean-ctx bootstrap rule — sessions drift off a rule stated once at the top and not repeated near the point of use.

## Entry point

This file is rules, not a self-driving procedure — unlike `deep-review`, it doesn't discover its own task. Invoked bare (no task given, e.g. a standalone `/backlog` with no further instruction): read `BACKLOG.md` in full first (it's data-only, small, safe to read wholesale) before asking the user what to do — don't guess a task from silence. Invoked with a task (add/close/triage a specific `BUG-ID`, check the release bar): read `BACKLOG.md` in full, and if the task touches a row whose `Detail` links to a dated `NN-NN-REVIEW.md`, read that file's relevant section too before acting. Invoked by `deep-review` mid-sweep: that skill already supplies the specific finding and file context — no separate read needed here. Given "fix everything in `Open`": same task, no separate mode — rule 6's one-commit-per-bug atomicity makes it resumable for free (interrupted partway, whatever's still in `Open` is what's left). Work severity order (Sev1, Sev2, Sev3, per the release bar below) so an interruption leaves the least release-blocking bugs behind, not the reverse — and re-locate each bug by its Title before editing, not by a possibly-stale `file:line` from `Detail`, since an earlier fix in the same sitting can shift lines a later bug also touches. Re-check Step 0 before starting each bug in the sequence, not just once at invocation — a single statement made at the start of a long loop decays by bug 5 or 6; restating it per-iteration doesn't (skip it only if that check already failed earlier in this same run — don't retry a dead MCP server every iteration).

## What `BACKLOG.md` is (and isn't)

`BACKLOG.md` holds data only: one table row per known bug/gap, in `Open`/`Fixed`/`Wontfix` sections. It does not hold the rules for maintaining itself (that's this file) and does not hold investigative detail (that's a dated `NN-NN-REVIEW.md`, one row's `Detail` column links to the relevant section — see "Review-file lifecycle" below for what happens to that link once the file's fully resolved). Keep it that way — if you're about to write a paragraph of prose into `BACKLOG.md`, stop and either shorten it to fit the entry-format rule below or put it in a review file and link out.

## Entry rules

1. **One entry = one table row + at most 3 lines of prose.** File:line, one-sentence problem, one-sentence fix direction. Anything longer goes in a dated review file (or a `*_PLAN.md` for multi-phase design) — link to it, don't paste it.
2. **No narrative.** Don't record _how_ a bug was found or what was tried — that belongs in the commit message or the source review file.
3. **Move status by moving rows between tables, not by adding a status column.** `BACKLOG.md` is three separate tables by section (`Open`/`Fixed`/`Wontfix`), not one table with a status field — fixing BUG-004 means deleting its row from `Open` and inserting it into `Fixed`. Never leave a stale copy behind in `Open`, never append a second row for a bug already tracked elsewhere.
4. **Close stale entries aggressively.** Turns out to be a non-issue on investigation → `wontfix` with a one-line reason, move on.
5. **Severity is fixed at triage time** and doesn't get re-litigated per entry, except reassigning `needs-verification` to a real severity (confirmed bug) or to `N/A` (confirmed not a bug, moves to `Wontfix`) — see rule 7. If the severity definition itself needs to change, change it once here, not per-row.
6. **The commit that fixes a bug updates this file's status in the same commit, and its message references the `BUG-ID` it closes** (e.g. `Closes BUG-012`). No separate "update the backlog" follow-up — same diff, or the tracker silently goes stale exactly when it's most useful. Referencing the `BUG-ID` is the one direction `git blame` doesn't cover for free: blame on a `BACKLOG.md` row finds its commit automatically, but blame on the actual fixed source line doesn't find its `BUG-ID` unless the message says so — especially once the review file's deleted.
7. **If applying these rules hits a genuine conflict or an undefined case, stop and flag it — don't resolve it silently and move on.** Rule 6 and the `needs-verification` severity entry below both exist because a real conflict/gap got resolved silently instead of surfaced, once each, before this rule existed. Treat any judgment call in this file the same weight as a design tradeoff on the actual code: surface it, don't guess and stay quiet about it.
8. **`Fixed` = what the commit changed, not a permanent guarantee.** Re-verify a cited `BUG-ID` if a stronger source has since appeared (happened once: commit `925a739` caught BUG-012/083 called "already correct" from `pybambu`/`bambuddy` only, before BambuStudio was consulted — cite commits here, not `*_PLAN.md` filenames, which get deleted once their plan lands). Cheap to recheck, costly to carry forward stale.

## Batching fixes to save verification cost

When fixing multiple `Open` bugs in one sitting (e.g. "fix everything in `Open`"), batch bugs that touch the same file or tightly-related files into **one commit** running `make check-fast` **once**, instead of one commit-and-verify cycle per bug. `check-fast` is expensive (multi-target build/test/clippy) and mostly redundant between two tiny adjacent fixes in the same file — paying that cost per-bug instead of per-batch is the same waste rule 6/Docs-regen/Review-file-lifecycle already reject elsewhere in this file.

This doesn't relax rule 6: every `BUG-ID` in the batch still gets its row moved `Open` → `Fixed` in that same commit's diff, and the commit message lists all of them (`Closes BUG-116, BUG-117`). A batch is just a wider atomic unit — resuming after an interruption is still "whatever's left in `Open` is what's left," since a batch that didn't finish committing leaves every one of its bugs' rows still in `Open`.

Don't fold in a bug that's paused mid-sitting (blocked on a decide-first question, or waiting on user input) just because its edits happen to be sitting in the working tree at commit time — `git add -A` will silently sweep up unrelated in-progress changes. Stage only the batch's own files explicitly, or `git reset` the paused bug's files first. This happened once (a paused bug's draft fix nearly got bundled into an unrelated batch's commit before it was caught) — treat it as a real footgun, not a hypothetical.

Group by what's naturally already being read/edited together, not by an artificial cap — one file touched by 3 unrelated bugs is one batch; two files each touched by one bug you happen to be doing back-to-back is two batches (verifying twice there is cheap to skip, but not worth forcing a same-commit merge that makes the diff harder to review).

## Next BUG-ID

Scan `Open` + `Fixed` + `Wontfix` for the current highest `BUG-NNN`, use `NNN+1`. Never reuse a retired number, never restart numbering because a section looks short.

## `Fixed` / `Wontfix` row schema

Same columns as `Open` plus one: `ID | Sev | Module | Title | Found | Closed | Detail`. No standing `commit:` column — see rule 6 for why a fix commit can't carry one, and "Review-file lifecycle" below for how the hash gets in eventually. `Closed` is the date the row moved out of `Open` (fixed or marked wontfix, not the date it was found). `Wontfix`'s `Detail` states the one-line reason it's not a real issue, same 3-line budget as everything else in this file.

```
| BUG-NNN | SevX | module/path.rs | one-line title | YYYY-MM-DD | YYYY-MM-DD | link to review-file section, or a terse inline note + fix commit's short hash once that file's deleted |
```

## Review-file lifecycle

This step is safely resumable by construction: if interrupted after rewriting some rows but before deleting the file, nothing's corrupted — the un-rewritten rows still resolve, and re-running the grep below picks up exactly where it left off. Once every row that references a dated `NN-NN-REVIEW.md` has left `Open` (grep `Open` for the filename to confirm — zero hits means clear) _and_ that file's own "Plausible, Unverified Findings" section (if the `deep-review` skill produced it) is empty or has been manually triaged — a `PLAUSIBLE` finding never gets a `BUG-ID` on its own, so it's invisible to the `Open`-table check and would be silently lost if the file's deleted while one's still sitting there: delete the review file in its own signposted commit, same convention as a completed `*_PLAN.md` (see `CLAUDE.md`'s Key Conventions). Before deleting, rewrite each affected `Fixed`/`Wontfix` row's `Detail` column from a link into that file to a terse inline note (file:line + fix direction, same 3-line budget as rule 1) including the fix commit's short hash — look it up via `git log --grep="BUG-NNN"` (findable because rule 6 requires the `BUG-ID` in the commit message) rather than guessing or leaving it out. This is a cleanup commit, not a fix commit, so rule 6 (same-commit-as-the-fix) doesn't constrain it, and the hash it's citing already exists and is immutable — no chicken-and-egg like a fix commit trying to embed its own hash.

## Docs regen

Before ending a session that closed one or more `BUG-ID`s, check whether any of them touched public API or doc comments. If so, run `make docs` and commit any changes in their own commit — not folded into a fix commit (same reasoning as everything else here: batch it, don't pay the cost per-fix). This isn't tied to review-file deletion — an ad hoc single `BUG-ID` closed outside a sweep, with no review file involved at all, still needs this before the session ends.

## Severity

- **Sev1** — can cause unsafe physical behavior (temp overshoot past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard). Blocks release.
- **Sev2** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible/common condition. Blocks release.
- **Sev3** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps. Tracked, non-blocking.
- **needs-verification** — can't be triaged into the above without something only real hardware can confirm (a wire capture, physical behavior on a specific model). Not a severity in itself. Assigning one once evidence lands is a surfaced decision, same as any other design tradeoff (rule 7) — state what resolved it (wire capture, cross-reference to a known-good source) in `Detail`, don't pick quietly.
- **N/A** — a resolved `Wontfix` row whose evidence-gathering started from `needs-verification` and concluded "not a bug." Distinct from leaving `needs-verification` in place on a resolved row, which reads as still-pending; `N/A` marks it as the closed, no-severity-applicable state it now is. Only ever appears in `Wontfix`, never in `Open` or `Fixed` — a confirmed real bug gets a real `SevX`, not `N/A`.

## Release bar

Zero open Sev1, zero open Sev2. Sev3 doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Relationship to a future `CHANGELOG.md`

Different audience — internal tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists for `BACKLOG.md`, and have its entries cite `BUG-ID`s from here for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
