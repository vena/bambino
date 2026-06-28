# bambino — Review Plan

**Important:** Before starting any phase, read `README.md` cover to cover. Understand what this library does and who it's for. Do not apply generic software engineering heuristics without grounding them in the project's actual goals.

---

## Phase 1: Add `TokioFtpDataStreamFactory`

### Problem

The library provides `TokioTlsConnector`, `TokioSecureConnector`, `TokioTimer`, and `TokioUdpSocket` as ready-to-use tokio implementations of the io traits. But there's no tokio implementation of `FtpDataStreamFactory`. Consumers wanting FTPS with tokio must write their own data stream factory (TCP connect + TLS wrap for the passive data channel). The CLI does this internally.

### Investigation

- Read the CLI's FTPS connection code to see how it creates data streams.
- Determine the minimal implementation: TCP connect + TLS wrap using `TokioTlsConnector`.
- Decide where it lives — `io/tokio.rs` is the natural fit alongside the other tokio impls.

### Fix

Add `TokioFtpDataStreamFactory` (or similar) to `io/tokio.rs`. Verify builds and tests.

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

## Progress Tracker

| Phase | Status |
|-------|--------|
| 1 | Add `TokioFtpDataStreamFactory` | Not Started |
| 2 | CLI dependency leakage | Not Started |
| 3 | Architectural review | Not Started |
