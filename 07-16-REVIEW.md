# bambino — Third-Party Cross-Verification Review (2026-07-16)

**Status:** COMPLETE (2026-07-16) — all findings resolved into `BUG-ID`s, zero left in `BACKLOG.md`'s `Open` table for this review; see §4 below for the "still to review" follow-up.

This review is not a full-crate module sweep (see the `deep-review` skill for that). It's a
targeted cross-verification: pull the latest commits from three third-party projects that
independently implement the same Bambu Lab LAN protocol —
`/Users/vena/Documents/Projects/Personal/bambuddy` (self-hosted print-farm manager),
`/Users/vena/Documents/Projects/Personal/ha-bambulab` (Home Assistant integration), and
`/Users/vena/Documents/Projects/Personal/BambuStudio` (Bambu's own slicer, the protocol's
origin) — and check whether anything they've recently fixed or discovered reveals a gap or bug
in bambino.

Same scope exclusions as `deep-review`: minor LAN-only security nitpicks and style/refactor
suggestions are out of scope, not overlooked. `CONFIRMED` findings are verified bugs; `PLAUSIBLE`
findings look real but couldn't be fully verified (e.g. no hardware access to confirm the
failure path) and are reported separately, not discarded. This file is meant to be read
standalone by a fresh session — it does not assume any prior conversation context. `file:line`
references may have drifted if other changes landed on `main` since this review ran.

## Source repos, pulled commit ranges

| Repo | Old HEAD (before pull) | New HEAD | Commits pulled |
|---|---|---|---|
| bambuddy | `a1a0cbdf` | `a6e7d671` | ~35 |
| ha-bambulab | `f7e52909` | (pulled, no protocol-relevant commits found) | ~10 |
| BambuStudio | `ba4f27b1` | `ba049f6` | ~180 (mostly CI/translation/UI churn) |

## 1. AMS remote drying silently no-ops on P1P/P1S — CONFIRMED, fixed

bambuddy's `fix(drying): P1 AMS drying is screen-only — stop offering it` (#2533) reported that
P1-connected AMS firmware acks `ams_filament_drying` with `result: success` and then discards it
— confirmed against Bambu's own P1 manual ("P1S connected AMS drying functions may only be
controlled from the P1S screen"). bambino's `PrinterClient::start_drying()` (`src/client/ams.rs`)
had no model guard and would dispatch the command on any host, including P1, silently no-oping.

Verified independently against real P1S hardware in this session (`bambino-cli control ... ams
dry`, command published successfully, `dry_status` telemetry never left `0`) and against a
running ha-bambulab instance (same result).

**Fixed as `BUG-143`** (see `BACKLOG.md`, `Fixed`) — commit `1edb858`. Added
`ModelQuirks::supports_ams_remote_drying()` (default `true`, `false` for P1);
`start_drying()` now returns `BambuError::ModelMismatch` before dispatch on P1 instead of
sending a command the firmware will accept-then-drop. `reference/05_materials_ams.md` updated
with the firmware caveat.

*(Note: this fix landed inline, before the user redirected this session back to deep-review
discipline — findings from here on are tracked in this file / `BACKLOG.md` first, not fixed
inline.)*

## 2. TLS 1.2 pinning — checked, no gap

bambuddy's `fix(tls): declare TLS 1.2 as the minimum for printer FTPS and MQTT` confirms (via
direct probing of an X1C and an H2D on :990/:8883) that Bambu printer firmware only accepts TLS
1.2 and rejects 1.0/1.1/1.3. bambino already pins `&[&rustls::version::TLS12]` explicitly in both
`build_unsafe_client_config_with_options` and `build_verified_client_config_with_options`
(`src/io/tokio.rs:119,165`), and pins `Tls1_2` as `min_version` in the embassy/mbedtls-rs backend
(`src/io/embassy.rs:194`). No gap — this cross-check just corroborates the existing pin, no
action needed.

## 3. AMS `tray_exist_bits`-authoritative presence — checked, no gap

bambuddy's `fix(ams): show "?" not "Empty" for non-RFID spools using tray_exist_bits` (#2527)
established that firmware's `tray_exist_bits` bitmask, not per-tray `state`/`tray_type`, is the
authoritative "spool physically present" signal (same one BambuStudio uses). bambino's
`ams::parser::evaluate_spool_presence` (`src/ams/parser.rs`) already derives presence from
`tray_exist_bits` directly, not from per-tray heuristics — already correct, no action needed.

## Plausible, Unverified Findings

### P1/A1 camera (port 6000) reconnect-timing footgun — RESOLVED as `BUG-161` (Open)

This lead was picked up and investigated by the later `07-16-MODULE-REVIEW.md` full-crate sweep
(§5, camera unit). Resolution: `BambuBinaryCameraStream` doesn't own dial/redial logic (only
wraps an already-connected stream), so there's no code defect to fix — but the underlying
printer-side single-connection behavior bambuddy found is real, so a caller-facing doc warning
against fast port-6000 redial is tracked as a real (doc-only) fix, not dismissed. See
`BACKLOG.md`'s `Open` table, `BUG-161`.

## 4. Full unfiltered commit-range re-check (resolves the original "still to review" note)

The original keyword-grep pass over each repo's pulled range was re-done as a full unfiltered
read of every commit subject (not just keyword matches), per this follow-up:

- **bambuddy** (`a1a0cbdf..a6e7d671`, 51 commits, not ~35 — original count was off): every
  commit read. `fix(jog): stop disabling firmware endstops; warn that limits aren't enforced`
  (#2579, landed after the original pull) found a real gap — see §5 below. `fix(ftp): stop a
  slow upload from being retried on top of itself` (#2529) found a second real gap — see §6.
  `fix(diagnostic): skip external-storage check on P1S/P1P` (#2524) is bambuddy's own
  diagnostic-UI feature, no bambino equivalent. `Support non-0.4mm nozzles in AMS Slot config`
  (#1899) needs 3MF slice-metadata parsing bambino doesn't do (FTPS only moves raw bytes) — out
  of scope, not a gap. Everything else confirmed app/UI/DB/deploy-layer (inventory, queue,
  camwall, backup, currency, smart-plugs, cloud sign-in status — explicitly out of scope,
  bambino is LAN-only).
- **ha-bambulab**: `git fetch origin` confirms `origin/main` is still at `f7e52909`, unchanged
  since 2026-06-28 — zero new commits exist to check. The original "worth a second look" note
  was based on a false premise (implied unreviewed commits existed); there were none.
- **BambuStudio** (`ba4f27b1..ba049f6`, 138 commits, not ~180): every commit subject read.
  Confirms the original 2 keyword matches (chamber/bed popup removal, thumbnail camera angle)
  are both UI-only, no gap. Nothing else in the range is wire-protocol relevant — rest is
  filament-preset/color-management/assembly-view/CI churn. E3D high-flow nozzle support
  (`ba968b0` and siblings) adds a new `nozzle_type` wire string bambino already handles as an
  opaque `Option<String>` (BambuStudio's `_str2_nozzle_flow_type` classification is display-only,
  operating on a separate slice_info field bambino never parses) — no bambino change needed.

## 5. X/Y relative moves have no distance cap — CONFIRMED, tracked

bambuddy's #2579 is from direct H2D hardware instrumentation: a clean relative move ran straight
past the travel limit on real hardware, with firmware ignoring `M211` state entirely for
MQTT-received G-code. bambino's Z-axis path (`client/motion.rs`, `quirks::format_z_move_gcode`)
never had bambuddy's specific bug (always sends `M211 S1`, never `S0`, so no toggle-and-leave-
disabled failure mode) and has its own real (if partial — distance-only, not position-aware)
protection via a client-side `z_max` bound. Doc comments overstating `M211`'s actual protection
have been corrected (`quirks/mod.rs`, `client/motion.rs`, `reference/04_toolhead_thermal_
motion.md`). The X/Y branch of `move_relative` has no distance bound at all, unlike Z — tracked
as `BUG-163` (`BACKLOG.md`, `Open`, Sev3; not Sev1, since neither bambino nor bambuddy can
implement true crash prevention without absolute position telemetry the printer doesn't send).

## 6. FTPS `upload_file` write path has no stall-timeout — CONFIRMED, tracked

While cross-checking bambuddy's #2529 (a too-short flat upload timeout killing healthy slow
transfers), found the opposite gap in bambino: `upload_file`'s data-write loop
(`ftps/client.rs:674-687`) has no deadline at all, unlike the STOR negotiation and
post-transfer confirmation reads immediately around it, which both use `read_deadline_ms`. A
stalled write hangs forever. Same class of gap `BUG-159` just fixed for MQTT's `write_frame`.
Tracked as `BUG-164` (`BACKLOG.md`, `Open`, Sev3).

---

**`BACKLOG.md` is the status source of truth from here on.** The summary table below is a
point-in-time snapshot as of this review and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | One-line |
|---|---|---|---|
| BUG-143 | Sev2 | client/ams.rs, quirks/mod.rs, quirks/models/p1.rs | AMS remote drying accepted-then-discarded by P1 firmware, now guarded |
