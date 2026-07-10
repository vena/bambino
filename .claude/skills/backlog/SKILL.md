---
name: backlog
description: Rules for adding, updating, and triaging entries in this repo's BACKLOG.md findings tracker — entry format, Fixed/Wontfix row schema, severity definitions (Sev1/Sev2/Sev3), the release bar, next-BUG-ID logic, status-update discipline, and when to delete a fully-closed dated review file. Use whenever adding a new bug/finding to BACKLOG.md, closing or triaging an existing BUG-ID, reassigning a needs-verification entry to a real severity, checking whether the crate meets its release bar, or deciding how a fix commit should update tracker status. Also covers the relationship to a future CHANGELOG.md. Invoked by the deep-review skill for its severity rubric — don't duplicate these definitions elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

**Step 0, mandatory, before touching `BACKLOG.md`:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_patch")` first and use `ctx_read(mode="anchored")` → `ctx_patch` for the edit, not native Read/Edit. This is restated here on purpose, redundant with the global lean-ctx bootstrap rule — sessions drift off a rule stated once at the top and not repeated near the point of use.

## What `BACKLOG.md` is (and isn't)

`BACKLOG.md` holds data only: one table row per known bug/gap, in `Open`/`Fixed`/`Wontfix` sections. It does not hold the rules for maintaining itself (that's this file) and does not hold investigative detail (that's a dated `NN-NN-REVIEW.md`, one row's `Detail` column links to the relevant section — see "Review-file lifecycle" below for what happens to that link once the file's fully resolved). Keep it that way — if you're about to write a paragraph of prose into `BACKLOG.md`, stop and either shorten it to fit the entry-format rule below or put it in a review file and link out.

## Entry rules

1. **One entry = one table row + at most 3 lines of prose.** File:line, one-sentence problem, one-sentence fix direction. Anything longer goes in a dated review file (or a `*_PLAN.md` for multi-phase design) — link to it, don't paste it.
2. **No narrative.** Don't record *how* a bug was found or what was tried — that belongs in the commit message or the source review file.
3. **Move status by moving rows between tables, not by adding a status column.** `BACKLOG.md` is three separate tables by section (`Open`/`Fixed`/`Wontfix`), not one table with a status field — fixing BUG-004 means deleting its row from `Open` and inserting it into `Fixed`. Never leave a stale copy behind in `Open`, never append a second row for a bug already tracked elsewhere.
4. **Close stale entries aggressively.** Turns out to be a non-issue on investigation → `wontfix` with a one-line reason, move on.
5. **Severity is fixed at triage time** and doesn't get re-litigated per entry, except reassigning `needs-verification` to a real severity — see rule 7. If the severity definition itself needs to change, change it once here, not per-row.
6. **The commit that fixes a bug updates this file's status in the same commit, and its message references the `BUG-ID` it closes** (e.g. `Closes BUG-012`). No separate "update the backlog" follow-up — same diff, or the tracker silently goes stale exactly when it's most useful. Referencing the `BUG-ID` is the one direction `git blame` doesn't cover for free: blame on a `BACKLOG.md` row finds its commit automatically, but blame on the actual fixed source line doesn't find its `BUG-ID` unless the message says so — especially once the review file's deleted.
7. **If applying these rules hits a genuine conflict or an undefined case, stop and flag it — don't resolve it silently and move on.** Rule 6 and the `needs-verification` severity entry below both exist because a real conflict/gap got resolved silently instead of surfaced, once each, before this rule existed. Treat any judgment call in this file the same weight as a design tradeoff on the actual code: surface it, don't guess and stay quiet about it.

## Next BUG-ID

Scan `Open` + `Fixed` + `Wontfix` for the current highest `BUG-NNN`, use `NNN+1`. Never reuse a retired number, never restart numbering because a section looks short.

## `Fixed` / `Wontfix` row schema

Same columns as `Open` plus one: `ID | Sev | Module | Title | Found | Closed | Detail`. No standing `commit:` column: a commit can't contain its own hash without amending (forbidden, see global git rules), and a follow-up commit to inject it right after the fact would violate rule 6's same-commit requirement. `git blame` on this row answers "which commit closed it" while the row still links to a review file. Once that link goes dead (see "Review-file lifecycle" below), the replacement inline note gets the commit hash written into it directly — by then it's a later, separate commit doing the rewrite, so there's no self-reference problem, just a `git log --grep` lookup.

```
| BUG-NNN | SevX | module/path.rs | one-line title | YYYY-MM-DD | YYYY-MM-DD | link to review-file section, or a terse inline note + fix commit's short hash once that file's deleted |
```

## Review-file lifecycle

Once every row that references a dated `NN-NN-REVIEW.md` has left `Open` (grep `Open` for the filename to confirm — zero hits means clear), delete that review file in its own signposted commit, same convention as a completed `*_PLAN.md` (see `CLAUDE.md`'s Key Conventions). Before deleting, rewrite each affected `Fixed`/`Wontfix` row's `Detail` column from a link into that file to a terse inline note (file:line + fix direction, same 3-line budget as rule 1) *including the fix commit's short hash* — look it up via `git log --grep="BUG-NNN"` (findable because rule 6 requires the `BUG-ID` in the commit message) rather than guessing or leaving it out. This is a cleanup commit, not a fix commit, so rule 6 (same-commit-as-the-fix) doesn't constrain it, and the hash it's citing already exists and is immutable — no chicken-and-egg like a fix commit trying to embed its own hash.

## Severity

- **Sev1** — can cause unsafe physical behavior (temp overshoot past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard). Blocks release.
- **Sev2** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible/common condition. Blocks release.
- **Sev3** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps. Tracked, non-blocking.
- **needs-verification** — can't be triaged into the above without something only real hardware can confirm (a wire capture, physical behavior on a specific model). Not a severity in itself. Assigning one once evidence lands is a surfaced decision, same as any other design tradeoff (rule 7) — state what resolved it (wire capture, cross-reference to a known-good source) in `Detail`, don't pick quietly.

## Release bar

Zero open Sev1, zero open Sev2. Sev3 doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Relationship to a future `CHANGELOG.md`

Different audience — internal tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists for `BACKLOG.md`, and have its entries cite `BUG-ID`s from here for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
