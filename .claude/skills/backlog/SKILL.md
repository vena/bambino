---
name: backlog
description: Rules for adding, updating, and triaging entries in this repo's BACKLOG.md findings tracker — entry format, severity definitions (Sev1/Sev2/Sev3), the release bar, next-BUG-ID logic, and status-update discipline. Use whenever adding a new bug/finding to BACKLOG.md, closing or triaging an existing BUG-ID, checking whether the crate meets its release bar, or deciding how a fix commit should update tracker status. Also covers the relationship to a future CHANGELOG.md. Invoked by the deep-review skill for its severity rubric — don't duplicate these definitions elsewhere; this file is the one source of truth.
---

# Backlog rules (bambino)

**Step 0, mandatory, before touching `BACKLOG.md`:** if this session has `mcp__lean-ctx__*` tools in its deferred-tools list, run `ToolSearch("select:mcp__lean-ctx__ctx_read,mcp__lean-ctx__ctx_patch")` first and use `ctx_read(mode="anchored")` → `ctx_patch` for the edit, not native Read/Edit. This is restated here on purpose, redundant with the global lean-ctx bootstrap rule — sessions drift off a rule stated once at the top and not repeated near the point of use.

## What `BACKLOG.md` is (and isn't)

`BACKLOG.md` holds data only: one table row per known bug/gap, in `Open`/`Fixed`/`Wontfix` sections. It does not hold the rules for maintaining itself (that's this file) and does not hold investigative detail (that's a dated `NN-NN-REVIEW.md`, one row's `Detail` column links to the relevant section). Keep it that way — if you're about to write a paragraph of prose into `BACKLOG.md`, stop and either shorten it to fit the entry-format rule below or put it in a review file and link out.

## Entry rules

1. **One entry = one table row + at most 3 lines of prose.** File:line, one-sentence problem, one-sentence fix direction. Anything longer goes in a dated review file (or a `*_PLAN.md` for multi-phase design) — link to it, don't paste it.
2. **No narrative.** Don't record *how* a bug was found or what was tried — that belongs in the commit message or the source review file.
3. **Update status in place.** Fixing BUG-004 means editing BUG-004's row (`status: fixed`, `closed: <date>`, `commit: <sha>`) — never append a new row for the same bug.
4. **Close stale entries aggressively.** Turns out to be a non-issue on investigation → `wontfix` with a one-line reason, move on.
5. **Severity is fixed at triage time** and doesn't get re-litigated per entry. If the definition itself needs to change, change it once here, not per-row.
6. **The commit that fixes a bug updates this file's status in the same commit.** No separate "update the backlog" follow-up — same diff, or the tracker silently goes stale exactly when it's most useful.

## Next BUG-ID

Scan `Open` + `Fixed` + `Wontfix` for the current highest `BUG-NNN`, use `NNN+1`. Never reuse a retired number, never restart numbering because a section looks short.

## Severity

- **Sev1** — can cause unsafe physical behavior (temp overshoot past a real hardware ceiling, uncommanded/unsafe motion, bypass of a documented safety guard). Blocks release.
- **Sev2** — silent data corruption, silent success-on-failure, or a core feature broken under a plausible/common condition. Blocks release.
- **Sev3** — everything else: narrow edge cases, footguns with a workaround, doc drift, process gaps. Tracked, non-blocking.
- **needs-verification** — can't be triaged into the above without something only real hardware can confirm (a wire capture, physical behavior on a specific model). Not a severity in itself; assign one once verified.

## Release bar

Zero open Sev1, zero open Sev2. Sev3 doesn't block. (This bar itself can change — if it does, edit this one line, don't restate it elsewhere.)

## Relationship to a future `CHANGELOG.md`

Different audience — internal tracker vs. user-facing release notes. Don't conflate or auto-generate one from the other. When `CHANGELOG.md` exists, build a `changelog` skill the same way this one exists for `BACKLOG.md`, and have its entries cite `BUG-ID`s from here for traceability. Until then, this note is enough — don't build the changelog skill speculatively for a file that doesn't exist yet.
