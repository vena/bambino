# bambino Deep Review — 08-19

**Status:** COMPLETE (20/20 units reviewed, all findings triaged and independently verified)

> **Verification note (2026-08-19).** This sweep ran while lean-ctx 3.9.19 was silently deleting lines from file reads (no elision markers; only a trailing `[lean-ctx: N lines filtered by triage]` counter). Agents that hit it fell back to native reads, but the defect made some intermediate evidence unreliable. After reverting to lean-ctx 3.9.18 — which does not exhibit the bug — **every `P-critical`, `P-high`, and `P-low` finding below was re-verified by a direct, unfiltered read of the cited lines.** All held. One finding's mechanism was corrected during that pass (`src/client/ams.rs:99` — see Correction 2). Two `needs-verification` items remain blocked on hardware and two on an upstream wire capture; those are marked as such and are not claimed as verified.

> **Sweep interrupted 2026-08-19 by an API session limit and resumed.** Nine units landed on the first pass; the eleven killed mid-run were re-reviewed on a second pass. All 20 units are complete and every section below is final. The interruption affected scheduling only, not coverage.

## What this is

A full-crate correctness review sweep of **bambino**, an async Rust crate for controlling Bambu Lab 3D printers over LAN (MQTT, FTPS, camera, SSDP discovery), compiling to three targets from one codebase: host (tokio/rustls), ESP-IDF (std), and bare-metal (embassy/no_std).

Methodology: the `src/` and `tests/` trees were walked fresh, partitioned into 20 review units by directory boundary and file weight, and one review agent was spawned per unit in parallel. Each agent received the unit's file list, the relevant `README.md` architecture excerpt, and any `.claude/rules/*.md` or nested `CLAUDE.md` invariants pre-matched to its files.

**This file is meant to be read standalone by a fresh session with none of the sweep's conversation context.** File paths and line numbers are exact as of the sweep. If other changes have landed on `main` since, line numbers may have drifted — verify the surrounding code before acting on a reference.

## Scope exclusions (deliberate, not oversights)

- **Minor security issues are out of scope.** This crate is explicitly LAN-only by design (no Bambu Cloud). Cert-verification bypass, plaintext fallback, and similar are intentional design points — only flagged where implemented incorrectly against the crate's *own* stated behavior.
- **Style, refactor, and naming preferences are out of scope** — except where a name actively misrepresents behavior (that's a footgun, not a style nit).
- **Hypothetical internal-invariant validation is out of scope** — findings need a concrete failure scenario, not a theoretical one.

## CONFIRMED vs PLAUSIBLE

Findings are tagged by the reviewing agent's confidence. **CONFIRMED** means the bug is real and the failure path is verified. **PLAUSIBLE** means it looks real but couldn't be fully verified — e.g. the failure path may not actually trigger, or the invariant it violates is itself ambiguous. PLAUSIBLE findings are re-verified by direct code read before this sweep is marked COMPLETE, then either promoted into their unit's section or annotated inline as triaged-not-a-bug. Only CONFIRMED findings carry a priority label.

---

## 1. src/{lib,error,models,identity}.rs (core) — COMPLETE

NO CONFIRMED ISSUES FOUND. Three PLAUSIBLE findings — see the Plausible section.

Verified clean: all 13 `resolve_model()` serial prefixes agree with both `MODEL_MATRIX.csv` and `reference/01_network_discovery.md` §1.5; `DEV_MODEL_TOKENS` covers every code in §1.4 with no duplicates; `models.rs` contains no `BambuModel` variant dispatch (Key Invariant #2 holds — dispatch lives at `src/quirks/mod.rs:225`); `test_display_consistency` passes.

## 2. src/discovery/ — COMPLETE

### src/discovery/parser.rs:226 — `P-high` — filed as #69
**Issue:** The `DevModel` → NT/ST model fallback fires only when `DevModel.bambu.com` is absent or empty, but `reference/01_network_discovery.md` Protocol Violation #7 requires it when the header is "missing **or malformed**."
**Detail:** `effective_dev_model = raw.dev_model.filter(|s| !s.is_empty()).or_else(...)`. A packet carrying an unrecognized `DevModel` token plus a resolvable `NT: urn:bambulab-com:device:P1S:1` keeps the junk token, so `resolve_model()` returns `PrinterModel::Unknown` when the serial prefix is also unrecognized. The device is still accepted (line 240-243), so the caller gets a live printer bound to `Unknown` — and therefore the wrong quirks profile. This chains into the `Unknown`→`X1CQuirks` fallback flagged in unit 4: an entry-level machine can inherit X1C's permissive limits. `test_empty_dev_model_header_does_not_block_nt_st_fallback` (line 395) covers only the empty case.
**Suggested fix:** Resolve with `DevModel` first; if the result is `Unknown`, retry with `extract_model_from_nt_st(raw.nt_or_st)` — make the fallback conditional on resolution failure, not on header absence.

### src/discovery/parser.rs:256 — `P-low` — filed as #79
**Issue:** `SsdpDevice::discovery_port`'s doc comment promises a value the public `parse_ssdp_payload` never produces.
**Detail:** parser.rs:29-30 documents the field as "SSDP port on which the device was discovered (2021 or 1990)", but `parse_ssdp_payload` — publicly re-exported at mod.rs:21 — hardcodes `discovery_port: 0`. Only `DiscoveryEngine::poll_next_device` (mod.rs:137) stamps the real port. A caller feeding captured datagrams to `parse_ssdp_payload` directly gets `0`, which is neither documented value.
**Suggested fix:** Document that the field is `0` until stamped by `poll_next_device`, or take the port as a `parse_ssdp_payload` argument.

Five further PLAUSIBLE findings — see the Plausible section.

Verified clean: no panics on adversarial input (`buf[..5]` length-guarded at line 189, all header access `Option`-based, truncated/non-UTF-8/non-Bambu packets rejected without unwinding); `.claude/rules/udp-socket-binding.md` satisfied (`discover_devices` bounds on `BindableUdpSocket` at mod.rs:186); Key Invariants #1 and #2 hold; Protocol Violations #1, #3, #4, #5, #6 correctly implemented and test-covered.

## 3. src/ams/ — COMPLETE

### src/ams/parser.rs:132 — `P-high` — filed as #70
**Issue:** `clean_stale_tray_data` clears 25 of `AmsTray`'s fields but misses `remain_g` and `filament_setting_id`, so stale material data survives the cleanse.
**Detail:** A 500 g PLA spool in AMS 0 slot 0 is removed; the printer pushes `{id: 0, state: 9}`. `PrinterClient::sanitized_ams()` (`src/client/telemetry.rs:423`) calls `clean_stale_tray_data`, which nulls `tray_type`/`tray_color`/`remain` but leaves `remain_g` holding the old spool's grams and `filament_setting_id` holding its profile id. `AmsTray::filament_remain_weight()` (`src/types/telemetry/ams.rs:741-744`) then reports positive remaining weight for an empty slot.
**Root-cause pattern:** the clear list is a hand-maintained field-by-field enumeration with no compile-time link to `AmsTray`. Every new field must be remembered here by convention; nothing fails to compile when it's missed. Related to closed issue #27 (same function, different gap).
**Suggested fix:** Add the two missing clears, and replace the enumeration with a destructuring reset (`*tray = AmsTray { id: core::mem::take(&mut tray.id), state: ..., remain: Some(-1), ..Default::default() }`) so a future field addition is a compile error.

### src/ams/parser.rs:25 — `P-low` — filed as #80
**Issue:** `AMS_EXTERNAL_SPOOL_ID` / `AMS_EXTERNAL_SPOOL_ALT_ID` have their primary/alternate roles inverted relative to the protocol — the names misrepresent behavior.
**Detail:** `AMS_EXTERNAL_SPOOL_ID = 254`, `AMS_EXTERNAL_SPOOL_ALT_ID = 255`. Per `reference/05_materials_ams.md:165-166` and `:200`, **255** is `VIRTUAL_TRAY_MAIN_ID` (single-nozzle external spool, IDEX right/primary) and **254** is the IDEX left/deputy. So the constant named "ALT" is the main id. This is why `MaterialSource::ExternalSpool` (mapping.rs:108) and `Unmapped` (mapping.rs:120) both read as `..._ALT_ID`, and it is the standing trap behind the repeated 254/255 confusion (closed issues #56, #42, #50).
**Suggested fix:** Rename to `AMS_EXTERNAL_SPOOL_MAIN_ID = 255` / `AMS_EXTERNAL_SPOOL_DEPUTY_ID = 254`.

Four further PLAUSIBLE findings — see the Plausible section.

Minor doc drift (not staged separately): parser.rs:18 and parser.rs:43 cite different `DevFilaSystem.cpp` line numbers for the same function; `reference/05_materials_ams.md:16` names the validator `validate_ams_pool_composition()` while the real export is `is_ams_pool_composition_valid` (mapping.rs:303).

## 4. src/quirks/ — COMPLETE

NO CONFIRMED ISSUES FOUND. Two PLAUSIBLE findings — see the Plausible section. Note both are **regressions of closed issues**: the `Unknown`→`X1CQuirks` fallback finding re-raises closed issue **#54**, and the `line_has_unsafe_homing` finding is the mirror-image case of closed issue **#55** (that fix handled a comment *after* `G28`; this is `G28` appearing *inside* a comment).

Verified clean against `MODEL_MATRIX.csv` and `reference/04_toolhead_thermal_motion.md` / `02_ftps.md` §2.1 / `03_mqtt_telemetry.md:70` / `05_materials_ams.md:12-16` / `06_cameras.md:10` — **all matched**: nozzle/bed/chamber temperatures for all 13 models; the X1C voltage inversion (`Some(true)→110`, `Some(false)→120`, `None→110`) with X1E correctly opting out; `camera_protocol()`'s RTSPS-vs-BinaryJpeg split; aux-left/chamber-exhaust/airduct fan capabilities; door sensors and their `home_flag`-vs-`stat` field split; nozzle counts, AMS pools, chamber-sensor flags, prompt sound, buzzer, the `stg_cur` idle bug scope, and FTPS TLS 1.2 enforcement. The `quirks()` dispatch match (mod.rs:226-248) is exhaustive and every variant maps to the correct strategy struct.

**Maintenance note (not a bug):** `X1CQuirks`/`X1EQuirks` (x1.rs:115-121), `P1PQuirks`/`P1SQuirks` (p1.rs:84-90), `P2Quirks` (p2.rs:106-112) and `A1Quirks`/`A1MiniQuirks` (a1.rs:88-94) all return the `*_Z_MAX` constant from `x_max()` and `y_max()`. Every value is currently correct because those models have cubic build volumes, but the correctness is coincidental — a future non-cubic revision would inherit a wrong X/Y bound silently. `A2LQuirks`, `X2Quirks`, and the H2 family already use distinct per-axis constants.

## 5. src/io/ (core + tokio) — COMPLETE

### src/io/tokio/cert_verify.rs:194 — `P-high` — filed as #71
**Issue:** The chain-of-trust walk never checks `basicConstraints` (CA:TRUE), `pathLenConstraint`, or `keyUsage`/`keyCertSign` on the certs it accepts as issuers — so any end-entity cert can act as a CA.
**Detail:** The loop at lines 194–233 accepts a hop on exactly two conditions: subject/issuer name equality and `verify_signature`. That is the classic missing-basicConstraints hole (CVE-2002-0862 class). In the two-level-CA deployment this verifier explicitly supports and tests (`test_cn_fallback_verifier_accepts_leaf_via_intermediate`, tests.rs:252), anyone holding any leaf legitimately issued by the user's trusted CA can mint a sub-certificate with `CN=<printer A's serial>`, present `[forged_leaf, printerB_leaf]`, and have the walk verify forged←printerB←root and set `chain_trusted = true`. The file's own doc (lines 93–100) advertises real chain-of-trust and `.claude/rules/tls-identity-sni.md` calls it an "independent chain-of-trust check" — so this is a correctness bug against the crate's own stated behavior, not a LAN-only hardening gap.
**Suggested fix:** Require issuer certs to carry `basicConstraints` with `ca == true` and (if present) `keyUsage.key_cert_sign`; enforce `pathLenConstraint` against hops walked. Keep the v1 pinning case exempt: real Bambu leaf certs carry no extensions, and a root supplied directly by the caller is trusted by fiat — the constraint belongs on certs pulled from the peer-supplied `intermediates` list.

### src/io/tokio/cert_verify.rs:254 — `P-high` — filed as #72
**Issue:** `verify_tls13_signature` omits rustls's `SignatureScheme::supported_in_tls13()` gate, so TLS 1.3 handshakes accept schemes forbidden in TLS 1.3 (RSA PKCS#1, SHA-1).
**Detail:** The doc comment at lines 271–276 claims this matches the `rustls-webpki` free functions it replaces. It does not: `rustls-0.23.43/src/webpki/verify.rs:194-196` opens with that gate, and `rustls/src/enums.rs:663-666` marks all `RSA_PKCS1_*` schemes unsupported in TLS 1.3. A downgrading peer can sign CertificateVerify with `RSA_PKCS1_SHA1` and be accepted, contrary to RFC 8446 §4.2.3. `supported_verify_schemes` (line 263) advertises the whole ring mapping including PKCS#1, which is what lets the peer pick one.
**Suggested fix:** Add the `supported_in_tls13()` equivalent to the `try_all == false` path, returning `PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme`.

### src/io/mod.rs:392 — `P-low` — filed as #81
**Issue:** `read_chunk` discards the `Result` from `timer.sleep()` and reports `SocketError::TimedOut` even when the sleep failed instantly with no time elapsed.
**Detail:** `Raced::Right(_) => Err(SocketError::TimedOut)` swallows `Err(TimerError)`. `src/io/esp_idf.rs:62` returns exactly that error *before* awaiting anything when `new_async_timer()` fails. Under `esp_timer` slot exhaustion, `sleep_fut` is instantly ready, so `race` resolves `Right` on the first poll of essentially every read — every MQTT/camera read reports `TimedOut` with zero wall-clock elapsed, driving the reconnect loop at full speed. This is the failure mode `has_real_clock()` was introduced to prevent, reappearing through a door `has_real_clock()` cannot detect (a real `EspIdfTimer` correctly reports `true`). Related to closed issue #18.
**Suggested fix:** Match `Raced::Right(Err(e))` separately and surface it as a non-timeout error; only `Raced::Right(Ok(()))` should mean `TimedOut`.

### src/io/mod.rs:338 — `P-low` — filed as #82
**Issue:** `read_chunk`'s entire rationale doc block is welded onto `map_embedded_io_error_kind` — no blank line at 337/338 — leaving `read_chunk` (line 360) undocumented.
**Detail:** Lines 317–345 are one contiguous `///` run, so rustdoc attaches all of it to `map_embedded_io_error_kind` at line 346, whose rendered summary becomes "Reads up to `buf.len()` bytes via a single underlying `read()` call…". Doubly bad given the global rule that the first `///` line is taken verbatim as the item summary by `cargo-doc-md`. The load-bearing "why a single `read()` step" cancellation-safety argument — the `.claude/rules/wire-read-deadline.md` correctness hinge — is filed under the wrong symbol.
**Suggested fix:** Move lines 317–337 to immediately above `pub(crate) async fn read_chunk`.

### src/io/tokio.rs:105 — `P-low` — filed as #83
**Issue:** `.claude/rules/tls-identity-sni.md` names `bambino::io::tokio::CnFallbackServerVerifier`, but the type is not publicly reachable at that path or any other.
**Detail:** Line 104 is `mod cert_verify;` (private) and line 105 is `use` (not `pub use`), so both `CnFallbackServerVerifier` and `NoCertificateVerification` are crate-private. A consumer following the rule doc, or building a `rustls::ClientConfig` by hand rather than through `build_verified_client_config_with_options`, cannot name the type. Root-cause pattern: the rule file documents an intended public surface nothing gates against drift.
**Suggested fix:** Either `pub use cert_verify::CnFallbackServerVerifier;`, or correct the rule file to describe it as internal.

### src/io/tokio/cert_verify.rs:7 — `P-low` — filed as #84
**Issue:** `NoCertificateVerification`'s doc says it "bypasses standard CA chain authority validation" — it in fact bypasses *all* verification including the handshake signature.
**Detail:** `verify_tls12_signature`/`verify_tls13_signature` (lines 30–46) both return `HandshakeSignatureValid::assertion()` unconditionally, so the peer never proves possession of the private key matching the presented cert. The sibling verifier's own doc (lines 101–106) calls the handshake-signature check "what actually prevents MITM here, not the chain check alone." Filed as doc-contradicts-code, not as a complaint about the unsafe path existing.
**Suggested fix:** State plainly that it disables certificate *and* handshake-signature verification.

Three further PLAUSIBLE findings — see the Plausible section.

## 6. src/io/ (esp_idf + embassy) — COMPLETE

### src/io/esp_idf.rs:815 — `P-low` — filed as #85
**Issue:** `EspIdfTlsConnector::with_certs` is documented as accepting PEM, but the implementation only ever supports DER — PEM input silently fails the handshake.
**Detail:** The doc names its parameters `ca_cert_pem`/`cert_pem`/`key_pem` and says "PEM or DER-encoded CA certificate bytes", but `build_tls_config` (esp_idf.rs:410, 416-417) always constructs `X509::der(...)`. In `esp-idf-svc-0.52.1/src/tls.rs:93`, `X509::der` stores the slice with `len == bytes.len()`, whereas `X509::pem`/`pem_until_nul` deliberately include the trailing NUL — and `mbedtls_x509_crt_parse` only takes its PEM branch when `buf[buflen-1] == '\0'`. PEM handed in via `der` falls through to `mbedtls_x509_crt_parse_der` and fails with `MBEDTLS_ERR_X509_INVALID_FORMAT`, surfacing as an opaque `SocketError::Other` with no hint that encoding was the problem. The crate's actual convention is DER-only (tokio's verified config takes `CertificateDer`), so the code is right and the doc is wrong.
**Suggested fix:** Correct the doc to say DER-only and rename the doc's parameter references to match the real names.

### src/io/esp_idf.rs:780 — `P-low` — filed as #86
**Issue:** `EspIdfTlsConnector`'s doc comment makes two now-false claims about the Embassy backend, understating a gap that exists on both platforms.
**Detail:** Lines 780-781 read: "`io/tokio.rs` (`tokio-rustls`) and `io/embassy.rs` (`embedded-tls`) have no equivalent gap; both expose a genuine max-protocol-version knob." (a) Embassy's backend is now `mbedtls-rs`, not `embedded-tls` (`Cargo.toml:116`, `src/io/embassy.rs:180`, recorded in `src/io/CLAUDE.md`). (b) `EmbassyTlsConnector::connect` sets only `min_version` (embassy.rs:195), no max, and `negotiated_version` returns `None` unconditionally (embassy.rs:234-236). Since `src/ftps/client.rs:419` tests `negotiated_version(stream) != Some(TlsVersion::Tls12)`, Embassy fails that check closed for every `enforces_ftps_tls_1_2()` model — exactly the gap this comment says Embassy doesn't have. A future session reading it while deciding where to fix P2S/X2D FTPS is pointed at the wrong platform.
**Suggested fix:** Rewrite to state that only `io/tokio.rs` exposes a max-version knob, that Embassy has the same gap for a different reason, and that both rely on `with_ftps_allow_unverified_tls_1_2(true)`.

Two further PLAUSIBLE findings — see the Plausible section (one tagged `needs-verification`).

Resource-safety checks that came back **clean** (recorded so a future session doesn't re-derive them): `EspTls::adopt` does not call `Socket::release()`, but `EspTls`'s `Drop` calls `release()` before `esp_tls_conn_destroy` (`esp-idf-svc-0.52.1/src/tls.rs:786-798`), so `EspIdfTcpStream::release`'s `into_raw_fd()` hands the fd off exactly once on every error path in `EspIdfTlsConnector::connect` — no double-close, no fd leak. Both `unsafe` FFI blocks in `query_negotiated_tls_version` (esp_idf.rs:530, 537, 542) null-check before dereferencing, and the `poll()` block (esp_idf.rs:193) is sound. `EspIdfTimer`'s take/restore race is correct and hardware-verified (closed issue #65).

## 7. src/mqtt/client/ — COMPLETE

### src/mqtt/client/frame.rs:137 — `P-high` — filed as #73
**Issue:** The oversized-payload and malformed-varint guards reset `FrameReadState` to `Idle` and return `InvalidInput`, leaving the TCP stream permanently desynced with nothing preventing the caller from continuing to poll it.
**Detail:** At lines 137-141 (`rem_len > MQTT_MAX_PAYLOAD_BYTES`) and 128-131 (`*multiplier > 128*128*128`), the header and remaining-length bytes have already been consumed but the `rem_len` payload bytes have not. Setting `*state = Idle` means the next `read_exact_packet` call reads the middle of the discarded payload as a fresh fixed-header byte — exactly the permanent parser desync the file's own doc (frame.rs:53-65) says must never happen. That doc says "the caller must reconnect," but nothing enforces it: `poll_wire` propagates the error, `PrinterClient` has no auto-invalidation (`disconnect_mqtt()` is manual only), so a caller that retries on error reads garbage forever.
**Root-cause pattern:** the write side closes this with a `write_poisoned` flag (mod.rs:100); the read side relies on a doc comment. The asymmetry is the cause.
**Suggested fix:** Add a terminal `Poisoned` variant to `FrameReadState` that makes every subsequent `read_exact_packet` fail fast, mirroring `write_frame_guarded`.

### src/mqtt/client/mod.rs:470 — `P-high` — filed as #74
**Issue:** In-flight saturation is permanent and is reported as `SocketError::TimedOut`, a name that misrepresents the condition and invites an infinite caller retry loop.
**Detail:** `in_flight` entries are removed only by a matching PUBACK (mod.rs:682). There is no per-entry expiry and no way to drain it. A broker that drops even a few PUBACKs leaks entries permanently; after 200 leaks every `publish_command` returns `Err(Error::Network(SocketError::TimedOut))` forever. A caller applying the natural retry-on-timeout policy loops indefinitely against a condition that is not a timeout and will never clear.
**Suggested fix:** Use a distinct error (a dedicated `Backpressure` variant or `ProtocolViolation`), and age out in-flight entries in `tick_zombie_check`.

### src/mqtt/client/mod.rs:100 — `P-low` — filed as #87
**Issue:** `write_poisoned` is unobservable from outside `MqttClient`, so callers cannot distinguish a retryable `TimedOut` from a permanently dead client.
**Detail:** Once `write_frame_guarded` (mod.rs:271-282) sets the flag, every later `publish_command`, `send_ping`, and the automatic PUBACK inside `poll_wire` returns `ConnectionAborted` forever. There is no `is_poisoned()` accessor and no distinguishing `Error` variant. `PrinterClient` keeps the same client in its slot on error, so a retry loop spins on a client that can never recover, with no signal to trigger the only escape (`disconnect_mqtt()`).
**Suggested fix:** Expose `pub fn is_poisoned(&self) -> bool`, or return a distinct error that `PrinterClient` maps to dropping `self.mqtt`.

### src/mqtt/client/mod.rs:90 — `P-low` — filed as #88
**Issue:** `ping_outstanding` is write-only dead state — an unanswered PINGREQ is never detected — and its doc comment misdescribes the field.
**Detail:** Across `src/` and `tests/`, the field is only ever assigned (mod.rs:433, 686, 714 plus three test constructors) and never read. `send_ping_with_timer` sets it true; `poll_wire`'s PINGRESP arm clears it; nothing acts on a ping that is never answered. Keepalive failure is caught only indirectly by `secs_since_last_message >= MQTT_STALE_CONNECTION_SECS` (60s), a different condition — any inbound traffic resets it, so a broker streaming telemetry while no longer answering PINGREQ is never flagged. The doc "Incremental scale of unacknowledged ping requests" describes a counter, not a `bool`.
**Suggested fix:** Either check the flag in `tick_zombie_check` (fail if a second ping is sent while one is outstanding) or delete the field and fix the comment.

### tests/common/mock_mqtt.rs:60 — `P-low` — filed as #89
**Issue:** The mock broker cannot validate any of the write-shape or read-deadline logic `src/mqtt/client/` now depends on, and `tests/mqtt_test.rs` has no coverage of it.
**Detail:** Per `.claude/rules/wire-framing-hardware-verification.md`, `read_packet` consumes the stream with `read_exact` regardless of how many `write_all` calls produced the bytes, so it cannot distinguish `write_frame` from `write_frame_with_timer`'s raced write, nor detect a partial frame left on the wire by a timed-out write. Separately, `tests/mqtt_test.rs` covers only the happy path — nothing there exercises `write_poisoned`, `MQTT_WRITE_TIMEOUT_SECS`, `MQTT_READ_TIMEOUT_SECS`, or frame resumption; those live only in the in-crate unit tests (`frame.rs:224-359`, `mod.rs:911-980`) using raw `tokio::io::duplex`. Coverage/fidelity gap, not a bug in the mock.
**Suggested fix:** No mock change needed; record that `tests/mqtt_test.rs` passing is not verification for any change to write/read granularity — that class needs hardware.

Four further PLAUSIBLE findings — see the Plausible section.

Verified clean: `encode_remaining_length` round-trips at 0 and the 4-byte boundary, and the decoder correctly permits exactly 4 bytes; all bounds checks in `poll_wire`'s PUBLISH/PUBACK arms (mod.rs:586-616, 670-674) are correct — no slice panic or overflow reachable from malformed wire input; `FrameReadState` resumption is correct across all three phases (`value`/`multiplier`/`filled` all persist through an early return); the zombie-timer arming/clearing invariant holds; `push_pending`'s eviction loop is sound and `pending.rs:23`'s `const _: () = assert!` correctly ties `MQTT_MAX_PAYLOAD_BYTES + u16::MAX` to the buffer cap.

## 8. src/mqtt/commands/ — COMPLETE

### src/mqtt/commands/print_job.rs:327 — `P-high` — filed as #75
**Issue:** An explicit `nozzle_offset_calibration()` override bypasses the single-nozzle hardware gate, sending `nozzle_offset_cali: 1` or `2` to a printer with no second carriage.
**Detail:** `from_config` consults `model.quirks().supports_nozzle_offset_calibration()` only inside `unwrap_or_else` — i.e. only when `config.nozzle_offset_cali` is `None`. `PrintJobConfig::new("job.3mf", …).nozzle_offset_calibration(true)` on a P1S/A1/X1 serializes `nozzle_offset_cali: 1` at line 354. `reference/03_mqtt_telemetry.md` states the field is exposed only on multi-nozzle carriage platforms (H2D, H2D Pro, H2C, X2D) and that on single-nozzle architectures it "resolves to `0` to prevent the firmware from initiating sensor checks for non-existent secondary hardware carriages." Upstream bambuddy hard-gates it identically (`bambu_mqtt.py:5080`). No downstream gate exists — `src/client/print.rs:113` passes `config` straight through.
**Suggested fix:** Make the quirk a hard ceiling, not just a default: `let nozzle_offset = if model.quirks().supports_nozzle_offset_calibration() { config.nozzle_offset_cali.unwrap_or(CalibrationMode::On) } else { CalibrationMode::Off };`, and update the `from_config` doc (lines 290-292), which currently describes the quirk as default-only.

Three further PLAUSIBLE findings — see the Plausible section.

Verified clean: **every** `pub fn new(...)` in the unit takes `impl Into<ClampedTaskId>`, whose only constructor is the clamping `From<u64>` — the task-ID clamping invariant is now **type-enforced, not convention-enforced**, and no clamp is missing (closed issue #59's fix held). Wrapper field names (`print:`/`pushing:`/`info:`/`system:`) and all `#[serde(rename)]` attrs match `reference/03_mqtt_telemetry.md` and `reference/05_materials_ams.md` exactly. `CalibrationMode`'s `Off`=0/`On`=1/`Auto`=2 encoding and the `bed_leveling` bool + `auto_bed_leveling` int split both match the reference and bambuddy's live `_tristate_wire` map; `flow_cali: false` under `Auto` matches upstream exactly. AMS drying, change-filament, RFID, filament-setting, ledctrl, airduct, buzzer, print_option, calibration, print_speed, skip_objects, gcode_line, pushall, and get_version payloads all match `reference/` field-for-field. No `println!`, no `to_string` payload serialization, no `BambuModel` variant matches.

## 9. src/ftps/ (client + parser) — COMPLETE

NO CONFIRMED ISSUES FOUND. Two PLAUSIBLE findings — see the Plausible section.

Verified clean, line by line: **the poisoning invariant holds across every public method** (`list_directory`, `get_file_size`, `delete_file`, `upload_file`, `download_file`, `create_directory`, `remove_directory`, `rename_file`, `get_available_space`, `negotiate_passive_port`, `disconnect`). Every transport-level `write_command`/`read_response` failure poisons; every wrong-but-received reply code (RNFR≠350, RNTO≠250, SIZE≠213, DELE/RMD outside {250,550}, MKD≠257, AVBL≠213, PASV≠227, and the LIST/STOR/RETR opening and confirmation codes) correctly does **not** poison. `upload_file`'s manual final `read_response` poisons on `Err` only. **BUG-004 is fully and correctly fixed**, with dedicated regression coverage in `tests/ftps_test.rs` for essentially every case. **TLS identity/SNI**: both `connect_control_stream` (`client.rs:225`) and `open_data_channel` (`client.rs:444`) pass `serial`, never `ip`. **`validate_ftp_path`** is called at the top of every path-taking method, and `parse_unix_listing` applies it to parsed names (`parser.rs:208`), silently skipping failures. **The TLS-1.2 opt-out's justification holds** — both compensating checks exist and are correct: `upload_file`'s `SIZE` recheck (`client.rs:662-667`) and `download_file`'s `226`/`426` acceptance plus its own `SIZE` recheck (`client.rs:730-747`).

## 10. src/ftps/ (protocol + mock) — COMPLETE

### src/ftps/protocol.rs:122 — `P-high` — filed as #76
**Issue:** `write_command` — the only writer on the control channel — has no write deadline, while the data-channel write loop does.
**Detail:** `write_all` (line 131) and `flush` (line 135) are awaited unconditionally with no `race`/`has_real_clock` guard. `upload_file` (`src/ftps/client.rs:609-646`) wraps both its chunk write and flush in a `race` against `FTPS_WRITE_TIMEOUT_SECS`, and every control-channel *read* is deadline-bounded via `ftps_deadline_ms`. A printer whose firmware wedges with a full receive window blocks `write_command` forever: `write_command_poisoning` (`client.rs:368`) never gets to set `poisoned`, and every public method that starts with a write (`SIZE`, `DELE`, `MKD`, `RMD`, `RNFR`, `AVBL`, `PASV`, `QUIT`) hangs with no upper bound. `FTPS_WRITE_TIMEOUT_SECS`'s own doc frames itself as covering `upload_file`'s data-write loop only.
**Suggested fix:** Give `write_command` a `deadline_ms` parameter (or a `&T: TimerProvider`) and apply the same `has_real_clock()`-gated `race` shape used at `client.rs:610-621`, mapping the timeout branch to `SocketError::TimedOut`.

### src/ftps/protocol.rs:398 — `P-high` — filed as #77
**Issue:** `validate_ftp_path`'s leading-dash rule is outbound-argument hygiene, but the function is also reused as an *inbound* filter, silently deleting legitimately-named files from directory listings.
**Detail:** `src/ftps/parser.rs:208` calls `validate_ftp_path(&name)` on every parsed `FtpFile.name` and `continue`s (silently skips) on error. That is sound for the control-character check, but the final-segment `starts_with('-')` check (protocol.rs:398-407) and the `..` check (line 388) are properties of an unsafe *command argument*, not an unsafe *name*. A file the user actually created named `-timelapse.mp4` is dropped from `list_directory` with no warning, no log line, and no error — the user sees a file in Bambu Studio that bambino claims does not exist, and `get_file_size`/`download_file` on it also refuse.
**Root-cause pattern:** one `pub(crate)` function serving two different contracts with no way for the caller to select the subset it needs; `src/ftps/CLAUDE.md:3` describes the parser reuse as applying "the same filter", which papers over the mismatch.
**Suggested fix:** Split into `validate_ftp_path_bytes()` (control chars + NUL, used by the parser) and `validate_ftp_path()` (that plus `..` and leading-dash, used by the command builders). At minimum, have the parser skip only on control-character failure and `log::warn!` when it drops an entry.

### tests/common/mock_ftps.rs:381 — `P-high` — filed as #78
**Issue:** The 426-recovery mock derives its `SIZE` reply from however many bytes it happened to read, making the post-transfer size verification tautological.
**Detail:** `run_mock_server_upload_426_recovery` reads once into a 100-byte buffer (lines 381-385) then replies `format!("213 {}\r\n", bytes_read)` (line 400). `upload_file` compares that against `data.len()` and returns `Ok(())` on a match. Because the mock echoes what it observed rather than an independently-known length, a client bug that truncated the upload makes the mock read N, reply `213 N`, and the test passes. This matters specifically because `src/ftps/CLAUDE.md:4` justifies the fail-open `allow_unverified_tls_1_2` opt-out on the grounds that "`upload_file`'s `SIZE` recheck … independently catch[es] a truncated transfer" — the test that proves that recheck works cannot fail. (`run_mock_server_upload_multi_chunk` at line 207 does this correctly with a caller-supplied `expected_len`.) Adjacent to closed issue #58.
**Suggested fix:** Take an `expected_len: usize` like the multi-chunk mock does, loop the read until that many bytes arrive, and reply `213 {expected_len}`.

### tests/common/mock_ftps.rs:78 — `P-low` — filed as #90
**Issue:** `handle_pasv` advertises a passive port that nothing ever checks — the data factory ignores both host and port, so the entire PASV→dial wiring is untested end-to-end.
**Detail:** `handle_pasv` responds `227 Entering Passive Mode (127,0,0,1,192,168)` (port 49320) and pre-stuffs the data stream into `data_container`; `MockDataStreamFactory::dial` (`tests/common/io.rs:167-176`) signature-ignores `_host` and `_port`. So (a) a regression producing port `0` or a stale value would pass every integration test — only the pure unit tests at `src/ftps/protocol/tests.rs:376-425` cover the arithmetic, nothing covers the wiring; (b) per `.claude/rules/tls-identity-sni.md` the dial host (IP) differs from the TLS host (serial), and `client.rs` calls `self.data_factory.dial(&self.ip, port)` — a regression swapping `self.ip` for `self.serial` is invisible, even though `test_ftps_control_channel_connects_with_serial_not_ip` guards the TLS side.
**Suggested fix:** Give `MockDataStreamFactory` an `Arc<Mutex<Option<(String, u16)>>>` capture field, record `(host, port)` in `dial`, and assert `("127.0.0.1", 49320)` in the LIST/RETR/STOR tests.

### src/ftps/protocol/tests.rs:263 — `P-low` — filed as #91
**Issue:** `test_read_to_eof_rejects_oversized_transfer` really allocates 512 MiB of RSS in the unit-test suite.
**Detail:** `FTPS_MAX_TRANSFER_BYTES` is 512 MiB (protocol.rs:54). `InfiniteReader` feeds 4096 bytes per poll and `read_to_eof` pushes every chunk into `out` until the cap trips — ~131,000 iterations growing `out` to 512 MiB, with `Vec` doubling putting the transient peak near 768 MiB–1 GiB. That cost is paid on every `cargo test`, including the pre-commit hook and both CI workflows, and `cargo test` runs test fns in parallel threads so it can coincide with other allocations.
**Suggested fix:** Make the cap injectable (a `max_bytes` parameter on `read_to_eof` defaulting from the constant) and drive the test with a small value.

### src/ftps/protocol.rs:374 — `P-low` — filed as #92
**Issue:** `validate_ftp_path`'s doc comment contains a duplicated, garbled sentence fragment.
**Detail:** Lines 374-376 read "…the bytes / written to the control channel contain an embedded line break that the FTP server parses as a / written to the control channel contain an embedded line break that the FTP server parses as / a *second*…" — a botched edit left half a sentence repeated. This is the reference explanation for the crate's only command-injection guard, and it is the paragraph a future session reads before touching that predicate.
**Suggested fix:** Delete the duplicated line 375 fragment.

Five further PLAUSIBLE findings — see the Plausible section.

Verified clean: no direct platform I/O in `protocol.rs` (all through `AsyncIo`/`TimerProvider`/`race`/`read_chunk`); no panic paths in reply parsing — every slice index at lines 262/270/297/308 is guarded by a preceding `len()` check and all `from_utf8` calls use `unwrap_or("")`; buffering is bounded on both axes (`FTP_MAX_RESPONSE_LINE_BYTES`, `FTP_MAX_RESPONSE_LINES`, `FTPS_MAX_TRANSFER_BYTES`); `validate_ftp_path`'s rejection set has no encoding bypass (`&str` cannot carry byte `0xFF` so Telnet IAC is unreachable; U+2028/U+2029 are not FTP line terminators; there is no decoding step between validation and `write_command`); and it is called by all nine path-taking client methods (`client.rs:472,533,554,583,685,755,776,798,799`) with no gaps.

## 11. src/camera/ — COMPLETE

### src/camera/binary.rs:161 — `P-low` — filed as #93
**Issue:** The doc comment calls `with_max_frame_size` a "Non-consuming builder" when it is a consuming builder.
**Detail:** The signature is `pub fn with_max_frame_size(mut self, max: usize) -> Self` — it takes `self` by value and returns `Self`, the standard *consuming* pattern (the value must be reassigned or chained; dropping the return discards the change, mitigated only by `#[must_use]`). The comment at line 156 reads "Non-consuming builder, matching the `PrinterClient::with_mqtt_port`/`with_ftps_port` convention (`src/client/mod.rs`)." Both referenced methods (`src/client/connect.rs:206` and `:303`) are *also* consuming builders — so the code is internally consistent with its siblings, but "Non-consuming" is backwards terminology for what all three actually do. The referenced path is also stale: those methods live in `src/client/connect.rs`, not `src/client/mod.rs`. Note `.claude/rules/client-builder-api.md` uses "consuming"/"non-consuming" as load-bearing terms for which builders change type parameters, so an inverted use of the term here is genuinely misleading rather than cosmetic.
**Suggested fix:** Change to "Consuming builder" (or drop the qualifier) and correct the file reference to `src/client/connect.rs`.

No other issues — no PLAUSIBLE findings from this unit.

Verified clean against the highest-risk areas: the frame-size cap **is** enforced before allocation (`binary.rs:293` checks `size > self.max_frame_size`; the `vec![0u8; size]` is at line 307); length arithmetic uses `usize::try_from` on `raw_size: u32` rather than an `as` cast, so a sub-32-bit `usize` target errors instead of truncating; JPEG SOI/EOI validation (`binary.rs:328-337`) short-circuits on `size < 4` before the `size - 2`/`size - 1` indexing, so no underflow is reachable; `CameraFrameReadState` persists `filled`/`remaining` across timed-out calls and is covered by dedicated byte-loss and desync regression tests; a declared length of 0 is rejected with `ProtocolViolation` and resets to `Idle`; a declared length over the cap transitions to `DiscardingOversizedPayload` and drains in bounded 512-byte `CAMERA_DISCARD_CHUNK_SIZE` chunks rather than allocating an attacker-controlled `remaining` (this is the fixed version of a documented past bug where draining was skipped entirely, permanently desyncing the stream). `authenticate()`'s write-only semantics match its documentation exactly and are covered end-to-end by `test_binary_camera_rejected_handshake_surfaces_on_first_read`. `build_rtsps_url` validates `access_code` as non-empty ASCII-alphanumeric before interpolation (blocking `@`/`:`/`/`/newline injection) and requires `ip`/`printer_ip` to parse as `core::net::IpAddr`; `rewrite_rtsp_request_uri`'s rewritten host/port always comes from the validated `printer_ip`, never from the caller-supplied `request_uri`, so an unvalidated request URI cannot redirect the connection. `RtpTimestampCorrector` wraps correctly past ~13.25 hours via a `u64` intermediate.

**Context note (not a finding):** `authenticate_with_timer`'s write-on-timeout is non-resumable, documented as an acceptable simplification because a partial write plus retry could desync framing. This is currently unreachable: `ensure_camera()` (`src/client/connect.rs:390-424`) is the only caller passing a real timer, and it dials a fresh stream and constructs a fresh `BambuBinaryCameraStream` on every attempt, never reusing a stream after a failed attempt. Recorded in case a future caller starts reusing a stream instance across retries.

## 12. src/client/ (core) — COMPLETE

### src/client/dummy.rs:83-86 — `P-low` — filed as #94
**Issue:** `PreConnected`'s doc comment claims `RawStreamFactory::dial()` is "genuinely unreachable"; it is reachable, and it directly contradicts `disconnect_mqtt()`'s own doc comment about the same code path.
**Detail:** The comment states that `ensure_mqtt()` short-circuits on `self.mqtt.is_some()` before either impl is called, so `dial` "is genuinely unreachable." That holds only for a `from_mqtt()`-built client's initial lifetime. Once `disconnect_mqtt()` (`connect.rs:152-156`) is called, `self.mqtt` becomes `None`, and any subsequent `ensure_mqtt()`-gated call (`poll_telemetry()`, `mqtt()`, `get_version()`) **will** call `self.mqtt_factory.dial(...)` — which for a `from_mqtt()` client is `PreConnected`, unconditionally returning `Err(SocketError::NotConnected)`. `disconnect_mqtt()`'s own doc (`connect.rs:148-151`) describes exactly this: "its `PreConnected` factory's `dial()` always errors, so `ensure_mqtt()`'s lazy-dial fallback only recovers a `connect()`-built client, never one built via `from_mqtt()`." The two comments contradict each other; `dummy.rs` predates or ignores `disconnect_mqtt()`. No test in `tests/client_reconnect_test.rs` exercises disconnect-then-re-`ensure_mqtt()` on a `from_mqtt()` client, which is why the stale claim survived. Functionally nothing needs to change — the error behavior is correct and intentional — but a reader trusting the "unreachable" claim would mis-model the reconnect contract. Adjacent to closed issue **#7** (`attach_mqtt()` and the documented reconnect path).
**Suggested fix:** Update the `PreConnected` doc to name the `disconnect_mqtt()`-then-`ensure_mqtt()` path as the one case where `dial()` is reached and correctly returns `NotConnected`, instead of claiming blanket unreachability.

No PLAUSIBLE findings from this unit.

Verified clean across all three connect trios: serial-as-SNI at all three TLS call sites; `connect_timeout_secs == 0` and `DummyTimer` both correctly bypass the race in `race_against_connect_timeout`; `ftps_config` is `.take()`n only **after** a successful connect (same for the camera config), so a failed attempt doesn't silently consume it; `ensure_camera()`'s `camera_protocol()` check precedes the `.with_camera()`-configured check, so an RTSPS model fails immediately and never dials; `next_sequence_id()` routes through the canonical `clamp_task_id()`; and `TelemetryCache::last_home_flag` is populated straight from `print.home_flag`, satisfying the bed-temp-voltage invariant's input side (the consuming logic in `thermal.rs` was verified separately in unit 13). The telemetry cache's field-by-field staleness semantics match their documentation.

## 13. src/client/ (operations) — COMPLETE

NO CONFIRMED ISSUES FOUND. One PLAUSIBLE finding — see the Plausible section (`src/client/ams.rs:99`, a sibling of closed issue **#9**).

Verified correct: **bed-temp voltage invariant** (`thermal.rs:64-73`) — `set_bed_temperature` derives `mains_220v` from `cache.last_home_flag` via `POWER_220V_BITMASK` and passes it to `bed_temp_max()`; `x1.rs:45-53` has `X1C_BED_TEMP_MAX_220V = 110` / `X1C_BED_TEMP_MAX_110V = 120` (correctly inverted) with `None` clamping to the safer 110 °C. `clamp_temp` (`client/mod.rs:64-76`) uses `value > max`, correctly letting the ceiling value itself through. Chamber-heater ceilings (X1E 60, H2 65, X2D 65) match `MODEL_MATRIX.csv`; X1C/P1/P2S/A1/A2 correctly return `None` so `set_chamber_temperature` rejects rather than silently applying a bogus cap. **K-profile priming** (`ams.rs:338-369`) matches the documented auto-prime/opt-out behavior. **FTPS poisoning and camera trio** — `disconnect_storage()` and `disconnect_camera()` both reset their slot to `None`. **Fan/hardware guards** (`hardware.rs`) — `set_fan_speed` warns before clamping `> 100` and gates `AuxiliaryLeft`/`ChamberExhaust`/`AuxiliaryLeft2` against the matching `ModelQuirks` capability; `set_airduct_mode`/`set_prompt_sound`/`set_buzzer_mode` all gate via `model.quirks()` with no `BambuModel` variant matching anywhere in the unit. **Motion safety** (`motion.rs`) — `home_axes` rejects Z-only homing on bed-on-Z models; `move_relative`/`extrude` treat unhomed-axis state as advisory-only per their docs; travel-limit clamping uses `distance.abs() > axis_max` (not off-by-one), and the `distance == 0.0` no-op short-circuit correctly precedes the travel-limit check so a zero-move never raises a spurious `ModelMismatch`.

`print.rs` has no model-specific safety branching to verify — pause/resume/stop/clear-error/speed/skip-objects/calibration/start-print dispatch directly, consistent with those commands having no physical-limit dimension at this layer. `camera.rs`/`storage.rs` are thin accessor/disconnect wrappers matching their documented invariants.

## 14. src/diagnostics/ — COMPLETE

### src/diagnostics/kprofile.rs:255-257 — `P-low` — filed as #95
**Issue:** The "IDEX External-Spool Addressing Cheat-Sheet" doc comment on `ExtrusionCaliSelRequest::new` gives the wrong `tray_id` for `ams_filament_setting` addressing — the exact pre-BUG-117 value.
**Detail:** The comment states `ams_filament_setting — Dual-Nozzle IDEX: Ext-L requires ams_id: 254 / tray_id: 0; Ext-R requires ams_id: 255 / tray_id: 0`. Two independent sources contradict it: `reference/05_materials_ams.md:193` ("Ext-R requires `ams_id: 255` / `tray_id: 254` (never `0` — BUG-117 / BambuStudio `DeviceManager.cpp:1667-1693`)"), and the doc on the actual implementing constructor `AmsFilamentSettingRequest::new` (`src/mqtt/commands/ams.rs:56-61`), which states both Ext-L and Ext-R require `tray_id: 254`, explicitly citing BUG-117 and "never `0`". This cheat sheet is a stale copy that regressed the very value BUG-117 fixed. Doc-only in `kprofile.rs` — the constructor doesn't take `ams_filament_setting` params — but it is presented as canonical and cross-referenced from both files, so a caller building an `ams_filament_setting` request from this docstring reproduces BUG-117. **Sibling of closed issue #42**, which fixed the same stale value in `reference/05_materials_ams.md:193`; this copy was missed.
**Suggested fix:** Change `tray_id: 0` to `tray_id: 254` in both `ams_filament_setting` bullets, matching `src/mqtt/commands/ams.rs:56-61` and `reference/05_materials_ams.md:192-193`.

No PLAUSIBLE findings from this unit.

Verified clean: all five K-profile request constructors (`ExtrusionCaliGetRequest`, `ExtrusionCaliSetRequest`, `ExtrusionCaliSelRequest`, `StandardCaliDelRequest`, `IdexCaliDelRequest`) follow the Payload+Request pattern with a `print:` wrapper field and take `impl Into<ClampedTaskId>` — task-ID clamping is **type-enforced** here (`ClampedTaskId`'s `From<u64>` calls `clamp_task_id()`, confirmed at `src/mqtt/commands/mod.rs:69-72`), with no hand-rolled sites left. The `extrusion_cali_sel` addressing actually implemented in this file matches `reference/05_materials_ams.md:194-197` exactly (254/254 single-nozzle, 254/254 Ext-L, 255/255 Ext-R) — only the cheat-sheet comment above is wrong. `ExtrusionCaliGetRequest`'s priming-quirk doc matches `reference/07_diagnostics_hms.md:209-212` and the implementation in `src/client/ams.rs:333-377`. `KProfileEntry`'s per-entry `nozzle_diameter: Option<String>` fallback-to-envelope matches `reference/07_diagnostics_hms.md:129-132` and is tested. In `hms.rs`: `HmsSeverity::from_code` reads severity from `code >> 16` (not `attr`, per BUG-108); `decode_hms_alert`'s `is_status_step` compares the **full** 32-bit `code` against `HMS_FAULT_THRESHOLD` (per BUG-109) while `decode_print_error` compares only the low 16 bits — both match `reference/07_diagnostics_hms.md:59-80` precisely, including wiki-key/short-code word ordering and module-ID extraction `(attr >> 24) & 0xFF`.

## 15. src/types/ (telemetry core) — COMPLETE

NO ISSUES FOUND in src/types/ (telemetry core) — no CONFIRMED and no PLAUSIBLE findings. All seven files read in full and cross-checked against `reference/03_mqtt_telemetry.md`, `05_materials_ams.md`, and `07_diagnostics_hms.md` section by section.

Verified clean: `unpack_temperature()` (`report.rs:348`) is called **only** on `ExtruderInfo.temp` (`device.rs:445`) and on `BedInfo.temp` via `unpack_bed_telemetry()` (`mod.rs:119`) — never on a plain field; `nozzle_temper`/`bed_temper`/`nozzle_target_temper`/`bed_target_temper` are all `Option<f64>` direct values. `TelemetryReport::device()` checks top-level `device` before `print.device`. `is_ethernet_active()` reads `print.net.conf` bit 0; the wrong bit-18 `home_flag` heuristic is gone, retained only as the documented `wifi_signal == "-90dBm"` fallback. `ExtruderCollection.state`'s bit split (count low 4 bits, active index bits 4–7) and `ExtruderInfo.snow`/`spre`/`star` 8/8 AMS-routing split match the cited BambuStudio references. `AmsTray.id: String` and `CtcInfo.temp: Option<u32>` are as documented. `AmsUnit.info` bitmask constants (`ams.rs:660-670`) match the reference table exactly — type bits 0–3, dry_status 4–7, extruder assignment 8–11 (0xE sentinel handled), dry fan1/fan2 18–19/20–21, and **dry_sub_status at bits 22–23, with 24–25 correctly left unmodeled as `bind_switch_in`** (the BUG-104 boundary holds). `BedTelemetry`/`BedInfo` composite packing matches, with the old-gen `bed_temper`/`bed_target_temper` path kept separate and unpacked in `decode_bed_temperatures()`. `vir_slot: Option<Vec<VirtualTray>>` vs `vt_tray: Option<VirtualTray>` are correctly split IDEX-vs-single-nozzle on the same schema. `HmsEntry.ts_boot: Option<u64>` / `ts_unix: Option<String>` match the reference. Both prior-bug classes the invariants warn about are absent: `home_flag`'s `deserialize_signed_as_u32` correctly masks a signed i64 into the u32 bit pattern (closed issue #49), and `VersionModule.name` already carries `#[serde(default)]` (closed issue #52).

## 16. src/types/telemetry/tests/ — COMPLETE

### tests/telemetry_replay_test.rs:75-146 — `P-low` — filed as #96
**Issue:** The replay loop never checks that `poll_telemetry()` actually parsed a `TelemetryEvent::Report` — a replay that silently produces `TelemetryEvent::Unknown` for every message still passes.
**Detail:** Lines 76-79 call `client.poll_telemetry().await.unwrap_or_else(|e| panic!(...))` and then discard the `TelemetryEvent` — it is never bound or matched. `poll_telemetry()` (`src/client/telemetry.rs:140-154`) returns `Ok(TelemetryEvent::Unknown(msg))` both when a payload is a command echo and when deserialization fails outright (`Err(_) => Ok(TelemetryEvent::Unknown(msg))`); neither path panics or returns `Err`. So if every fixture line were misclassified as a command echo (an `is_command_echo` false positive), or the fixture's schema drifted out of sync with `PrinterTelemetry` so every message failed to deserialize, the telemetry cache would simply never update — and all the downstream plausibility assertions (`bed_temperatures`, `nozzle_temperatures`, `print_progress`, fan speeds) either read default/zero values or sit inside `if let Some(...)` guards that are trivially satisfied when the field stays `None`. The test passes end-to-end with zero telemetry actually parsed.
**Suggested fix:** Assert `matches!(event, TelemetryEvent::Report(..))`, or count `Report` events and assert the count equals `lines.len()` (or is at least non-zero), before running the accessor checks.

One PLAUSIBLE finding — see the Plausible section.

Verified clean: `DeviceTelemetry` dual-location precedence (top-level wins) is pinned by `tests/device.rs:393-408`. `is_ethernet_active()` (net.conf bit 0) and the `wifi_signal` fallback are both pinned (`tests/device.rs:116-191`). **The BUG-104 boundary is genuinely pinned**, not accidentally — `test_ams_unit_info_accessors_dry_sub_status_distinct_bits` (`tests/ams.rs:412-437`) deliberately sets bit 24 nonzero while bits 22–23 give a distinct value, so it would fail on that class of shift/mask error rather than passing on an all-zero fixture. `ExtruderCollection.state`/`ExtruderInfo.temp` composite tests use non-zero neighbouring bits (`tests/nozzle.rs:118-182`), avoiding the weakness class of closed issue **#24** — and that test itself was already fixed to include real composite values (`tests/misc.rs:310-333`). `AmsTray.id`/`CtcInfo.temp` types and the `vir_slot`/`vt_tray` split are exercised and match the reference docs. **Fixture values (AMS `info: "11002103"`, `cfs: [2,9,5,7]`, temperature composite `6553700`) match the reference docs' own worked examples rather than being invented to fit the Rust structs** — the main risk this unit was checked for.

## 17. tests/client_{telemetry_cache,core,negative}_test.rs — COMPLETE

NO ISSUES FOUND in tests/client_{telemetry_cache,core,negative}_test.rs — no CONFIRMED and no PLAUSIBLE findings. No test asserts the wrong thing, and no mock is more permissive than the protocol it emulates. Assertions were cross-verified against the actual implementations they pin: bed-temp voltage clamping (`quirks/models/x1.rs`), HMS decode (`diagnostics/hms.rs`), AMS target derivation (`client/ams.rs`), fan step-decode, chamber-temp composite packing, `printing_tray_global_id` snow-field decode, and the in-flight saturation limit.

Invariant-by-invariant, scoped to these three files:
- **`connect-timeouts.md`** (`connect_timeout_secs == 0`): not applicable here — every client in these files is built via `connect_test_client()` (`tests/common/client.rs`), which returns a `PreConnected`-typed client and never calls `ensure_mqtt()`/`ensure_ftps()` or `.with_timer()`. The connect-timeout path isn't exercised in this unit at all.
- **`poll-telemetry-dispatch.md`** (stashed-before-fresh drain order): not covered here, but pinned elsewhere at `tests/client_version_test.rs:72` and `:244-246` — not a project-wide gap.
- **`bed-temp-voltage.md`**: fully covered and correct — `client_core_test.rs:266-331` exercises all three `last_home_flag` states (`None`, `Some(true)` via bit 3 set, `Some(false)` via bit 3 clear) against the 110/120 °C inversion.
- **`task-id-clamping.md`**: `client_negative_test.rs:805-826` plus its comment correctly note that the real wraparound math is covered by the colocated `mqtt::commands::tests::test_clamp_task_id_wraps_near_max` — an accurate self-description, not an overclaim.
- **`wire-framing-hardware-verification.md`**: several tests are named `*_wire_payload` (`test_start_print_wire_payload`, `test_skip_objects_wire_payload`), but they assert JSON *content* via `read_publish_payload`, not write/read framing shape — **the naming does not overclaim**, which was the specific risk checked for.
- **Key Invariant #2 / negative coverage**: per-model guard rejections are well covered — over-temp/voltage clamp, unsafe Z-homing (`test_homing_safety_interlocks`), unsupported fan targets (`test_cooling_fans_and_peripheral_switches`, `test_chamber_exhaust_fan_success_and_model_mismatch`), chamber-heater absence (`test_thermal_guards_and_temperatures`), and P1 drying rejection (`client_negative_test.rs:505-526`).

## 18. tests/client_{reconnect,session,version,gcode}_test.rs + tests/common/ — COMPLETE

NO CONFIRMED ISSUES FOUND. One PLAUSIBLE finding — see the Plausible section.

Verified clean: **`tests/common/client.rs`'s `connect_test_client()` builds with `DummyTimer`, but this is NOT a helper-introduced weakness** — it matches `PrinterClient::from_mqtt()`'s own default (`src/client/mod.rs:239`), and the two timeout-specific tests (`test_ensure_mqtt_bounds_post_dial_handshake_by_connect_timeout`, `test_with_connect_timeout_zero_disables_timeout`) bypass the helper and chain `.with_timer(TokioTimer::new())` explicitly. No timeout assertion in these files is vacuous. The `connect_timeout_secs == 0` disable is pinned. **The `poll-telemetry-dispatch` buffer invariant is well covered** — `test_get_version_round_trip` and `test_poll_until_buffers_unmatched_messages` both verify that telemetry arriving mid-`get_version()` is stashed and later drained rather than dropped. `task-id-clamping` and `camera-trio`'s `disconnect_camera()` have no test in these files but are covered in `tests/client_negative_test.rs` and `tests/camera_test.rs` respectively — not project-wide gaps. `test_with_ftps_panics_on_from_mqtt_client` matches the exact assert and message at `src/client/connect.rs:271-275`; `test_ensure_mqtt_reseed_skipped_without_real_clock` matches the `has_real_clock()` skip-reseed logic at `connect.rs:108-114`; `test_get_version_round_trip`'s `visible == true` default matches `default_visible()` (`src/types/version.rs:9-11,33`).

## 19. src/bin/bambino-cli/ (main + control + monitor) — COMPLETE

Both findings are README staleness — no code change needed. `main.rs`'s embedded `after_help` (the actual `--help` output users see) is correct in both cases; only README's separately-maintained transcript has drifted. Note the `readme-review` skill exists for exactly this class.

### README.md:387-398 — `P-low` — filed as #97
**Issue:** README's documented command surface omits the `ack-probe` subcommand entirely.
**Detail:** `main.rs`'s `Commands` enum has an `AckProbe { ip, serial, access_code, output, tests, window }` variant (lines 122-137) with its own `-o/--output`, `-t/--tests`, `--window` options, and `main.rs`'s own `after_help` mentions it (line 58: `"Ack-probe: -o/--output -t/--tests --window"`). README's `### Usage` block (lines 384-418) — a `bambino-cli --help` transcript — lists only `discover info monitor dump probe control files camera inspect-cert verify-tls help`, with no `ack-probe` row and no options line. A README reader cannot discover the subcommand exists.
**Suggested fix:** Add an `ack-probe` row and an `Ack-probe: -o/--output -t/--tests --window` line, mirroring `main.rs`'s `after_help`.

### README.md:415 — `P-low` — filed as #98
**Issue:** README's "Files actions" line omits `clock-check`.
**Detail:** `storage.rs:53` defines `FilesAction::ClockCheck`, and `main.rs`'s `after_help` lists it correctly (line 55: `"Files actions: list upload delete space clock-check"`). README line 415 reads `"Files actions:    list  upload  delete  space"`. `files clock-check` exists and works, but a README reader won't find it — same hidden-subcommand problem as above.
**Suggested fix:** Add `clock-check` to README.md line 415.

No PLAUSIBLE findings from this unit.

Verified clean: all seven files start with `#![cfg(feature = "cli")]`; `Cargo.toml`'s `cli` feature correctly gates `crossterm`/`env_logger`/`clap`/`time`/`unicode-width` with no leak into the `tokio` feature; **`connection.rs::create_printer` constructs the monitor's client with `.with_timer(TokioTimer::new())`, not the default `DummyTimer`** — so the long-running poll loop in `monitor/mod.rs` actually gets the per-read stall deadline `.claude/rules/wire-read-deadline.md` requires; no `BambuModel` variant matching anywhere in the unit; **no access code or serial is ever printed, logged, or `{:?}`-debugged** — `ControlAction` carries no credential field and `validate_params`'s error reports only `access_code.len()`, never the code; no panics on malformed user or wire input in `table.rs`/`dashboard.rs` (`format_color_swatch`'s ASCII-prefix check correctly guards the char-boundary slicing and has a regression test).

## 20. src/bin/bambino-cli/ (probes + storage + certs) — COMPLETE

This unit independently re-confirmed the two README findings already staged under unit 19 (`ack-probe` missing from the Commands list, `clock-check` missing from the Files actions line) — not staged twice; see unit 19. Unit 20 adds that `ack-probe` is not merely undocumented but can dispatch **physically-actuating** commands depending on `-t` (`ams_change_filament`, and `project_file`, the print-start command, which induces a `0500_C010` panel error it then tries to clear) — which raises the stakes on that doc gap beyond ordinary staleness.

### src/bin/bambino-cli/ack_probe.rs:268-274 and probe.rs:174-176 — `P-low` — filed as #99
**Issue:** Both `AckReport` and `ProbeReport` serialize the printer's `serial` as a top-level field, and `run()` writes that JSON straight to disk — against the standing rule never to write a serial into a file in this repository.
**Detail:** `ack_probe.rs`'s `AckReport { serial: serial_owned, .. }` (populated at `ack_probe.rs:717,734`) and `probe.rs`'s `ProbeReport { serial: serial_owned, .. }` (populated at `probe.rs:640,650`) are written to `output`/`output_path`, whose defaults (`ack_probe_report.json`, `probe_report.json`) land in the CWD — normally the repo root for a dev running the CLI from a clone. **Partially mitigated:** `.gitignore:26-28` already excludes `probe_report*.json` and `ack_probe_report*.json`, with a comment acknowledging the embedded serial, so the default filenames won't be committed. But `-o`/`--output` (`main.rs:114,129`) accepts an arbitrary path; anything outside that glob (`-o report.json`, `-o results/p1s.json`) is unprotected and can be swept in by `git add -A`. The rule is enforced here by a `.gitignore` pattern that the tool's own flag lets the user step outside of.
**Suggested fix:** Warn on stderr when `output` doesn't match the known-safe glob, or drop `serial` from the report body entirely (model + timestamp usually suffice; log the serial to stderr instead of embedding it in the artifact).

No PLAUSIBLE findings from this unit.

Verified clean: all seven files start with `#![cfg(feature = "cli")]`; no `unwrap()`/`expect()`/`panic!` on user input, file I/O, or network results — the only `.expect(...)` calls are internal post-handshake invariants in `inspect_cert.rs` (mutex-poison guards and one assuming `verify_server_cert` ran on a completed handshake), acceptable in a diagnostic-only tool. `camera.rs:43` routes BinaryJpeg-vs-RTSPS through `printer.model().quirks().camera_protocol()`, never a hardcoded model list. **No false assurance from the cert tools**: `inspect_cert.rs:99` and `verify_tls.rs:32` both pass `serial` (not `ip`) as the TLS `ServerName`; `verify_tls.rs` genuinely exercises the library's real `CnFallbackServerVerifier` via `build_verified_client_config` rather than reimplementing weaker logic, and `inspect_cert.rs`'s unconditional-trust verifier is documented as diagnostic-only and only ever claims the cert was "captured", never "verified". `ack_probe.rs` mints sequence IDs via `client.next_sequence_id()` through the library's `impl Into<ClampedTaskId>` constructors, so clamping is type-enforced and not bypassed; hand-built payload argument order and types were cross-checked against each constructor's real signature. **Closed issue #51's busy-state gate holds in both probes** — `probe.rs::refuse_if_busy` (called at `probe.rs:635`) and `ack_probe.rs::refuse_if_busy` (at `ack_probe.rs:709`) both refuse to run in `Preparing`/`Running`/`Paused`. **No access code is ever echoed or written** anywhere in the unit. `discover.rs` prints serial/IP to stdout but writes no file.

---

# Triaged Findings (reported as unverified, now resolved)

These were reported as unverified by the reviewing agents. **All have now been triaged by direct code read and carry a priority** — they stay in this section rather than being moved, so the provenance of each stays visible. Two were materially corrected during re-verification (noted inline). Nothing here is left open for a human to re-derive.

### Re-verification results

| Finding | Disposition |
|---|---|
| `quirks/mod.rs:240` Unknown→X1C bed ceiling | **CONFIRMED — `P-critical`**, see correction below |
| `quirks/mod.rs:263` homing comment scan | CONFIRMED — `P-low` |
| `client/ams.rs:99` AMS-HT + slot 254 | CONFIRMED — `P-low`, reasoning corrected below |
| `discovery/mod.rs:130` `&buf[..len]` | CONFIRMED — `P-low` |
| `discovery/mod.rs:235` `?` on inter-burst sleep | CONFIRMED — `P-low` |
| `discovery/parser.rs:214` compound USN | CONFIRMED — `P-low` |
| `discovery/mod.rs:275` `Ok(None)` yield contract | CONFIRMED — `P-low` |
| `discovery/mod.rs:171` 5s doc-example timeout | CONFIRMED — `P-low` |
| `io/tokio.rs:289` `Ok(0)` → `write_all` panic | CONFIRMED — `P-low` |
| `cert_verify.rs:330` SAN parse error → CN fallback | CONFIRMED — `P-low` |
| `cert_verify.rs:213` expired same-subject decoy | CONFIRMED — `P-low` (fail-closed, availability only) |
| `codec.rs:28` unbounded `encode_remaining_length` | CONFIRMED — `P-low` |
| `mqtt/client/mod.rs:417` SUBACK accepts non-granted codes | CONFIRMED — `P-low` |
| `frame.rs:145` 1 MiB alloc on every target | CONFIRMED — `P-low` (cap **is** checked pre-allocation; the issue is the constant is not target-scaled) |
| `codec.rs:25` advertised keepalive never honored | CONFIRMED — `P-low` |
| `error.rs:117` no `core::error::Error` under no_std | CONFIRMED — `P-low` |
| `identity.rs:13` `Debug` prints access code | CONFIRMED — `P-low` (latent; no live leak) |
| `lib.rs:77` module guide omits `identity` | CONFIRMED — `P-low` |
| `ams/parser.rs:71` AMS-HT `tray_id` unbounded | CONFIRMED — `P-low` |
| `ams/mapping.rs:171` unbounded `vec![-1; max_id]` | CONFIRMED — `P-low` |
| `ams/mapping.rs:112` `254` on single-nozzle `ams_mapping2` | CONFIRMED — `P-low` |
| `print_job.rs:319` `ams_mapping` pub-field bypass | CONFIRMED — `P-low` |
| `ftps/client.rs:512` LIST `426` with no integrity check | CONFIRMED — `P-low` (the code comment itself calls it a "sibling gap") |
| `ftps/parser.rs:123-283` unbounded entry count | CONFIRMED — `P-low` |
| `ftps/protocol.rs:166` partial-progress not mirrored | CONFIRMED — `P-low` (safe today via poisoning; documentation gap) |
| `ftps/protocol.rs:430` duplicated deadline logic | CONFIRMED — `P-low` |
| `ftps/protocol.rs:342` PASV whitespace strictness | CONFIRMED — `P-low` |
| `ftps/protocol/tests.rs:471` no CR/LF/NUL test | CONFIRMED — `P-low` |
| `mock_ftps.rs:33` no multi-line / split reply coverage | CONFIRMED — `P-low` |
| `telemetry/tests/bed.rs` no >500 flat-field test | CONFIRMED — `P-low` |
| `client_reconnect_test.rs:156` never actually poisons | CONFIRMED — `P-low` |
| `ams/parser.rs:114` AMS-HT state-9 carve-out | **`needs-verification`** — reference doc and code disagree; needs an upstream/hardware decision, see entry |
| `esp_idf.rs:123` `send_to` `WouldBlock` | **`needs-verification`** — hardware only |
| `esp_idf.rs:195` `poll()` `EINTR` | **`needs-verification`** — hardware only |
| `print_job.rs:355` `vibration_cali` P2S/N7 gate | **`needs-verification`** — upstream cross-check |
| `print_job.rs:215` three omitted `project_file` fields | **`needs-verification`** — wire capture |

### Correction 1 — `src/quirks/mod.rs:240` is CONFIRMED and is `P-critical`

Verified directly. `quirks()`'s `Unknown` arm returns `&models::x1::X1CQuirks` after a `log::warn!` whose text reads "falling back to X1C quirks; travel and temperature limits (256mm axes, **110C bed**, 300C nozzle) may exceed this machine's real ceilings". But `x1c_bed_temp_max` is:

```
Some(true)  => X1C_BED_TEMP_MAX_220V   // 110
Some(false) => X1C_BED_TEMP_MAX_110V   // 120
None        => X1C_BED_TEMP_MAX_220V   // 110
```

So the warning's "110C bed" holds only until the first `home_flag` arrives. Once an unrecognized printer reports `home_flag` with bit 3 clear (110 V region), `set_bed_temperature`'s clamp for that machine becomes **120 °C** — above the real ceiling of every entry-level model in `MODEL_MATRIX.csv`. Per the `backlog` skill this is `P-critical` ("temp overshoot past a real hardware ceiling"), and it **blocks release**.

Trigger conditions, stated plainly so this can be re-judged: the printer's serial prefix must be absent from `SERIAL_PREFIXES` (i.e. a model newer than this crate), **or** it reaches `Unknown` via the unit-2 finding at `discovery/parser.rs:226`. All 13 currently-known models resolve correctly, so this cannot fire on today's hardware — it is a latent trap that springs the day Bambu ships a model bambino doesn't know. That is also exactly the case the fallback exists to handle.

Closed issue **#54** covered this ground and was closed; the warning text was made accurate for the `None` case but the `Some(false)` path was not addressed. Treat as a reopen, not a new bug.

### Correction 2 — `src/client/ams.rs:99` is CONFIRMED but the mechanism was misdiagnosed

The reviewing agent claimed `(ams_id: 130, slot_id: 254)` "sends a command shaped for external-spool addressing to a physical AMS-HT unit." **That part is wrong.** `reference/05_materials_ams.md:205` (BUG-116, confirmed against BambuStudio's `command_ams_change_filament`) documents the derivation as: `255` on unload; **the `ams_id` itself for any AMS-HT/external-spool unit (`ams_id >= 16`, covering both `128`-`135` and `254`/`255`)**; otherwise `(ams_id * 4) + slot_id`. So `target = 130` is exactly per spec, and the `ams_id >= 16` branch deliberately spans both ranges.

The real defect is narrower and still real: `is_valid_ams_id` admits `128..=135`, and `pair_valid = slot_id != 254 || ams_id >= 16` therefore lets `(130, 254)` through, even though the reference documents an AMS-HT slot only as `{"ams_id": ams_id, "slot_id": 0}` (line 162) and `slot_id: 254` only against the external-spool `ams_id` (line 229-231). The command goes out with a correct `target` but a nonsensical slot. `P-low` — a validation gap that lets malformed input reach the printer, not a mis-derived target.

## From unit 1 — src/{lib,error,models,identity}.rs

### src/error.rs:117 — filed as #114
**Issue:** `Error` implements `core::error::Error` only under `std`; the no_std/embassy build gets a bare `Display` impl and no error trait at all.
**Detail:** Under `std`, `#[cfg_attr(feature = "std", derive(Error))]` (line 27) gives `impl std::error::Error`. Under `not(feature = "std")` the only impl is `core::fmt::Display` (lines 117-122) — nothing implements `core::error::Error`, available in `core` since Rust 1.81 and usable here (edition 2024 ⇒ rustc ≥ 1.85). A downstream embassy consumer writing `fn handle<E: core::error::Error>(e: E)` compiles on host and fails on the embassy target with identical source — an API surface that silently differs across the three advertised targets. Related to closed issue #13.
**Suggested fix:** `#[cfg(not(feature = "std"))] impl core::error::Error for Error {}` next to the `Display` impl (no `source()` needed — no variant carries a `#[source]`/`#[from]` field).

### src/identity.rs:13 — filed as #115
**Issue:** `#[derive(Debug)]` on `PrinterIdentity` makes the LAN access code printable via any `{:?}`, with nothing at the type level preventing a future log line from leaking it.
**Detail:** The struct holds `access_code: String`, documented at line 19 as the printer's network credential; root `CLAUDE.md` says to treat it as any other credential. A single `log::debug!("connecting with {:?}", identity)` in `client/`, `mqtt/client/`, `ftps/client.rs`, or `camera/binary.rs` — all of which take `&PrinterIdentity` (`ftps/client.rs:219`, `mqtt/client/mod.rs:298`, `camera/binary.rs:176`, `client/mod.rs:121`) — writes the access code into user-visible logs. The agent grepped and found **no such call site today**, so this is a latent footgun, not a live leak.
**Suggested fix:** Manual `Debug` impl printing `access_code: "<redacted>"`, keeping `ip`/`serial`/`model` verbatim.

### src/lib.rs:77 — filed as #116
**Issue:** The crate-level "Module guide" omits `identity`, the one module the quick-start example actually names.
**Detail:** The guide (lines 71-83) enumerates every top-level `pub mod` **except** `identity` (line 90), even though the quick start imports `bambino::identity::PrinterIdentity` (line 25) and `lib.rs:106` re-exports it at the crate root. Passes `#![deny(missing_docs)]` because the module itself is documented; only the index is incomplete. Lowest-value item in the sweep.
**Suggested fix:** Add an `identity` line to the guide.

## From unit 2 — src/discovery/

### src/discovery/mod.rs:130 — filed as #102
**Issue:** `&buf[..len]` panics if an `AsyncUdpSocket` implementation reports a length exceeding the buffer.
**Detail:** `AsyncUdpSocket` is a public, unsealed trait (`src/io/mod.rs:158`), so third-party and FFI-backed impls are expected — `EspIdfUdpSocket` (`src/io/esp_idf.rs:132`) wraps a raw syscall return. A `len` greater than `buf.len()` panics inside the discovery loop rather than dropping the datagram. This is the one unguarded index on data crossing a network/FFI boundary in the unit.
**Suggested fix:** `let Some(datagram) = buf.get(..len) else { log::warn!(...); return Ok(None); };`

### src/discovery/parser.rs:214 — filed as #104
**Issue:** A UPnP-spec-compliant compound `USN` yields a garbage serial rather than the hardware serial.
**Detail:** The code strips only a leading `uuid:`. The UPnP-standard compound form `USN: uuid:<serial>::urn:bambulab-com:device:3dprinter:1` becomes the serial `"<SERIAL>::URN:BAMBULAB-COM:DEVICE:3DPRINTER:1"`. That still passes `resolve_model`'s 3-char prefix match, so the device is accepted with a corrupt serial — which flows into MQTT topic routing and TLS identity, where reference §1.6 says an exact-string mismatch produces an accepted subscription with zero telemetry (silent failure). The reference documents only bare and `uuid:`-prefixed forms today, so this is firmware-track exposure, not a currently-observed break. Adjacent to closed issue #11.
**Suggested fix:** After stripping `uuid:`, truncate at the first `::`.

### src/discovery/mod.rs:275 — filed as #105
**Issue:** The listen loop's `Ok(None)` arm has no yield point of its own and relies on an unstated `AsyncUdpSocket::recv_from` contract.
**Detail:** mod.rs:46-52 documents a backoff for the `Err` arm precisely because it "has no `.await` yield point," but `Ok(None)` — covering both `TimedOut` and non-printer packets — gets no pacing. Safe only because every shipped backend happens to yield (`TokioUdpSocket` wraps recv in a 100 ms timeout, `io/tokio.rs:84`; `EspIdfUdpSocket` sleeps 15 ms on `WouldBlock`, `io/esp_idf.rs:132`). Nothing in the trait docs states that requirement, and the unit's own `QuickExitSocket` mock (mod.rs:522) returns `Err(TimedOut)` with no await, so `test_discover_devices_wall_clock_timeout` busy-spins at 100% CPU for its full 300 ms window — a live demonstration of what a conforming-but-non-yielding external impl would do for the whole discovery duration.
**Suggested fix:** Document the yield requirement on the trait, or make the loop self-pacing when a full pass produced no packet.

### src/discovery/mod.rs:235 — filed as #103
**Issue:** A timer error during the initial scan aborts the whole sweep, contradicting the tolerance the surrounding code deliberately establishes.
**Detail:** `timer.sleep(...).await?` is the only `?` left in the discovery path. Every neighbouring failure mode was deliberately made non-fatal — bind failures degrade (mod.rs:195-224), per-engine broadcast failures are swallowed with a comment explaining that `?` "aborted the whole sweep even when a healthy port could still have found printers," and the backoff sleep at 282 uses `let _ =`. A `TimerError` from this one 50 ms inter-burst sleep returns `Err` with both sockets bound and the listen loop never entered.
**Suggested fix:** `let _ = timer.sleep(...).await;` with a `log::debug!`, matching line 282.

### src/discovery/mod.rs:171 — filed as #106
**Issue:** The doc example's 5-second timeout is below what this crate's own reference doc says is required to find a P1S.
**Detail:** `reference/01_network_discovery.md` states the P1S "ignores M-SEARCH on port 2021, where it relies entirely on periodic NOTIFY advertisements at ~10.1-second intervals," and concludes clients "should allow at least 20 seconds of listening time to guarantee capturing one full NOTIFY cycle." Both this rustdoc example and README's `discover_devices::<TokioUdpSocket, _>(Duration::from_secs(5), &timer)` show 5 s. A user copying either gets intermittent empty results on the exact model the reference was captured from.
**Suggested fix:** Change both examples to 20 s and note that NOTIFY-only models need ≥20 s.

## From unit 3 — src/ams/

### src/ams/parser.rs:71 — filed as #117
**Issue:** The AMS-HT branch of `evaluate_spool_presence` never bounds-checks `tray_id`, so a non-zero `tray_id` aliases onto a different HT unit's presence bit.
**Detail:** `shift_ht = 16 + (ams_id - 128) + tray_id`. Because AMS-HT units are single-slot, the formula is unambiguous only when `tray_id == 0`. `evaluate_spool_presence("20000", 128, 1, true)` returns `Some(true)` — bit 17, which is unit **129**'s bit. A caller enumerating a wire `AmsUnit.tray` array uniformly (the natural loop, since `tray` is a `Vec` and nothing forces HT units to report one element) silently reads a neighbouring unit's presence. Variant: if HT firmware reports the tray's global id (`"128"`), `shift_ht = 144` and the `>= 32` guard returns `None`, indistinguishable from the shutdown-exception `None` at line 65.
**Root-cause pattern:** the standard branch *does* guard (`tray_id >= AMS_SLOTS_PER_UNIT` at line 83, with a test at line 420); the HT branch relies only on the overflow guard. Input bounds are enforced per-branch by convention.
**Suggested fix:** Reject `tray_id != 0` in the HT branch, or hoist a single bounds check above both branches.

### src/ams/parser.rs:114 — filed as #130
**Issue:** The AMS-HT state-9 carve-out has no backing in `reference/` and contradicts what the reference doc states.
**Detail:** parser.rs:118–122 gates `AMS_TRAY_STATE_EMPTY` (9) on `!is_ht`, so state 9 on units 128–135 is treated as loaded. `reference/05_materials_ams.md:45` states unconditionally that codes `9`/`0` mean "Empty Slot", with no AMS-HT exception, and its BUG-012 verification block (line 56) says `clean_stale_tray_data` "now clears on both." The behavior came from commit `3ed570f` (closed issue #27), which never updated the reference doc. If the carve-out is wrong, a genuinely empty AMS-HT chamber reports the previous spool's material forever. If it is right, the reference doc is wrong and violates the convention that reference docs get corrected. Note `AMS_TRAY_STATE_POWER_OFF` (0) is *not* gated by `is_ht` even though the reference groups 0 and 9 together.
**Suggested fix:** Add the exception (with its verification source) to `reference/05_materials_ams.md`, or revert the gate if #27's evidence doesn't hold up — and treat codes 0 and 9 consistently either way.

### src/ams/mapping.rs:112 — filed as #119
**Issue:** Nothing rejects `ams_id: 254` in an `ams_mapping2` destined for a single-nozzle printer, which the reference documents as a hard firmware error.
**Detail:** `reference/05_materials_ams.md:200`: the payload "must always send `ams_id = 255` … Transmitting `254` during dispatch commands causes the printer's internal lookup to target physical AMS tray 0 instead of the external spool feed, producing a 'Failed to get AMS mapping table' exception." `MaterialSource::ExternalSpoolLeft` emits `{254, 0}` and is documented as IDEX-only, but nothing enforces that. On a single-nozzle P1S with `[StandardAms{0,0}, ExternalSpoolLeft]`, `is_external_spool_safety_valid(true, …)` returns `true`, so `ProjectFilePayload::from_config` forwards `ams_mapping2` verbatim including `{254, 0}` — producing error `0700_8012`. `PrintJobConfig::with_ams()` sanitizes the flat path against exactly this (closed issue #56), but `with_ams_mapping2()` has no equivalent.
**Suggested fix:** On the single-nozzle path, normalize `254`→`255` (or reject with a `log::warn!`) before emission.

### src/ams/mapping.rs:171 — filed as #118
**Issue:** `build_ams_mapping`/`build_ams_mapping2` size their output from an unbounded caller-supplied `usize` filament id.
**Detail:** `let max_id = allocations.iter().map(|(id, _)| *id).max()` then `vec![-1; max_id]`. A single allocation with `filament_id: usize::MAX` (or any large value from an untrusted slicer project file) aborts on allocation failure — on the `alloc`/`embassy` targets an OOM abort in a fixed heap, not a recoverable error. The guard at line 175 only rejects `id == 0`; the `> max_id` half is dead by construction.
**Suggested fix:** Cap `max_id` at a documented maximum project filament count (16 flat channels, 20 with AMS-HT) and drop out-of-range ids via the existing `log::warn!` path.

## From unit 4 — src/quirks/

### src/quirks/mod.rs:240-247 — regression of closed issue #54 — filed as #68
**Issue:** The `PrinterModel::Unknown` fallback to `X1CQuirks` inherits X1C's voltage-dependent bed ceiling, so an unrecognized printer can be given a **120 °C** bed target — 10 °C above what the fallback's own warning promises, and 40 °C above the real ceiling of several shipping models.
**Detail:** `X1CQuirks::bed_temp_max` (`models/x1.rs:45-53`) returns `X1C_BED_TEMP_MAX_110V = 120` for `Some(false)`. `set_bed_temperature` (`client/thermal.rs:65-70`) derives `mains_220v` from `last_home_flag & POWER_220V_BITMASK`, so as soon as an unknown-model printer publishes a `home_flag` with bit 3 clear (110 V region), the clamp becomes 120 °C. The `log::warn!` at mod.rs:242-245 explicitly says the fallback caps at "110C bed", true only before the first `home_flag` arrives. A new entry-level machine whose serial prefix isn't yet in `SERIAL_PREFIXES` resolves to `Unknown` and would be allowed a bed target far past its real 80–100 °C ceiling. `test_unknown_fallback_quirks` (mod.rs:727-734) never calls `bed_temp_max`. **Note this compounds with the unit-2 finding at `parser.rs:226`, which is a second route to `Unknown`.**
**Root-cause pattern:** the `Unknown` arm reuses a *real model's* strategy struct rather than a purpose-built conservative one, so every future per-model override added to `X1CQuirks` silently becomes the unknown-machine default. The safety intent lives only in a `log::warn!` string nothing verifies against the struct it points at.
**Suggested fix:** Give `Unknown` its own dedicated conservative strategy struct with a flat, voltage-independent `bed_temp_max` (and the smallest travel/nozzle limits across the family).
**Triage note:** if confirmed, this is a candidate for `P-critical` — it is a bed-temperature ceiling above a real hardware limit. Verify the `home_flag`-arrival path before assigning.

### src/quirks/mod.rs:263-289 — mirror of closed issue #55 — filed as #100
**Issue:** `line_has_unsafe_homing` scans for `G28` at every byte offset including offsets inside a comment, so a comment mentioning an axis-constrained `G28` blocks an otherwise legitimate command.
**Detail:** The comment truncation at mod.rs:277 strips the comment portion **after** the matched `G28` — it never checks whether the match itself sits inside a comment that started earlier. `is_unsafe_homing_command("M400 ; then G28 Z manually")` or `"; G28 Z"` finds the in-comment `G28`, computes `rest = " Z manually"`, sees `Z`, and returns `true`, rejecting a payload whose executable G-code is safe. Same for `"M117 G28 Z"`. This is the mirror image of the issue-#55 regression that `test_unsafe_homing_ignores_trailing_comment` (mod.rs:867-875) was written for — that test covers a comment after a *bare* `G28`, never a `G28` after a comment marker. Fails conservatively (over-blocking), but is a user-visible false rejection.
**Suggested fix:** Truncate each line at its first `;`/`(` **before** starting the `G28` scan rather than after the match.

## From unit 5 — src/io/ (core + tokio)

### src/io/tokio/cert_verify.rs:213 — filed as #109
**Issue:** The intermediate-selection predicate doesn't include the validity-window check, so an expired same-subject decoy hard-aborts the walk instead of letting a valid sibling candidate be tried.
**Detail:** Lines 213–221 select the first unused intermediate matching subject *and* signature; validity is only checked afterward at line 225, and a failure there `return`s outright. The comment at 207–212 explicitly designed the `find` loop so a same-subject decoy can't abort the walk — but that reasoning covers only signature mismatch, not expiry. A rotated intermediate pair (old expired + new valid, same subject) presented old-first is rejected with `Expired` even though the valid one is right there. Fail-closed, so availability-only, not a trust hole. Adjacent to closed issue #17.
**Suggested fix:** Fold `c.validity().is_valid_at(now_asn1)` into the `find` predicate and keep the post-selection check only as the no-candidate error path.

### src/io/tokio/cert_verify.rs:330 — filed as #108
**Issue:** A SAN extension that fails to parse is silently downgraded to CN matching by `.ok().flatten()`.
**Detail:** `leaf.subject_alternative_name()` returns `Result<Option<_>>`; `x509-parser` returns `Err` for a duplicate or malformed SAN extension. Line 332's `.ok()` maps that to `None`, indistinguishable from "no SAN present", falling through to `match_subject_cn` at line 354. A cert carrying a real SAN for `otherprinter` plus a malformed second SAN, with `CN=targetserial`, gets identity-matched on CN — the opposite of the SAN-then-CN precedence documented at line 108 and asserted by `test_cn_fallback_verifier_prefers_san_over_mismatched_cn` (tests.rs:221). Reachable only in combination with a validly-chained cert.
**Suggested fix:** Distinguish `Err(_)` (reject with `CertificateError::BadEncoding`) from `Ok(None)` (legitimate CN fallback).

### src/io/tokio.rs:289 — filed as #107
**Issue:** `TokioIo`'s blanket `embedded_io_async::Write` impl forwards Tokio's return value verbatim, so an `Ok(0)` from the inner writer panics inside `write_all`.
**Detail:** `embedded-io-async-0.7.0/src/lib.rs:143` is `Ok(0) => panic!("write() returned Ok(0)")`, and the trait docs say impls "should never return `Ok(0)` when `buf.len() != 0`". `TokioIo<T>` is generic over *any* `tokio::io::AsyncWrite` and nothing at 289–292 upholds that contract. `write_all` is on live network paths (`mqtt/client/mod.rs:232`, `ftps/protocol.rs:132`, `ftps/client.rs:611`, `camera/binary.rs:202`), so a writer returning `Ok(0)` panics the library rather than surfacing an error. Concrete `TcpStream`/`TlsStream` rarely do this, but the blanket impl is a contract hole regardless.
**Suggested fix:** Map `Ok(0)` on a non-empty `buf` to `std::io::ErrorKind::WriteZero`.

## From unit 6 — src/io/ (esp_idf + embassy)

### src/io/esp_idf.rs:123 — `needs-verification` — filed as #131
**Issue:** `EspIdfUdpSocket::send_to` has no `WouldBlock` handling even though the socket is deliberately non-blocking, so a transient lwIP buffer shortage aborts SSDP discovery outright.
**Detail:** `bind` calls `crate::io::configure_std_udp_socket` (esp_idf.rs:110), which ends with `set_nonblocking(true)` (`io/mod.rs:109-111`). `recv_from` handles the resulting `WouldBlock` explicitly (esp_idf.rs:135-140) and returns `TimedOut`, which `poll_next_device` treats as benign (`discovery/mod.rs:153`). `send_to` does not: the error goes to `map_std_io_error`, whose `_` arm (`io/mod.rs:86-89`) turns `WouldBlock` into `SocketError::Other`. `DiscoveryEngine::broadcast_search` (`discovery/mod.rs:101-113`) fires multicast and broadcast M-SEARCH back to back and returns `Err(Error::Network(..))` if both fail — and if the lwIP pbuf pool is momentarily exhausted under Wi-Fi load, the second send is very likely to fail for the same reason as the first. Discovery aborts with an opaque "ESP-IDF platform BSD network error" instead of retrying.
**Root-cause pattern:** `configure_std_udp_socket` is shared with tokio, where non-blocking is mandatory and tokio's own async `send_to` waits for writability — so nothing forces the ESP-IDF wrapper to cover the send side the way it covers the receive side.
**Suggested fix:** Mirror `recv_from` — on `WouldBlock`, sleep `UDP_RECV_POLL_INTERVAL` and retry a bounded number of times, or return `TimedOut` so callers can distinguish "try again" from a terminal fault.
**Verification:** reproducing this needs a flashed board under Wi-Fi load. Per `src/io/CLAUDE.md` and `.claude/rules/wire-framing-hardware-verification.md`, hand off to the user — do not self-verify. The `esp32-hw-probe/` harness is the tool for it.

### src/io/esp_idf.rs:195 — filed as #132
**Issue:** `poll_connect_revents` treats every `rc < 0` from `poll()` as a terminal connect failure, including a retryable `EINTR`.
**Detail:** `let rc = unsafe { ... poll(&mut poll_fd, 1, 0) }` followed by `if rc < 0 { return Err(SocketError::Other(...)) }` (esp_idf.rs:193-199) discards `errno` entirely. If lwIP's `poll` ever returns `-1` with `EINTR`, `poll_connect_until_complete` propagates the error through `EspIdfTcpStream::connect` and `EspIdfRawStreamFactory::dial`, failing a connection that was still progressing — and the caller cannot tell it from a genuine socket fault, since the message is a fixed string with no errno. Low likelihood (ESP-IDF's `poll` on lwIP has few interrupt sources), but the cost is a spurious connect failure at the start of every MQTT/FTPS session. Adjacent to closed issue #64.
**Suggested fix:** Read `std::io::Error::last_os_error()` on the `rc < 0` path; retry on `EINTR` and include the real errno otherwise, matching `map_esp_tls_connect_error`'s existing convention (esp_idf.rs:346).

## From unit 7 — src/mqtt/client/

### src/mqtt/client/frame.rs:145 — filed as #112
**Issue:** `vec![0u8; rem_len]` eagerly allocates up to `MQTT_MAX_PAYLOAD_BYTES` (1 MiB) on firmware-controlled input, with the same constant on every target.
**Detail:** `MQTT_MAX_PAYLOAD_BYTES` is 1 MiB unconditionally (frame.rs:10). On the ESP-IDF and Embassy targets — where `pending.rs:12` itself notes RAM "measured in KB" — a single PUBLISH declaring a 1 MiB remaining length forces a 1 MiB allocation before any payload byte is validated. On a heap that cannot satisfy it, the allocator aborts rather than returning an error. The same constant is load-bearing for `pending.rs:23`'s `const _: () = assert!(...)`, so it cannot be lowered per-target without revisiting that assertion.
**Suggested fix:** `cfg`-gate the constant down on `no_std`/ESP targets (adjusting `MQTT_PENDING_BUFFER_MAX_BYTES` in step), or grow `buf` incrementally.

### src/mqtt/client/codec.rs:28 — filed as #110
**Issue:** `encode_remaining_length` has no upper bound — a length above 268,435,455 silently emits a 5-byte varint, which is a malformed MQTT frame.
**Detail:** The decoder (frame.rs:128) correctly rejects varints longer than 4 bytes, but the encoder has no matching guard and `publish_command` applies no size check to the caller's payload — asymmetric with the read path's `MQTT_MAX_PAYLOAD_BYTES`. `publish_command` is public and the README advertises sending raw payloads through it, so an oversized payload writes a malformed frame and desyncs the broker instead of returning an error. The `debug_assert!`s at codec.rs:51-53/88/109 show the same concern was recognized for the u16 string-length fields but not extended here.
**Suggested fix:** Reject payloads whose remaining length exceeds 268,435,455 (or `MQTT_MAX_PAYLOAD_BYTES`) in `encode_publish_qos1`/`publish_command`.

### src/mqtt/client/mod.rs:417 — filed as #111
**Issue:** SUBACK validation rejects only the exact value `0x80`; any other non-granted return code is accepted as success.
**Detail:** MQTT 3.1.1 defines exactly four valid SUBACK return codes (0x00/0x01/0x02 granted, 0x80 failure). A broker returning anything else — firmware bug, or a byte read at the wrong offset because the SUBACK carried multiple topic results — passes the check, and `connect()` returns a client subscribed to nothing, surfacing later as a silent "no telemetry ever arrives" hang rather than a connect-time error. `connect()` also never checks that the SUBACK echoes packet id 1.
**Suggested fix:** Accept only `0x00..=0x02`, reject the rest with `ProtocolViolation`, and optionally validate the echoed packet id.

### src/mqtt/client/codec.rs:25 — filed as #113
**Issue:** The CONNECT packet advertises a 30-second keepalive that the library itself never honors — only `bambino-cli` sends PINGREQ.
**Detail:** `MQTT_KEEP_ALIVE_SECS = 30` is written into every CONNECT (codec.rs:65), obligating the client to send traffic within 1.5× that (45s) or be disconnected. Nothing in `MqttClient` or `PrinterClient` sends a periodic PINGREQ; the only caller that does is `src/bin/bambino-cli/monitor/mod.rs:165`, on its own timer. A library consumer following the README's "connect, then `poll_telemetry()` in a loop" pattern sends zero client→broker bytes and gets dropped by the printer's broker after ~45s. `MQTT_STALE_CONNECTION_SECS` (60s) is longer than that window, so the disconnect surfaces as an I/O error before the staleness check can explain it.
**Suggested fix:** Either drive pings from `PrinterClient` (e.g. inside `poll_telemetry_with_timer` when `secs_since_last_message` crosses a threshold) or advertise keepalive 0 and document that pings are the caller's job.

## From unit 8 — src/mqtt/commands/

### src/mqtt/commands/print_job.rs:319 — filed as #120
**Issue:** Flat-channel sanitization lives only in the `with_ams` builder, but `PrintJobConfig.ams_mapping` is a `pub` field, so unsanitized values reach the wire.
**Detail:** `with_ams` (lines 132-148) folds out-of-range flat channel ids (notably 254/255) to `-1`, per closed issue #56 and `reference/05_materials_ams.md:151` (error `0700_8012`). `from_config` line 319 then does `config.ams_mapping.clone()` with no re-validation. Because every field is public and the struct is not `#[non_exhaustive]`, `config.ams_mapping = vec![255]; config.use_ams = true;` reproduces exactly the firmware error the sanitizer was added to prevent. Same convention-vs-type-system pattern that produced closed issue #59 and that `ClampedTaskId` closed. The `ams_mapping2` path is unaffected — line 318 always routes through `flat_channel_id_for_entry`.
**Suggested fix:** Move the sanitizing `map` out of `with_ams` into `from_config`'s `flat_mapping` derivation so both paths are sanitized at serialization time, or make `ams_mapping` private behind the builder.

### src/mqtt/commands/print_job.rs:355 — `needs-verification` — filed as #133
**Issue:** `vibration_cali` is serialized unconditionally with no model gate, unlike upstream, which force-disables it on P2S/N7.
**Detail:** Line 355 sends `config.run_vibration_compensation.as_wire_bool()` for every model. bambuddy's `start_print` overrides it after building the payload (`bambu_mqtt.py:5092`: `if self.model … in ("P2S", "N7"): command["print"]["vibration_cali"] = False`, commented "P2S printer doesn't support vibration calibration like X1/P1 series"). There is no corresponding quirk in `src/quirks/`, and `reference/` does not document the P2S exception.
**Suggested fix:** Verify against BambuStudio/bambuddy, then add a `supports_vibration_compensation()` quirk consulted here, and record the P2S/N7 exception in `reference/03_mqtt_telemetry.md`.

### src/mqtt/commands/print_job.rs:215 — `needs-verification` — filed as #134
**Issue:** `ProjectFilePayload` omits three fields present in upstream's current `project_file` payload: `md5`, `cfg`, and `extrude_cali_manual_mode`.
**Detail:** bambuddy sends `"md5": ""` (line 5052), `"cfg": "0"` (line 5063), and `"extrude_cali_manual_mode": 0` (line 5073) alongside the fields bambino already has. `reference/03_mqtt_telemetry.md:305-332` does not list them, so reference and code agree with each other but not with the upstream capture — the situation root CLAUDE.md's "external source contradicts the reference doc" rule covers. Low impact: these look like inert defaults, and the four fields that mattered were already added. No evidence firmware requires them.
**Suggested fix:** Confirm against a BambuStudio wire capture; if firmware tolerates their absence, note the deliberate omission in `reference/03_mqtt_telemetry.md` so a future sweep doesn't re-raise it.

## From unit 9 — src/ftps/ (client + parser)

### src/ftps/client.rs:512-521 — filed as #121
**Issue:** `list_directory` tolerates a `426` transfer-confirmation reply (the documented P2S/X2D TLS 1.3 close-race) but, unlike `upload_file`/`download_file`, has no compensating integrity check for the listing it just read.
**Detail:** Both `upload_file` and `download_file` follow their 226/426 tolerance with an independent `SIZE` recheck that catches a truncated transfer — and `src/ftps/CLAUDE.md` leans on exactly those two checks to justify the fail-open `allow_unverified_tls_1_2` opt-out. `list_directory` has no equivalent: a data channel closing early (premature `426`) yields a listing payload silently truncated mid-line, and `parse_unix_listing` treats the truncated tail as just another malformed line and drops it via `continue`. The caller gets a shorter-than-actual file list with no error and no signal anything was missed. Closed issue **#15** added mock coverage for the `426` tolerance itself; the missing compensating check is a separate gap.
**Suggested fix:** Either restrict the `426` tolerance for LIST to cases where a clean close can be established, or add a completeness signal — or, if the asymmetry is accepted deliberately, document it next to the tolerance and in `src/ftps/CLAUDE.md` so the opt-out's justification isn't read as covering LIST too.

### src/ftps/parser.rs:123-283 — filed as #122
**Issue:** `parse_unix_listing` allocates one `FtpFile` (with a heap `String` name) per parsed line, with no upper bound on entry count.
**Detail:** The raw payload is bounded at `FTPS_MAX_TRANSFER_BYTES` (512 MiB, `protocol.rs:54`) before reaching this function — and that constant's own doc says it exists to avoid the uncatchable `alloc_error_handler` abort on no_std/Embassy targets. Nothing bounds the *parsed* `Vec<FtpFile>`: a corrupted or adversarial `LIST` response of millions of short lines (~20 bytes each) within the 512 MiB cap yields tens of millions of `FtpFile` entries, each with its own heap `String` — a larger and far more fragmented footprint than the raw buffer. Same risk class the byte cap was introduced to prevent, one level downstream of where the cap is enforced.
**Suggested fix:** Cap the parsed entry count (mirroring `FTP_MAX_RESPONSE_LINES` in `protocol.rs`), or document in this function that entry count is only indirectly bounded by the byte cap.

## From unit 13 — src/client/ (operations)

### src/client/ams.rs:99 — filed as #101
**Issue:** `change_filament`'s `pair_valid` check only prevents `slot_id == 254` when `ams_id < 16`; it does not distinguish the AMS-HT physical bus range (`128..=135`) from the true external-spool sentinels (`254`/`255`), both of which satisfy `ams_id >= 16`.
**Detail:** `pair_valid = slot_id != 254 || ams_id >= 16` passes for e.g. `(ams_id: 130, slot_id: 254)` — a real AMS-HT bus unit paired with the single-nozzle external-spool-load sentinel, which `change_filament`'s own doc describes only for `ams_id: 254`/`255`. Target derivation then computes `target = ams_id` (130) at ams.rs:105-109, sending an `ams_change_filament` command shaped for external-spool addressing to a physical AMS-HT unit. The doc comment explicitly warns that a target mismatched from BambuStudio's derivation is a real hardware-misconfiguration risk (error class `07FF_8012`). Not covered by `tests/client_negative_test.rs`, which tests only `(1,2)`, `(255,254)`, and an invalid `ams_id` of 99. **Sibling of closed issue #9** ("`change_filament` validates `ams_id` and `slot_id` independently, allowing nonsensical combinations") — that fix added the `pair_valid` check; this is a remaining hole in it.
**Suggested fix:** Restrict the `slot_id == 254` pairing to the actual sentinel range (`ams_id == 254 || ams_id == 255`) rather than the broader `ams_id >= 16` — or document why AMS-HT plus slot 254 is a meaningful combination if it in fact is.

## From unit 18 — tests/client_{reconnect,session,version,gcode}_test.rs

### tests/client_reconnect_test.rs:156 — filed as #129
**Issue:** `test_disconnect_storage_clears_ftps_for_clean_reconnect` never actually poisons the FTPS client before calling `disconnect_storage()`, so it doesn't pin the `ftps-poisoning.md` invariant it is named for.
**Detail:** The test does a normal successful `storage()` connect, then `disconnect_storage()`, then asserts the next `storage()` call fails with `ProtocolViolation` ("not configured") — which really just confirms `ftps_config` was already consumed by the first connect (a separate, already-tested invariant), not that `disconnect_storage()` recovers a client poisoned by a control-channel desync. **Broken code that reset `self.ftps` to `None` only on the ordinary-disconnect path and not on the poisoned path would still pass this test.** No test in this file or elsewhere in `tests/` drives a `PrinterClient`-level FTPS session to poisoned, calls `disconnect_storage()`, and verifies a fresh connect succeeds afterward. The poisoning mechanism itself is thoroughly tested at the `BambuFtpsClient` level in `tests/ftps_test.rs` — just not through this `PrinterClient`-level recovery path, which is the one the rule actually documents.
**Suggested fix:** Extend the test to first force a control-channel desync (reusing `tests/common/mock_ftps.rs`'s poisoning-regression server) to poison `self.ftps`, then call `disconnect_storage()`, then construct a fresh `PrinterClient` with new FTPS config and confirm `storage()` connects — closing the loop the test's name implies.

## From unit 16 — src/types/telemetry/tests/

### src/types/telemetry/tests/bed.rs:88-97, misc.rs:291-308, nozzle.rs:278-298 — filed as #128
**Issue:** No regression test proves `unpack_temperature()`'s composite decoding is never applied to the flat, never-composite-packed `print.bed_temper`/`bed_target_temper`/`nozzle_temper`/`nozzle_target_temper` fields.
**Detail:** `src/types/telemetry/CLAUDE.md` calls out this exact gap. Every test exercising these old-gen flat fields uses values ≤ 500 (`55.5`/`60.0` in `bed.rs:91-92`; `27.625`/`29.46875` and `100`/`40` in `misc.rs:293,301`; `210.0`/`220.0` in `nozzle.rs:287-288,311-312,334-335`). None uses a value **> 500** — the composite threshold — to confirm the raw value passes straight through the old-gen fallback branches (`mod.rs:112-114`, `158-159`) without being run through `unpack_temperature()`. The current code is correct (those branches do a direct `as u16` cast with no unpack call), but a future change routing these fields through the composite unpacker — e.g. someone "fixing" what looks like an inconsistency with `chamber_temper`/`ExtruderInfo.temp` — would produce wildly wrong temperatures (an old-gen bed reporting `600.0` would decode to actual=0/target=0 instead of 600) with no test catching it.
**Suggested fix:** Add a case to `test_bed_temperatures_old_gen_direct` and the nozzle fallback tests using a value > 500 (e.g. `600.0`) and assert the decoded value is `600`.

## From unit 10 — src/ftps/ (protocol + mock)

### src/ftps/protocol.rs:166 — filed as #123
**Issue:** The `.claude/rules/wire-read-deadline.md` correctness hinge — "on timeout, bytes already read for the in-progress frame must not be lost" — is *not* mirrored here; the FTPS path is safe only by the separate poisoning convention.
**Detail:** The `has_real_clock()` half of the rule is faithfully mirrored (protocol.rs:79 and 430). The partial-progress half is not: `read_line_raw` line 184 does `line_buf.append(fill_buf)`, which *moves* leftover bytes out of the persistent `fill_buf` into the per-call scratch `line_buf`. If the subsequent `read_chunk` (line 193) returns `TimedOut`, those bytes are unrecoverable — `line_buf` is a fresh `Vec::new()` per call (`client.rs:382`) and is `clear()`ed at line 173 anyway. Today this cannot desync the parser, because `read_response_poisoning` (`client.rs:381-398`) sets `poisoned = true` on any error and `.claude/rules/ftps-poisoning.md` forbids un-poisoning. But that safety is enforced entirely by the convention that every timeout is fatal, in a different file, while MQTT's sibling path solves the same problem structurally with `FrameReadState`. A future change making a control-channel timeout retryable would silently desync the reply stream.
**Suggested fix:** Either drain into `line_buf` only on success, or add an explicit note to `read_line_raw`'s doc and to `.claude/rules/wire-read-deadline.md` recording that FTPS deliberately substitutes poisoning for partial-progress persistence.

### src/ftps/protocol/tests.rs:471 — filed as #126
**Issue:** No test covers `\r`, `\n`, or NUL rejection — the exact three bytes `src/ftps/CLAUDE.md` names as the reason `validate_ftp_path` exists.
**Detail:** `test_validate_ftp_path_rejects_non_crlf_control_chars` deliberately tests `\x01` and `\x7f`; traversal and leading-dash have their own tests. The CR/LF/NUL command-injection case is covered only incidentally by the `b < FTP_PATH_CONTROL_CHAR_MAX` predicate. If someone narrowed that predicate to an allow-list (a plausible refactor, since the current rule also rejects tab and is stricter than any FTP spec requires), the injection guard could be weakened with the suite still green. The rejection *set* itself was verified complete — the gap is coverage, not logic.
**Suggested fix:** Add `assert!(validate_ftp_path("/model/a\r\nDELE /model/b").is_err())` plus `\n`-only and `\0` cases.

### src/ftps/protocol.rs:430 — filed as #124
**Issue:** `read_to_eof` reimplements `ftps_deadline_ms` inline instead of calling it — the `has_real_clock()` invariant is held by copy, not by construction.
**Detail:** Lines 430–434 are a verbatim re-derivation of `ftps_deadline_ms` (lines 78–84), differing only in taking milliseconds rather than seconds. `ftps_deadline_ms`'s doc positions it as *the* place this gating decision lives, and the rule file names that gate as what keeps `DummyTimer` clients working. Two copies means a future change to the policy can be applied to one and not the other, and only the data-channel copy is exercised by `test_read_to_eof_stalled_connection_times_out`. Textbook convention-across-call-sites pattern.
**Suggested fix:** Add an `ftps_deadline_from_ms(timer, budget_ms)` sibling and call it from `read_to_eof`.

### src/ftps/protocol.rs:342 — filed as #125
**Issue:** `parse_pasv_port` rejects a `227` reply whose tuple has whitespace after the commas.
**Detail:** `inner.split(',')` yields `" 192"` for `(127, 0, 0, 1, 192, 168)`, and `" 192".parse::<u16>()` fails (Rust's integer parser does not skip leading whitespace), so the connection fails with `ProtocolViolation("Failed to parse PORT_1 in PASV")`. RFC 959 does not permit the spaces and Bambu firmware is not known to emit them, so this is a robustness gap — but the surrounding parser is otherwise deliberately lenient (it accepts `p1 > 255`, catching only the final range overflow), so the strictness is inconsistent with its own posture. `tests.rs:376-425` has no whitespace case.
**Suggested fix:** `.map(str::trim)` before parsing each component.

### tests/common/mock_ftps.rs:33 — filed as #127
**Issue:** The mock never emits a multi-line reply or a reply split across socket reads, so `read_response`'s hardest paths have no integration-level coverage; the one case it does cover (150/226 coalescing) depends on tokio scheduler ordering rather than being forced.
**Detail:** Every `respond()` writes one complete single-line reply, only after the corresponding command has been read. So no integration test exercises a `220-`-style multi-line greeting (real vsftpd-family daemons, which the mock imitates by banner, routinely send one), a reply arriving in two socket reads, or the `FTP_MAX_RESPONSE_LINES`/`FTP_MAX_RESPONSE_LINE_BYTES` caps. `read_response`'s doc (protocol.rs:220-223) credits `tests/ftps_test.rs::test_ftps_download_file` with catching the `fill_buf`-scoping desync, but that test only reproduces the coalescing because the mock's back-to-back `150`→data→`226` writes happen to complete without yielding to the client task — nothing *forces* both replies into one read, so the same regression could go undetected under a different runtime or a small reordering. The deterministic coverage lives in the `ChunkedReader` unit tests (`protocol/tests.rs:81-226`), which is fine, but the doc comment overstates what the mock guarantees.
**Suggested fix:** Add a mock variant writing `150 …\r\n226 …\r\n` in a single `write_all`, and one sending a multi-line `220-`/`220 ` greeting.
