# bambino — Review Plan

**Important:** Before starting any phase, read `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

**Pre-release:** This library has not been released. All API changes are on the table. Do not preserve backward compatibility for external consumers — only for tests and the CLI within the same crate, and only when the phase specifies it.

---

## Phase 1: Foundation Types and Library Adapters — Complete

Added building blocks for phases 2–4. Pure additions, no existing code changed.

- `DummySecureConnect` and `PreConnected<IO>` in `src/client/dummy.rs`, exported from `src/client/mod.rs`. Both implement `SecureConnect` and return `Err(SocketError::NotConnected)` if called. `DummySecureConnect` uses `Stream = DummyRawIo` (default type param). `PreConnected<IO>` is a `PhantomData<IO>` marker with `Stream = IO` (for wrapping pre-connected streams in phase 3's `from_mqtt()`).
- `TokioFtpDataStreamFactory` in `src/io/tokio.rs`. Public unit struct implementing `FtpDataStreamFactory<TokioIo<TcpStream>>`. Mirrors the CLI's private `TokioDataStreamFactory`.
- `pub(crate) const MQTTS_PORT: u16 = 8883` in `src/mqtt/mod.rs`, `pub(crate) const FTPS_PORT: u16 = 990` in `src/ftps/mod.rs`. Default ports for lazy connection builders.

---

## Phase 2: PrinterClient Struct Migration (Backward Compatible)

### Problem

`PrinterClient`'s first type parameter is `IO: AsyncIo` — a passive stream type. Phases 3–4 need it to be `Conn: SecureConnect` — an active connector. This phase performs the structural migration while keeping all existing constructor signatures working, so tests and the CLI compile without modification.

### Design

Change the struct definition and all impl blocks from `IO: AsyncIo` to `Conn: SecureConnect`. The MQTT client type becomes `BambuMqttClient<Conn::Stream>`. The `mqtt` field becomes `Option<BambuMqttClient<Conn::Stream>>` to prepare for lazy connection.

New struct fields: `connector: Conn`, `ip: String`, `access_code: String`, `ftps_config: Option<(Tls, Factory)>`, `mqtt_port: u16`, `ftps_port: u16`.

**Backward compatibility strategy:** The existing constructors (`new`, `new_with_timer`, `new_with_storage`) keep their parameter signatures but now constrain `Conn = PreConnected<IO>`. Since all callers use type inference (`let mut client = PrinterClient::new(mqtt, serial, model)`), the return type change from `PrinterClient<IO, ...>` to `PrinterClient<PreConnected<IO>, ...>` is invisible to them. No test or CLI changes needed.

**`ensure_mqtt()` pattern:** Add a private `ensure_mqtt(&mut self) -> Result<(), BambuError>` that checks if `self.mqtt` is `Some` (short-circuit) or creates a new connection via `self.connector.secure_connect()` + `BambuMqttClient::connect()`. In this phase, it's structurally a no-op (mqtt is always `Some` from the constructors) but establishes the pattern for phase 3.

Update every method in `mod.rs` that accesses `self.mqtt` to use `ensure_mqtt().await?` + `self.mqtt.as_mut().unwrap()`. Update the `mqtt()` accessor to return `Option<&mut BambuMqttClient<Conn::Stream>>` — no current caller uses it (the escape hatch was added for future use), so the signature change is safe.

**Submodule updates:** Every impl block in `thermal.rs`, `motion.rs`, `print.rs`, `hardware.rs`, `ams.rs`, `storage.rs` changes its first type bound from `IO: AsyncIo` to `Conn: SecureConnect`. No method bodies change — all MQTT access flows through `self.publish_request()` and `self.poll_until()` in `mod.rs`.

### Verification

```sh
cargo build && cargo test && cargo clippy && cargo build --no-default-features --features alloc --lib
```

All existing tests and the CLI must pass without modification.

---

## Phase 3: Lazy MQTT Connection and Constructor Redesign

### Problem

After phase 2, `PrinterClient` has the internal structure for lazy connection but still requires a pre-connected `BambuMqttClient` at construction time. This phase activates lazy MQTT connection, introduces the new constructor API, and migrates all consumers.

### Design: constructor API

**Rename** the existing `new(mqtt_client, serial, model)` to `from_mqtt()`. It keeps its behavior — wraps a pre-connected `BambuMqttClient` in `PrinterClient<PreConnected<IO>>`.

**Add** a new `new(connector, ip, serial, access_code, model)` that takes a `SecureConnect` connector and defers MQTT connection. The `mqtt` field starts as `None`; `ensure_mqtt()` creates the connection on first use.

**Replace** `new_with_timer()` and `from_mqtt_with_timer()` (if it exists) with a consuming `.with_timer(timer)` builder method that works on either construction path. This returns a `PrinterClient` with a different `Timer` type parameter. Same pattern: `.with_mqtt_port(port)` overrides the default port (non-type-changing, returns `Self`).

```rust
// Lazy (tokio, ESP-IDF):
let mut printer = PrinterClient::new(secure, ip, serial, access_code, model)
    .with_timer(TokioTimer::new());

// Pre-connected (Embassy, tests):
let mut printer = PrinterClient::from_mqtt(mqtt, serial, model)
    .with_timer(timer);
```

**Add** `connect_mqtt()` (public, idempotent, delegates to `ensure_mqtt()`) and `mqtt_connected() -> bool`.

### Consumer migration

**Tests (`tests/client_test.rs`):** Mechanical rename — `PrinterClient::new(mqtt,` → `PrinterClient::from_mqtt(mqtt,`. Mock infrastructure unchanged.

**CLI (`src/bin/bambino-cli/connection.rs`):** Replace `connect_mqtt()` (which returns a raw `BambuMqttClient`) with a helper that constructs a lazy `PrinterClient` using `TokioSecureConnector`. Remove the `MqttClient` type alias. Keep `validate_params()`.

**CLI consumers (`control.rs`, `monitor/mod.rs`):** Replace the 3-line connect+resolve+construct pattern with a single call to the new connection helper. Convert `monitor/mod.rs`'s `dump()` function from raw `BambuMqttClient` usage to `PrinterClient` — this eliminates the need for the CLI to expose a raw MQTT client at all.

### Verification

```sh
cargo build && cargo test && cargo clippy && cargo build --no-default-features --features alloc --lib
```

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
| 2 | `PrinterClient` struct migration (backward compatible) | Not Started |
| 3 | Lazy MQTT connection and constructor redesign | Not Started |
| 4 | Lazy FTPS connection and API alignment | Not Started |
| 5 | Documentation | Not Started |
| 6 | CLI dependency leakage | Not Started |
| 7 | Camera integration in `PrinterClient` | Not Started |
