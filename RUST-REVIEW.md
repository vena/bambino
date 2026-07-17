**Status:** COMPLETE (21/21 units complete)

# bambino — Idiomatic Rust Adherence Sweep (2026-07-16)

This is a `deep-review`-skill sweep, but with the default scope substituted: instead of
general correctness bugs, this run specifically audits **idiomatic Rust adherence** across
the whole crate. Trigger: a prior session found and fixed a non-idiomatic name on the crate's
main error type (`BambuError` → `Error`); this sweep checks whether similar patterns exist
elsewhere.

Methodology: the crate (`src/`, `tests/`) was partitioned into 21 review units along module
boundaries, and one subagent was spawned per unit in parallel to review only its assigned
files. Findings were deduped against `BACKLOG.md`, `CONFIRMED` findings got new `BUG-ID` rows
immediately, and `PLAUSIBLE` findings were re-verified and promoted to `Open` or `Wontfix`
(not left for manual triage) before this sweep closed.

## Scope for this run (idiomatic Rust, not general correctness)

In scope: non-idiomatic naming (Rust API Guidelines — `get_`/`set_`/`is_`/`as_`/`to_`/`into_`,
predicate booleans, error-enum naming); `unwrap()`/`expect()`/`panic!()`/unchecked
indexing/arithmetic in **library** code reachable with wire/attacker-controlled input; needless
`.clone()`/`.collect()`; manual loops that should be iterator combinators; missing
`Default`/`From`/`TryFrom`; `String`/`Cow` misuse; over-broad `pub`; inconsistent builder
self-consumption; non-idiomatic `Result`/`Option` combinators; missing `#[must_use]`. Only
recurring patterns or clear API-guidelines violations were flagged — not single subjective
preference calls.

Out of scope (unchanged from a normal deep-review sweep): minor security issues (crate is
explicitly LAN-only by design); general correctness bugs unrelated to idiom; hypothetical
internal-invariant validation with no concrete violation behind it.

`CONFIRMED` = agent was sure it's a real, fixable idiom violation with a concrete downside.
`PLAUSIBLE` = looked real but unverified by the reviewing agent. All `PLAUSIBLE` findings below
were re-verified during Step 5 and promoted to `BACKLOG.md`'s `Open` (BUG-184–BUG-198, all
Sev3) or `Wontfix` (BUG-199–BUG-204, `N/A`) — see each unit's findings for the disposition.

This file is meant to be read standalone by a fresh session with no memory of this
conversation. File:line references may have drifted if other changes landed on `main` since
this sweep ran — verify against current `main` before acting on any of them. **`BACKLOG.md` is
the status source of truth for all promoted findings from here on** — this file is a
point-in-time snapshot of the sweep itself, not a live tracker.

## Units

## 1. core (src/error.rs, src/models.rs, src/lib.rs)

- **BUG-165** (CONFIRMED): `BambuModel` (src/models.rs:10) redundantly repeats the crate's own
  subject ("Bambu") — same pattern as the prior `BambuError`→`Error` fix. ~200 call sites
  across `src/`/`tests/`/`docs/`; public re-export at crate root. Decided: rename to
  `PrinterModel` (bare `Model` rejected — too generic/overloaded with ORM/ML/data-model
  senses), mirroring `PrinterClient`.
- **BUG-184** (promoted from PLAUSIBLE): `Error::NetworkError`/`Error::SerializationError`
  (error.rs:32,57) redundantly suffix "Error" on a variant of an enum already named `Error`,
  inconsistent with sibling variants (`TlsHandshakeFailed`, `AccessDenied`, etc.). Suggest
  alternative, require user decision.
- **BUG-199** (promoted from PLAUSIBLE, Wontfix): missing `#[non_exhaustive]` on `Error`/
  `BambuModel` — defensible pre-1.0 choice, revisit before first external release.

## 2. ams (src/ams/mapping.rs, src/ams/mod.rs, src/ams/parser.rs)

- **BUG-166** (CONFIRMED): `validate_external_spool_safety` and ~8 pure return-value-only
  siblings (mapping.rs:229,304,335; several in parser.rs) missing `#[must_use]` — silent-discard
  risk reproduces the `07FF_8012` single-nozzle lockup the function exists to prevent.
- **BUG-185** (promoted from PLAUSIBLE, shared with unit 3): `validate_*()` bool-returning fns
  named as verbs, not predicates (mapping.rs:229,304,335).

## 3. diagnostics (src/diagnostics/hms.rs, src/diagnostics/kprofile.rs, src/diagnostics/mod.rs)

- **BUG-167** (CONFIRMED): identical setting-ID validation error-construction boilerplate
  duplicated verbatim in `ExtrusionCaliSetRequest::new` (kprofile.rs:173-176) and
  `StandardCaliDelRequest::new` (kprofile.rs:315-318).
- **BUG-185** (promoted from PLAUSIBLE, shared with unit 2): `validate_setting_id`
  (kprofile.rs:33) named as a verb, not a predicate — same crate-wide pattern as `ams/mapping.rs`.
- Considered and not promoted: `HmsSeverity::from_code` (hms.rs:47) could be `impl
  From<u32>`, but a bare `u32` source type risks misleading callers about which bits it
  actually interprets — kept as the clearer explicitly-named API.

## 4. camera (src/camera/mod.rs, src/camera/binary.rs, src/camera/rtsps.rs, tests/camera_test.rs, tests/common/mock_camera.rs)

- **BUG-168** (CONFIRMED): `RtpTimestampCorrector::init` (rtsps.rs:159-165) is the sole `init`
  constructor in the crate — ~35 other constructors use `new`.
- No other findings — builder shape, error types, and wire-boundary handling all checked clean.

## 5. client-core (src/client/mod.rs, src/client/connect.rs, src/client/telemetry.rs, src/client/types.rs, src/client/dummy.rs)

- **BUG-169** (CONFIRMED): `mqtt_connected()`/`ftps_connected()`/`camera_connected()`
  (connect.rs:125,372,431) omit the `is_` prefix used by every sibling bool predicate elsewhere
  in the crate (`is_all_axes_homed`, `is_ethernet_active`, etc.).
- **BUG-186** (promoted from PLAUSIBLE): `door_open()` (telemetry.rs:289) also lacks the `is_`
  prefix used by its sibling door-sensor decoders.
- **BUG-195** (promoted from PLAUSIBLE, shared with units 17, 19): no `#[must_use]` on
  `PrinterClient`'s consuming builder methods — a dropped return silently discards an entire
  connector configuration.

## 6. client-commands (src/client/ams.rs, src/client/camera.rs, src/client/hardware.rs, src/client/motion.rs, src/client/print.rs, src/client/storage.rs, src/client/thermal.rs)

- **BUG-187** (promoted from PLAUSIBLE): `self.camera.as_mut().unwrap()`
  (camera.rs:68,79-81) and `self.ftps.as_mut().unwrap()` (storage.rs:65) should be
  `.expect("ensure_camera()/ensure_ftps() guarantees Some on Ok(())")`.
- **BUG-188** (promoted from PLAUSIBLE): duplicated `ams_valid` address-space validation block
  in `change_filament()` (ams.rs:82-85) and `select_k_profile()` (ams.rs:247-250).
- **BUG-200** (promoted from PLAUSIBLE, Wontfix): `get_version()`/`get_k_profiles()`
  (ams.rs:279,307) use `get_` on network round-trip methods — defensible as-is, not flagged.

## 7. client-tests (tests/client_test.rs, tests/common/mod.rs)

- **BUG-170** (CONFIRMED): pervasive copy-pasted 3-statement MQTT connection boilerplate
  (`duplex`→`BambuMqttClient::connect`→`PrinterClient::from_mqtt`) at ~60 of the file's ~78
  tests. `tests/common/mock_mqtt.rs` already centralizes handshake helpers but not this step.

## 8. discovery (src/discovery/mod.rs, src/discovery/parser.rs)

**NO ISSUES FOUND in discovery.** Naming, panic-on-wire-input, and pub-scope all checked clean.

## 9. ftps-client (src/ftps/client.rs, src/ftps/mod.rs, tests/ftps_test.rs, tests/common/mock_ftps.rs)

- **BUG-171** (CONFIRMED): the poisoning-check-then-return pattern is duplicated verbatim 34
  times across 13 methods in client.rs instead of funneled through a shared helper — root-cause
  context on BUG-004's shape (missed poisoning was exactly this kind of copy-paste miss).
- **BUG-172** (CONFIRMED): `upload_file`'s manual offset/chunk_size loop (client.rs:676-712)
  should use `.chunks(FTPS_UPLOAD_CHUNK_SIZE)` — produces identical wire writes, not gated by
  the wire-framing-hardware-verification rule.
- **BUG-196** (promoted from PLAUSIBLE): `connect`/`connect_control_stream`/`from_control_stream`
  (client.rs:152-165,186-198,291-303) suppress `too_many_arguments` instead of a config struct;
  needs a design decision before implementing.

## 10. ftps-protocol (src/ftps/parser.rs, src/ftps/protocol.rs, src/ftps/protocol/tests.rs)

- **BUG-173** (CONFIRMED): repeated identical 4-line token-skip block copy-pasted 3x in
  `parse_unix_listing` (parser.rs:141-153).
- **BUG-174** (CONFIRMED): `parse_pasv_port` (protocol.rs:327-330) uses 4 manual `let _ =
  parts.next();` instead of `.skip(4)`.
- **BUG-189** (promoted from PLAUSIBLE): `parse_unix_listing` (parser.rs:398) takes 4
  consecutive same-typed `u8` params — transposition footgun.
- **BUG-201** (promoted from PLAUSIBLE, Wontfix): `write_command` (protocol.rs:151-153) builds
  its payload via `String::from`+`push_str` instead of one `format!` — purely stylistic.

## 11. io-core (src/io/mod.rs, src/io/tokio.rs, src/io/tokio/cert_verify.rs, src/io/tokio/tests.rs, tests/common/io.rs)

- **BUG-190** (promoted from PLAUSIBLE): `to_socket_error` (tokio.rs:102, called at 6 sites)
  should be `impl From<std::io::Error> for SocketError`.
- No other findings — `Cow` usage, trait design, and wire-boundary panics all checked clean.

## 12. io-embedded (src/io/esp_idf.rs, src/io/embassy.rs)

- **BUG-175** (CONFIRMED): `EspTlsStream` (esp_idf.rs:307) is the lone type in the file that
  drops the `EspIdf` prefix every sibling type carries (`EspIdfTcpStream`, `EspIdfTlsConnector`,
  etc.) — misleading about which crate/module it belongs to.

## 13. mqtt-client (src/mqtt/client/mod.rs, codec.rs, frame.rs, pending.rs, src/mqtt/mod.rs, tests/mqtt_test.rs, tests/common/mock_mqtt.rs)

- **BUG-180** (CONFIRMED): `get_in_flight_count()` (mod.rs:588) uses the disallowed `get_`
  prefix on a plain getter — the only `get_`-prefixed method in the unit.
- **BUG-197** (promoted from PLAUSIBLE): `BambuMqttClient` (mod.rs:68) redundantly repeats the
  crate's subject, same class as `BambuError`→`Error` and `BambuModel` (BUG-165) — wider-reaching
  rename, confirm all call sites first.

## 14. mqtt-commands (src/mqtt/commands/mod.rs, ams.rs, control.rs, gcode.rs, hardware.rs, print_job.rs, status.rs)

- No `CONFIRMED` findings. Task-ID clamping invariant verified intact (`ClampedTaskId`
  type-state funnels every constructor, no inline reimplementation found) and Payload+Request
  naming shape is consistent across all 7 files.
- **BUG-202** (promoted from PLAUSIBLE, Wontfix): stringly-typed command/param args
  (control.rs:30, ams.rs:127) instead of closed-set enums — agent's own assessment: optional
  polish, not a correctness bug.
- Considered and not promoted: `AmsFilamentSettingRequest::new`/`AmsFilamentDryingRequest::new`
  (ams.rs:69,238) suppress `too_many_arguments` — real but lower-value than BUG-196's ftps
  equivalent; not separately tracked.

## 15. quirks-engine (src/quirks/mod.rs)

- **BUG-176** (CONFIRMED): `enforce_ftps_tls_1_2` (line 34) breaks the trait's third-person-
  singular boolean-predicate naming convention (`uses_plaintext_ftps_data_channel`,
  `requires_wallclock_rtsp_timestamps`).
- **BUG-177** (CONFIRMED): `door_sensor_field_present` (line 49) lacks the `is_`/`has_` prefix
  used by its immediate neighbors `is_door_open`/`has_door_sensor`.
- **BUG-191** (promoted from PLAUSIBLE): `auxiliary_fan_uses_percentage` (line 170) is
  subject-first, inconsistent with the trait's prevailing verb-first shape.

## 16. quirks-models (src/quirks/models/mod.rs, a1.rs, a2.rs, h2.rs, p1.rs, p2.rs, x1.rs, x2.rs)

- **BUG-179** (CONFIRMED): `H2S_X_MAX`'s doc comment (h2.rs:23-24) wraps mid-sentence — the
  sole outlier among all 8 sibling files, violating the project's mandatory doc-comment rule.
- No sibling-inconsistency findings beyond this — the 8 structurally-parallel strategy structs
  checked clean on naming, const usage, and per-model dispatch shape.

## 17. types-core (src/types/mod.rs, version.rs, telemetry/mod.rs, telemetry/report.rs, telemetry/diagnostics.rs, telemetry/tests.rs, telemetry/tests/misc.rs, tests/telemetry_replay_test.rs)

**NO ISSUES FOUND in types-core.** `TelemetryReport::device()`/`::fun()` already use idiomatic
`.or_else()` fallback chains; custom `Deserialize` impls degrade gracefully on malformed input
rather than panicking. (Crate-wide `#[must_use]` gap noted here too — folded into BUG-195.)

## 18. types-ams (src/types/telemetry/ams.rs, src/types/telemetry/tests/ams.rs)

- **BUG-178** (CONFIRMED): `AmsTray::get_state()` (ams.rs:642) uses the disallowed `get_`
  prefix — the sole `get_`-prefixed accessor in the file, used externally at
  `client/telemetry.rs:336,342`.
- **BUG-203** (promoted from PLAUSIBLE, Wontfix): three `merge_from` impls repeat
  `if incoming.field.is_some() { ... }` ~40 times — agent explicitly recommended against
  DRYing this out, since the per-field doc comments justifying divergent merge behavior are
  more valuable than the deduplication.
- Bit-offset/mask constants already correctly extracted as named `pub(crate)` consts, directly
  addressing the BUG-104-adjacent risk this unit was checked for.

## 19. types-device (src/types/telemetry/device.rs, tests/device.rs, tests/nozzle.rs, tests/bed.rs, tests/ctc.rs, tests/fun_field.rs)

- **BUG-192** (promoted from PLAUSIBLE): `active_extruder_index()`/`extruder_count()`
  (device.rs:378-386) use `.map().unwrap_or()` instead of `.map_or()`.
- **BUG-195** (promoted from PLAUSIBLE, shared with units 5, 17): several pure getters lack
  `#[must_use]` — folded into the crate-wide finding.
- **BUG-204** (promoted from PLAUSIBLE, Wontfix): `fire_ext: Option<serde_json::Value>`
  (device.rs:38) is untyped unlike every sibling field — reads as an intentional placeholder
  for a not-yet-fully-reverse-engineered field, not an idiom lapse.
- Composite-packing decode logic (the pattern specifically flagged as worth checking) is
  already properly centralized through `PrinterTelemetry::unpack_temperature()` — no finding.

## 20. cli-core (src/bin/bambino-cli/main.rs, connection.rs, discover.rs, table.rs, verify_tls.rs, inspect_cert.rs)

- **BUG-181** (CONFIRMED, shared with unit 21): `Error::ProtocolViolation` reused as a
  catch-all for CLI-local failures (bad IP/serial/access-code, PEM load failure, file write
  failure) across connection.rs, verify_tls.rs, inspect_cert.rs — 9+ sites in this unit alone.
- **BUG-193** (promoted from PLAUSIBLE): `enum Command` (main.rs:67) collides in name with
  `clap::Command`, the builder type from the same dependency.

## 21. cli-commands (src/bin/bambino-cli/camera.rs, control.rs, probe.rs, storage.rs, monitor/mod.rs, monitor/dashboard.rs)

- **BUG-181** (CONFIRMED, shared with unit 20): same `Error::ProtocolViolation` misuse — 8+
  additional sites across camera.rs, control.rs, storage.rs, monitor/mod.rs, probe.rs.
- **BUG-182** (CONFIRMED): `fs::metadata` error discarded (storage.rs:123), reports misleading
  "does not exist" for any failure (permission-denied, symlink loop, etc.).
- **BUG-183** (CONFIRMED): needless full clone of the `_device` JSON subtree on every dashboard
  redraw (dashboard.rs:270-274) — `DeviceTelemetry::deserialize` works directly on `&Value`.
- **BUG-194** (promoted from PLAUSIBLE): ~40 repeated `writeln!/write!(...).unwrap_or(())`
  should be a shared macro/helper (dashboard.rs, throughout `render_*`).
- **BUG-198** (promoted from PLAUSIBLE): `probe.rs`'s `run()` (lines 365-601) is a single
  ~235-line function mixing 5 distinct responsibilities.

## Summary table

`BACKLOG.md` is the status source of truth from here on — this table is a point-in-time
snapshot of what this sweep produced, not a live tracker.

| BUG-ID | Sev | Module | One-line |
|---|---|---|---|
| BUG-165 | Sev3 | models.rs | `BambuModel` → `PrinterModel` rename (rejected bare `Model`) |
| BUG-166 | Sev3 | ams/mapping.rs | `validate_*` fns missing `#[must_use]` |
| BUG-167 | Sev3 | diagnostics/kprofile.rs | Duplicated setting-ID validation boilerplate |
| BUG-168 | Sev3 | camera/rtsps.rs | `RtpTimestampCorrector::init` should be `new` |
| BUG-169 | Sev3 | client/connect.rs | `*_connected()` missing `is_` prefix |
| BUG-170 | Sev3 | tests/client_test.rs | Duplicated MQTT connection boilerplate |
| BUG-171 | Sev3 | ftps/client.rs | Poisoning-check duplicated 34x |
| BUG-172 | Sev3 | ftps/client.rs | Manual chunk loop should use `.chunks()` |
| BUG-173 | Sev3 | ftps/parser.rs | Repeated token-skip block |
| BUG-174 | Sev3 | ftps/protocol.rs | Manual field-skip instead of `.skip(4)` |
| BUG-175 | Sev3 | io/esp_idf.rs | `EspTlsStream` breaks naming convention |
| BUG-176 | Sev3 | quirks/mod.rs | `enforce_ftps_tls_1_2` naming |
| BUG-177 | Sev3 | quirks/mod.rs | `door_sensor_field_present` naming |
| BUG-178 | Sev3 | types/telemetry/ams.rs | `get_state()` disallowed `get_` prefix |
| BUG-179 | Sev3 | quirks/models/h2.rs | Doc comment wraps mid-sentence |
| BUG-180 | Sev3 | mqtt/client/mod.rs | `get_in_flight_count()` disallowed `get_` |
| BUG-181 | Sev3 | bin/bambino-cli/** | `ProtocolViolation` misused as CLI catch-all |
| BUG-182 | Sev3 | bin/bambino-cli/storage.rs | Discarded `fs::metadata` error |
| BUG-183 | Sev3 | bin/bambino-cli/monitor/dashboard.rs | Needless clone on every redraw |
| BUG-184 | Sev3 | error.rs | Redundant `Error`-suffixed variant names |
| BUG-185 | Sev3 | ams/mapping.rs, diagnostics/kprofile.rs | `validate_*` verb-not-predicate naming |
| BUG-186 | Sev3 | client/telemetry.rs | `door_open()` missing `is_` prefix |
| BUG-187 | Sev3 | client/camera.rs, storage.rs | `.unwrap()` should be `.expect()` |
| BUG-188 | Sev3 | client/ams.rs | Duplicated `ams_valid` block |
| BUG-189 | Sev3 | ftps/parser.rs | 4 same-typed positional params |
| BUG-190 | Sev3 | io/tokio.rs | `to_socket_error` should be `From` impl |
| BUG-191 | Sev3 | quirks/mod.rs | `auxiliary_fan_uses_percentage` naming |
| BUG-192 | Sev3 | types/telemetry/device.rs | `.map().unwrap_or()` should be `.map_or()` |
| BUG-193 | Sev3 | bin/bambino-cli/main.rs | `enum Command` collides with `clap::Command` |
| BUG-194 | Sev3 | bin/bambino-cli/monitor/dashboard.rs | Repeated `.unwrap_or(())` pattern |
| BUG-195 | Sev3 | crate-wide | No `#[must_use]` anywhere in the crate |
| BUG-196 | Sev3 | ftps/client.rs | Constructors suppress `too_many_arguments` |
| BUG-197 | Sev3 | mqtt/client/mod.rs | `BambuMqttClient` redundant prefix |
| BUG-198 | Sev3 | bin/bambino-cli/probe.rs | `run()` mixes 5 responsibilities |
| BUG-199 | N/A | error.rs, models.rs | Missing `#[non_exhaustive]` — Wontfix, pre-1.0 |
| BUG-200 | N/A | client/ams.rs | `get_` on network methods — Wontfix |
| BUG-201 | N/A | ftps/protocol.rs | `format!` style nit — Wontfix |
| BUG-202 | N/A | mqtt/commands/*.rs | Stringly-typed args — Wontfix |
| BUG-203 | N/A | types/telemetry/ams.rs | `merge_from` boilerplate — Wontfix |
| BUG-204 | N/A | types/telemetry/device.rs | Untyped `fire_ext` field — Wontfix |

**Release bar impact:** zero Sev1, zero Sev2 findings from this sweep — all 34 Sev3/N/A. Does
not block release.
