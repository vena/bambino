**Status:** COMPLETE

# Deep Review — 2026-07-17

This is the final review sweep before this crate's initial release. It was produced by partitioning the full `src/` and `tests/` tree into 19 module-scoped review units and running one parallel review agent per unit (see `.claude/skills/deep-review/SKILL.md` for the methodology). Each agent read only its assigned files plus any cross-cutting invariants (`.claude/rules/*.md`, nested `CLAUDE.md` files) that matched its unit's paths.

**Scope exclusions (by design, not oversight):**
- Minor security issues are skipped — this crate is explicitly LAN-only (see `README.md`'s opening paragraph and Safety Notice). Cert-verification bypass, plaintext fallback, etc. are only flagged if they violate this crate's *own* stated behavior.
- Style/refactor suggestions and naming preferences are skipped entirely, except where a name actively misrepresents behavior (inverted-sense booleans, a function name implying the opposite of what it does) — those are correctness/footgun risks in disguise, not style.

**CONFIRMED vs. PLAUSIBLE:** Each finding is tagged by the reviewing agent. `CONFIRMED` means the agent verified the failure path actually triggers. `PLAUSIBLE` means the finding looks real but couldn't be fully verified in-agent (e.g. can't confirm the failure path triggers, or the invariant it violates is itself ambiguous). `CONFIRMED` findings are promoted to `BACKLOG.md` immediately with an assigned severity. `PLAUSIBLE` findings are collected in their own section below and re-verified by direct code read during Step 5 (finalization) before this file is marked complete — none are left for a human to manually triage.

This file is meant to be read standalone by a fresh session with no other context from the sweep that produced it.

**Caveat:** file:line references may have drifted if other commits landed on `main` since this sweep ran.

## 1. src/io/{mod.rs, tokio.rs, tokio/tests.rs, tokio/cert_verify.rs}
NO ISSUES FOUND. read_chunk EOF-vs-no-bytes-yet mapping, race/Raced cancel-safety, TokioUdpSocket 100ms timeout bound, port-less TlsConnector::connect, and CnFallbackServerVerifier's chain-walk (off-by-one/infinite-loop/duplicate-subject) all verified correct.
## 2. src/io/{embassy.rs, esp_idf.rs}
NO ISSUES FOUND. Cross-checked against BACKLOG.md — all previously-noted discrepancies (TLS error mapping, UDP pacing, timeout-zero semantics, EmbassyRawStreamFactory error collapsing) already tracked Fixed (BUG-031, BUG-049, BUG-050, BUG-063, BUG-064, BUG-076, BUG-077).
## 3. src/mqtt/client/{mod.rs, codec.rs, frame.rs, pending.rs} + tests/mqtt_test.rs + tests/common/mock_mqtt.rs
One PLAUSIBLE finding (see Plausible section below) — `write_frame_with_timer` can silently transmit a partial frame on timeout without poisoning the connection. Note: distinct from already-Fixed BUG-159 (which added the timeout itself) — this is a gap in that fix, not a duplicate. No other issues; mock broker byte-for-byte matches codec.rs.
## 4. src/mqtt/commands/*.rs
NO ISSUES FOUND. Task-ID clamping is fully type-enforced (`ClampedTaskId`) across all 21 constructors, not just convention. Wire field/envelope names verified against reference/03 and reference/05.
## 5. src/ftps/{mod.rs, parser.rs, protocol.rs, protocol/tests.rs}
One PLAUSIBLE finding (see Plausible section below) — `reference/02_ftps.md` §2.2 still describes the disproven pre-BUG-088-fix filename-joining behavior; the code itself (verified) is correct. No code-level issues found.
## 6. src/ftps/client.rs + tests/ftps_test.rs + tests/common/mock_ftps.rs
NO ISSUES FOUND. Poisoning invariant (every public method poisons on transport error, never on a wrong-but-received reply code) verified fully recurred-and-fixed post BUG-004, no new misses. SNI/serial and validate_ftp_path both correctly applied everywhere.
## 7. src/camera/{mod.rs, binary.rs, rtsps.rs} + tests/camera_test.rs + tests/common/mock_camera.rs
NO ISSUES FOUND. Frame-size cap, resumable read state machine, JPEG marker bounds, handshake packet construction, and RTSPS URL helpers all verified correct; connect-time bound on authenticate() confirmed via outer race_against_connect_timeout in connect.rs.
## 8. src/client/{mod.rs, connect.rs, types.rs, dummy.rs, storage.rs, camera.rs}
NO ISSUES FOUND. Camera protocol-check ordering, FTPS poison/reset, SNI-vs-IP on every TlsConnector::connect() call, connect_timeout_secs==0 special case, task-id clamping, and consuming/non-consuming builder semantics all verified against documented invariants.
## 9. src/client/{ams.rs, hardware.rs, motion.rs, print.rs, thermal.rs, telemetry.rs}
NO ISSUES FOUND. Bed-temp voltage clamp direction/default, K-profile auto-priming + reset-on-disconnect, fan-speed quirks guards, poll_telemetry pending-buffer drain, motion homing safety, and AMS addressing ranges all verified correct against reference docs and quirks constants.
## 10. tests/client_test.rs + tests/common/{client.rs, io.rs}
NO ISSUES FOUND. Safety-critical coverage confirmed present and correct for bed/nozzle/chamber clamping, X1C voltage-dependent ceiling, homing interlocks, fan routing, K-profile priming, FTPS retry/reconnect, and connect-timeout-zero/SNI behavior. No test asserts an unverifiable write-count/framing claim.
## 11. src/types/telemetry/{mod.rs, ams.rs, device.rs, diagnostics.rs, report.rs, version.rs}
NO ISSUES FOUND. Note: `version.rs` does not exist in this codebase (only mod/ams/device/diagnostics/report present) — confirmed via ctx_tree, nothing to review there. All unpack_temperature()/unpack_bed_telemetry() call sites, bitmask constants (incl. BUG-104 bit-24-25 boundary), IDEX nozzle routing, and merge_from implementations verified against reference docs.
## 12. src/types/telemetry/tests/*.rs + tests/telemetry_replay_test.rs
One PLAUSIBLE finding (see Plausible section below) — dry_sub_status() (the exact BUG-104 field) is only ever asserted against Some(0) across all 4 tests exercising it, a coverage gap for that bug class. No other issues; bit-math, merge_from semantics, and BUG-110 ethernet-active regression test all verified correct.
## 13. src/ams/{mapping.rs, parser.rs, mod.rs}
NO ISSUES FOUND. mod.rs is a pure re-export module.
## 14. src/discovery/{mod.rs, parser.rs}
NO ISSUES FOUND. discover_devices's generic bound is correctly BindableUdpSocket (not the weaker AsyncUdpSocket) — udp-socket-binding.md invariant holds. Parsing matches reference/01_network_discovery.md §1.1 throughout.
## 15. src/diagnostics/{hms.rs, kprofile.rs, mod.rs}
NO ISSUES FOUND. HMS decoding matches reference spec (incl. historically-fixed BUG-108/109 semantics); K-profile priming doc/impl consistent with client/ams.rs; all 5 kprofile.rs sequence-ID constructors go through ClampedTaskId; IDEX/single-nozzle addressing matches reference/05_materials_ams.md §5.3.
## 16. src/quirks/{mod.rs, models/*.rs}
One PLAUSIBLE finding (see Plausible section below) — shared P1Quirks reports supports_auxiliary_left_fan()==true unconditionally, but MODEL_MATRIX.csv lists the aux fan as Optional on P1P (vs Yes on P1S). No CONFIRMED bugs. Crate-wide check found no BambuModel-variant behavioral dispatch outside src/quirks/ (Key Invariant #2 upheld). All temp/build-volume/fan/AMS-pool quirk values verified against MODEL_MATRIX.csv and reference docs.
## 17. src/bin/bambino-cli/{main.rs, connection.rs, discover.rs, error.rs, table.rs, verify_tls.rs, inspect_cert.rs, camera.rs}
NO ISSUES FOUND. Argument order/positional shapes cross-checked against callee signatures; SNI-uses-serial convention followed in verify_tls.rs/inspect_cert.rs; no argument-order or misleading-flag-name bugs found.
## 18. src/bin/bambino-cli/{control.rs, storage.rs, probe.rs, monitor/dashboard.rs, monitor/mod.rs}
NO ISSUES FOUND. gcode-raw --unsafe gate fails closed on any non-"yes" input; monitor::run/dump(follow) both call tick_zombie_check every ping tick per the documented pattern; start_drying argument order, storage() reborrow, deep_merge, and probe dispatch all verified correct.
## 19. src/{error.rs, models.rs, lib.rs, identity.rs}, src/types/mod.rs, src/mqtt/mod.rs
NO ISSUES FOUND. All 9 Error variants present/matching in both std (thiserror) and no_std Display impls, covered by test_display_consistency. resolve_model() confirmed as the sole model-resolution dispatch point, no behavioral BambuModel matching found beyond it. Re-exports verified against actual pub declarations.

## Plausible, Unverified Findings — Re-verified and Triaged

All four `PLAUSIBLE` findings from the sweep were re-verified by direct code read during finalization (not by re-stating the reviewing agent's claim) and promoted to `BACKLOG.md`'s `Open` table. All landed at Sev3 (non-blocking) — none involve unsafe physical behavior or silent data corruption:

- **§3 (mqtt-client):** Confirmed by reading `src/mqtt/client/mod.rs:121-156` directly — `write_frame_with_timer` races the multi-syscall `write_frame` (which internally loops via `write_all`) against a single timeout, and every caller (`publish_command_with_timer`, `send_ping_with_timer`, the PUBACK ack in `poll_wire`) treats a timeout as a plain retryable error with no connection-poisoning. Real gap, but MQTT framing makes a resulting desync likely to surface as a loud parse/protocol error on the next read rather than silent corruption — promoted as **BUG-249**, Sev3.
- **§5 (ftps-protocol):** Confirmed — pure documentation drift in `reference/02_ftps.md` §2.2, the code itself (`parse_unix_listing`) is correct per BUG-088's fix. Promoted as **BUG-250**, Sev3 (doc drift).
- **§12 (telemetry-tests):** Confirmed — a real test-coverage gap for the exact `AmsUnit.info` bit range BUG-104 broke once before. Promoted as **BUG-251**, Sev3 (process gap).
- **§16 (quirks):** Confirmed against `MODEL_MATRIX.csv`'s Aux Part Cooling Fan row — P1P is listed `Optional`, P1S `Yes`, but the shared `P1Quirks` struct reports `true` unconditionally for both. Promoted as **BUG-252**, Sev3 (footgun, likely low-impact if printer telemetry already reports fan-absent).

## Summary

**`BACKLOG.md` is the status source of truth from here on** — the table below is a point-in-time snapshot as of this sweep's completion and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | One-line |
|---|---|---|---|
| BUG-249 | Sev3 | mqtt/client/mod.rs | Write timeout can leave a partial frame on the wire without poisoning the connection |
| BUG-250 | Sev3 | reference/02_ftps.md | Reference doc describes pre-BUG-088 filename-joining behavior the code no longer has |
| BUG-251 | Sev3 | types/telemetry/tests/ams.rs | `dry_sub_status()` test coverage never exercises a nonzero value |
| BUG-252 | Sev3 | quirks/models/p1.rs | Shared P1Quirks over-reports aux fan support for P1P |

19/19 units clean of CONFIRMED findings. No Sev1 or Sev2 bugs found anywhere in the crate during this sweep — the release bar (zero open Sev1, zero open Sev2) is unaffected by this sweep's results. All 4 promoted bugs are Sev3 (non-blocking).
