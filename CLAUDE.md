# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```sh
cargo build                                          # Default host build (tokio + rustls)
cargo build --bin bambino-cli                         # CLI binary only
cargo test                                           # Run all tests
cargo test --lib                                     # Library tests only
cargo test test_name                                 # Single test by name
cargo build --no-default-features --features alloc --lib  # no_std compatibility check (must pass)
```

Every change must compile under both the default `tokio` feature set and the `no_std`+`alloc` library target. Run `cargo clippy` as part of the verification gate and fix warnings before considering a phase complete. The `--lib` flag scopes the no_std check to library code only — the CLI is a host-only verification tool and is not part of the embedded target. Use `#[cfg(not(feature = "std"))]` imports from `alloc` (String, Vec, format!) for no_std paths.

## Architecture

**bambino** is a multi-platform async Rust crate for controlling Bambu Lab 3D printers over LAN. It compiles to three targets from one codebase: host (tokio/rustls), ESP-IDF (std), and bare-metal (embassy/no_std).

### Platform Abstraction Layer (`src/io/`)
All network I/O goes through abstract traits: `AsyncIo`, `TlsConnector`, `AsyncUdpSocket`, `TimerProvider`. Platform-specific implementations live in `tokio.rs`, `esp_idf.rs`, `embassy.rs`. Never use `tokio::` or `std::net::` directly in library code outside `src/io/`.

### Quirks Engine (`src/quirks/`)
Printer-model differences (TLS modes, fan rounding, Z-axis safety, door sensors) are handled polymorphically via the `ModelQuirks` trait. Each model family has a strategy struct in `src/quirks/models/` (a1.rs, p1.rs, p2.rs, x1.rs, x2.rs, h2.rs). All model-specific behavior modifications must go through the quirks engine. Access quirks via `model.quirks()` — never match on `BambuModel` variants for behavioral dispatch elsewhere.

### Command Pattern (`src/mqtt/commands.rs`, `src/diagnostics/kprofile.rs`)
MQTT commands follow a strict Payload+Request pattern:
1. A `#[derive(Serialize)]` payload struct with typed fields
2. A wrapper struct with a single `pub print: PayloadType` field (or `pushing:`, `system:`, `info:` depending on the MQTT envelope)
3. An `impl` block with `pub fn new(...)` constructor

### Client Coordinator (`src/client.rs`)
`PrinterClient` wraps MQTT + optional FTPS. Client methods use the private `publish_request` helper:
```rust
let seq = self.next_sequence_id();
let req = SomeRequest::new(args, seq);
self.publish_request(&req).await
```
`publish_request<T: Serialize>` handles serialization via `serde_json::to_vec` and publishes to MQTT in one step.
Public helper types (`PrintSpeed`, `CalibrationOption`) live alongside `PrinterClient` in this module. `CalibrationOption` is a newtype bitmask supporting `BitOr` for combining calibration routines. `PrintJobConfig` (in `src/mqtt/commands.rs`, re-exported via `mqtt::PrintJobConfig`) is a builder struct for print job submission — use it with `start_print(&config)` instead of positional parameters. `clamp_task_id()` returns `u32` (not `String`).

### Reference Documentation (`reference/`)
Protocol specs live in numbered markdown files. When adding or modifying commands, always verify field names and types against the reference docs — PLAN.md field names may be approximate.

## Key Conventions

- All MQTT sequence IDs and task IDs must be clamped to 32-bit signed integer max (`TASK_ID_MAX` in `src/mqtt/commands.rs`). Use `clamp_task_id()` for task IDs.
- `serde_json` is used with `default-features = false` — always use `serde_json::to_vec` (not `to_string`) for payloads.
- The `BambuModel` enum and `resolve_model()` live in `src/models.rs` as the canonical model identity module. Re-exported from `src/lib.rs` and `src/discovery/mod.rs` for backward compatibility. Internal crate code imports via `crate::models::BambuModel`.
- The CLI (`src/bin/bambino-cli/`) is a testing/verification tool, not part of the library API.
- Library code uses the `log` crate facade (`log::debug!`, `log::trace!`, `log::warn!`) for diagnostic output — never `println!`. The CLI initializes `env_logger` from the `-v` flag. No `#[cfg(feature = "std")]` gates are needed on log statements.
- `BambuError` derives `Debug` and `Clone`. Under `std`, `thiserror` provides `Display`/`Error`. Under `no_std`, a manual `Display` impl is kept in sync (verified by `test_display_consistency`). `ProtocolViolation` uses `Cow<'static, str>` under `alloc`/`std` to accept both static and dynamic error messages.
- Magic numbers are extracted into named `pub(crate) const` blocks in each module (e.g., `PACKET_TYPE_*` and `MQTT_*` in `src/mqtt/client.rs`, `FTP_*` in `src/ftps/client.rs`, `AMS_*` in `src/ams/parser.rs`). Use existing constants rather than introducing new magic numbers.
- `BambuError` implements `From<SocketError>`, so `?` can be used directly in functions returning `Result<_, BambuError>` for socket operations.
- When adding public types, modules, traits, or changing conventions, update this file to reflect the new state. BE CONCISE. CLAUDE.md must stay in sync with the codebase — treat it as a living document, not a one-time snapshot.
- Automatically invoke the find-docs skill (context7) for any queries regarding package versions, framework setups, or API usage, especially for `serde`, `serde_json`, `tokio`, `embedded-io-async`, `thiserror`, and other dependencies. Don't rely solely on training data for crate APIs — versions and interfaces change.
