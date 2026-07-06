# Pre-Release Review

Deep review ahead of initial release. Findings only — no praise, no clean-module notes. Each finding is actionable by a clean session with no prior context: file:line, the defect, a concrete failure scenario, and severity (critical/high/medium/low).

Severity legend: **critical** = security/safety/data-corruption; **high** = correctness bug that will bite real usage; **medium** = architecture/DRY violation with real cost; **low** = code smell/minor/documentation gap.

---

## 1. Transport layer (`src/io/`, `src/client/connect.rs`, `src/error.rs`)

### Critical

- **`src/io/esp_idf.rs:439-442` (`EspIdfTcpStream::connect`) and `:620-624` (`EspIdfRawStreamFactory::dial`)** — `dial()` is `async fn` but its body is a single call to *synchronous* `std::net::TcpStream::connect((host, port))` with no `.await` point. `race()` (`src/io/mod.rs`) polls both raced futures cooperatively via `poll_fn` on the same task; a future with no internal yield point runs to completion the instant it's first polled. `race_against_connect_timeout` (`src/client/connect.rs`) polls this dial future first, so the whole calling task blocks for however long the OS-level blocking connect takes — the `timer.sleep(connect_timeout_secs)` future never gets a chance to run. A printer that's off, on another subnet, or behind a silent packet-dropping firewall causes `ensure_mqtt()`/`ensure_ftps()`/`ensure_camera()` to hang for the OS/lwIP's own connect timeout (can be far longer than the configured value), not the documented `connect_timeout_secs`. This directly breaks the guarantee CLAUDE.md documents for `connect_timeout_secs`, and unlike Embassy's documented "no built-in connect timeout" caveat, nothing acknowledges this gap for ESP-IDF. Fix requires either a genuinely non-blocking connect (nonblocking socket + poll loop, matching the pattern `EspIdfTlsConnector`'s handshake already uses) or spawning the blocking connect onto a separate execution context that can be preempted.

### High

- **`src/io/esp_idf.rs` (`build_tls_config`, whole `EspIdfTlsConnector`)** — no way to force TLS 1.2 on ESP-IDF. `esp_idf_svc::tls::Config` (vendored crate, `esp-idf-svc-0.52.1/src/tls.rs`) has no min/max-version field, and `build_tls_config` never attempts a version constraint. Meanwhile `ftps/client.rs`'s `require_tls_1_2_if_enforced` fail-closed guard rejects any FTPS connection where `model.quirks().enforce_ftps_tls_1_2()` is true (true for several models per grep: p1, p2, x2, a1, a2, h2, one x1 variant) but negotiated version isn't exactly TLS 1.2. On ESP-IDF, if the printer's vsFTPd offers/prefers TLS 1.3, the connection is permanently rejected with no API to fix it — no `.with_force_tls_1_2()` equivalent exists for this backend. Embassy documents the same limitation explicitly; ESP-IDF does not.

### Medium

- **`src/io/tokio.rs:55-86` vs `src/io/esp_idf.rs:71-94`** — UDP multicast/broadcast bind setup (`set_broadcast(true)`, `join_multicast_v4(239.255.255.250, 0.0.0.0)`, `set_nonblocking(true)`, same error-swallowing-via-log pattern) is duplicated verbatim across both backends. A future fix to the join logic has to land in two places. Extract a shared `configure_std_udp_socket(&std::net::UdpSocket) -> Result<(), SocketError>` helper (feature = "std") in `io/mod.rs`.
- **`src/io/tokio.rs:345-361` (`TokioIoError::kind`) vs `src/io/esp_idf.rs:392-410` (`EspIdfIoError::kind`)** — identical `embedded_io_async::Error::kind()` match-arm mappings over `std::io::ErrorKind`, duplicated rather than centralized the way `map_std_io_error` already centralizes the equivalent `SocketError` mapping. Adding a new `ErrorKind` variant requires updating both.
- **`src/io/esp_idf.rs:296-315` and `:317-336` (`EspTlsStream::read`/`write`)** — identical WouldBlock-retry-loop shape duplicated between read and write, differing only in which `EspTls` method is called. Factor into one shared retry helper.

### Low

- **`src/io/esp_idf.rs:73, 87, 102`** — `.map_err(|e| to_esp_socket_error(e))` should be `.map_err(to_esp_socket_error)`; `cargo clippy` (part of this project's own gate) will flag `clippy::redundant_closure`.
- **`src/io/esp_idf.rs:337-339` (`EspTlsStream::flush`)** — unconditional no-op with no comment explaining why (mbedTLS/esp_tls writes are presumably unbuffered past the socket write, but nothing states that assumption — can't tell "verified no-op" from "unimplemented").

---

## 2. MQTT (`src/mqtt/`)

### High

- **`src/mqtt/commands/print_job.rs:73-76, 178-211`** — `ams_mapping2` is serialized unconditionally regardless of the computed `use_ams`. `with_ams_mapping2()` never sets `self.use_ams = true` (unlike `with_ams()`, which does). Separately, when `validate_external_spool_safety()` flips computed `use_ams` to `false`, `ams_mapping` correctly collapses to `AmsMappingTable::Inactive("")` but `ams_mapping2` still serializes the original `Some(vec![...])` untouched. Failure scenario: a caller uses `.with_ams_mapping2(...)` without `.with_ams(...)`, or the safety interlock trips — either way the printer receives `{"use_ams":false,"ams_mapping":"","ams_mapping2":[...]}`, an internally contradictory payload of the same shape the reference doc says causes `0700_8012 "Failed to get AMS mapping table"`. No test exercises this combination. Fix: gate `ams_mapping2` on the same computed `use_ams` (set `None` when false), and/or have `with_ams_mapping2()` set `use_ams = true` for symmetry with `with_ams()`.

### Medium

- **Every `pub fn new(..., sequence_id: u64)` constructor across `ams.rs`, `control.rs`, `gcode.rs`, `hardware.rs`, `print_job.rs`, `status.rs`** — no clamping of `sequence_id` to `i32::MAX`, unlike `subtask_id` (via `clamp_task_id()`). Today's only in-repo caller (`PrinterClient::next_sequence_id()`) does clamp before calling these constructors, so it's not reachable internally — but these constructors are re-exported public API, and an external consumer building a payload directly with e.g. an epoch-millisecond `u64` reproduces the documented 32-bit-overflow failure (printer permanently locks into `IDLE`).
- **`src/mqtt/client/mod.rs:456-478` (`tick_zombie_check`)** — correctly implemented (verified against reference doc thresholds and semantics) but has zero production call sites. `bambino-cli`'s monitor loop drives `send_ping()` off a timer but never calls `tick_zombie_check`. The "safety-critical" zombie/stale-connection detection this provides is inert end-to-end in the one shipped application built on this library.

### Low

- **`src/mqtt/commands/hardware.rs:29-44` (`LedCtrlRequest::new`)** — only exposes on/off; protocol supports a "flashing" `led_mode` with nonzero on/off/loop/interval timing (per reference doc) but there's no public constructor path to build it.
- **`src/mqtt/mod.rs:19-25`** — top-level `pub use commands::{...}` omits `AmsMappingTable`, even though it's re-exported one level down and is a field type on public `ProjectFilePayload::ams_mapping`. Consumers must reach into `crate::mqtt::commands::AmsMappingTable` instead of the promoted path its siblings get.
- **`src/mqtt/client/mod.rs:98-110` (`write_frame`)** — collapses every write/flush I/O failure to a single `SocketError::ConnectionAborted`, discarding the actual underlying error (timeout vs. genuine reset), inconsistent with the precision the crate is otherwise careful about (ESP-IDF error-mapping precedent).

---

## 3. FTPS (`src/ftps/`) and Camera (`src/camera/`)

### Critical

- **`src/camera/rtsps.rs:65-75` (`build_rtsps_url`)** — validates `access_code` (non-empty ASCII alphanumeric) but never validates `ip`, which is interpolated directly into the URL's authority alongside the credential: `format!("rtsps://bblp:{}@{}:322/streaming/live/1", access_code, ip)`. If `ip` originates from an untrusted network source (e.g. an SSDP/mDNS discovery response — spoofable by any device on the LAN) and contains an embedded `@`, e.g. `"1.2.3.4@attacker.example.com"`, the resulting URL's host resolves to `attacker.example.com` under standard userinfo/host-splitting-on-last-`@` parsing — sending the LAN access code to the attacker. Classic userinfo-redirection injection; the function already reasons carefully about validating `access_code` for this exact class of mistake but never applied the same reasoning to `ip`. Validate `ip` (parse as `IpAddr`, or at minimum reject `@`/`/` and other URL-structural characters) before interpolating.

### High

- **`src/ftps/protocol.rs:80-115, 259-272` (`read_line_raw`, `read_to_eof`)** — never go through `io::read_chunk`/`race`, so no FTPS operation has a per-read wall-clock deadline. Contrast with MQTT's `poll_wire` and camera's `read_next_frame_with_timer`, both of which race every low-level read against a `TimerProvider` deadline specifically to fix "connection hangs forever." `BambuFtpsClient` has no `Timer` type parameter at all — this can't be bolted on without a signature change. Failure scenario: printer stalls mid-transfer (firmware hang during microSD flush, after `150`/`125` but before `226`) — `list_directory`/`upload_file`/`download_file`/`get_available_space` block the calling task indefinitely. `PrinterClient`'s `connect_timeout_secs` only bounds the one-time dial+login sequence, not any of these post-connect calls.
- **`src/ftps/protocol.rs:259-272` (`read_to_eof`)** — used by `list_directory`'s listing payload and `download_file`'s file payload, accumulates into an unbounded `Vec<u8>` with no maximum-size cap, unlike camera's `max_frame_size` guard (whose doc comment explicitly warns unbounded allocation triggers an uncatchable `alloc_error_handler` abort on `no_std`/Embassy targets). A misbehaving printer, MITM, or very large timelapse/listing response that never sends EOF grows the Vec without bound until OOM — the exact risk class already mitigated for camera frames but not here.

### Medium

- **`src/ftps/protocol.rs:249-256` (`validate_ftp_path`)** — only rejects `\r`/`\n`/`\0`. Does not reject path traversal (`..`) sequences, so nothing prevents a caller-supplied path from escaping the intended directory root via `delete_file`/`rename_file`/`remove_directory`/`upload_file` (overwrite). Impact depends on the printer's own vsFTPd sandboxing, which this client has no visibility into.
- **`src/ftps/client.rs:198-297, 359-465, 470-550`** — `list_directory`, `upload_file`, `download_file` each repeat an almost-identical ~40-line "secure vs. plaintext data channel" branch (TLS connect → TLS-1.2 re-check → poison-on-error → transfer); `upload_file`'s chunked-write loop is duplicated verbatim between the secure and plaintext branches. This is exactly the shape of duplication that produced the documented `write_command` regression (commit `6385019`) — a future fix applied to one branch and missed in its sibling would silently reintroduce that failure class (mocks would still pass).
- **`src/camera/rtsps.rs:99-108` (`rewrite_rtsp_request_uri`)** — `printer_ip` parameter unvalidated before being spliced into `format!("rtsps://{}:322/{}", printer_ip, path)`. No credentials involved (lower impact than the `build_rtsps_url` finding), but a value containing `@` or `/` can redirect the proxy's outbound connection or produce a malformed URI.
- **`src/camera/binary.rs:153-164` (`authenticate()`)** — `write_all`/`flush` calls have no deadline at all, unlike the read side (`read_next_frame_with_timer`). If the printer never drains its TCP receive buffer during handshake, `authenticate()` can hang forever with no API to bound it.

### Low

- **`src/ftps/protocol.rs:249-256` / `src/ftps/parser.rs:127`** — `validate_ftp_path` (and `parse_unix_listing`, which reuses it on parsed names) also doesn't reject a leading `-` (some FTP-daemon argument-handling paths interpret a leading-dash filename as flags) or other C0/DEL control chars (can smuggle ANSI escapes into a filename a caller later prints/logs).
- **`src/camera/binary.rs:57-79` (`build_handshake_packet`)** — only checks `access_code.len() <= 32`, doesn't validate ASCII-alphanumeric the way `rtsps.rs::build_rtsps_url` does for the same conceptual credential. Not independently exploitable (copied into a fixed-width binary field, not interpolated into text), but an inconsistency between the two validation sites.

---

## 4. Client API (`src/client/`)

### Medium

- **`src/client/ams.rs:53-108, 124-142`** — no bounds/sanity validation on any AMS addressing parameter before serialization: `change_filament`'s `ams_id`/`slot_id`/`target` (documented valid values `{0..3, 255}` / `{0..3, 254}` / `{1, 255}`), `scan_rfid`'s `ams_id`/`slot_id`, `select_k_profile`'s `ams_id`/`tray_id` (documented valid combos exactly `{254,254}` or `{255,255}` per the IDEX cheat-sheet in the same doc comment). Every other hazardous parameter elsewhere in the client goes through a quirks-based guard (fan targets, chamber heater, homing) — these don't, despite the doc comment itself calling out the mis-routing hazard for the IDEX Ext-R case.
- **`src/client/ams.rs:75-94` (`start_drying`)** — `dry_temp`/`dry_time` have zero ceiling enforcement anywhere in the crate (no `ams_dry_temp_max`-equivalent quirks method exists, confirmed via grep of `src/quirks/`), unlike every other heater-setting client method in `thermal.rs`, which clamps via `model.quirks()`. Inconsistent with the "model-aware safety checks" the README advertises for every other thermal setpoint.

### Low

- **`src/client/mod.rs:306`, `src/client/motion.rs:217`** — stale doc-comment references to `src/mqtt/client.rs`, which no longer exists (split into `src/mqtt/client/{mod,codec,frame,pending}.rs`); the cleanup commit (`b133c9d`) missed these two.
- **`src/client/mod.rs:288-290` (`set_command_timeout`)** — doc comment doesn't state that `secs = 0` disables the wall-clock timeout entirely in `poll_until` (falls back to a 200-message cap with no time bound) — a plausible footgun for a caller expecting `0` to mean "immediate timeout."
- **DRY: `mod.rs`/`ams.rs`/`hardware.rs`/`print.rs`** — the three-line `next_sequence_id()` → build request → `publish_request()` sequence is repeated ~15 times; a small `dispatch()` helper would collapse this and remove the risk of a call site forgetting `next_sequence_id()`.
- **`src/client/thermal.rs:61-131`** — `set_bed_temperature`/`set_nozzle_temperature`/`set_chamber_temperature` repeat an identical clamp-and-warn block differing only in label string; extract a shared `clamp_temp(value, max, label)` helper.
- **`src/client/hardware.rs:65-91` / `src/client/telemetry.rs:399`** — fan port IDs are inline magic numbers with no shared constant (write-side `10` for `AuxiliaryRight` in hardware.rs vs. read-side `160` in telemetry.rs for the same fan) — no compiler-enforced link, so correcting one without the other silently desyncs write and read paths.
- **`src/client/hardware.rs:139` (`set_buzzer_mode`)** — takes a raw unvalidated `i32` ("0/1/2" per doc comment) with no enum/range check, unlike every sibling setter in the same file (all typed). A caller passing e.g. `7` gets no error.
- **`src/client/thermal.rs:86-104` (`set_nozzle_temperature`)** — `nozzle_id: u8` never validated against `ModelQuirks::physical_nozzle_count()` (confirmed unreferenced anywhere under `src/client/` via grep). A caller can request an out-of-range nozzle index on a single-nozzle model with no model-aware guard.

---

## 5. Quirks/models/types/discovery/ams/diagnostics

### Medium

- **`src/ams/parser.rs:80-121` (`clean_stale_tray_data`)** — clears `tray_type`/`tray_color`/`tray_info_idx`/`tag_uid`/`tray_uuid`/`remain`/`tray_sub_brands`/`nozzle_temp_max`/`nozzle_temp_min`/`tray_diameter`/`tray_weight`/`tray_id_name`/`xcam_info`/`k`/`n`/`cali_idx`/`cols`/`ctype`/`total_len`/`bed_temp`/`bed_temp_type` when a tray goes empty, but never clears `tray_temp`, `tray_time`, `drying_temp`, `drying_time` (fields on `AmsTray`, `src/types/telemetry/ams.rs:240-250`). Failure scenario: a spool with a configured drying profile is removed and replaced with a spool that has no drying config; the incremental telemetry update omits drying keys for the new tray, so client-side state retains the *previous* spool's stale drying temp/time — a UI can show a phantom drying countdown for filament that isn't present. No test exercises these four fields.

### Low

- **`src/quirks/models/x1.rs:31-155`** — `X1CQuirks`/`X1EQuirks` are hand-duplicated in full and differ in only 4 of 13 methods, unlike `a1.rs`/`h2.rs` which share their near-identical variants via `macro_rules!`. Values are correct (verified against `MODEL_MATRIX.csv`/reference docs) — this is a pure DRY/consistency gap, increasing risk that a future capability change applied to one struct gets forgotten on the other.
- **`reference/01_network_discovery.md:122`** — Port 6000 "Model Availability" list reads "A1, A1 Mini, P1P, P1S" and omits A2L, even though `MODEL_MATRIX.csv` and `A2LQuirks::camera_protocol()` (`src/quirks/models/a2.rs:32-34`) correctly agree A2L uses the same binary-JPEG protocol. Code is correct; the reference doc predates A2L's addition and should be updated per this repo's stated convention of correcting stale reference docs.

---

## 6. CLI (`src/bin/bambino-cli/`) and build tooling

### High

- **`Makefile:8-12` (`check-fast`)** — never builds, tests, or lints the CLI binary. It runs `cargo build` (default features, which exclude `cli`), `cargo test`, the `alloc`/`embassy` lib checks, and `cargo clippy` (also default features) — none of which include `cargo build --bin bambino-cli --features cli`, despite CLAUDE.md explicitly listing that command and stating `check-fast` "runs all of the above... in one command." `.github/workflows/ci.yml` just invokes `make check-fast`, so it inherits the same gap. A change to any file under `src/bin/bambino-cli/`, or a `cli`-gated dependency bump, that fails to compile or trips clippy will pass the documented gate cleanly — the entire CLI surface is currently unverified by tooling.

### Medium

- **`src/bin/bambino-cli/monitor/dashboard.rs:520-526` (`format_color_swatch`)** — slices the printer-supplied `tray_color` telemetry string (untrusted wire data) at raw byte offsets `[0..2]`, `[2..4]`, `[4..6]` after checking only byte length (`< 6`), not char-boundary safety. A `tray_color` ≥6 bytes containing a multi-byte UTF-8 character positioned so a byte offset falls mid-codepoint panics ("byte index N is not a char boundary"), killing the `monitor` dashboard mid-session. Low likelihood (hex colors are normally ASCII) but no defense exists against a malformed value.

### Low

- **`src/bin/bambino-cli/table.rs:28-38` (`separator_width`)** — `(col_count - 1) * 3` is a non-saturating `usize` subtraction; `Table::new` with an empty header vector underflows (panic in debug, huge wraparound value feeding a `format!` repeat-count in release). Currently unreachable (every call site passes non-empty headers) but `Table` is `pub` — latent landmine for a future caller.
- **`src/bin/bambino-cli/storage.rs:116, 168-171`** — the 1 GiB magic number (`1_073_741_824`) is duplicated three times (a locally-scoped const plus two bare literals in `format_size`), against the project's own magic-number convention.
- **`src/bin/bambino-cli/control.rs:203-378`** — nearly every `ControlAction` arm repeats an identical "Dispatching..." / call / "...published successfully" triplet (16+ occurrences); collapsible into a small `dispatch(before_msg, after_msg, fut)` helper.
- **`src/bin/bambino-cli/control.rs:122-123`** — the `--unsafe` flag (`bypass_safety`) that skips the interactive confirmation prompt for `gcode-raw` isn't mentioned in README.md's Usage block, unlike every other control action.
- **`src/bin/bambino-cli/control.rs:212`** — `move` action's `axis.chars().next()` silently takes only the first character of user input (e.g. `move xy 10` is silently treated as `move x 10` instead of erroring on the extra character).
