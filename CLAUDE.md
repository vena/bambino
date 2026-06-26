# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```sh
cargo build                                          # Default host build (tokio + rustls)
cargo test                                           # Run all tests
cargo test --lib                                     # Library tests only
cargo test test_name                                 # Single test by name
cargo build --no-default-features --features alloc --lib  # no_std compatibility check (must pass)
```

Every change must compile under both the default `tokio` feature set and the `no_std`+`alloc` library target. Run `cargo clippy` as part of the verification gate. The `--lib` flag scopes the no_std check to library code only — the CLI is host-only. Use `#[cfg(not(feature = "std"))]` imports from `alloc` (String, Vec, format!) for no_std paths.

## Architecture

**bambino** is a multi-platform async Rust crate for controlling Bambu Lab 3D printers over LAN. It compiles to three targets from one codebase: host (tokio/rustls), ESP-IDF (std), and bare-metal (embassy/no_std).

### Key Invariants

1. **No direct platform I/O in library code.** All network I/O goes through abstract traits in `src/io/` (`AsyncIo`, `TlsConnector`, `SecureConnect`, `AsyncUdpSocket`, `TimerProvider`). Never use `tokio::` or `std::net::` outside `src/io/`. `TlsConnector` wraps an existing stream (tokio/embassy); `SecureConnect` creates its own TCP+TLS connection (ESP-IDF). `TimerProvider::now_millis()` provides monotonic clock for platform-agnostic timeouts.

2. **All model-specific behavior goes through the quirks engine.** Access via `model.quirks()` — never match on `BambuModel` variants for behavioral dispatch. Strategy structs live in `src/quirks/models/`.

3. **MQTT commands follow the Payload+Request pattern** (`src/mqtt/commands.rs`, `src/diagnostics/kprofile.rs`):
   - A `#[derive(Serialize)]` payload struct with typed fields
   - A wrapper struct with a single `pub print: PayloadType` field (or `pushing:`, `system:`, `info:`)
   - An `impl` block with `pub fn new(...)` constructor

### Non-Obvious Type Decisions

- **Temperature fields** are `Option<f64>` — the wire sends both integers (H2D) and floats (P1S/A1). Bed and nozzle targets arrive as separate `_target_temper` fields (never composite-packed). Use `unpack_temperature()` only for `chamber_temper` on models with active chamber heaters, and for `ExtruderInfo.temp` on IDEX platforms.
- **`DeviceTelemetry`** appears at two wire locations: top-level `{"device": {...}}` for incremental updates, and nested inside `{"print": {"device": {...}}}` for pushall on H2/P2/X2 models.
- **`ExtruderInfo.temp`** uses the same composite packing as `chamber_temper` (values > 500 encode target << 16 | actual). Use `ExtruderInfo::temperatures()` to decode. The `ExtruderCollection.state` bitmask encodes extruder count (low 4 bits) and active index (bits 4–7); use `active_extruder_index()` / `extruder_count()`.
- **`PrinterClient::poll_telemetry()`** returns `TelemetryEvent` (discriminated enum), not raw `MqttMessage`. Use `poll_raw()` or `BambuMqttClient::poll_telemetry()` for raw access.
- **`AmsTray.id`** is `String` (wire sends `"0"`, not `0`). **`CtcInfo.temp`** is `u32` (composite-packed integers, not floats).
- **K-profile priming quirk:** the firmware silently ignores the first `extrusion_cali_get` after connection. `PrinterClient::get_k_profiles()` auto-primes; opt out via `set_k_profile_primed(true)`.

## Key Conventions

- `serde_json` is used with `default-features = false` — always use `serde_json::to_vec` (not `to_string`) for payloads.
- Library code uses the `log` crate facade (`log::debug!`, `log::trace!`, `log::warn!`) — never `println!`.
- `BambuError` has dual `Display` impls: `thiserror` under `std`, manual under `no_std` (kept in sync by `test_display_consistency`). `ProtocolViolation` uses `Cow<'static, str>`.
- Magic numbers are extracted into named `pub(crate) const` blocks in each module. Use existing constants rather than introducing new literals.
- All MQTT sequence IDs and task IDs must be clamped to 32-bit signed integer max (`TASK_ID_MAX`). Use `clamp_task_id()` for task IDs.
- Protocol specs live in `reference/` as numbered markdown files. Always verify field names and types against reference docs when adding or modifying commands. When external sources (pybambu, Bambuddy, Bambu Studio, wire captures) contradict a reference doc, update the reference doc with the correction and note the verification source.
- Use MODEL_MATRIX.md to track physical characteristics of printer models. When new information is learned about a printer model, update MODEL_MATRIX.md
- When adding public types, modules, traits, or changing conventions, update this file. Keep it concise — document constraints and gotchas, not API summaries.
- **PLAN.md phases must be self-contained.** Each phase must be implementable by a clean session with zero prior conversation context beyond the existing codebase. Inform the next session of what we learned and guide it by spelling out: the problem being solved (not just what to build), design constraints and trade-offs that shape the implementation, ordering dependencies between items, and which items are trivially independent. If a task has a hard design decision, state the options and either pick one or mark it as "decide first." A phase that requires reading git history or guessing at intent is underspecified.
