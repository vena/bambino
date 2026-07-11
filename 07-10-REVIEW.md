# 07-10 Deep Review — full-crate sweep

This is a full-crate deep code review of **bambino**, a multi-platform async Rust crate that controls Bambu Lab 3D printers over LAN (host/tokio, ESP-IDF/std, and bare-metal/embassy/no_std targets from one codebase). The review was run via the `deep-review` skill: the crate's current `src/` and `tests/` structure was discovered fresh (89 files across 20 review units, split by weighted size — large directories like `client/`, `io/`, `ftps/`, `types/telemetry/`, and `bin/bambino-cli/` were each split into 2 units), and one subagent was spawned per unit in parallel, each reading only its assigned files plus `CLAUDE.md`, `README.md`, and every relevant `.claude/rules/*.md`/nested `CLAUDE.md` invariant doc.

**Recovery note:** mid-sweep, 11 of the 20 subagents were killed by a session-usage-limit error before reporting. All 11 were successfully resumed in place (not re-spawned from scratch) and delivered real, evidence-based results — nothing in this file is fabricated or inferred to fill a gap. One unit (tests/common mock infra, §20) ran out of budget before finishing a planned cross-check of `mock_mqtt.rs` against the real MQTT client's frame-reassembly state machine; that gap is noted in its section rather than silently dropped.

## Scope exclusions (by design, not oversight)

- **Security issues are out of scope** except where a stated invariant is implemented incorrectly. This crate is explicitly LAN-only (see README's Safety Notice) — cert-verification bypass, plaintext fallback, etc. are intentional design, not findings.
- **Style/refactor/naming preferences are out of scope**, except a name that actively misrepresents behavior (inverted-sense boolean, a function that does the opposite of what it claims) — that's a correctness footgun in naming's clothing, not a style nit.
- Findings are tagged **CONFIRMED** (the agent is sure it's a real bug) or **PLAUSIBLE** (looks real, but couldn't be fully verified — e.g. the failure path can't be confirmed to trigger, or the invariant it violates is itself ambiguous/reverse-engineered). Only `CONFIRMED` findings get promoted to a `BUG-ID` in `BACKLOG.md`; `PLAUSIBLE` findings stay in this file's own section below for a human to manually triage.

**This file is meant to be read standalone** by a session with no memory of the sweep that produced it. **Caveat:** file:line references may have drifted if other commits landed on `main` after 2026-07-10.

## Modules reviewed with no issues

`src/diagnostics/` (hms.rs, kprofile.rs, mod.rs) and `src/quirks/` (mod.rs + all 7 per-model strategy files) — both reviewed in full against their reference docs and `.claude/rules/*.md`, zero findings of either tier.

---

## 1. ams/ (`src/ams/mapping.rs`, `mod.rs`, `parser.rs`)

### src/ams/parser.rs:60-65
**Issue**: `evaluate_spool_presence` bounds-checks `ams_id` before the bit-shift but not `tray_id`, despite an adjacent comment naming this exact failure mode.
**Detail**: `shift_standard = (ams_id * AMS_SLOTS_PER_UNIT) + tray_id` feeds `parsed_mask >> shift_standard`. A valid `ams_id` (0-3) with `tray_id >= 20` pushes the shift past 31 bits — panics in debug, silently wrong in release. `tray_id` comes straight off the wire (a parsed printer status payload), so a malformed/corrupted packet reaches this unchecked arithmetic directly. No existing test varies `tray_id`.
**Suggested fix**: Add `if tray_id >= AMS_SLOTS_PER_UNIT { return None; }` alongside the existing `ams_id` guard, mirroring `resolve_global_tray_id`'s equivalent check in the same file.

### src/ams/parser.rs:50-54
**Issue**: `evaluate_spool_presence` unconditionally returns `Some(true)` for AMS-HT unit IDs (128-135), never actually evaluating presence.
**Detail**: `reference/05_materials_ams.md` §5.1 says AMS-HT presence must be read from slot-state parameters, not the bitmask this function takes — but the code just hardcodes `true` instead of deferring to that state or returning `None`. An AMS-HT chamber with no spool loaded is reported as "spool present."
**Suggested fix**: Return `None` for the AMS-HT ID range (forcing callers to consult tray `state` instead), or thread the real slot-state signal through.

---

## 3. bin/bambino-cli core

### src/bin/bambino-cli/main.rs:42-53
**Issue**: `--help`'s `after_help` text is missing the `gcode-raw`/`--unsafe` documentation that both README and `control.rs`'s actual behavior have.
**Detail**: README's Usage block (presented as real `--help` output) documents that `gcode-raw` skips model safety checks and normally prompts for confirmation unless `--unsafe` is passed. The flag genuinely exists in `control.rs` — but `main.rs`'s own `after_help` string stops short and never mentions it, so `bambino-cli --help` doesn't warn about it.
**Suggested fix**: Add the missing lines back to `after_help` in `main.rs`, or regenerate README's Usage block from real `--help` output so they can't drift again.

---

## 4. bin/bambino-cli control/probe/monitor

### src/bin/bambino-cli/probe.rs:502 (also 414)
**Issue**: A capture-window error aborts the whole probe run via `?` and discards every previously-captured test result — nothing gets written to the output file.
**Detail**: Several earlier tests in the run may already have performed physical actions (homing, fan/temp changes) before a mid-run capture error propagates out of `run()`. `entries`/`report`/`fs::write` only execute after the loop completes normally, so one error throws away all prior captured firmware-response data.
**Suggested fix**: Convert the capture error into a per-entry error field (as already done for `send_command` failures) and `continue`, or capture the `Result` and still write out `entries` collected so far before returning the error.

---

## 5. client/ core

### src/client/connect.rs:92-114 (`ensure_mqtt`)
**Issue**: MQTT — the one connection every `PrinterClient` requires — has no public recovery path once the stream goes bad; FTPS and camera both got this treatment, MQTT didn't.
**Detail**: `ensure_mqtt()` short-circuits on `self.mqtt.is_some()`, and nothing in `src/client/` ever resets `self.mqtt` to `None`. Once a stream error or `tick_zombie_check()`-detected zombie occurs mid-session, every subsequent call reuses the dead stream and keeps failing, with no `disconnect_mqtt()`/`attach_mqtt()` (unlike `disconnect_camera()`/`attach_camera()` and `disconnect_storage()`/`attach_storage()`, which exist specifically for this).
**Suggested fix**: Add `disconnect_mqtt()` and `attach_mqtt()` mirroring the camera/storage pair, so a caller that detects a dead MQTT session has a supported redial path instead of rebuilding the whole `PrinterClient`.

### src/client/connect.rs:107-109
**Issue**: The wall-clock sequence-counter reseed meant to prevent two independent sessions colliding on sequence IDs is a no-op under the documented, first-class default (`DummyTimer`, used whenever `.with_timer()` isn't chained).
**Detail**: The reseed does `clamp_task_id(self.timer.now_millis())`; `DummyTimer::now_millis()` always returns `0`, so two independent default-configured `PrinterClient`s connecting to the same printer both reseed to `0` and both send their first command with sequence ID `1` — exactly the collision the comment says this code prevents. Unlike `race_against_connect_timeout` in the same file, this reseed has no `timer.has_real_clock()` guard.
**Suggested fix**: Skip the reseed when `!self.timer.has_real_clock()`, matching the existing guard pattern in this file.

### src/client/connect.rs:273-306, 328-361 (`ensure_ftps`, `ensure_camera`)
**Issue**: A *failed* (not just successful) first FTPS/camera connect attempt permanently consumes the config via `.take()`, silently disabling retry.
**Detail**: `.take()` fires before the fallible dial+TLS+handshake sequence runs. Any failure there — including simply hitting `connect_timeout_secs` on a slow LAN — leaves `self.ftps`/`self.camera` at `None` but with the config already gone, so the next call sees `.take()` on an already-empty `Option` and reports "not configured" even though the caller never asked to disable it.
**Suggested fix**: Don't `.take()` until the connect attempt has succeeded, or put the config back on the `Err` path before propagating.

---

## 6. client/ commands

### src/client/telemetry.rs:156
**Issue**: `last_door_open` is overwritten unconditionally on every telemetry message, breaking `TelemetryCache`'s own documented staleness-preservation contract — every sibling field is guarded, this one isn't.
**Detail**: `is_door_open()` derives from `home_flag`/`stat`, both of which `.unwrap_or(false)` when absent. Since `home_flag` is demonstrably sometimes absent (that's why the sibling `last_home_flag` field *is* guarded a few lines above), any print-carrying message that omits it resets `last_door_open` to `Some(false)` ("confirmed closed") even if the door was previously known open. Existing tests only send messages that always include `home_flag`, so this is untested.
**Suggested fix**: Gate the update on the underlying field actually being present in that message, mirroring every other field in `update_state_cache`.

---

## 7. tests/client_test.rs

### tests/client_test.rs:1346-1372
**Issue**: `test_sequence_id_wrapping` never exercises wrapping.
**Detail**: It sends exactly one command from a freshly-constructed client (sequence ID `10001`) and asserts `seq <= i32::MAX`, which is true for any working client and would still pass if `clamp_task_id()`'s wraparound logic were deleted. `sequence_counter` is `pub(crate)`, so this external test has no way to seed it near `TASK_ID_MAX` through the public API.
**Suggested fix**: Rename to reflect what it actually checks, and add real wraparound coverage via a `#[cfg(test)]` seam or a unit test colocated with `clamp_task_id()`.

### tests/client_test.rs:213-291, 543-584
**Issue**: Zero coverage for X1C's mains-voltage-dependent bed-temperature ceiling.
**Detail**: X1C's `bed_temp_max` yields three different ceilings depending on cached `home_flag` telemetry (220V/110V/unknown-conservative), but every bed-clamping test in this file only exercises X1E, which ignores the parameter entirely. A regression that swapped the two constants or the `None` fallback direction would pass every existing test.
**Suggested fix**: Add a test constructing an X1C client, feeding `home_flag` telemetry via `poll_telemetry()`, and asserting both ceilings plus the pre-telemetry conservative default.

---

## 9. discovery/

### src/discovery/mod.rs:113-134 (`poll_next_device`)
**Issue**: Never stamps `discovery_port` on the returned `SsdpDevice` — it stays `0` for any caller driving `DiscoveryEngine` directly.
**Detail**: `parse_ssdp_payload` always initializes `discovery_port: 0`; only the `discover_devices()` convenience wrapper patches it in afterward. `DiscoveryEngine` direct use is exactly the documented, required pattern for Embassy (since `discover_devices()` is std-only) — so every Embassy caller gets `discovery_port == 0` for every device, silently violating the field's own doc comment. Untested (`test_discovery_engine_broadcast_and_poll` never asserts on it).
**Suggested fix**: Set `device.discovery_port = self.port;` inside `poll_next_device` itself.

### src/discovery/mod.rs:8
**Issue**: Module doc claims `discover_devices()` "works across std, ESP-IDF, and Embassy," contradicting both its own `#[cfg(feature = "std")]` gate and README's explicit Embassy caveat.
**Detail**: `DiscoveryEngine` itself does work on all three targets — the doc just attributes that to the wrong item. An embedded developer following this doc hits a compile error it told them wouldn't happen.
**Suggested fix**: Reword to attribute cross-platform support to `DiscoveryEngine`/`AsyncUdpSocket`, note `discover_devices()` is std-only.

---

## 10. core (error.rs, lib.rs, models.rs)

### src/lib.rs:16, 65
**Issue**: Crate-root doc table and Feature Flags table both claim the `embassy` target's TLS backend is `embedded-tls`; it was replaced by `mbedtls-rs`.
**Detail**: `Cargo.toml`'s `embassy` feature depends on `mbedtls-rs` — no `embedded-tls` dependency exists anywhere. `src/io/CLAUDE.md` states the switch explicitly ("breaking, pre-1.0"). This is rendered rustdoc, the primary place a consumer would learn which crypto library secures the bare-metal target.
**Suggested fix**: Change both occurrences to `mbedtls-rs`.

### src/lib.rs:63
**Issue**: Feature Flags table claims the `tokio` feature enables the CLI binary; the CLI actually requires the separate `cli` feature, which `tokio` does not imply.
**Detail**: `cargo build --features tokio -- --bin bambino-cli` fails (`required-features: cli`) — directly contradicting CLAUDE.md's own stated invariant that CLI deps must never be added to the `tokio` feature. No row exists for `cli` at all.
**Suggested fix**: Remove "CLI binary" from the `tokio` row, add a `cli` row.

---

## 11. ftps/ protocol+parser

### src/ftps/protocol.rs:208-260 (`read_response`)
**Issue**: Multi-line FTP reply parsing doesn't follow RFC 959 — intermediate lines without a matching 3-digit-code prefix are silently dropped, and any line matching `\d\d\d ` is accepted as the terminator regardless of whether its code matches the reply's opening code.
**Detail**: RFC 959 §4.2 explicitly warns that intermediate lines can contain arbitrary text (including something that looks like `NNN `) that must not be mistaken for the terminator. This parser does exactly what the RFC warns against — a genuine intermediate line starting with digits+space gets returned early with a truncated response and a wrong status code. Every real call site observed in `client.rs` only sends single-line commands and `STAT` (the RFC's own multi-line example) is never invoked by this crate — so today's blast radius is small, but it's a real spec violation at a wire-parsing boundary.
**Suggested fix**: Track the header code from the reply's first line; only treat a later line as the terminator if its separator is `' '` *and* its code matches. Treat every other line as body text.

---

## 12. ftps/ client + tests

### src/ftps/client.rs:389-400, 558-569, 641-652
**Issue**: `list_directory`/`upload_file`/`download_file` don't poison the client when their *initial* `write_command`/`read_response` (LIST/STOR/RETR) fails — every other control-channel operation in this file does, per BUG-004's precedent.
**Detail**: If the initial write/read for one of these three fails mid-flight, the control channel is left in the same "unknown reply pairing" state BUG-004's poisoning mechanism exists to prevent, but `self.poisoned` stays `false`. A subsequent call on the same client can silently misread a stale/late reply as its own. Untested by the existing suite (`test_ftps_data_channel_failure_poisons_client` only covers failure *after* the 150 reply is read).
**Suggested fix**: Convert these three write/read pairs to the same poison-on-`Err` pattern used elsewhere in the file (worth factoring into a shared helper, since the pattern is now duplicated ~10 times by convention).

### src/ftps/client.rs:598-657
**Issue**: `download_file`'s SIZE-recheck recovery isn't actually symmetric with `upload_file`'s despite the doc comment's explicit claim.
**Detail**: `upload_file` attempts its SIZE-based recovery on both `226` and `426` (covering the documented P2S/X2D TLS 1.3 close race). `download_file` only attempts its SIZE recheck when `code != 226` — wait, actually only proceeds on non-`226`... it treats `426` as an unconditional hard failure with no recovery attempt, discarding an already-fully-received payload on exactly the race the comment says the recheck exists to catch.
**Suggested fix**: Extend `download_file` to also attempt the SIZE recheck on `426`, matching `upload_file`, or correct the doc comment if the asymmetry is intentional.

---

## 14. io/ embedded

### src/io/esp_idf.rs:521-539
**Issue**: `EspIdfTcpStream`'s `Read`/`Write` impls call genuinely blocking `std::net::TcpStream` I/O with no yield point — unlike every other I/O path in this file, no outer timeout can preempt a stuck read/write here.
**Detail**: `EspIdfTcpStream::connect` explicitly flips the socket back to *blocking* mode after dialing, and this stream is used for real FTPS data-channel transfers on models with `uses_plaintext_ftps_data_channel() == true`. The file's own doc comment identifies and fixes this exact hazard for the *dial* phase but not for the connected read/write phase (contrast with `EspTlsStream::read`/`write`, which retry via `retry_on_would_block`/`TLS_POLL_INTERVAL` specifically to stay preemptible). A stalled peer mid-transfer on ESP-IDF (network partition, printer reboot) blocks the task indefinitely — no command-level timeout, no cancellation, can interrupt it.
**Suggested fix**: Apply the same non-blocking-socket + timer-paced retry pattern already used for `EspTlsStream` to `EspIdfTcpStream`'s plaintext `Read`/`Write` impls, or at minimum set `SO_RCVTIMEO`/`SO_SNDTIMEO` on the raw socket.

---

## 15. mqtt/client/

### src/mqtt/client/mod.rs:182-201 (`connect()`)
**Issue**: All non-zero CONNACK codes are reported as `BambuError::AccessDenied`, but the doc comment specifically promises that variant only for an invalid-access-code rejection.
**Detail**: MQTT v3.1.1 CONNACK codes 1-3 (protocol version, identifier rejected, server unavailable) are distinct from 4-5 (bad credentials/not authorized), but the code collapses all non-zero codes into `AccessDenied`. A transient code-3 (broker temporarily over capacity) would misdiagnose as "check your access code." Real-world risk is likely low (Bambu firmware presumably only sends 0 or 5), but the doc/code mismatch is real and untested.
**Suggested fix**: Either narrow the doc comment to "any non-zero code," or map codes 1-3 to a distinct error and reserve `AccessDenied` for 4-5.

---

## 16. mqtt/commands/

### src/mqtt/commands/print_job.rs:88-93, 216-220
**Issue**: `PrintJobConfig::with_ams_mapping2()` populates `ams_mapping2` but leaves `ams_mapping` untouched, so a wire payload can go out with a non-empty `ams_mapping2` next to an empty `ams_mapping` — breaking the documented 1-to-1 index pairing between the two arrays.
**Detail**: `reference/05_materials_ams.md:148` states the two arrays must stay parallel; `src/ams/mapping.rs`'s builders are always called together, confirming this is a firmware wire contract. The crate's own test `test_ams_mapping2_sets_use_ams_true` exercises exactly the triggering call pattern (`with_ams_mapping2()` alone) but only asserts `use_ams: true`, never inspecting `ams_mapping`'s contents — so the gap wasn't caught. No current caller in the crate uses `with_ams_mapping2()` outside tests, but any consumer following the builder's own doc comment would trip the documented `0700_8012`-style firmware mapping error.
**Suggested fix**: Either require/derive a matching flat `ams_mapping` when `with_ams_mapping2()` is used, or build `ams_mapping` from `ams_mapping2`'s length in `from_config` when it's the active source.

---

## 18. types/telemetry model

### src/types/telemetry/mod.rs:52
**Issue**: `TelemetryReport.fun` documents the identical dual-wire-location drift problem `TelemetryReport::device()` was built to solve, but has no equivalent merging accessor.
**Detail**: The doc comment says `fun` "drifts between top-level and `print.fun` depending on firmware version" — structurally identical to `DeviceTelemetry`'s problem, which got `device()` (checks top-level first, falls back to `print.device`, and CLAUDE.md tells callers to prefer it). `fun` has no such method; `is_developer_mode()` takes a plain string with no idea which location it came from. Not currently consumed anywhere in `src/client/` or `src/bin/` (only exercised by tests), so today's blast radius is zero, but any future caller reading `report.fun` naively will silently miss firmware that only populates `print.fun`.
**Suggested fix**: Add a `TelemetryReport::fun()` accessor mirroring `device()`'s fallback order.

---

## 19. types/telemetry/tests.rs

### src/types/telemetry/tests.rs:1416-1449
**Issue**: `test_ams_unit_info_bitmask` doesn't call the `AmsUnit` accessor methods it claims to verify — it hand-rolls the same bit math in the test body instead of calling `unit.ams_type()`/`unit.extruder_assignment()`.
**Detail**: A regression in the real accessors' shift/mask constants would not be caught by this test (though later Phase-21 tests do call the real methods and would catch it, so coverage isn't actually lost — this specific test just doesn't test what its name claims).
**Suggested fix**: Replace the manual recomputation with the real accessor calls, or delete as superseded.

### src/types/telemetry/mod.rs:119 (`decode_nozzle_temperatures`, no coverage in tests.rs)
**Issue**: Zero test coverage for a documented, non-obvious wire quirk — for IDEX models with no live `ExtruderInfo`, the flat `nozzle_temper`/`nozzle_target_temper` fields belong to *different* nozzles (a routing swap the function's own doc comment calls out).
**Detail**: None of the three branches (composite path, single-nozzle fallback, IDEX swapped-fallback) is exercised anywhere in this file, despite every comparable cross-model accessor (`is_ethernet_active`, `bed_temperatures()`, `device()`) having dedicated tests.
**Suggested fix**: Add tests for all three branches, specifically asserting the IDEX field-swap.

### src/types/telemetry/report.rs:320 (`is_220v_power`, no coverage in tests.rs)
**Issue**: Zero test coverage for the bit-3 `home_flag` heuristic that gates X1C's bed-temperature safety ceiling (110°C @220V vs 120°C @110V).
**Detail**: Sibling bit-accessors on the same struct each have 2-3 dedicated tests; this one — despite being explicitly documented as "confirmed, not disputed" and directly feeding a real thermal ceiling — has none. A regression in the bitmask constant would silently propagate into a wrong bed-temp ceiling undetected.
**Suggested fix**: Add a test analogous to `test_door_open_from_home_flag` (set/clear/missing `home_flag`).

---

## 20. tests/common mock infra

**Scope caveat**: this unit ran out of budget before cross-checking `mock_mqtt.rs` against `src/mqtt/client/frame.rs`'s real resumable-read state machine — its "no MQTT fidelity issues" conclusion is based on reading `mock_mqtt.rs` alone, not verified against the real client.

### tests/common/mock_ftps.rs:181-217 (`read_cmd`, used by every mock server function)
**Issue**: Every FTP command is captured via a single non-looping `stream.read(buf)`, not a read-to-completion loop — so this shared harness structurally cannot detect a regression in `write_command`'s single-write-call guarantee.
**Detail**: `write_command`'s single-`write_all` behavior exists because splitting it into two writes previously broke against real embedded vsFTPd firmware (a dedicated unit test in `protocol.rs` guards this via a `WriteRecorder`). Under tokio's cooperative scheduling, two sequential small writes normally coalesce into one `.read()` before the reader task is ever polled — so if `write_command` regressed back to two writes, every FTPS integration test built on this harness would very likely keep passing, silently. The `protocol.rs` unit test is the only thing actually guarding this invariant end-to-end.
**Suggested fix**: Either document that `read_cmd` isn't a safety net for this (the unit test is), or make it assert it received the full expected line and loop otherwise.

---

## Plausible, Unverified Findings

Findings below look real but couldn't be fully verified by the reviewing agent — no `BUG-ID` assigned yet; flagged here for manual triage.



### Unit 3 — bin/bambino-cli core
**src/bin/bambino-cli/storage.rs:40-49** — `current_date_utc()` feeds `list_directory`'s year-rollover disambiguation with UTC time; if the printer's `LIST` mtimes are actually in local (non-UTC) time, a timezone offset near a year boundary could flip a recently-modified file's inferred year. Reference doc doesn't specify which convention the printer uses.

### Unit 4 — bin/bambino-cli control/probe/monitor
**src/bin/bambino-cli/monitor/mod.rs:52-58** — `TerminalGuard::enter()` enables raw mode before constructing the guard; if the subsequent alt-screen/cursor-hide write fails, `Drop` never runs and the terminal is left stuck in raw mode.
**src/bin/bambino-cli/control.rs:203-238** — `ControlAction::Move`'s `axis` is a free-form `String` (only length-checked), unlike every other CLI arg in this file which uses a `ValueEnum`. A stray single-byte value could be spliced into raw G-code sent via `send_gcode_raw` with none of `gcode-raw`'s interactive confirmation.

### Unit 7 — tests/client_test.rs
**tests/client_test.rs:629-664** — `test_in_flight_saturation` only asserts `is_err()`, not the specific `BambuError::NetworkError(SocketError::TimedOut)` variant the saturation path is documented to return — weaker than the sibling `test_connection_drop_during_operation`, which does assert the specific variant.

### Unit 9 — discovery/
**src/discovery/mod.rs:249-256** — The `discover_devices()` poll loop silently discards `Err` from `poll_next_device` with no logging and no backoff; a persistently-erroring socket (e.g. an ICMP-port-unreachable-induced synchronous error, a known cross-platform UDP quirk) could spin for the rest of the discovery window with zero operator-visible signal.
**src/discovery/parser.rs:196** — A present-but-empty `DevModel.bambu.com` header (`Some("")`) is treated as authoritative and skips the NT/ST fallback that exists to catch exactly this class of missing-model-hint case. No test covers an empty (vs. absent) header.

### Unit 13 — io/ host
**src/io/tokio.rs:298-320** — `CnFallbackServerVerifier`'s chain-walk finds the *first* unused intermediate matching the issuer's subject name; if that specific candidate fails signature verification, the walk `break`s instead of trying other unused intermediates with the same subject name. Same shape as the already-fixed BUG-008, narrower trigger (duplicate-subject-name chains). Fail-closed only — an attacker cannot exploit this to get a bad cert accepted, only a valid chain spuriously rejected.

### Unit 14 — io/ embedded
**src/io/embassy.rs:70, 82** — `EmbassyUdpSocket::send_to`/`recv_from` collapse every underlying error into `SocketError::ConnectionReset`, discarding the actual failure mode — same collapsing-error-at-an-FFI-boundary pattern this crate has previously fixed elsewhere (`map_esp_tls_connect_error`).
**src/io/embassy.rs:292-295** — `EmbassyRawStreamFactory::dial` collapses every `TcpClient::connect` failure (including pool exhaustion) into `SocketError::ConnectionRefused`, potentially misrouting retry/backoff decisions that key off `TimedOut` vs `ConnectionRefused`.
**src/io/esp_idf.rs:491, 648** — A fresh `EspIdfTimer` (and its own `EspTimerService`) is allocated per dial/handshake phase rather than shared across a connect sequence; could contend for ESP-IDF's `esp_timer` slot cap under connection-heavy workloads (e.g. FTPS opening a fresh data channel per transfer). Unverified without real hardware.

### Unit 15 — mqtt/client/
**src/mqtt/client/pending.rs:38-58** — `push_pending`'s eviction loop can't enforce `MQTT_PENDING_BUFFER_MAX_BYTES` against a single message that already exceeds the cap on its own — currently unreachable since `MQTT_MAX_PAYLOAD_BYTES` (1 MiB) is smaller than the pending cap (2 MiB), but that relationship is enforced only by convention across two separate constants in two files, with no static assertion linking them.
**src/mqtt/client/mod.rs:374-433** — QoS 2 (and reserved QoS 3) PUBLISH frames are parsed like QoS 1 but never acknowledged (no PUBREC handshake) — low risk since the client only ever subscribes at QoS 1, but an unhandled protocol case with no error surfaced.

### Unit 16 — mqtt/commands/
**src/mqtt/commands/mod.rs:47-49** — Task-ID clamping is enforced only by convention (every constructor manually calling `clamp_task_id`), not by the type system or a shared code path — the exact shape that produced BUG-001. All 22 current call sites clamp correctly, but nothing prevents a future 23rd constructor from skipping it undetected by the existing regression test (which only covers 2 of the 22 constructors).

### Unit 18 — types/telemetry model
**src/types/telemetry/device.rs:45** — `DeviceTelemetry.bed_temp: Option<u32>` is parsed off the wire but never consulted by any decode path (`decode_bed_temperatures()` only reads `device.bed.info.temp`). A fixture shows both fields carrying the identical value in one payload, suggesting it may be an intentional fallback source that was never wired up — or intentionally-unused reserved data. Reference doc doesn't mention this field either way.

### Unit 20 — tests/common mock infra
**tests/common/mock_ftps.rs:243-263** — Upload capture in `run_mock_server` uses a single non-looping read into a fixed 100-byte buffer; every test payload built on this harness is far under one 64KB upload chunk, so `upload_file`'s multi-chunk loop is never exercised past one iteration by any test using it.
**tests/common/mock_camera.rs** — Only implements the happy path (valid handshake, well-formed frames); no variant simulates auth rejection, a malformed frame header, or a mid-frame connection drop, so no *integration-level* regression coverage exists for those documented failure modes (though `src/camera/binary.rs`'s own unit tests do cover some of this directly via raw duplex streams).

---

## Summary table

`BACKLOG.md` is the status source of truth from here on — this table is a point-in-time snapshot from the 2026-07-10 sweep and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | File(s) | One-line |
|---|---|---|---|---|
| BUG-014 | Sev2 | ams/parser.rs | src/ams/parser.rs:60-65 | `evaluate_spool_presence` doesn't bounds-check `tray_id` before a bit-shift |
| BUG-015 | Sev2 | ams/parser.rs | src/ams/parser.rs:50-54 | AMS-HT slots always report `Some(true)` presence regardless of actual state |
| BUG-016 | Sev3 | bin/bambino-cli/main.rs | src/bin/bambino-cli/main.rs:42-53 | `--help` missing `gcode-raw --unsafe` documentation |
| BUG-017 | Sev3 | bin/bambino-cli/probe.rs | src/bin/bambino-cli/probe.rs:502,414 | Capture-window error discards all previously-captured probe results |
| BUG-018 | Sev2 | client/connect.rs | src/client/connect.rs:92-114 | No `disconnect_mqtt()`/`attach_mqtt()` — dead MQTT session has no recovery path |
| BUG-019 | Sev2 | client/connect.rs | src/client/connect.rs:107-109 | Sequence-counter reseed is a no-op under the default `DummyTimer` |
| BUG-020 | Sev2 | client/connect.rs | src/client/connect.rs:273-306,328-361 | `ensure_ftps`/`ensure_camera` consume config even on a failed connect attempt |
| BUG-021 | Sev2 | client/telemetry.rs | src/client/telemetry.rs:156 | `last_door_open` cache overwritten unconditionally, ignoring absent-field staleness contract |
| BUG-022 | Sev3 | tests/client_test.rs | tests/client_test.rs:1346-1372 | `test_sequence_id_wrapping` never exercises wraparound |
| BUG-023 | Sev3 | tests/client_test.rs | tests/client_test.rs:213-291,543-584 | No test coverage for X1C's voltage-dependent bed-temp ceiling |
| BUG-024 | Sev2 | discovery/mod.rs | src/discovery/mod.rs:113-134 | `poll_next_device` never stamps `discovery_port` |
| BUG-025 | Sev3 | discovery/mod.rs | src/discovery/mod.rs:8 | Module doc wrongly claims `discover_devices()` works on Embassy |
| BUG-026 | Sev3 | lib.rs | src/lib.rs:16,65 | Docs claim embassy TLS backend is `embedded-tls`, actually `mbedtls-rs` |
| BUG-027 | Sev3 | lib.rs | src/lib.rs:63 | Feature Flags table wrongly claims `tokio` enables the CLI binary |
| BUG-028 | Sev3 | ftps/protocol.rs | src/ftps/protocol.rs:208-260 | `read_response` doesn't follow RFC 959 multi-line reply parsing |
| BUG-029 | Sev3 | ftps/client.rs | src/ftps/client.rs:389-400,558-569,641-652 | LIST/STOR/RETR initial write/read not poisoned on failure |
| BUG-030 | Sev3 | ftps/client.rs | src/ftps/client.rs:598-657 | `download_file`'s 426→SIZE recheck isn't symmetric with `upload_file`'s |
| BUG-031 | Sev2 | io/esp_idf.rs | src/io/esp_idf.rs:521-539 | `EspIdfTcpStream` Read/Write block with no preempt point |
| BUG-032 | Sev3 | mqtt/client/mod.rs | src/mqtt/client/mod.rs:182-201 | CONNACK codes 1-3 collapsed into `AccessDenied` along with 4-5 |
| BUG-033 | Sev2 | mqtt/commands/print_job.rs | src/mqtt/commands/print_job.rs:88-93,216-220 | `with_ams_mapping2()` doesn't sync `ams_mapping`, breaking documented 1:1 pairing |
| BUG-034 | Sev3 | types/telemetry/mod.rs | src/types/telemetry/mod.rs:52 | No `fun()` merging accessor despite documented dual-location drift |
| BUG-035 | Sev3 | types/telemetry/tests.rs | src/types/telemetry/tests.rs:1416-1449 | `test_ams_unit_info_bitmask` doesn't call the accessors it claims to verify |
| BUG-036 | Sev3 | types/telemetry/mod.rs | src/types/telemetry/mod.rs:119 | `decode_nozzle_temperatures()` has zero test coverage |
| BUG-037 | Sev3 | types/telemetry/report.rs | src/types/telemetry/report.rs:320 | `is_220v_power()` has zero test coverage despite gating a safety ceiling |
| BUG-038 | Sev3 | tests/common/mock_ftps.rs | tests/common/mock_ftps.rs:181-217 | `read_cmd`'s single read can't detect a `write_command` framing regression |
