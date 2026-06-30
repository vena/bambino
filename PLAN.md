# bambino — Lazy connections and API consistency

**Important:** Before starting any phase, read this document in its entirety. Read the `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

**When completing a phase:** Update this PLAN.md marking the phase complete. Update the completed phases summary, strictly including **only** what is necessary to inform clean sessions implementing the next phases which cannot be learned from the code itself. Once summarized, remove the phase from PLAN.md.

---

## Phases 1–11: Complete

Non-obvious decisions a future session cannot derive from the code alone:

- **`from_mqtt()` does not reseed `sequence_counter`** — `ensure_mqtt()` reseeds from `TimerProvider::now_millis()` on lazy connect to de-correlate independent sessions, but `from_mqtt()` (tests and Embassy) intentionally starts at `INITIAL_SEQUENCE_ID` so injected fixture responses remain predictable.
- **`move_relative()`/`extrude()` warn on unhomed axis, never error** — the policy is warn-and-proceed; the `log::warn!` calls in motion.rs are deliberate, not a placeholder for a future error return.
- **`wait_for_homing()` overrides `command_timeout_secs` to 90s** — homing takes far longer than normal commands; the override is intentional.
- **Clap positional `bool` requires explicit action** — `AmsAction::Dry.rotate` uses `#[arg(action = ArgAction::Set, value_parser = BoolishValueParser::new())]`. Clap defaults `bool` fields to `ArgAction::SetTrue` (flag semantics), which panics for positional args at startup. Any future positional bool in the CLI needs the same treatment.

**Decisions informing future phases:**

- **Consuming builders change type params; non-consuming builders return `Self`.** `.with_timer()` and `.with_ftps()` consume `self` because they change type parameters. `.with_mqtt_port()` and `.with_ftps_port()` return `Self`. Phase 12's camera builder must follow the same convention.
- **`ensure_*()` is the lazy connection pattern.** `ensure_mqtt()` and `ensure_ftps()` short-circuit on `Some`, otherwise connect lazily. `ensure_ftps()` uses `.take()` to consume `ftps_config`, so reconnection requires a new `PrinterClient`. Phase 12 should consider whether camera's persistent streaming nature needs a different reconnection story.
- **Each protocol's TLS config is independent.** FTPS may need `force_tls_1_2` (model quirk) while MQTT does not. Phase 12's camera TLS may also differ — don't assume a shared connector.
- **CLI storage now routes through `PrinterClient`.** Phase 12's camera CLI command should follow the same pattern rather than constructing protocol clients directly.
- **Message buffer is on `BambuMqttClient`.** `poll_telemetry()` drains buffered messages first, then reads the wire. `poll_wire()` bypasses the buffer (used by `PrinterClient::poll_until()`). `push_pending()` stashes non-matching messages. `PrinterClient` delegates all reads through these methods. `mqtt().await?` returns a client whose `poll_telemetry()` is safe to call directly — no split-brain, no warnings.

---

## Phase 12: Camera integration in `PrinterClient`

### Problem

`PrinterClient` has no camera awareness. The CLI's camera command bypasses `PrinterClient` and uses `BambuBinaryCameraStream` directly, duplicating connection logic that `PrinterClient` already owns.

### Background

Bambu printers use two camera protocols (determined by `model.quirks().camera_protocol()`):

- **Binary JPEG (port 6000, A1/P1 series)** — `src/camera/binary.rs` provides `BambuBinaryCameraStream`, a complete client that authenticates and streams JPEG frames over TLS. Persistent streaming connection.
- **RTSPS (port 322, X1/X2/H2/P2S series)** — `src/camera/rtsps.rs` provides helper utilities only (URL generation, proxy URI rewriting, timestamp correction). No RTSP client — consumers integrate with external media frameworks.

### Design questions to answer first

- **Streaming vs request/response** — Binary JPEG is a persistent stream, unlike FTPS's connect-operate-disconnect pattern. Does the `.with_ftps()` + lazy `storage()` pattern work for a long-lived stream?
- **Two protocols, one slot?** A printer uses either binary JPEG or RTSPS, never both. Single `camera()` accessor returning an enum, or separate methods? RTSPS has no connection state.
- **Type parameter impact** — Can camera reuse the existing `Conn: SecureConnect` connector, or does camera TLS differ from MQTT's?
- **Lazy connection** - like MQTT and FTPS, camera connection should be lazy and not required to instantiate a PrinterClient.

### Scope

Answer the design questions based on the current codebase, then write a concrete implementation plan. Do not start implementation without a plan.

---

## Phase 13: Door-open and active-fault telemetry accessors

### Problem

Same gap `print_status()` filled for `gcode_state`, found while auditing for other missing helpers after Phase 8: `ModelQuirks::is_door_open(&self, telemetry: &PrinterTelemetry)` (`src/quirks/mod.rs`) already does full per-model dispatch (X1 reads `home_flag` bit 23, H2/P2/X2 read the `stat` hex string, A1/A2 hardcode `false` — no sensor) and `diagnostics::hms::{decode_print_error, decode_hms_alert}` (`src/diagnostics/hms.rs`) already decode fault state — both fully tested — but `PrinterClient` caches neither, so consumers must manually retain a `TelemetryReport` and call these themselves on every check.

### Design (resolved — implement as stated)

- **`door_open()`**: cache `last_door_open: Option<bool>` on `PrinterClient`, set inside `poll_telemetry()` by calling `self.model.quirks().is_door_open(print)` whenever `report.print` is present (mirrors `last_home_flag`/`last_gcode_state`). Expose `pub fn door_open(&self) -> Option<bool>`. **On models without a door sensor (`has_door_sensor() == false`), `door_open()` must return `None`, not `Some(false)`**, regardless of telemetry observed — distinguishes "no sensor, inapplicable" from "sensor confirms closed." This is a deliberate deviation from the raw quirks method's contract (which returns `false` for sensorless models) — the cached accessor adds the `None` case on top.
- **`active_fault()`**: cache `last_print_error: Option<u32>` (the raw register, same "cache raw, decode on access" shape as `last_home_flag`), set inside `poll_telemetry()` from `print.print_error`. Expose `pub fn active_fault(&self) -> Option<DecodedPrintError>`, computed via `decode_print_error(self.last_print_error?)`. Unlike `is_all_axes_homed()`, collapsing "no telemetry observed yet" and "observed, register reads 0 (no fault)" into the same `None` is acceptable here — both cases warrant the same caller action (nothing to address).
- **`hms` array caching is out of scope for this phase** — it's a `Vec<HmsEntry>` (clone-per-update cost, unlike the scalar fields above) and not needed to close the gap that motivated this phase. Revisit only if a concrete need for multi-alert state (not just "is there a genuine fault") comes up.
- Both caches must be populated only inside `poll_telemetry()`, not `poll_raw()`/`poll_wire()`/`poll_until()` — those bypass deserialization and never touch report fields, so they cannot update either cache.

### Verification

Unit tests mirroring `test_print_status_cache_from_telemetry`/`test_home_flag_cache_and_advisory_warnings` (`tests/client_test.rs`): inject synthetic `stat`/`home_flag`/`print_error` telemetry against models on both sides of `has_door_sensor()` (e.g. X1C/H2D vs. P1S/A1) and confirm `door_open()`/`active_fault()` update correctly, including the `None`-for-no-sensor and pre-first-telemetry cases.

---

## Phase 14: AMS/tray and progress/temperature telemetry accessors (investigation)

### Problem

Beyond door-open and active-fault (Phase 13), `PrinterTelemetry` has other fields a consumer might reasonably want cached accessors for — AMS/tray state (`ams`, `ams_status`, `vt_tray`, `tray_exist_bits`) and print progress/temperature (`mc_percent`, `mc_remaining_time`, `layer_num`/`total_layers`, `bed_temper`/`nozzle_temper`/`chamber_temper`). Unlike Phase 13's two fields, there isn't a single obvious "first accessor" here — AMS state spans multiple interrelated sub-fields with their own existing types (`AmsStatusReport`, `AmsTray`, `AmsUnit` in `src/types/telemetry/ams.rs`), and progress/temperature fields change continuously rather than representing a discrete state flag consulted *between* polls at a decision point (which is what makes homed/busy/door/fault worth caching).

### Design questions to answer first

- Do these warrant `PrinterClient`-level caching at all, or is "call `poll_telemetry()` yourself and read the report" sufficient? Door-open/active-fault/homed/busy are valuable to cache specifically because they're consulted *between* polls (e.g. before issuing a command); progress/temperature are typically consumed directly off each report inside a monitoring loop already — caching may not address a real gap.
- If AMS state is worth exposing, what's the right shape — one bundled accessor, or several scoped to specific questions (e.g. "is a tray loaded", "is this filament dry")?
- `TelemetryReport::bed_temperatures()` (`src/types/telemetry/mod.rs`) already exists as a purpose-built multi-wire-format decoder — confirm whether a caching accessor adds anything beyond what it already provides before building one.

### Scope

Answer the design questions based on the current codebase, then write a concrete implementation plan. Do not start implementation without a plan.

---

## Progress Tracker

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Foundation types and library adapters | Complete |
| 2 | `PrinterClient` struct migration (backward compatible) | Complete |
| 3 | Lazy MQTT connection and constructor redesign | Complete |
| 4 | Lazy FTPS connection and API alignment | Complete |
| 5 | Documentation | Complete |
| 6 | Move message buffer to `BambuMqttClient` | Complete |
| 7 | Advisory homed-state tracking from `home_flag` | Complete |
| 8 | Homing completion detection | Complete |
| 9 | Sequence ID correlation hygiene for query commands | Complete |
| 10 | CLI dependency leakage | Complete |
| 11 | Migrate CLI argument parsing to `clap` | Complete |
| 12 | Camera integration in `PrinterClient` | Not Started |
| 13 | Door-open and active-fault telemetry accessors | Not Started |
| 14 | AMS/tray and progress/temperature telemetry accessors (investigation) | Not Started |
