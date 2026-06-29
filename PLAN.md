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

Most `PrinterClient` command methods are fire-and-forget: they publish an MQTT payload and return the packet ID without checking the printer's response. The printer does respond to commands — with ack/nack results, error codes, and prompts (e.g. "home axes before moving"). We're ignoring all of that today.

Real example: on a P1S, attempting to move the bed or print head when the printer hasn't been homed recently causes the firmware to reject the command and prompt for homing. Our library doesn't surface this — the command silently does nothing.

### Prerequisites

Phase 6 (unified message buffer in `BambuMqttClient`) must be complete. Without a single read path, response checking would re-introduce the split-brain problem.

### Investigation (requires real printer)

This phase requires manual testing against real hardware to catalog what the printer actually sends back in response to commands. The firmware's response format is not fully documented in our reference docs.

**Step 1 — Capture response patterns.** Use `bambino-cli` (or a test harness) with `RUST_LOG=debug` to observe what the printer sends back after common commands:

- Motion commands when unhomed (G28, relative moves)
- Temperature commands at/beyond limits
- Print control when no print is active (pause/resume/stop)
- AMS commands when no AMS is connected
- Calibration commands during an active print
- LED/fan commands (do these ack at all?)

Document the response payload structure for each case: which JSON fields indicate success vs rejection, how errors are keyed, whether the sequence ID is echoed back.

**Step 2 — Design the response model.** Based on captured data, determine:

- Is there a uniform ack/nack envelope, or does each command family have its own response shape?
- Can we match responses to commands via sequence ID?
- Should rejected commands return `Err`, or should we return a `CommandResult` enum that distinguishes "executed" from "rejected with reason"?

**Step 3 — Implement selectively.** Start with commands where silent failure is most dangerous (motion, temperature), not every command at once.

### Verification

Unit tests can cover the response parsing once the wire format is known. Integration testing against a real printer for the end-to-end flow.

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
| 7 | Command response validation | Not Started |
| 8 | CLI dependency leakage | Not Started |
| 9 | Camera integration in `PrinterClient` | Not Started |
