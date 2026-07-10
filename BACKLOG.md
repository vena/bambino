# bambino — Findings Backlog

Data only: one row per known bug/gap, `Open`/`Fixed`/`Wontfix`. Doesn't replace REVIEW/PLAN files (full investigative record, linked per row) or hold its own rules — and stands in for a real issue tracker until this repo has a GitHub remote, migrate to Issues then rather than extending this further.

**Rules, severity definitions, release bar, and next-BUG-ID logic live in the `backlog` skill, not here** — invoke it before adding or updating an entry. Keeping the rules out of this file is deliberate: a passive preamble nobody re-reads before editing is exactly how `CLAUDE.md` got out of hand.

---

## Open

| ID | Sev | Module | Title | Found | Detail |
|---|---|---|---|---|---|
| BUG-014 | Sev2 | ams/parser.rs | `evaluate_spool_presence` doesn't bounds-check `tray_id` before a bit-shift | 2026-07-10 | src/ams/parser.rs:60-65 — shift can exceed 31 bits on malformed wire data → `07-10-REVIEW.md` §1 |
| BUG-015 | Sev2 | ams/parser.rs | AMS-HT slots always report `Some(true)` presence regardless of actual state | 2026-07-10 | src/ams/parser.rs:50-54 → `07-10-REVIEW.md` §1 |
| BUG-016 | Sev3 | bin/bambino-cli/main.rs | `--help` missing `gcode-raw --unsafe` documentation | 2026-07-10 | src/bin/bambino-cli/main.rs:42-53 → `07-10-REVIEW.md` §3 |
| BUG-017 | Sev3 | bin/bambino-cli/probe.rs | Capture-window error discards all previously-captured probe results | 2026-07-10 | src/bin/bambino-cli/probe.rs:502,414 → `07-10-REVIEW.md` §4 |
| BUG-018 | Sev2 | client/connect.rs | No `disconnect_mqtt()`/`attach_mqtt()` — dead MQTT session has no recovery path | 2026-07-10 | src/client/connect.rs:92-114 → `07-10-REVIEW.md` §5 |
| BUG-019 | Sev2 | client/connect.rs | Sequence-counter reseed is a no-op under the default `DummyTimer` | 2026-07-10 | src/client/connect.rs:107-109 → `07-10-REVIEW.md` §5 |
| BUG-020 | Sev2 | client/connect.rs | `ensure_ftps`/`ensure_camera` consume config even on a failed connect attempt | 2026-07-10 | src/client/connect.rs:273-306,328-361 → `07-10-REVIEW.md` §5 |
| BUG-021 | Sev2 | client/telemetry.rs | `last_door_open` cache overwritten unconditionally, ignoring absent-field staleness contract | 2026-07-10 | src/client/telemetry.rs:156 → `07-10-REVIEW.md` §6 |
| BUG-022 | Sev3 | tests/client_test.rs | `test_sequence_id_wrapping` never exercises wraparound | 2026-07-10 | tests/client_test.rs:1346-1372 → `07-10-REVIEW.md` §7 |
| BUG-023 | Sev3 | tests/client_test.rs | No test coverage for X1C's voltage-dependent bed-temp ceiling | 2026-07-10 | tests/client_test.rs:213-291,543-584 → `07-10-REVIEW.md` §7 |
| BUG-024 | Sev2 | discovery/mod.rs | `poll_next_device` never stamps `discovery_port` | 2026-07-10 | src/discovery/mod.rs:113-134 → `07-10-REVIEW.md` §9 |
| BUG-025 | Sev3 | discovery/mod.rs | Module doc wrongly claims `discover_devices()` works on Embassy | 2026-07-10 | src/discovery/mod.rs:8 → `07-10-REVIEW.md` §9 |
| BUG-026 | Sev3 | lib.rs | Docs claim embassy TLS backend is `embedded-tls`, actually `mbedtls-rs` | 2026-07-10 | src/lib.rs:16,65 → `07-10-REVIEW.md` §10 |
| BUG-027 | Sev3 | lib.rs | Feature Flags table wrongly claims `tokio` enables the CLI binary | 2026-07-10 | src/lib.rs:63 → `07-10-REVIEW.md` §10 |
| BUG-028 | Sev3 | ftps/protocol.rs | `read_response` doesn't follow RFC 959 multi-line reply parsing | 2026-07-10 | src/ftps/protocol.rs:208-260 → `07-10-REVIEW.md` §11 |
| BUG-029 | Sev3 | ftps/client.rs | LIST/STOR/RETR initial write/read not poisoned on failure | 2026-07-10 | src/ftps/client.rs:389-400,558-569,641-652 → `07-10-REVIEW.md` §12 |
| BUG-030 | Sev3 | ftps/client.rs | `download_file`'s 426→SIZE recheck isn't symmetric with `upload_file`'s | 2026-07-10 | src/ftps/client.rs:598-657 → `07-10-REVIEW.md` §12 |
| BUG-031 | Sev2 | io/esp_idf.rs | `EspIdfTcpStream` Read/Write block with no preempt point | 2026-07-10 | src/io/esp_idf.rs:521-539 → `07-10-REVIEW.md` §14 |
| BUG-032 | Sev3 | mqtt/client/mod.rs | CONNACK codes 1-3 collapsed into `AccessDenied` along with 4-5 | 2026-07-10 | src/mqtt/client/mod.rs:182-201 → `07-10-REVIEW.md` §15 |
| BUG-033 | Sev2 | mqtt/commands/print_job.rs | `with_ams_mapping2()` doesn't sync `ams_mapping`, breaking documented 1:1 pairing | 2026-07-10 | src/mqtt/commands/print_job.rs:88-93,216-220 → `07-10-REVIEW.md` §16 |
| BUG-034 | Sev3 | types/telemetry/mod.rs | No `fun()` merging accessor despite documented dual-location drift | 2026-07-10 | src/types/telemetry/mod.rs:52 → `07-10-REVIEW.md` §18 |
| BUG-035 | Sev3 | types/telemetry/tests.rs | `test_ams_unit_info_bitmask` doesn't call the accessors it claims to verify | 2026-07-10 | src/types/telemetry/tests.rs:1416-1449 → `07-10-REVIEW.md` §19 |
| BUG-036 | Sev3 | types/telemetry/mod.rs | `decode_nozzle_temperatures()` has zero test coverage | 2026-07-10 | src/types/telemetry/mod.rs:119 → `07-10-REVIEW.md` §19 |
| BUG-037 | Sev3 | types/telemetry/report.rs | `is_220v_power()` has zero test coverage despite gating a safety ceiling | 2026-07-10 | src/types/telemetry/report.rs:320 → `07-10-REVIEW.md` §19 |
| BUG-038 | Sev3 | tests/common/mock_ftps.rs | `read_cmd`'s single read can't detect a `write_command` framing regression | 2026-07-10 | tests/common/mock_ftps.rs:181-217 → `07-10-REVIEW.md` §20 |

## Fixed

| ID | Sev | Module | Title | Found | Closed | Detail |
|---|---|---|---|---|---|---|
| BUG-001 | Sev2 | mqtt/commands, diagnostics/kprofile | `sequence_id` unclamped in 6 public command constructors | 2026-07-09 | 2026-07-09 | 6 constructors serialized `sequence_id`/`subtask_id` without `clamp_task_id()`, risking the documented 32-bit firmware lockup — fixed in `0ae1e51` |
| BUG-002 | Sev3 | mqtt/client/mod.rs | `get_in_flight_count` doc comment says slice, returns `usize` | 2026-07-09 | 2026-07-09 | mqtt/client/mod.rs:501 — doc comment said "returns a slice," function returns `usize` — corrected in `93e6ca1` |
| BUG-003 | Sev2 | ftps/client.rs | `download_file` has no integrity recheck (unlike `upload_file`'s SIZE recheck) | 2026-07-09 | 2026-07-09 | ftps/client.rs:600-669 `download_file` had no SIZE-recheck symmetric to `upload_file`'s, so a truncated transfer could report success — fixed in `a46281f` |
| BUG-004 | Sev3 | ftps/client.rs | 6 single-reply commands don't poison on `read_response`/`write_command` failure | 2026-07-09 | 2026-07-09 | 6 single-reply methods didn't poison the client on failure — poisoned in `403655f`, matching the transfer-method precedent; user chose poisoning (deterministic) over the cheaper drain-and-continue alternative |
| BUG-005 | Sev3 | camera/rtsps.rs | IPv6 addresses not bracketed in RTSPS URLs | 2026-07-09 | 2026-07-09 | camera/rtsps.rs `build_rtsps_url`/`rewrite_rtsp_request_uri` didn't bracket an IPv6 host — fixed in `db6cb10` |
| BUG-006 | Sev3 | camera/binary.rs | `build_handshake_packet` accepts empty access code | 2026-07-09 | 2026-07-09 | camera/binary.rs `build_handshake_packet`'s alphanumeric check passed vacuously on an empty string — fixed in `5bdc364` |
| BUG-007 | Sev3 | client/connect.rs | `with_connect_timeout(0)` fails instantly instead of disabling the timeout | 2026-07-09 | 2026-07-09 | client/connect.rs `race_against_connect_timeout` treated `connect_timeout_secs == 0` as an instant deadline — fixed in `957d747` |
| BUG-008 | Sev3 | io/tokio.rs | `CnFallbackServerVerifier` ignores intermediate certs (single-hop chain only) | 2026-07-09 | 2026-07-09 | io/tokio.rs:257 `verify_server_cert` ignored `_intermediates`, single-hop chains only — now walks the full chain, fixed in `6316561` |
| BUG-009 | Sev2 | discovery/mod.rs | Degraded-mode SSDP bind is order-dependent, not "try all ports" | 2026-07-09 | 2026-07-09 | discovery/mod.rs:176-190 returned `Err` on the first port's bind failure instead of trying the rest — fixed in `91e25f3` |
| BUG-010 | Sev2 | discovery/mod.rs | Initial broadcast loop aborts whole sweep on one engine's send failure | 2026-07-09 | 2026-07-09 | discovery/mod.rs:203-207 — one engine's send failure aborted the whole sweep — fixed in `478df11` |
| BUG-011 | Sev2 | discovery/parser.rs | SSDP-discovered serial not uppercased despite doc promise | 2026-07-09 | 2026-07-09 | discovery/parser.rs `parse_ssdp_payload` didn't uppercase the discovered serial — fixed in `ab7f8c2` |
| BUG-012 | Sev2 | ams/parser.rs | State-10 tray-clearing: doc said clear, code didn't | 2026-07-09 | 2026-07-09 | ams/parser.rs:112-116 `clean_stale_tray_data` — state 10 alone didn't clear stale tray data — fixed in `66f3f6c`; resolved via pybambu/Bambuddy cross-check (no H2D hardware available), see `reference/05_materials_ams.md`'s verification-source note |
| BUG-013 | Sev3 | error.rs | `no_std` `Display` impl sync unverified by any test that actually runs | 2026-07-09 | 2026-07-09 | error.rs — `test_display_consistency` only runs under `std`, so the `no_std` impl's sync was never test-verified, just manually inspected — doc comment corrected in `f288cd8` (process gap, no `no_std` test harness exists to fix it properly) |

## Wontfix

*(none yet)*
