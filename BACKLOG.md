# bambino — Findings Backlog

Data only: one row per known bug/gap, `Open`/`Fixed`/`Wontfix`. Doesn't replace REVIEW/PLAN files (full investigative record, linked per row) or hold its own rules.

**Rules, severity definitions, release bar, and next-BUG-ID logic live in the `backlog` skill, not here** — invoke it before adding or updating an entry. Keeping the rules out of this file is deliberate: a passive preamble nobody re-reads before editing is exactly how `CLAUDE.md` got out of hand.

---

## Open

| ID | Sev | Module | Title | Found | Detail |
|---|---|---|---|---|---|
| BUG-004 | Sev3 | ftps/client.rs | 6 single-reply commands don't poison on `read_response` timeout | 2026-07-09 | [07-09-REVIEW.md §3b](07-09-REVIEW.md#3b-srcftpsclientrs--poisoning-coverage-gap-on-single-reply-commands) — needs a design call (poison vs. document), not just a mechanical fix |
| BUG-005 | Sev3 | camera/rtsps.rs | IPv6 addresses not bracketed in RTSPS URLs | 2026-07-09 | [07-09-REVIEW.md §4](07-09-REVIEW.md#4-srccamerartspsrs--ipv6-addresses-not-bracketed-in-rtsps-urls) |
| BUG-006 | Sev3 | camera/binary.rs | `build_handshake_packet` accepts empty access code | 2026-07-09 | [07-09-REVIEW.md §4b](07-09-REVIEW.md#4b-srccamerabinaryrs--build_handshake_packet-accepts-empty-access-code) |
| BUG-007 | Sev3 | client/connect.rs | `with_connect_timeout(0)` fails instantly instead of disabling the timeout | 2026-07-09 | [07-09-REVIEW.md §5](07-09-REVIEW.md#5-srcclientconnectrs--with_connect_timeout0-causes-immediate-spurious-timeout) |
| BUG-008 | Sev3 | io/tokio.rs | `CnFallbackServerVerifier` ignores intermediate certs (single-hop chain only) | 2026-07-09 | [07-09-REVIEW.md §6](07-09-REVIEW.md#6-srciotokiors--cnfallbackserververifier-ignores-intermediate-certs-single-hop-chain-only) |
| BUG-012 | needs-verification | ams/parser.rs | State-10 tray-clearing: doc says clear, code doesn't — unclear which is right | 2026-07-09 | [07-09-REVIEW.md §8](07-09-REVIEW.md#8-srcamsparserrs--possible-doccode-mismatch-on-state-10-tray-clearing-needs-hardware-verification-not-a-confirmed-bug) — needs a real H2D wire capture, not a code-only fix |
| BUG-013 | Sev3 | error.rs | `no_std` `Display` impl sync unverified by any test that actually runs | 2026-07-09 | [07-09-REVIEW.md §9](07-09-REVIEW.md#9-srcerrorrs--no_std-display-impl-sync-is-unverified-by-any-test-run-in-practice-soft-finding-not-a-live-bug) |

## Fixed

| ID | Sev | Module | Title | Found | Closed | Detail |
|---|---|---|---|---|---|---|
| BUG-001 | Sev2 | mqtt/commands, diagnostics/kprofile | `sequence_id` unclamped in 6 public command constructors | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §1](07-09-REVIEW.md#1-srcmqttcommands--srcdiagnosticskprofilers--unclamped-sequence-ids-real-bug-public-api-facing) |
| BUG-002 | Sev3 | mqtt/client/mod.rs | `get_in_flight_count` doc comment says slice, returns `usize` | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §2](07-09-REVIEW.md#2-srcmqttclientmodrs--stale-doc-comment-on-get_in_flight_count) |
| BUG-003 | Sev2 | ftps/client.rs | `download_file` has no integrity recheck (unlike `upload_file`'s SIZE recheck) | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §3](07-09-REVIEW.md#3-srcftpsclientrs--download_file-missing-integrity-recheck) |
| BUG-009 | Sev2 | discovery/mod.rs | Degraded-mode SSDP bind is order-dependent, not "try all ports" | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §7](07-09-REVIEW.md#7-srcdiscoverymodrs--degraded-mode-ssdp-bind-is-order-dependent-not-try-all-ports) |
| BUG-010 | Sev2 | discovery/mod.rs | Initial broadcast loop aborts whole sweep on one engine's send failure | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §7b](07-09-REVIEW.md#7b-srcdiscoverymodrs--initial-broadcast-loop-aborts-sweep-on-single-engine-send-failure) |
| BUG-011 | Sev2 | discovery/parser.rs | SSDP-discovered serial not uppercased despite doc promise | 2026-07-09 | 2026-07-09 | [07-09-REVIEW.md §7c](07-09-REVIEW.md#7c-srcdiscoveryparserrs--ssdp-discovered-serial-not-uppercased-despite-doc-promise) |

## Wontfix

*(none yet)*
