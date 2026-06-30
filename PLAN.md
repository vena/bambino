# bambino — Lazy connections and API consistency

**Important:** Before starting any phase, read this document in its entirety. Read the `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

**When completing a phase:** Update this PLAN.md marking the phase complete. Update the completed phases summary, strictly including **only** what is necessary to inform clean sessions implementing the next phases which cannot be learned from the code itself. Once summarized, remove the phase from PLAN.md.

---

## Phases 1–7: Complete

Phases 1–4 migrated `PrinterClient` to lazy connections for both MQTT and FTPS, with symmetric APIs for both protocols. Phase 5 updated all documentation. Phase 6 moved the MQTT message buffer from `PrinterClient` to `BambuMqttClient`, eliminating the split-brain read path and the `owned_by_printerclient` runtime flag. Phase 7 added advisory (non-blocking) homed-state tracking: `PrinterClient::last_home_flag` is cached opportunistically inside `poll_telemetry()` only, exposed via `is_axis_homed()`/`is_all_axes_homed()`, and consulted by `move_relative()`/`extrude()` to `log::warn!` (never error) on a known-unhomed axis.

**Decisions informing future phases:**

- **The `home_flag` cache only updates via `poll_telemetry()`.** `poll_raw()` and `poll_until()` (used internally by command-response methods) read the wire without touching `last_home_flag`. Phase 8's `home_axes_and_wait()` should track `mc_print_sub_stage` itself via its own `poll_until` matcher — don't assume the homed-state cache updates during that wait.

- **Consuming builders change type params; non-consuming builders return `Self`.** `.with_timer()` and `.with_ftps()` consume `self` because they change type parameters. `.with_mqtt_port()` and `.with_ftps_port()` return `Self`. Phase 12's camera builder must follow the same convention.
- **`ensure_*()` is the lazy connection pattern.** `ensure_mqtt()` and `ensure_ftps()` short-circuit on `Some`, otherwise connect lazily. `ensure_ftps()` uses `.take()` to consume `ftps_config`, so reconnection requires a new `PrinterClient`. Phase 12 should consider whether camera's persistent streaming nature needs a different reconnection story.
- **Each protocol's TLS config is independent.** FTPS may need `force_tls_1_2` (model quirk) while MQTT does not. Phase 12's camera TLS may also differ — don't assume a shared connector.
- **CLI storage now routes through `PrinterClient`.** Phase 12's camera CLI command should follow the same pattern rather than constructing protocol clients directly.
- **Message buffer is on `BambuMqttClient`.** `poll_telemetry()` drains buffered messages first, then reads the wire. `poll_wire()` bypasses the buffer (used by `PrinterClient::poll_until()`). `push_pending()` stashes non-matching messages. `PrinterClient` delegates all reads through these methods. `mqtt().await?` returns a client whose `poll_telemetry()` is safe to call directly — no split-brain, no warnings.

---

## Phase 8: Homing completion detection

### Problem

`home_axes()` returns immediately after publishing the `G28` gcode — the ack arrives almost instantly and only means "received," not "homing finished." Callers have no way to know when homing has actually completed.

### Findings (P1S, tested 2026-06-28, see Phase 7 for the full ack-envelope investigation)

Homing takes **~45-60 seconds** on a P1S (observed range; unverified on other models). The completion lifecycle is observable via incremental `push_status` telemetry:
1. Gcode ack arrives immediately (`result: "success"`) — tells you nothing about progress
2. `mc_print_sub_stage` changes `0 → 1` (homing in progress)
3. `home_flag` bits 0-2 are set progressively as each axis completes (X,Y home before Z on a P1S — see [REF-HOMEFLAG] in `reference/03_mqtt_telemetry.md`)
4. `mc_print_sub_stage` changes `1 → 0` (homing complete)

`mc_print_sub_stage`'s `1 → 0` transition is the reliable completion signal — not `home_flag` reaching all-set, since that only confirms axes, not the full routine (toolhead park, etc.). **However, this field is confirmed not homing-exclusive**: Bambuddy's independent reverse-engineering (`backend/app/services/bambu_mqtt.py`) documents the same field as a filament-change step indicator used by OrcaSlicer/BambuStudio to track load/unload progress — see the updated [REF-MOTO-HOME] in `reference/04_toolhead_thermal_motion.md`. A `0 → 1 → 0` blip is evidence of homing, not proof.

### Design decisions (resolved — implement as stated, don't re-derive)

- **`home_axes()` is unchanged.** Stays fire-and-forget, returns `Result<u16, BambuError>` exactly as today (pre-release API freedom doesn't mean change it gratuitously — there's simply no reason to here).
- **Split into a generic primitive and a homing-specific wrapper — don't name the primitive after homing.** `mc_print_sub_stage` is confirmed shared with filament-change tracking (Bambuddy, cited above), so a method that just watches this field cycle has no grounds to claim it's homing-specific. Naming it `wait_for_homing()` would assert a fact the signal alone can't support — exactly the kind of unproven claim to avoid. A future filament-change wait would watch the *identical* field with the *identical* cache-edge mechanism; it must not require a second, near-duplicate copy of that logic under a different name.
  - **`wait_for_sub_stage_idle(&mut self) -> Result<(), BambuError>`** — the primitive. Waits for `mc_print_sub_stage` to leave its baseline (`0`) and return to it. No homing claim anywhere in this layer. Fully standalone — no dependency on `home_axes()` having been called by this client, since a consumer may be observing a cycle triggered by the touchscreen, OrcaSlicer, or another instance of this library.
  - **`wait_for_homing(&mut self) -> Result<bool, BambuError>`** — the homing-specific convenience. Calls `wait_for_sub_stage_idle()`, then checks `is_all_axes_homed()` (Phase 7 — bits 0-2 are confirmed homing-exclusive, unlike `mc_print_sub_stage`) and returns whether homing was actually corroborated (`true`) or not (`false`, e.g. the cycle was probably something else, like a filament change). The name is earned by combining two independent signals, not by trusting the sub-stage field alone. Still doesn't error on a `false` — that's a legitimate, informative outcome, not a failure.
  - A `wait_for_filament_change()` (or similar) is a real future need now that this field's overlap is confirmed, but it's out of scope for this phase — don't build it speculatively. It would reuse `wait_for_sub_stage_idle()` with its own corroboration (e.g. AMS status), not duplicate the cache logic.
- **Implement the primitive via Phase 7's cache pattern, not a one-shot `poll_until` matcher.** Add `last_mc_print_sub_stage: Option<i32>` and a `sub_stage_idle_transitions: u64` generation counter to `PrinterClient`, both updated only inside `poll_telemetry()` (same gateway constraint as `last_home_flag` — `poll_raw()`/`poll_until()` don't touch them). On each call, compare the new `mc_print_sub_stage` to the cached previous value; a transition back to `0` after having been nonzero increments `sub_stage_idle_transitions`. `wait_for_sub_stage_idle()` snapshots the counter, then loops `poll_telemetry()` until the counter advances past the snapshot — it never inspects current state directly. This correctly handles a cycle already in progress when the wait starts (the counter hasn't incremented for that cycle yet, so the wait still catches its eventual completion) and avoids resolving on a stale `0` left over from before anything happened.
- **Must temporarily override the command timeout.** `DEFAULT_COMMAND_TIMEOUT_SECS` is 10 seconds; homing takes 45–60+ seconds. `wait_for_sub_stage_idle()` must call `set_command_timeout()` with a generous value (e.g. 90 seconds) before looping, and restore the caller's previous timeout value afterward — on both the success and error/timeout paths — so the override doesn't leak into unrelated subsequent commands.
- **If the relevant cycle already fully completed before the wait is called, it will not return immediately.** It waits for the *next* cycle (the natural consequence of the generation-counter design) and times out after 90s if none occurs. This is the correct trade-off for "wait for an externally-triggered event" — not a bug to fix.

### Verification

Unit test with a mock MQTT stream feeding a synthetic message sequence (sub_stage=1, sub_stage=0) confirming `wait_for_sub_stage_idle()` resolves only after both are observed, not on a premature `0`. Add a case where `mc_print_sub_stage` is already `1` when the wait starts (simulating an externally-triggered cycle already underway) to confirm it still catches the eventual `1 → 0`. Add cases for `wait_for_homing()` confirming it returns `true` when `is_all_axes_homed()` agrees after the cycle and `false` when it doesn't (simulating a filament-change blip instead of homing). Integration test using the probe CLI's existing `-t home_axes` capture (60s window) to confirm the `0 → 1 → 0` transition order holds against real hardware.

---

## Phase 9: Sequence ID correlation hygiene for query commands

### Problem

`get_version()` and `get_k_profiles()` use `poll_until` matchers that check only `command == "get_version"` / `command == "extrusion_cali_get"` — neither compares the response's `sequence_id` against the one we sent. Discovered while investigating Phase 7's command-ack envelope. If a second MQTT client (OrcaSlicer, Bambu Studio, or a second instance of our own library) is connected to the same printer and issues the same query while we're waiting, our `poll_until` could consume *their* response instead of ours.

### Why this is hygiene, not an active bug

Both `get_version()` and `get_k_profiles()` return printer state that's invariant regardless of who asked — there's no request parameter that would make two different callers' valid answers differ. Consuming a stray response from another client asking the same question still returns factually correct, current printer state. The risk is latent: it only becomes a real correctness bug if a future query-style command's response is parameterized by something in the request (i.e., two different valid answers exist depending on what was asked). Do not frame this as fixing broken behavior — it's making the existing pattern correct by default before something is built on top of it that actually needs it.

### Fix

1. Update the `poll_until` matcher closures in `get_version()` and `get_k_profiles()` (`src/client/ams.rs`) to also compare the response's echoed `sequence_id` against the `seq` value generated for that call, not just the command name. Apply the same pattern to any other existing `poll_until`-based methods that don't already check it.
2. Consider seeding `PrinterClient`'s sequence counter (`sequence_counter`, see `INITIAL_SEQUENCE_ID` in `src/client/mod.rs`) from `TimerProvider::now_millis()` at connect time instead of the fixed constant `10000`. This de-correlates independent sessions (e.g., two processes both running our library against the same printer) without needing a new RNG abstraction — `TimerProvider` already exists uniformly across host/ESP-IDF/embassy targets.
3. **True random sequence ID generation was considered and rejected.** `no_std`/embassy targets have no portable entropy source without adding a new platform abstraction trait (mirroring `TimerProvider`/`TlsConnector`), which is disproportionate complexity for a marginal benefit once matchers actually check `sequence_id`. Don't revisit this without a concrete reason the timer-seeded approach is insufficient.
4. For context: Bambuddy hardcodes fixed sequence IDs (e.g. `"20000"` for `project_file`) for a *different* reason — multi-client disambiguation ("is this command mine or did Orca/Studio send it"), not response validation. That's not a pattern to copy here; our fix is about correlating our own request to our own response, not detecting other clients' traffic.

### Verification

Extend the existing `get_version()`/`get_k_profiles()` unit tests to inject a decoy response with the correct `command` but a mismatched `sequence_id`, and confirm `poll_until` does not consume it (keeps waiting for the correctly-sequenced response, or times out if only the decoy is ever sent).

---

## Phase 10: CLI dependencies leak into library `tokio` feature

### Problem

`crossterm` and `env_logger` are optional deps gated behind `tokio` in `Cargo.toml`, but neither is used in library code — only by `src/bin/bambino-cli/`. The CLI shipping in the same crate is intentional (README: "Ships as a binary in the same crate"), but the dep gating means any external consumer using `bambino` with default features pulls in a terminal manipulation library and a concrete log sink.

### Investigation

- Confirm no library code imports `crossterm` or `env_logger` (already verified).
- Evaluate options: (a) gate both behind a dedicated `cli` feature not implied by `tokio`, or (b) accept the current state since external consumers are not the primary use case yet at 0.1.0.
- If (a), verify that `cargo build --bin bambino-cli` still works when `cli` is enabled and that `cargo build --lib` no longer pulls in `crossterm`/`env_logger`.

### Fix

Apply if warranted. Verify `cargo build`, `cargo build --no-default-features --features alloc --lib`, and `cargo test` all pass.

---

## Phase 11: Migrate CLI argument parsing to `clap`

### Prerequisites

Phase 10 must be complete first. If Phase 10 introduces a dedicated `cli` feature for gating CLI-only deps (`crossterm`, `env_logger`), `clap` must be gated the same way — it's exclusively a CLI dependency and must never affect `cargo build --lib` or the `no_std`/`alloc` target.

### Problem

`src/bin/bambino-cli/` hand-rolls argument parsing throughout: `main.rs` manually strips `--verbose`/`-v` before positional matching; `control.rs` re-implements a length check (`if action_args.len() < N`) and a hand-written usage string per action (`home`, `move`, `extrude`, `fan`, `temp`, `led`, `pause`/`resume`/`stop`, `gcode`, `gcode-raw`, `speed`, `clear-error`, `airduct`, `calibrate`, `ams dry`/`dry-stop`); `probe.rs` has its own ad-hoc `while` loop for `--output`/`-o` and `--tests`/`-t`. Two real bugs were found this way during Phase 7 testing: the `move` and `extrude` actions both required their *optional* trailing `feedrate` argument due to off-by-one length checks, contradicting their own usage strings (`Edit` history: `src/bin/bambino-cli/control.rs`). The usage strings are hand-maintained separately from `main.rs`'s static `print_usage()` text and can drift out of sync with actual validation — which is exactly how the bug went unnoticed.

### Design decisions (resolved — implement as stated)

- **Use `clap`'s derive API, not the builder API.** Confirmed via current `clap` docs: the derive macro's subcommand-enum pattern (`#[derive(Subcommand)]`) maps directly onto the nested command/action structure already in `main.rs`/`control.rs` — each top-level command and each `control` action becomes an enum variant with typed fields, replacing the manual length checks and `.parse()` calls entirely.
- **Feature selection:** `clap`'s default features (`std`, `color`, `help`, `usage`, `error-context`, `suggestions`) are already lean; add the `derive` feature (opt-in, not default) for the macro. No need for `cargo`, `env`, `unicode`, or `wrap_help`.
- **Replace `main.rs`'s hand-written `print_usage()` and `--verbose` stripping entirely.** `clap` auto-generates `--help`/`-h` output from the same struct/enum definitions used for parsing, eliminating the drift risk that caused the `move`/`extrude` bug. The existing `help`/`-h`/`--help` match arm in `main.rs` becomes redundant once `clap` is wired up — remove it rather than keeping both paths.
- **Full migration, not partial.** Migrate every subcommand (`discover`, `info`, `monitor`, `dump`, `probe`, `control` + all its actions, `files` + all its actions, `camera` + its actions) in one pass. Leaving some commands on hand-rolled parsing and others on `clap` would mean inconsistent help text and error messages across the same binary, which is worse than the current uniform-but-buggy state.
- **Business logic is unaffected.** Only the argument-extraction layer changes — every call into `PrinterClient`/`create_printer` etc. stays the same. This is mechanical, not a design problem, even though the surface area (6 files: `main.rs`, `control.rs`, `storage.rs`, `camera.rs`, `discover.rs`, `probe.rs`) is large.

### Scope note

If this turns out to be too large for one session despite being mechanical, the natural split point is **top-level command dispatch (`main.rs`) first, then `control.rs`'s actions** (by far the largest single file in argument-parsing terms), with `storage.rs`/`camera.rs`/`probe.rs` as a follow-up — but attempt it as one phase first.

### Verification

`cargo build`, `cargo build --no-default-features --features alloc --lib` (confirm `clap` does not leak into the no_std target), `cargo test`, and manually re-running the exact commands that exposed the original `move`/`extrude` bug to confirm `clap` rejects/accepts arguments correctly.

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
| 8 | Homing completion detection | Not Started — investigation complete, ready to implement |
| 9 | Sequence ID correlation hygiene for query commands | Not Started |
| 10 | CLI dependency leakage | Not Started |
| 11 | Migrate CLI argument parsing to `clap` | Not Started |
| 12 | Camera integration in `PrinterClient` | Not Started |
