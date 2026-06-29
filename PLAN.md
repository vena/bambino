# bambino — Lazy connections and API consistency

**Important:** Before starting any phase, read this document in its entirety. Read the `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

**When completing a phase:** Update this PLAN.md marking the phase complete. Update the completed phases summary, strictly including **only** what is necessary to inform clean sessions implementing the next phases which cannot be learned from the code itself. Once summarized, remove the phase from PLAN.md.

---

## Phases 1–6: Complete

Phases 1–4 migrated `PrinterClient` to lazy connections for both MQTT and FTPS, with symmetric APIs for both protocols. Phase 5 updated all documentation. Phase 6 moved the MQTT message buffer from `PrinterClient` to `BambuMqttClient`, eliminating the split-brain read path and the `owned_by_printerclient` runtime flag.

**Decisions informing future phases:**

- **Consuming builders change type params; non-consuming builders return `Self`.** `.with_timer()` and `.with_ftps()` consume `self` because they change type parameters. `.with_mqtt_port()` and `.with_ftps_port()` return `Self`. Phase 9's camera builder must follow the same convention.
- **`ensure_*()` is the lazy connection pattern.** `ensure_mqtt()` and `ensure_ftps()` short-circuit on `Some`, otherwise connect lazily. `ensure_ftps()` uses `.take()` to consume `ftps_config`, so reconnection requires a new `PrinterClient`. Phase 9 should consider whether camera's persistent streaming nature needs a different reconnection story.
- **Each protocol's TLS config is independent.** FTPS may need `force_tls_1_2` (model quirk) while MQTT does not. Phase 9's camera TLS may also differ — don't assume a shared connector.
- **CLI storage now routes through `PrinterClient`.** Phase 9's camera CLI command should follow the same pattern rather than constructing protocol clients directly.
- **Message buffer is on `BambuMqttClient`.** `poll_telemetry()` drains buffered messages first, then reads the wire. `poll_wire()` bypasses the buffer (used by `PrinterClient::poll_until()`). `push_pending()` stashes non-matching messages. `PrinterClient` delegates all reads through these methods. `mqtt().await?` returns a client whose `poll_telemetry()` is safe to call directly — no split-brain, no warnings.

---

## Phase 7: Command response validation

### Problem

Most `PrinterClient` command methods are fire-and-forget: they publish an MQTT payload and return the packet ID without checking the printer's response. We want to understand what the printer sends back and whether we can surface failures.

### Prerequisites

Phase 6 (unified message buffer in `BambuMqttClient`) must be complete. Without a single read path, response checking would re-introduce the split-brain problem.

### Investigation findings (P1S, tested 2026-06-28)

A `bambino-cli probe` command was added to send commands and capture all MQTT responses within a timed window. Three runs were performed against a P1S, all from an unhomed state (each run's `home_axes` test re-homes the printer; the printer was power-cycled between runs to return to unhomed). Raw captures are in `probe_report.json` at the project root.

**Step 1 findings — Command ack envelope:**

All commands produce a uniform ack response with the same shape:
```json
{"print": {"command": "<echoed>", "param": "<echoed>", "reason": "success", "result": "success", "sequence_id": "<echoed>"}}
```
- `command` echoes the command name (`gcode_line`, `pause`, `stop`, `clean_print_error`, etc.)
- `sequence_id` is echoed back, confirming correlation works
- LED commands use a `system` envelope instead of `print`, and echo all parameters (led_node, led_mode, timing fields)
- Gcode commands echo the full gcode string in `param`

**Key finding: the P1S firmware never sends a rejection/nack over MQTT.** Every command tested returned `result: "success"`, including:
- Motion commands (Z and X relative moves) when unhomed — acked as success, motion executes partially (small jog visible, same as touchscreen behavior)
- Pause/resume/stop when no print is active — acked as success, no-op
- Clear error when no error exists — acked as success, no-op

The "home axes before moving" prompt described in the original problem statement is a **touchscreen UI behavior only** — it does not propagate over MQTT. The motion controller executes the gcode regardless of homed state.

**Step 1 findings — Telemetry during long-running commands:**

Homing (`G28`) takes ~45 seconds on a P1S. The completion lifecycle is observable via incremental `push_status` messages:
1. Gcode ack arrives immediately (`result: "success"`)
2. `mc_print_sub_stage` changes `0 → 1` (homing in progress)
3. `home_flag` changes during homing (observed: `6374672` unhomed → `6374675` mid-homing → `6374679` homed)
4. `mc_print_sub_stage` changes `1 → 0` (homing complete)

`mc_print_sub_stage` is the reliable completion signal.

**`home_flag` bitmask — per-axis homed state (confirmed via OrcaSlicer source):**
- Bit 0: X axis homed
- Bit 1: Y axis homed
- Bit 2: Z axis homed
- Bit 11: store-to-SD-card
- Bit 18: wired/Ethernet connection
- Bit 23: door open (X1 family only; other models use the `stat` field)

Observed values from P1S probe runs:
- `6374672` (`0x00614510`): unhomed — bits 0-2 all zero
- `6374675` (`0x00614513`): mid-homing — X,Y homed (bits 0-1), Z not yet (bit 2 zero)
- `6374679` (`0x00614517`): fully homed — bits 0-2 all set

**Step 1 findings — Background telemetry noise:**

The printer sends incremental `push_status` updates every ~1-2 seconds regardless of commands. These carry rotating subsets of telemetry (bed_temper, nozzle_temper, wifi_signal, AMS state, lights_report). They are interleaved with command acks and use their own independent sequence_id counter (starting from 0/1, separate from the 10000+ range used by our commands).

**Step 2 — Design implications:**

- **No ack/nack distinction exists** on the P1S for gcode commands. The ack means "received and dispatched," not "executed successfully." This may differ on other models (untested).
- **Sequence ID correlation works.** Our command sequence IDs (10001+) are echoed back, and the printer's own telemetry uses a separate counter, so matching is unambiguous.
- **A `CommandResult` enum is not useful** given that every response is `result: "success"`. The original design question is moot for the P1S.
- **Completion detection for long-running commands** (homing, calibration) is feasible by watching `mc_print_sub_stage` transitions via `poll_until`.

### External implementation review (2026-06-29)

**OrcaSlicer** (`src/slic3r/GUI/DeviceManager.cpp`, `StatusPanel.cpp`, `RecenterDialog.cpp`):
- Parses `home_flag` from MQTT telemetry via `parse_home_flag()`, stores as `m_home_flag`
- `is_axis_at_home(axis)` checks bits 0/1/2 for X/Y/Z respectively
- Before every jog button press, calls `is_axis_at_home()` — if not homed, shows a "Please home all axes" dialog (`RecenterDialog`) with "Go Home" / "Close" buttons
- **Strict policy:** the move is blocked entirely until the user homes. No "move anyway" option
- Command sending is fire-and-forget (`publish_gcode`) — no response parsing
- Homing uses a bare `G28` (or `back_to_center` MQTT command on newer models via `m_support_mqtt_homing`)

**Bambuddy** (`backend/app/services/bambu_mqtt.py`, `frontend/src/pages/PrintersPage.tsx`):
- Parses `home_flag` for SD card (bit 11), door open (bit 23, X1 only), and wired network (bit 18) — but **not** for homed state
- Motion commands (`move_axis`, `home_axes`) are fire-and-forget via `send_gcode()`
- "Not homed" warning is a **blanket first-use prompt** per browser session (stored in `sessionStorage`), not based on actual printer state
- Offers "move anyway" option that sends `M211 S0` (disable soft endstops) before the move, then re-enables with `M211 S1`

**Conclusion:** No third-party client attempts firmware-level rejection detection. The universal pattern is:
1. Read `home_flag` bits 0-2 from telemetry to know if axes are homed
2. Check client-side before sending motion commands
3. Either block the move (OrcaSlicer) or warn and use `M211 S0` to bypass endstops (Bambuddy)

### Remaining investigation

- **Other models.** The P1S may not be representative. H2D/X1C/A1 models may have different ack behavior. The `probe` CLI command can be run against any model.
- **Error conditions not yet tested:** temperature commands beyond model limits, calibration during a print, AMS commands without AMS connected, gcode with syntax errors.

### Implementation plan

Based on probe data and external review, Phase 7 has three concrete deliverables:

1. **Parse `home_flag` bits 0-2 into per-axis homed state.** Add `is_axis_homed(axis)` and `is_all_axes_homed()` methods to `PrinterClient` (or on the telemetry report). The `home_flag` field already exists in `TelemetryReport`; this just decodes the bits. Gate motion commands (`move_relative`, `extrude`) behind a homed check, returning a new `BambuError::NotHomed` variant. Callers who want to bypass (like Bambuddy's force mode) can use `send_gcode_raw()`.

2. **Add completion detection for homing.** `home_axes()` currently returns immediately after publishing. Add a `home_axes_and_wait()` (or make `home_axes` await completion) that uses `poll_until` watching `mc_print_sub_stage` transition `0 → 1 → 0`. This gives callers a way to know when homing is actually done.

3. **Parse the command ack envelope.** Even though the P1S always returns `result: "success"`, the ack structure is uniform and sequence-ID-correlated. Parsing it costs little and future-proofs against models or firmware versions that do send rejections. Add a `CommandAck` struct and optionally return it from `publish_request`.

### Verification

Unit tests can cover the response parsing once the wire format is known. The `bambino-cli probe` command handles integration testing against real hardware.

---

## Phase 8: CLI dependencies leak into library `tokio` feature

### Problem

`crossterm` and `env_logger` are optional deps gated behind `tokio` in `Cargo.toml`, but neither is used in library code — only by `src/bin/bambino-cli/`. The CLI shipping in the same crate is intentional (README: "Ships as a binary in the same crate"), but the dep gating means any external consumer using `bambino` with default features pulls in a terminal manipulation library and a concrete log sink.

### Investigation

- Confirm no library code imports `crossterm` or `env_logger` (already verified).
- Evaluate options: (a) gate both behind a dedicated `cli` feature not implied by `tokio`, or (b) accept the current state since external consumers are not the primary use case yet at 0.1.0.
- If (a), verify that `cargo build --bin bambino-cli` still works when `cli` is enabled and that `cargo build --lib` no longer pulls in `crossterm`/`env_logger`.

### Fix

Apply if warranted. Verify `cargo build`, `cargo build --no-default-features --features alloc --lib`, and `cargo test` all pass.

---

## Phase 9: Camera integration in `PrinterClient`

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

## Progress Tracker

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Foundation types and library adapters | Complete |
| 2 | `PrinterClient` struct migration (backward compatible) | Complete |
| 3 | Lazy MQTT connection and constructor redesign | Complete |
| 4 | Lazy FTPS connection and API alignment | Complete |
| 5 | Documentation | Complete |
| 6 | Move message buffer to `BambuMqttClient` | Complete |
| 7 | Command response validation | In Progress — P1S probe complete, external review pending |
| 8 | CLI dependency leakage | Not Started |
| 9 | Camera integration in `PrinterClient` | Not Started |
