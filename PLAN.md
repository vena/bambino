# bambino — Lazy connections and API consistency

**Important:** Before starting any phase, read `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

**When completing a phase:** Update this PLAN.md marking the phase complete. Update the completed phases summary, strictly including **only** what is necessary to inform clean sessions implementing the next phases which cannot be learned from the code itself.

---

## Phases 1–3: Complete

Phases 1–3 migrated `PrinterClient` from requiring a pre-connected `BambuMqttClient` at construction to supporting lazy MQTT connection via a `SecureConnect` connector.

**Decisions informing future phases:**

- **Consuming builders change type params; non-consuming builders return `Self`.** `.with_timer(timer)` consumes `self` because it changes the `Timer` type parameter. `.with_mqtt_port(port)` returns `Self` because it only changes a field value. Phase 4's `.with_ftps()` must follow the consuming pattern since it changes `RawIO`, `Tls`, and `Factory`.
- **`ensure_*()` is the lazy connection pattern.** `ensure_mqtt()` short-circuits on `Some`, otherwise calls `connector.secure_connect()` + `BambuMqttClient::connect()`. Phase 4's `ensure_ftps()` should mirror this.
- **CLI connection helper absorbs model resolution.** `create_printer()` in `connection.rs` validates params, creates the `TokioSecureConnector`, resolves the model, and returns a fully-configured lazy `PrinterClient`. Phase 4's FTPS support will extend this helper or the call sites — the CLI's `storage.rs` currently bypasses `PrinterClient` entirely.
- **`TokioFtpDataStreamFactory`** was added in Phase 1 specifically to replace the CLI's private `TokioDataStreamFactory` during Phase 4's storage migration.
- **`new_with_storage()` still exists** in `storage.rs` in its own `PreConnected<IO>` impl block. Phase 4 removes it (replaced by `.with_ftps()`).

---

## Phase 4: Lazy FTPS Connection and API Alignment

### Problem

After phase 3, MQTT has lazy connection but FTPS does not. The CLI's storage command still bypasses `PrinterClient`. The MQTT and FTPS APIs are asymmetric. This phase completes the lazy connection story and aligns both protocols to a consistent API.

### Design: MQTT / FTPS symmetry

After this phase, both protocols follow the same pattern:

| Aspect | MQTT | FTPS |
|--------|------|------|
| Configure | `new(connector, ...)` | `.with_ftps(tls, factory)` |
| Port override | `.with_mqtt_port(port)` | `.with_ftps_port(port)` |
| Default port | 8883 | 990 |
| Eager connect | `connect_mqtt().await?` | `connect_ftps().await?` |
| Lazy connect | auto on first MQTT method | auto on first storage method |
| Raw access | `mqtt().await?` → `Result<&mut BambuMqttClient>` | `storage().await?` → `Result<&mut BambuFtpsClient>` |
| Status | `mqtt_connected()` | `ftps_connected()` |

The one inherent asymmetry: the MQTT connector is provided at `new()` because it determines the primary type parameter `Conn::Stream`. FTPS adapters are provided via `.with_ftps()` because FTPS is optional. This reflects domain reality, not an API inconsistency.

### `.with_ftps()` builder

A consuming builder that changes the `RawIO`, `Tls`, and `Factory` type parameters. Stores the adapters in `ftps_config` for lazy connection. The consumer's FTPS TLS config may differ from MQTT's (e.g., `force_tls_1_2` for P2S/X2D models), so the FTPS `TlsConnector` is independent.

Type constraint: `.with_ftps()` can't be a `&mut self` method because it changes type parameters. It must consume `self` and return a new type. This means FTPS configuration happens at construction time (builder chain), not at runtime.

### `ensure_ftps()`

Private helper mirroring `ensure_mqtt()`. If `self.ftps.is_some()`, short-circuit. Otherwise, take `(tls, factory)` from `self.ftps_config` via `.take()`, create a raw TCP connection to port `self.ftps_port` via `factory.create_data_stream()`, then call `BambuFtpsClient::connect()` which moves `tls` and `factory` into the FTPS client. Error if FTPS was never configured.

Note: `.take()` means the config is consumed on first connection. Reconnecting requires reconstructing the `PrinterClient` with `.with_ftps()` again. This is acceptable — FTPS reconnection is not a hot path.

### Raw accessor alignment

Change `mqtt()` from synchronous `Option<&mut ...>` to `async fn mqtt() -> Result<&mut BambuMqttClient<Conn::Stream>, BambuError>` with auto-connect. Make `storage()` match: `async fn storage() -> Result<&mut BambuFtpsClient<...>, BambuError>`. Both auto-connect, both return `Result`, both have synchronous `_connected()` companions for non-connecting state checks.

### Removals

Remove `new_with_storage()` — replaced by `.with_ftps()` builder + lazy connection. Keep `attach_storage()` for direct injection of a pre-connected `BambuFtpsClient` (test mocks, Embassy).

### CLI storage migration

Rewrite `src/bin/bambino-cli/storage.rs` to use `PrinterClient` with `.with_ftps()` instead of constructing `BambuFtpsClient` directly. Delete the private `TokioDataStreamFactory` (replaced by the library's `TokioFtpDataStreamFactory` from phase 1).

### Verification

```sh
cargo build && cargo test && cargo clippy && cargo build --no-default-features --features alloc --lib
```

---

## Phase 5: Documentation

### Problem

After phases 1–4, the `PrinterClient` API has changed significantly. The doc examples and README show the old pre-connected API.

### Changes

**`src/lib.rs` doc example** — Update to show lazy connection via `PrinterClient::new()` with a `TokioSecureConnector`.

**README "Connect" section** — Replace the 6-line connection plumbing with the new builder API. Show both construction paths (lazy and pre-connected via `from_mqtt()`).

**README "File transfer" section** — Show `PrinterClient` with `.with_ftps()` as the primary example. Keep direct `BambuFtpsClient` usage in a "Direct protocol access" note.

**README raw access** — Document `mqtt().await?` and `storage().await?` as async `Result`-returning accessors.

**CLAUDE.md** — Update the "Non-Obvious Type Decisions" section for `PrinterClient` to reflect the new constructor patterns, lazy connection semantics, builder methods, and `Conn: SecureConnect` type parameter.

### Verification

```sh
cargo build && cargo test && cargo clippy && cargo doc --no-deps
```

Verify `cargo doc` produces no warnings for broken doc links.

---

## Phase 6: CLI dependencies leak into library `tokio` feature

### Problem

`crossterm` and `env_logger` are optional deps gated behind `tokio` in `Cargo.toml`, but neither is used in library code — only by `src/bin/bambino-cli/`. The CLI shipping in the same crate is intentional (README: "Ships as a binary in the same crate"), but the dep gating means any external consumer using `bambino` with default features pulls in a terminal manipulation library and a concrete log sink.

### Investigation

- Confirm no library code imports `crossterm` or `env_logger` (already verified).
- Evaluate options: (a) gate both behind a dedicated `cli` feature not implied by `tokio`, or (b) accept the current state since external consumers are not the primary use case yet at 0.1.0.
- If (a), verify that `cargo build --bin bambino-cli` still works when `cli` is enabled and that `cargo build --lib` no longer pulls in `crossterm`/`env_logger`.

### Fix

Apply if warranted. Verify `cargo build`, `cargo build --no-default-features --features alloc --lib`, and `cargo test` all pass.

---

## Phase 7: Camera integration in `PrinterClient`

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
| 4 | Lazy FTPS connection and API alignment | Not Started |
| 5 | Documentation | Not Started |
| 6 | CLI dependency leakage | Not Started |
| 7 | Camera integration in `PrinterClient` | Not Started |
