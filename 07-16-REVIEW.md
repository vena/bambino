# bambino — Third-Party Cross-Verification Review (2026-07-16)

**Status:** IN PROGRESS

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

### P1/A1 camera (port 6000) reconnect-timing footgun — PLAUSIBLE, not yet investigated

bambuddy's `Fix P1/A1 camera black screen from fan-out churn on single-connection cams` (#2521)
found that port 6000 (binary JPEG chamber-image stream, P1/A1 series) is a single-connection
socket: reopening it before the previous connection's TCP FIN completes leaves the printer
serving an orphaned socket until TCP keepalive reaps it (~20 min stall observed). bambino's
`src/camera/binary.rs` implements the client side of this same port-6000 protocol. Not yet
checked whether `bambino`'s camera reconnect path (if any exists at the client level — bambino
is a library, not a long-lived multi-subscriber service like bambuddy, so the failure shape may
not transfer directly) has, or needs, any doc note warning callers not to redial port 6000
before the prior connection is confirmed closed. Needs a read of `src/camera/binary.rs`'s and
`src/camera/mod.rs`'s reconnect/redial surface before this can be triaged CONFIRMED or
Wontfix-as-caller-responsibility.

## Still to review

- Remaining bambuddy commits not yet individually checked (non-protocol-keyword-matched ones —
  cloud sign-in state, library-tag response model, non-0.4mm-nozzle AMS slot config — initial
  pass judged these app/UI-layer, not bambino-relevant, but not exhaustively confirmed).
- ha-bambulab: pulled, but no commits matched protocol-relevant keywords in the initial grep
  pass — worth a second look at the full unfiltered log in case the keyword filter missed
  something (e.g. a fix described without mentioning "mqtt"/"ams"/etc. by name).
- BambuStudio: ~180 commits pulled, dominated by CI/translation-file/UI churn; only 2 matched
  protocol-relevant keywords (`chamber/bed temp popup trigger removal`, `thumbnail camera angle`
  — both judged UI-only on first pass, not re-verified in depth).

---

**`BACKLOG.md` is the status source of truth from here on.** The summary table below is a
point-in-time snapshot as of this review and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | One-line |
|---|---|---|---|
| BUG-143 | Sev2 | client/ams.rs, quirks/mod.rs, quirks/models/p1.rs | AMS remote drying accepted-then-discarded by P1 firmware, now guarded |
