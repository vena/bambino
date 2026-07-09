# bambino — Full Module Review (2026-07-09)

Deep, module-by-module review of the `bambino` crate (async Rust library for controlling Bambu Lab 3D printers over LAN — tokio/desktop, ESP-IDF, and Embassy/bare-metal targets from one codebase). Conducted by 12 parallel review agents, each scoped to one module area, each starting from a full read of `CLAUDE.md` + `README.md` (and cross-checking the freshly-regenerated `docs/` API reference where relevant) before reviewing source. Minor/LAN-only security issues were explicitly out of scope per this crate's design (it's LAN-only by intent — see `README.md`'s Safety Notice). Style/naming/refactor suggestions were also out of scope; only correctness bugs and doc/behavior inconsistencies are listed below.

**This file is meant to be consumed by a fresh session with no memory of the review conversation.** Each finding below is self-contained: file, line, what's wrong, concrete failure scenario, suggested fix. Verify current line numbers before editing — the review was done against the `main` branch as of this date and line numbers may have drifted if other changes landed first.

Modules reviewed with **no issues found**: `src/client/{ams,camera,hardware,motion,print,storage,telemetry,thermal}.rs` (client domain methods), `src/quirks/` (all model strategy structs), `src/types/` (telemetry bit-packing/decoding), `src/diagnostics/hms.rs`, `src/bin/bambino-cli/` (entire CLI binary). These do not need further action.

---

## 0. FIRST ACTION: wire a local pre-commit hook running `make check-fast`

**Do this before touching any of the findings below.** `make check-fast` (build, test, both no_std/embassy feature-gate checks, clippy) already exists and is run manually before landing changes, but nothing enforces it — it's opt-in discipline, not a gate. There is no CI in this repo (no GitHub remote yet; `.github/workflows/` is dormant). A local git hook is the only available backstop right now and costs nothing to add.

**Why this is first, not last:** every other recommendation below (generalizing fixes, a findings backlog, a release bar) only pays off if regressions stop slipping in silently between review passes. Without the hook, the next feature session can reintroduce exactly the kind of inconsistency this sweep found (e.g. a new command constructor that forgets `clamp_task_id()`) and nothing will catch it until the next full sweep. With the hook, `make check-fast` at least catches build/test/clippy-level regressions at commit time, same session.

**What it won't catch:** none of today's findings (BUG-001 through BUG-013, see `BACKLOG.md`) would have been caught by `make check-fast` alone — they're all logic/consistency bugs, not build/test/clippy failures (existing tests pass; the bugs are in cases the tests don't cover). The hook is necessary but not sufficient — see the "generalizing fixes" recommendation in the accompanying discussion for how to close that gap for the specific bug classes found today.

**Suggested implementation:** `.git/hooks/pre-commit` (not tracked by git — add a checked-in `scripts/install-hooks.sh` or a `pre-commit`/`husky`-style setup step so it survives a fresh clone) running `make check-fast` and rejecting the commit on non-zero exit. Keep it fast enough not to be annoying — if `make check-fast` is too slow for every commit, consider a lighter `pre-commit` (fmt + clippy only) and reserve the full `check-fast` for `pre-push`.

---

## 1. `src/mqtt/commands/` + `src/diagnostics/kprofile.rs` — unclamped sequence IDs (real bug, public-API-facing)

**Files/lines:**
- `src/diagnostics/kprofile.rs:110` — `ExtrusionCaliGetRequest::new`
- `src/diagnostics/kprofile.rs:183` — `ExtrusionCaliSetRequest::new`
- `src/diagnostics/kprofile.rs:255` — `ExtrusionCaliSelRequest::new`
- `src/diagnostics/kprofile.rs:323` — `StandardCaliDelRequest::new`
- `src/diagnostics/kprofile.rs:354` — `IdexCaliDelRequest::new`
- `src/mqtt/commands/print_job.rs:229` — `ProjectFileRequest::from_config`

**Issue:** These six command constructors serialize `sequence_id` as `sequence_id.to_string()` without calling `clamp_task_id()` first. `.claude/rules/task-id-clamping.md` states as a hard invariant: "All MQTT sequence IDs and task IDs must be clamped to 32-bit signed integer max (`TASK_ID_MAX`). Use `clamp_task_id()` for task IDs." Every other command builder in `src/mqtt/commands/` (`ams.rs`, `control.rs`, `gcode.rs`, `hardware.rs`, `status.rs` — 13 constructors total) calls `clamp_task_id(sequence_id).to_string()`. These six don't. Notably, in `print_job.rs:229`, the adjacent `subtask_id` field in the *same struct literal* **is** correctly clamped (`clamp_task_id(config.raw_subtask_id)`), making the inconsistency visible in one literal.

**Currently masked at runtime via `PrinterClient`:** `PrinterClient::next_sequence_id()` (`src/client/mod.rs:311-315`) already clamps before passing `seq` into `dispatch()`, so calls routed through `PrinterClient::select_k_profile`, `get_k_profiles`, `start_print`, etc. never see a raw epoch value here. **But** these six structs and their constructors are `pub`, part of the crate's public API. A direct caller passing a raw `u64` epoch-millisecond sequence ID — exactly the scenario `src/mqtt/commands/mod.rs`'s existing regression test `test_command_constructor_clamps_unclamped_sequence_id` was written to guard against (it only covers `GCodeRequest`, not these six) — would reproduce the documented 32-bit-overflow firmware lockup. A print-job start is arguably the highest-impact command in the crate to get this wrong on.

**Fix:** Add `use crate::mqtt::commands::clamp_task_id;` to `src/diagnostics/kprofile.rs` and wrap the five `sequence_id: sequence_id.to_string()` sites with `clamp_task_id(sequence_id).to_string()`. In `print_job.rs:229`, change to `clamp_task_id(sequence_id).to_string()` (the `use super::clamp_task_id;` import is already present in that file). Extend `test_command_constructor_clamps_unclamped_sequence_id`-style coverage to all six constructors.

---

## 2. `src/mqtt/client/mod.rs` — stale doc comment on `get_in_flight_count`

**File/line:** `src/mqtt/client/mod.rs:501-503`

**Issue:** Doc comment says "Returns a slice containing current un-acknowledged QoS 1 packet identifiers," but the function is `pub fn get_in_flight_count(&self) -> usize { self.in_flight.len() }` — returns a count, not a slice of IDs. This wrong doc comment is published verbatim into the generated API docs (`docs/mqtt/client/index.md`), so anyone reading generated docs (their whole purpose) expects a list and gets a count. Likely stale from before the field was reduced to just `.len()`.

**Fix:** Change doc comment to "Returns the number of current un-acknowledged QoS 1 packets."

(Rest of `src/mqtt/client/{mod,codec,frame,pending}.rs` — resumable frame-read state machine, deadline math, pending-buffer bounds, zombie detection — reviewed in depth and found correct/consistent with documented design. No other issues.)

---

## 3. `src/ftps/client.rs` — `download_file` missing integrity recheck

**File/lines:** `src/ftps/client.rs:600-669` (`download_file`)

**Issue:** `upload_file` (client.rs:520-598) has a documented, mandatory post-transfer `SIZE` recheck specifically because a clean `226`/`426` control-channel reply alone doesn't prove the transfer completed intact (guards against silent SD-card write truncation on every model, not just the P2S/X2D TLS 1.3 close-race class). `download_file` has no symmetric check: it trusts EOF-based completion from `read_transfer_chunk` plus a bare `code == FTP_TRANSFER_COMPLETE` (226) check, with no comparison against expected file length. A data channel that closes early while the server still emits `226` (same firmware bug class already documented for P2S/X2D, or any other early-close condition) causes `download_file` to return `Ok(truncated_bytes)` — a corrupted/incomplete download silently reported as success. No existing test exercises this path.

**Fix:** Add a `SIZE`-based (or length-comparison) recheck to `download_file` mirroring `upload_file`'s, via `get_file_size` compared against `file_payload.len()`. Error (don't poison — control channel was read cleanly) on mismatch.

## 3b. `src/ftps/client.rs` — poisoning coverage gap on single-reply commands

**File/lines:** `get_file_size` (454-481), `delete_file` (483-518), `create_directory` (672-699), `remove_directory` (701-729), `rename_file` (731-774), `get_available_space` (776-800), `negotiate_passive_port`

**Issue:** The documented poisoning mechanism only covers `list_directory`/`upload_file`/`download_file`'s "150/125 → 226" window. But these single-reply commands share the identical desync hazard: if `read_response` times out (`FTPS_READ_TIMEOUT_SECS` = 30s) before the server's reply arrives, a *later* unrelated command's `read_response` call (shared `control_fill_buf`) can consume the stale reply. None of these methods poison on a `read_response`/`write_command` error. `delete_file`/`create_directory`/`remove_directory` mutate the SD card filesystem — exactly the class of operation this codebase already acknowledges can have unexpected latency (`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` = 300s exists for "microSD write latency exceptions" on transfer methods), yet these get only the ordinary 30s budget with no poisoning fallback. Lower probability than the `download_file` finding (metadata ops are typically fast), but a real coverage gap, not a documented scope choice.

**Fix:** Either poison on any `read_response`/`write_command` error for these methods too (matches existing "no un-poisoning" philosophy), or explicitly document in the `BambuFtpsClient` struct doc comment why single-reply commands are considered safe to leave un-poisoned.

(Rest of `src/ftps/` — `write_command` single-write-call invariant, `validate_ftp_path`, `parse_unix_listing` filtering, TLS-1.2 fail-closed enforcement, `parse_pasv_port` — verified correct and unchanged from documented/tested behavior.)

---

## 4. `src/camera/rtsps.rs` — IPv6 addresses not bracketed in RTSPS URLs

**File/lines:** `build_rtsps_url` (71-86), `rewrite_rtsp_request_uri` (121-135)

**Issue:** Both functions validate `ip`/`printer_ip` via `IpAddr::parse()`, accepting IPv4 and IPv6, and their doc comments explicitly promise IPv6 support. But the URL is built via plain `format!("rtsps://bblp:{}@{}:322/...", access_code, ip)` — for an IPv6 literal like `fe80::1` this produces `rtsps://bblp:12345678@fe80::1:322/...`. Per RFC 3986 §3.2.2, an IPv6 literal used as a URI host must be bracketed (`[fe80::1]`); without it, the address's colons are indistinguishable from the port separator to any conforming URI parser. The existing test `test_build_rtsps_url_accepts_ipv6` locks in the malformed output as "expected" rather than catching this.

**Fix:** Check `IpAddr::is_ipv6()` and wrap in brackets (`format!("[{ip}]")`) before substituting into both URL-building functions. Update the IPv6 tests.

## 4b. `src/camera/binary.rs` — `build_handshake_packet` accepts empty access code

**File/line:** `build_handshake_packet`, ~54-81 (alphanumeric check at line 66)

**Issue:** `!access_code.chars().all(|c| c.is_ascii_alphanumeric())` vacuously passes for an empty string (`.all()` on empty iterator = `true`), so no error is returned — the function silently builds a handshake packet with a zero-length password field. `rtsps.rs`'s `build_rtsps_url` has an explicit `access_code.is_empty() ||` guard for exactly this reason (documented rationale: catches copy-paste mistakes early). `build_handshake_packet` guards the credential that actually authenticates the camera session over the wire, yet lacks the equivalent check — per this file's own documented handshake limitation, an empty/wrong code only surfaces later as an ambiguous `ConnectionReset` on the next `read_next_frame()` call, indistinguishable from a network blip.

**Fix:** Add `if access_code.is_empty() { return Err(BambuError::ProtocolViolation("access_code must not be empty".into())); }` alongside the alphanumeric check. Add a regression test mirroring `test_build_rtsps_url_rejects_empty_access_code`.

(Rest of `src/camera/` — resumable `CameraFrameReadState` machine, frame-size cap ordering, single-implementation `read_next_frame`/`DummyTimer` delegation invariant, `RtpTimestampCorrector` — verified correct.)

---

## 5. `src/client/connect.rs` — `with_connect_timeout(0)` causes immediate spurious timeout

**File/lines:** `race_against_connect_timeout` (20-35), `with_connect_timeout` (180-184)

**Issue:** No special-case for `connect_timeout_secs == 0`. `timer.sleep(Duration::from_secs(0))` resolves effectively instantly and is raced against the dial+TLS+handshake future, which essentially never completes synchronously on its first poll — so the sleep branch (`Raced::Right`) wins nearly every time and every connect attempt fails with `SocketError::TimedOut`. This is a real footgun because the *sibling* field `command_timeout_secs` has the opposite, documented convention: `set_command_timeout`'s doc comment explicitly says "Passing `0` disables the wall-clock timeout entirely." A caller assuming `with_connect_timeout(0)` follows the same "0 = disabled" convention gets total connection failure instead — with `ensure_ftps()`/`ensure_camera()` having already consumed `ftps_config`/`camera_config` via `.take()` by that point, meaning per the documented (now `.claude/rules/client-builder-api.md`) behavior a fresh `PrinterClient` is required to retry.

**Fix:** Either special-case `connect_timeout_secs == 0` in `race_against_connect_timeout` to skip the race (mirroring `poll_until`'s `if timeout_ms > 0` guard and `set_command_timeout`'s "0 disables" semantics), or explicitly document on `with_connect_timeout` that `0` means "always fail immediately" so the inconsistency with `command_timeout_secs` is at least intentional and visible.

(Rest of `src/client/{mod,connect,dummy,types}.rs` — `ensure_camera()` model-check-before-configured-check ordering, `PreConnected::dial` unreachability, consuming-vs-non-consuming builder correctness, serial-not-ip SNI argument order through MQTT/FTPS/camera — all verified correct.)

---

## 6. `src/io/tokio.rs` — `CnFallbackServerVerifier` ignores intermediate certs (single-hop chain only)

**File/line:** `CnFallbackServerVerifier::verify_server_cert`, ~257

**Issue:** The `_intermediates: &[CertificateDer<'_>]` parameter is never referenced in the body. Chain validation only checks `leaf.issuer().as_raw() != root.subject().as_raw()` against `self.trusted_roots` — a single hop. If a caller sets up a two-level custom CA (offline root + issuing intermediate — a common PKI pattern) and passes the root to `build_verified_client_config`, while the printer's leaf is actually signed by the intermediate (presented in the handshake's intermediate-cert list), validation incorrectly fails with `CertificateError::UnknownIssuer` even though the chain is legitimate. Not called out anywhere as a deliberate scope limitation (unlike the documented P2S/X2D TLS-1.2 gaps), suggesting it's an overlooked edge case.

**Practical impact:** narrow — doesn't affect the default self-signed-cert path or `build_unsafe_client_config()`, only `build_verified_client_config()` with a multi-level custom CA hierarchy.

**Fix:** Either walk the full chain (leaf → intermediates → trusted root, validating each signature link) using `_intermediates`, or explicitly document that only single-hop chains are supported.

(Rest of `src/io/` — `race()`/`read_chunk()` deadline math, `TlsConnector::connect` signature consistency across tokio/ESP-IDF/Embassy, `SocketError::Other` as `Cow`, `has_real_clock()`, `negotiated_version()` per-backend honesty, ESP-IDF non-blocking polling — all verified correct. `cargo clippy` clean under `src/io/`.)

---

## 7. `src/discovery/mod.rs` — degraded-mode SSDP bind is order-dependent (not "try all ports")

**File/lines:** `discover_devices`, bind loop ~176-190

**Issue:**
```rust
let ports: &[u16] = &[SSDP_PORT, SSDP_PORT_ALT];   // [2021, 1990]
for &port in ports {
    match U::bind(bind_addr).await {
        Ok(socket) => engines.push(...),
        Err(e) => {
            if engines.is_empty() {
                return Err(BambuError::NetworkError(e));
            }
        }
    }
}
```
If binding the *first* port (2021) fails, `engines.is_empty()` is still true, so the function returns `Err` immediately — 1990 is never attempted. Degraded mode only actually works when the *second* port fails after the first succeeded, not "try all ports, fail only if none bind" as the surrounding comments/logs imply. Concrete failure: another process holds UDP port 2021 (the exact scenario cited in this function's own doc comment) — even though 1990 is free, `discover_devices()` fails outright. The existing test `test_discover_devices_succeeds_in_degraded_mode_when_one_port_fails_to_bind` only exercises the second-port-fails case, which is why this wasn't caught.

**Fix:** Don't return inside the loop — attempt binding all ports, track the last error, only return `Err` after the loop if `engines.is_empty()`.

## 7b. `src/discovery/mod.rs` — initial broadcast loop aborts sweep on single-engine send failure

**File/lines:** `discover_devices`, initial scan loop ~203-207 vs. periodic re-broadcast loop ~30 lines later

**Issue:**
```rust
for i in 0..2 {
    for (engine, _) in &engines {
        engine.broadcast_search().await?;      // propagates error
    }
    timer.sleep(...).await?;
}
...
for (engine, port) in &engines {
    let _ = engine.broadcast_search().await;   // ignored, later in same fn
}
```
If two SSDP ports are bound and one engine's send fails (plausible transiently — e.g. no route on one interface) while the other is healthy, the `?` in the initial scan loop aborts `discover_devices()` entirely before the listen loop is ever reached — even though the healthy port could have found printers. Directly contradicts the degraded-mode design used everywhere else in this function (bind loop above, periodic re-broadcast loop below both tolerate per-port failure).

**Fix:** Change `engine.broadcast_search().await?;` in the initial loop to `let _ = engine.broadcast_search().await;`, matching the periodic loop's pattern.

## 7c. `src/discovery/parser.rs` — SSDP-discovered serial not uppercased despite doc promise

**File/lines:** `SsdpDevice.serial` doc comment (~18-20), `parse_ssdp_payload` (~203-211)

**Issue:** The `serial` field's doc comment says it is "the unique **uppercase** physical hardware serial number," but `parse_ssdp_payload` passes the raw `USN` header value through unmodified — no `.to_ascii_uppercase()` anywhere in the crate for this field. `reference/01_network_discovery.md` §1.6 ("Case-Sensitive Serial Routing") explicitly documents this as a real hazard: SSDP `USN` casing varies by firmware compile target, but the local MQTT broker routes strictly by exact casing as printed on the physical label — wrong casing gives an accepted subscription with zero telemetry. Per `.claude/rules/tls-identity-sni.md`, `serial` is also now used as the TLS SNI/identity string, so a mixed-case discovered serial could cause a TLS identity mismatch too. The doc comment's promise is currently false.

**Fix:** Uppercase the parsed serial in `parse_ssdp_payload` (`serial.to_ascii_uppercase()`) to make the doc comment true — safer than just fixing the doc, given the reference doc's explicit warning about downstream MQTT/TLS routing.

---

## 8. `src/ams/parser.rs` — possible doc/code mismatch on state-10 tray clearing (needs hardware verification, not a confirmed bug)

**File/lines:** `clean_stale_tray_data`, ~112-116, vs. `reference/05_materials_ams.md:45`

**Issue:** The reference doc says a transition to state `9` (empty) **or `10`** (present but retracted) must be treated as an explicit clearing signal. The code's `is_absent_state` only matches `{9, 0, None}` — state `10` alone (with populated `tray_type`) is deliberately **not** cleared, and this is locked in by an existing test (`test_clean_stale_tray_data_state_10_with_type`) that asserts the field stays populated.

This is flagged with low confidence — physically, state `10` means the spool is still present just not fed, so keeping its material properties may well be the *correct* behavior, and the reference doc's wording may be imprecise rather than the code being wrong. Per this repo's own convention (update the reference doc or the code when they disagree, and note the verification source), this ambiguity should be resolved deliberately rather than left as-is.

**Fix:** Verify against a real H2D incremental-update wire capture whether state 10 alone (with populated `tray_type`) should clear. If code is correct, tighten the reference doc's wording. If state 10 should always clear, add it to `is_absent_state` and update the test.

(Rest of `src/ams/{mod.rs, mapping.rs}` — mapping/mapping2 array construction, `flat_channel_id`, out-of-range `ams_id` guards — verified correct against `reference/05_materials_ams.md`. `src/diagnostics/{mod.rs, hms.rs}` — no issues, bit-math verified against reference doc and real wire-capture test fixtures.)

---

## 9. `src/error.rs` — `no_std` `Display` impl sync is unverified by any test run in practice (soft finding, not a live bug)

**File/lines:** module doc comment (~8), `test_display_consistency` (~147)

**Issue:** The module doc comment claims the manual `no_std` `Display` impl is "kept in sync (verified by `test_display_consistency`)." But every documented test command (`cargo test`, `cargo test --lib`, `make check-fast`) uses the default `std` feature set, under which the manual `no_std` impl is `#[cfg(not(feature = "std"))]`-gated out entirely — the test only ever exercises the `thiserror`-generated `std` impl. The two commands that do turn `std` off (`--no-default-features --features alloc --lib` / `--features embassy --lib`) are `cargo check`/`cargo build` only, not `cargo test` — they confirm the manual impl compiles but never assert its output strings. Manual diff of both impls for all 9 variants currently matches exactly (not a live bug today), but nothing in the verification gate would catch future drift (e.g. a wording tweak to one impl not mirrored in the other).

**Fix:** Either add a way to run `test_display_consistency` against a no-`std` test build, or downgrade the doc comment's "verified by" wording to reflect that sync is currently maintained by manual inspection, not an automated check.

(`src/models.rs`, `src/lib.rs` — `resolve_model()` prefix table cross-checked against `MODEL_MATRIX.csv` and `quirks::mod.rs`'s dispatch match with no collisions or gaps; feature gating and public re-exports verified consistent with README's usage examples. No issues.)

---

## Summary table

`BUG-ID` and `Sev` columns match `BACKLOG.md` (added after that file was created — see `backlog` skill for the Sev1/Sev2/Sev3/needs-verification definitions). **`BACKLOG.md` is the status source of truth, not this table** — when you fix one of these, flip that `BUG-ID`'s row in `BACKLOG.md` (via the `backlog` skill), same commit. This table stays a point-in-time snapshot and won't be updated as things get fixed.

| # | BUG-ID | Module | File(s) | Sev | One-line |
|---|---|---|---|---|---|
| 0 | — | process | .git/hooks/pre-commit | do first | Wire `make check-fast` as a local commit gate |
| 1 | BUG-001 | mqtt/commands | kprofile.rs (x5), print_job.rs | Sev2 | 6 constructors skip `clamp_task_id()` |
| 2 | BUG-002 | mqtt/client | client/mod.rs:501 | Sev3 | `get_in_flight_count` doc says slice, returns usize |
| 3a | BUG-003 | ftps | client.rs:600-669 | Sev2 | `download_file` has no SIZE integrity recheck |
| 3b | BUG-004 | ftps | client.rs (6 methods) | Sev3 | Single-reply commands don't poison on timeout |
| 4a | BUG-005 | camera | rtsps.rs | Sev3 | IPv6 not bracketed in RTSPS URLs |
| 4b | BUG-006 | camera | binary.rs | Sev3 | Empty access_code silently accepted in handshake |
| 5 | BUG-007 | client/connect | connect.rs | Sev3 | `with_connect_timeout(0)` → immediate failure, not "disabled" |
| 6 | BUG-008 | io/tokio | tokio.rs:257 | Sev3 | Cert verifier ignores intermediates, single-hop only |
| 7a | BUG-009 | discovery | mod.rs:176-190 | Sev2 | Degraded-mode bind is order-dependent |
| 7b | BUG-010 | discovery | mod.rs:203-207 | Sev2 | Initial broadcast loop aborts sweep on one engine's failure |
| 7c | BUG-011 | discovery | parser.rs | Sev2 | SSDP serial not uppercased despite doc promise |
| 8 | BUG-012 | ams | parser.rs:112-116 | needs-verification | State-10 tray-clearing doc/code ambiguity |
| 9 | BUG-013 | error | error.rs | Sev3 | no_std Display impl sync unverified by test gate |
