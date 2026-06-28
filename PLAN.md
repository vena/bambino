# bambino — Review Plan

**Important:** Before starting any phase, read `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

---

## Phase 1: `PrinterClient` Lazy Connection Redesign

### Problem

`PrinterClient` requires a pre-connected `BambuMqttClient` at construction time. This means:

1. **FTPS-only use is impossible through `PrinterClient`.** The CLI's storage command (`src/bin/bambino-cli/storage.rs`) bypasses `PrinterClient` entirely, creating `BambuFtpsClient` directly. The CLI should demonstrate `PrinterClient` throughout — it's the library's primary API.
2. **Consumers must manually orchestrate TCP→TLS→MQTT before they can create a `PrinterClient`.** The README "Connect" example is 6 lines of connection plumbing before `PrinterClient::new()` is even called.
3. **There's no tokio implementation of `FtpDataStreamFactory`.** The CLI has a private `TokioDataStreamFactory` in `storage.rs` that should be in the library alongside `TokioTlsConnector`, `TokioTimer`, etc.

### Design

`PrinterClient` becomes the single coordination point for a printer. You construct it with the printer's identity and platform adapters, then opt into whichever protocols you need. Connections are established on demand or eagerly at the consumer's discretion.

#### 1. Type parameters

The first type param changes from `IO` (a passive stream type) to `Conn: SecureConnect` (an active connector that can create streams). The MQTT stream type becomes `Conn::Stream`. The other 4 params stay the same.

```
Before: PrinterClient<IO,   Timer, RawIO, Tls, Factory>
After:  PrinterClient<Conn, Timer, RawIO, Tls, Factory>
        where Conn: SecureConnect
```

Still 5 type params, but `Conn` carries the ability to establish connections, not just a stream type. Defaults stay the same pattern — `DummySecureConnect` replaces the old `IO` default.

#### 2. New struct fields

`PrinterClient` gains:

- `connector: Conn` — stored `SecureConnect` for lazy MQTT connection
- `ip: String` — printer IP, needed by both MQTT and FTPS lazy connection
- `access_code: String` — needed by MQTT and FTPS handshakes
- `ftps_config: Option<(Tls, Factory)>` — FTPS adapters stored before connection, consumed by `connect_ftps()`

`mqtt` changes from `BambuMqttClient<IO>` to `Option<BambuMqttClient<Conn::Stream>>`.

#### 3. Constructors

Two construction paths:

**Lazy path** (tokio, ESP-IDF) — primary API:
```rust
let secure = TokioSecureConnector::new(tls_connector, timeout);
let mut printer = PrinterClient::new(secure, ip, serial, access_code, model);
```

**Pre-connected path** (Embassy, backward compat):
```rust
let mut printer = PrinterClient::from_mqtt(mqtt_client, serial, model);
```

`from_mqtt()` wraps the connector type in a `PreConnected<IO>` marker that implements `SecureConnect` with `type Stream = IO` and returns an error if lazy connection is attempted. Since Embassy can't create its own TCP sockets (they must be pre-allocated from the network stack), this is the appropriate API for that platform.

Timer and FTPS are configured separately:
```rust
printer.set_timer(TokioTimer::new());
printer.configure_ftps(ftps_tls_connector, TokioFtpDataStreamFactory);
```

#### 4. Lazy MQTT connection

A private `ensure_mqtt(&mut self) -> Result<(), BambuError>` helper:
1. If `self.mqtt.is_some()`, return `Ok(())`
2. Call `self.connector.secure_connect(&self.ip, MQTTS_PORT)` to get a TLS stream
3. Call `BambuMqttClient::connect(stream, &self.serial, &self.access_code)` to handshake
4. Store the client in `self.mqtt`

Every method that uses MQTT calls `self.ensure_mqtt().await?` first, then accesses `self.mqtt.as_mut().unwrap()`. This is a mechanical change across all submodule files (`thermal.rs`, `motion.rs`, `print.rs`, `hardware.rs`, `ams.rs`).

Public `connect_mqtt()` exposes the same logic for eager pre-connection. Idempotent — safe to call multiple times.

#### 5. FTPS connection

`configure_ftps(tls, factory)` stores adapters in `self.ftps_config`.

`connect_ftps()`:
1. Takes `(tls, factory)` from `self.ftps_config` (errors if not configured)
2. Calls `factory.create_data_stream(&self.ip, 990)` to create raw TCP to the FTPS port
3. Calls `BambuFtpsClient::connect(raw, tls, factory, ...)` — this moves `tls` and `factory` into the FTPS client
4. Stores the connected client in `self.ftps`

`storage()` stays as `fn storage(&mut self) -> Option<&mut BambuFtpsClient<...>>` — returns `None` if FTPS isn't connected. Consumers call `connect_ftps()` explicitly. (FTPS is explicitly opted into, so lazy connection adds less value than for MQTT.)

#### 6. `DummySecureConnect` and `PreConnected<IO>`

Add to `src/client/dummy.rs`:

```rust
pub struct DummySecureConnect;

impl SecureConnect for DummySecureConnect {
    type Stream = DummyRawIo;
    async fn secure_connect(&self, _host: &str, _port: u16) -> Result<DummyRawIo, SocketError> {
        Err(SocketError::Other("dummy connector"))
    }
}
```

Add `PreConnected<IO>` (for `from_mqtt()`):

```rust
pub struct PreConnected<IO>(PhantomData<IO>);

impl<IO: AsyncIo> SecureConnect for PreConnected<IO> {
    type Stream = IO;
    async fn secure_connect(&self, _host: &str, _port: u16) -> Result<IO, SocketError> {
        Err(SocketError::Other("pre-connected client cannot create new connections"))
    }
}
```

#### 7. `TokioFtpDataStreamFactory`

Add to `src/io/tokio.rs` — a unit struct implementing `FtpDataStreamFactory<TokioIo<TcpStream>>`. Identical to the CLI's private `TokioDataStreamFactory`: TCP connect + `TokioIo` wrap. This is the missing tokio adapter.

#### 8. `MQTTS_PORT` constant

Add `pub(crate) const MQTTS_PORT: u16 = 8883;` to `src/mqtt/client.rs` (or `src/mqtt/mod.rs`). Currently only defined in the CLI's `connection.rs`. `PrinterClient::ensure_mqtt()` needs it.

### Implementation order

These are ordered by dependency. Items at the same level are independent of each other.

**Step 1** — Independent additions (no existing code changes):
- Add `TokioFtpDataStreamFactory` to `src/io/tokio.rs`
- Add `DummySecureConnect` and `PreConnected<IO>` to `src/client/dummy.rs`
- Add `MQTTS_PORT` constant to `src/mqtt/`

**Step 2** — Core refactor (depends on step 1):
- Redesign `PrinterClient` struct in `src/client/mod.rs`: change type params, add new fields, rewrite constructors, add `ensure_mqtt()` / `connect_mqtt()` / `configure_ftps()` / `connect_ftps()`
- Update `src/client/storage.rs`: remove `new_with_storage()` (replaced by `configure_ftps()` + `connect_ftps()`), keep `attach_storage()` for direct injection, keep `storage()` accessor

**Step 3** — Mechanical updates (depend on step 2):
- Update every method in `thermal.rs`, `motion.rs`, `print.rs`, `hardware.rs`, `ams.rs` to call `self.ensure_mqtt().await?` before accessing `self.mqtt`
- Update `poll_telemetry()`, `poll_raw()`, `poll_until()`, `publish_request()`, `request_pushall()`, `send_ping()`, `mqtt()` in `mod.rs`

**Step 4** — CLI refactor (depends on steps 1–3):
- Rewrite `src/bin/bambino-cli/connection.rs`: `connect_mqtt()` becomes a helper that creates a `TokioSecureConnector` and returns a `PrinterClient` (or just returns the connector + config for the caller to construct `PrinterClient`)
- Rewrite `src/bin/bambino-cli/storage.rs`: use `PrinterClient` with `configure_ftps()` + `connect_ftps()` instead of direct `BambuFtpsClient`. Delete the private `TokioDataStreamFactory` (replaced by library's `TokioFtpDataStreamFactory`).
- Update `control.rs`, `monitor/mod.rs` to use the new `PrinterClient` constructor

**Step 5** — Tests and docs (depends on steps 1–3):
- Update `tests/client_test.rs` and `tests/ftps_test.rs` for new constructor signatures
- Update README "Connect" and "File transfer" examples to show the new API
- Update `src/lib.rs` doc example

### Verification

All of these must pass:
```sh
cargo build
cargo test
cargo clippy
cargo build --no-default-features --features alloc --lib
```

Verify the CLI commands still work:
```sh
cargo build --bin bambino-cli
```

### What this phase intentionally does NOT do

- **Lazy FTPS connection** — FTPS requires explicit `connect_ftps()`. The consumer has already opted in by calling `configure_ftps()`, so the extra method call is not burdensome.
- **Reconnection** — `ensure_mqtt()` connects once. If the connection drops, the consumer must handle it. Reconnection logic is a separate concern.
- **Change `BambuFtpsClient` or `BambuMqttClient` APIs** — those stay as-is. Only `PrinterClient` (the wrapper) and the CLI change.

---

## Phase 2: CLI dependencies leak into library `tokio` feature

### Problem

`crossterm` and `env_logger` are optional deps gated behind `tokio` in `Cargo.toml`, but neither is used in library code — only by `src/bin/bambino-cli/`. The CLI shipping in the same crate is intentional (README: "Ships as a binary in the same crate"), but the dep gating means any external consumer using `bambino` with default features pulls in a terminal manipulation library and a concrete log sink.

### Investigation

- Confirm no library code imports `crossterm` or `env_logger` (already verified).
- Evaluate options: (a) gate both behind a dedicated `cli` feature not implied by `tokio`, or (b) accept the current state since external consumers are not the primary use case yet at 0.1.0.
- If (a), verify that `cargo build --bin bambino-cli` still works when `cli` is enabled and that `cargo build --lib` no longer pulls in `crossterm`/`env_logger`.

### Fix

Apply if warranted. Verify `cargo build`, `cargo build --no-default-features --features alloc --lib`, and `cargo test` all pass.

---

## Phase 3: Architectural review

### What this phase is — and is not

This is an evaluation of bambino's high-level design decisions against its stated goals. It is NOT a code review, a bug hunt, or a search for types in the wrong file. The previous attempt (phase 26) failed by doing exactly that — it grepped for import paths and `#[cfg]` gates and produced findings like "this enum should live in a different module." That's not architecture.

### Before you start

Read `README.md` cover to cover. The library's identity:
- Async Rust library for direct LAN control of Bambu Lab 3D printers
- One codebase compiling to three targets: desktop (tokio), ESP32 (ESP-IDF), bare-metal (Embassy)
- Two API levels: `PrinterClient` (high-level with model-aware safety) and direct protocol access
- Ships as a single crate with an integrated CLI for testing
- Protocols: MQTT (commands + telemetry), FTPS (file transfer), SSDP (discovery), camera (binary JPEG + RTSPS)

Every question below should be answered with: "Does this design decision serve these goals well? Will it hold up as the crate grows?" The output for each question is one of:
- **Sound** — the design is right, with a sentence on why
- **Concern** — the design works today but has a specific scaling/usability risk, with the risk stated
- **Problem** — the design is wrong and here's a concrete better approach

Do NOT produce findings about file organization, import paths, or feature flag placement. Those were already reviewed and resolved.

### Questions to evaluate

**Note:** Questions 1 and 2 were substantively addressed by the Phase 1 redesign (type parameter change from `IO` to `Conn: SecureConnect`, lazy connection, FTPS integration). Evaluate them lightly — confirm Phase 1's design is sound or flag remaining concerns, but don't deep-dive.

**1. Multi-platform type parameter strategy**

`PrinterClient<IO, Timer, RawIO, Tls, Factory>` has 5 generic type parameters to abstract over platforms. Default generics (`DummyTimer`, `DummyTls`, etc.) let the common case (`PrinterClient::new()`) avoid specifying them all.

- Does this strategy compose well when a consumer actually uses FTPS (suddenly all 5 params are real)?
- What happens when a 4th platform is added — do all 5 trait impls need new types?
- Is 5 type parameters the natural complexity for this domain, or is there a simpler formulation that achieves the same multi-platform story? Consider alternatives: trait objects with platform-specific constructors, feature-gated concrete types, a platform adapter struct that bundles all the transport pieces.
- Read the README's FTPS example — the consumer has to supply `raw_control, tls_connector, data_factory, model, ip, access_code`. Is this the right ergonomic tradeoff?

**2. PrinterClient scope and layering**

`PrinterClient` wraps MQTT (and optionally FTPS) with methods spanning thermal control, motion, print jobs, AMS operations, hardware (fans/LEDs/buzzers), and diagnostics (version queries, K-profiles). It's split into submodule files (`thermal.rs`, `motion.rs`, `print.rs`, `hardware.rs`, `ams.rs`, `storage.rs`).

- Is one struct doing too much, or is the submodule split sufficient to manage the complexity?
- The `.mqtt()` escape hatch gives raw access while keeping the high-level client. Does this layering hold up? Are there operations that are awkward because they don't fit either level?
- The `pending_messages` buffer: when `poll_until()` waits for a specific response, non-matching messages accumulate in a `VecDeque` with no bound. Is this a problem for long-running connections that call command-response methods frequently?
- `PrinterClient` holds `Option<BambuFtpsClient<...>>` — FTPS is optional and bolted on. Does this optional composition work well in practice, or does it create awkward `None` checks and type signature noise?

**3. Quirks engine scalability**

`ModelQuirks` is a trait with ~20 methods, dispatched as `&'static dyn ModelQuirks` via `BambuModel::quirks()`. Per-model strategy structs implement it.

- Bambu Lab ships new printer models regularly. What's the process for adding a model? Is it: add a variant to `BambuModel`, create a strategy struct, implement `ModelQuirks`, add a match arm in `quirks()`? Is this mechanical or does it require design decisions each time?
- Is `ModelQuirks` becoming a god-trait? At 20+ methods, is every method genuinely about "model-specific behavior," or have some accumulated that are really about feature presence (capabilities) vs. behavioral differences (quirks)?
- The trait uses default method implementations for most methods. Is there a risk of a new model silently inheriting wrong defaults?

**4. MQTT command pattern**

Every command follows: `#[derive(Serialize)]` payload struct → wrapper struct with `pub print:` (or `pushing:`, `system:`) field → `impl` with `pub fn new()`. There are ~20 command types across `mqtt/commands/` and `diagnostics/kprofile.rs`.

- Is the boilerplate justified? Does the two-struct pattern (payload + wrapper) earn its keep, or could a single struct with serde rename do the job?
- What happens at 50 commands? 100? Is there a macro or code-generation path that would reduce repetition without sacrificing readability?
- Are the command types well-grouped (by domain: `ams.rs`, `control.rs`, `gcode.rs`, `hardware.rs`, `print_job.rs`, `status.rs`), or would they be better organized differently at scale?

**5. Telemetry data model**

The printer pushes JSON state dumps over MQTT. These are deserialized into `TelemetryReport` (which contains `PrinterTelemetry` with dozens of optional fields). `poll_telemetry()` returns `TelemetryEvent::Report` or `TelemetryEvent::Unknown`.

- `TelemetryReport` / `PrinterTelemetry` are large flat(ish) structs with many `Option<>` fields because different messages populate different subsets. Is this the right representation, or would a delta/event model work better?
- The `Unknown` variant swallows all unrecognized messages. Is there telemetry data being silently dropped that consumers might want?
- How well does the telemetry model handle the `DeviceTelemetry` dual-location issue noted in CLAUDE.md (top-level vs nested inside `print`)? Is this transparent to consumers or a source of bugs?

**6. io/ trait surface area**

The transport traits are: `AsyncIo` (blanket impl), `TlsConnector` (wrap stream), `SecureConnect` (create connection), `AsyncUdpSocket`, `TimerProvider`, and `FtpDataStreamFactory`.

- Do these traits compose cleanly? Can a platform implementor look at the trait list and understand what they need to provide?
- `TlsConnector` vs `SecureConnect` exists because ESP-IDF manages its own TCP. Is this split well-documented? Would a new platform implementor understand when to use which?
- Could a hypothetical 4th platform (e.g., WASM, or a different RTOS) be added by just implementing these traits, or would the trait surface need to change?

### Output format

For each of the 6 questions, write a **Sound / Concern / Problem** verdict with 2–4 sentences of reasoning. If the verdict is Concern or Problem, state specifically what would go wrong and under what conditions. Do not produce code changes in this phase — it's evaluation only.

---

## Phase 4: Camera integration in `PrinterClient`

### Problem

`PrinterClient` has no camera awareness. The CLI's camera command (`src/bin/bambino-cli/camera.rs`) bypasses `PrinterClient` and uses `BambuBinaryCameraStream` directly. `PrinterClient` is the library's high-level coordination point for a printer — it manages MQTT and FTPS connections, holds the printer's identity (ip, serial, access code, model), and applies model-aware safety checks. Camera should be accessible through it too.

The CLI should demonstrate `PrinterClient` throughout. Today, the camera command manually creates a TLS connection, builds a `BambuBinaryCameraStream`, and handles authentication — duplicating connection logic that `PrinterClient` already has (it holds a `SecureConnect` connector and the printer's credentials).

### Background: camera module structure

Bambu printers use two camera protocols (determined by `model.quirks().camera_protocol()`):

- **Binary JPEG (port 6000, A1/P1 series)** — `src/camera/binary.rs` provides `BambuBinaryCameraStream`, a complete client that authenticates and streams JPEG frames over TLS. This is a persistent streaming connection.
- **RTSPS (port 322, X1/X2/H2/P2S series)** — `src/camera/rtsps.rs` provides helper utilities only (URL generation, proxy URI rewriting, P2S RTP timestamp correction). There is no RTSP client — consumers integrate with external media frameworks.

### Design questions to answer first

- **Binary JPEG has a persistent streaming connection** — unlike FTPS (connect, do operation, disconnect), the camera stream stays open and pushes frames continuously. `PrinterClient` manages FTPS via `configure_ftps()` + `connect_ftps()` + `storage()` accessor. Does the same pattern work for a long-lived stream, or does the streaming nature need a different API shape?
- **Two protocols, one slot?** A printer uses either binary JPEG or RTSPS, never both. Should `PrinterClient` expose a single `camera()` accessor that returns an enum, or separate `binary_camera()` / `rtsps_helpers()` methods? The RTSPS side is just utility functions with no connection state — it may not need the connect-on-demand pattern at all.
- **Type parameter impact** — binary JPEG needs a TLS stream. `PrinterClient` already holds a `Conn: SecureConnect` connector that can create TLS streams (used for lazy MQTT connection). Can camera reuse this, or does it need its own type parameter (e.g., if the camera TLS config differs from MQTT's)?

### Scope

This phase is intentionally underspecified. Answer the design questions above based on the current codebase, then write a concrete implementation plan. Do not start implementation without a plan.

---

## Progress Tracker

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | `PrinterClient` lazy connection redesign | Not Started |
| 2 | CLI dependency leakage | Not Started |
| 3 | Architectural review | Not Started |
| 4 | Camera integration in `PrinterClient` | Not Started |
