# bambino — Findings Backlog

Data only: one row per known bug/gap, `Open`/`Fixed`/`Wontfix`. Doesn't replace REVIEW/PLAN files (full investigative record, linked per row) or hold its own rules.

**Rules, severity definitions, release bar, and next-BUG-ID logic live in the `backlog` skill, not here** — invoke it before adding or updating an entry. Keeping the rules out of this file is deliberate: a passive preamble nobody re-reads before editing is exactly how `CLAUDE.md` got out of hand.

---

## Open

*(none open as of 2026-07-09 — release bar met)*

## Fixed

| ID | Sev | Module | Title | Found | Closed | Detail |
|---|---|---|---|---|---|---|
| BUG-001 | Sev2 | mqtt/commands, diagnostics/kprofile | `sequence_id` unclamped in 6 public command constructors | 2026-07-09 | 2026-07-09 | 6 constructors serialized `sequence_id`/`subtask_id` without `clamp_task_id()`, risking the documented 32-bit firmware lockup — fixed by wrapping each in `clamp_task_id()` |
| BUG-002 | Sev3 | mqtt/client/mod.rs | `get_in_flight_count` doc comment says slice, returns `usize` | 2026-07-09 | 2026-07-09 | mqtt/client/mod.rs:501 — doc comment said "returns a slice," function returns `usize` — corrected the doc comment |
| BUG-003 | Sev2 | ftps/client.rs | `download_file` has no integrity recheck (unlike `upload_file`'s SIZE recheck) | 2026-07-09 | 2026-07-09 | ftps/client.rs:600-669 `download_file` had no SIZE-recheck symmetric to `upload_file`'s, so a truncated transfer could report success — added a SIZE comparison after transfer |
| BUG-004 | Sev3 | ftps/client.rs | 6 single-reply commands don't poison on `read_response`/`write_command` failure | 2026-07-09 | 2026-07-09 | 6 single-reply methods didn't poison the client on failure — poisoned them, matching the transfer-method precedent; user chose poisoning (deterministic) over the cheaper drain-and-continue alternative |
| BUG-005 | Sev3 | camera/rtsps.rs | IPv6 addresses not bracketed in RTSPS URLs | 2026-07-09 | 2026-07-09 | camera/rtsps.rs `build_rtsps_url`/`rewrite_rtsp_request_uri` didn't bracket an IPv6 host, producing a malformed URL — wrapped in `[...]` when `IpAddr::is_ipv6()` |
| BUG-006 | Sev3 | camera/binary.rs | `build_handshake_packet` accepts empty access code | 2026-07-09 | 2026-07-09 | camera/binary.rs `build_handshake_packet`'s alphanumeric check passed vacuously on an empty string — added an explicit empty-string rejection |
| BUG-007 | Sev3 | client/connect.rs | `with_connect_timeout(0)` fails instantly instead of disabling the timeout | 2026-07-09 | 2026-07-09 | client/connect.rs `race_against_connect_timeout` treated `connect_timeout_secs == 0` as an instant deadline instead of disabled — special-cased `0` to actually disable it |
| BUG-008 | Sev3 | io/tokio.rs | `CnFallbackServerVerifier` ignores intermediate certs (single-hop chain only) | 2026-07-09 | 2026-07-09 | io/tokio.rs:257 `verify_server_cert` ignored `_intermediates`, single-hop chains only — now walks the full chain through intermediates to the trusted root |
| BUG-009 | Sev2 | discovery/mod.rs | Degraded-mode SSDP bind is order-dependent, not "try all ports" | 2026-07-09 | 2026-07-09 | discovery/mod.rs:176-190 returned `Err` on the first port's bind failure instead of trying the rest — now tries every port before failing |
| BUG-010 | Sev2 | discovery/mod.rs | Initial broadcast loop aborts whole sweep on one engine's send failure | 2026-07-09 | 2026-07-09 | discovery/mod.rs:203-207 — one engine's send failure aborted the whole sweep — now tolerates a single engine's initial-broadcast failure |
| BUG-011 | Sev2 | discovery/parser.rs | SSDP-discovered serial not uppercased despite doc promise | 2026-07-09 | 2026-07-09 | discovery/parser.rs `parse_ssdp_payload` didn't uppercase the discovered serial despite the doc's promise — now uppercases it |
| BUG-012 | Sev2 | ams/parser.rs | State-10 tray-clearing: doc said clear, code didn't | 2026-07-09 | 2026-07-09 | ams/parser.rs:112-116 `clean_stale_tray_data` — state 10 alone didn't clear stale tray data though the doc said it should — added state 10 to the clearing condition; resolved via pybambu/Bambuddy cross-check (no H2D hardware available), see `reference/05_materials_ams.md`'s verification-source note |
| BUG-013 | Sev3 | error.rs | `no_std` `Display` impl sync unverified by any test that actually runs | 2026-07-09 | 2026-07-09 | error.rs — `test_display_consistency` only runs under `std`, so the `no_std` impl's sync was never actually test-verified, just manually inspected — doc comment corrected to stop claiming otherwise (process gap, no `no_std` test harness exists to fix it properly) |

## Wontfix

*(none yet)*
