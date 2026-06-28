# Deep Code Review Plan

Phases 1–23 complete (library, CLI, reference docs, rustdoc audit). Details in git history.

---

## Phase 24: FTPS TLS Version Validation

### Problem

`BambuFtpsClient::connect()` receives a pre-built `TlsConnector` from the caller, and certain models (P2S, X2D) require TLS 1.2 only for FTPS data channels — their vsFTPd servers fail on TLS 1.3 session tickets. The quirks engine already knows this (`enforce_ftps_tls_1_2()` returns `true`), but today the caller has to manually query the quirk and configure TLS accordingly. If they forget, the connection silently breaks. A quirk that the library knows about but doesn't enforce is a footgun.

The library can't construct TLS configs itself (that would couple it to a specific platform stack), but it *can* detect the mismatch after the handshake and fail fast with a clear error.

### Approach

After the TLS control channel is established in `BambuFtpsClient::connect()`, check the negotiated TLS version against the model's quirk. If `enforce_ftps_tls_1_2()` is `true` and the session negotiated TLS 1.3, return a descriptive `BambuError` immediately rather than letting the session fail later with a cryptic protocol error.

### Design constraints

- The check must work at the `AsyncIo` trait level, not just for tokio/rustls. Consider whether `embedded-tls` and ESP-IDF expose negotiated version info, and whether the check should be best-effort (skip if version isn't detectable) or mandatory.
- The `TlsConnector` trait currently returns an opaque `Stream: AsyncIo`. Getting the negotiated version out of that stream may require adding an optional method to the trait, or a separate `TlsVersionInfo` trait that implementations can opt into.
- If adding a trait method is too invasive, an alternative is to check at the *data channel* level — the data channel is where TLS 1.3 actually causes failures (session ticket resumption). The control channel usually works fine on both versions. Investigate which layer actually breaks before deciding where the check goes.
- The CLI already does `build_unsafe_client_config_with_options(model.quirks().enforce_ftps_tls_1_2())` — after this phase, that wiring should still work, but a misconfigured caller should get a clear error instead of silent failure.
- Update the README to remove or reword the `_with_options` / `force_tls_1_2` line once the validation is in place.

---

## Phase 25: Raw MQTT Access Through PrinterClient

### Problem

Once a `BambuMqttClient` is handed to `PrinterClient::new()`, the caller loses direct access to it — the `mqtt` field is `pub(crate)`. If someone needs to send a custom MQTT payload (an unsupported command, a firmware experiment, a raw JSON blob), they have to abandon `PrinterClient` entirely and manage the connection themselves.

This undermines the "two levels of API" promise: the high-level client should be an upgrade over raw access, not a wall around it. Users should be able to reach for the raw protocol when they need to without giving up the safety checks, telemetry parsing, and sequence management they get from `PrinterClient`.

### What's already public

- `poll_raw()` — reads the next raw `MqttMessage`, respecting the internal pending buffer. Already public.
- `next_sequence_id()` — returns the next sequence ID, maintaining the counter. Already public.
- All command request structs (`GCodeRequest`, `AmsChangeFilamentRequest`, etc.) — already public in `mqtt::commands`.

### What's missing

- `publish_command(&mut self, payload: &[u8])` — sends an arbitrary byte payload to the printer's command topic. Currently `pub(crate)` on `BambuMqttClient`, not exposed through `PrinterClient` at all.
- `publish_request<T: Serialize>(&mut self, request: &T)` — serializes a struct and publishes it. Currently `pub(crate)` on `PrinterClient`.

### Approach

Expose two public methods on `PrinterClient`:

1. **`publish_raw(&mut self, payload: &[u8]) -> Result<u16, BambuError>`** — Delegates to `self.mqtt.publish_command()`. For sending hand-crafted JSON payloads. Does not touch the sequence counter (caller manages their own if needed).

2. **`publish_request<T: Serialize>(&mut self, request: &T) -> Result<u16, BambuError>`** — Change visibility from `pub(crate)` to `pub`. For sending any of the existing command structs that `PrinterClient` doesn't have a dedicated method for.

### Design constraints

- Both methods go *through* `PrinterClient` (not around it), so the pending message buffer stays consistent — `poll_raw()` / `poll_telemetry()` will still see all responses in order.
- The sequence counter is already public via `next_sequence_id()`, so callers can construct properly sequenced requests.
- No need to expose `&mut BambuMqttClient` directly — that would let callers read messages outside the pending buffer, breaking `poll_telemetry()`.
- FTPS raw access is already handled: `storage()` returns `Option<&mut BambuFtpsClient<...>>`. Camera and discovery are standalone modules — they don't go through `PrinterClient`. AMS/hardware/motion commands all go over MQTT, so they're covered by this same escape hatch.
- Update the README's "Two levels of API" section to mention the raw access pattern (with appropriate warnings).

---

## Phase 26: Architectural Review

### Problem

Phases 1–23 reviewed the library for correctness, code smells, and documentation accuracy — but never stepped back to evaluate whether the high-level structure is right. Questions like: are module boundaries drawn in the right places? Is the public API surface what a consumer would expect? Are there layering violations, circular dependencies, or abstractions that don't carry their weight? Does the crate organize well for the three-target story (tokio / ESP-IDF / embassy)?

A library can be bug-free and still be hard to use or extend because of how it's organized. This phase is about finding structural problems before they calcify.

### Scope

Review the library architecture across these dimensions (in priority order):

1. **Module boundaries and cohesion** — Does each module (`client/`, `mqtt/`, `ftps/`, `types/`, `io/`, `quirks/`, `ams/`, `diagnostics/`, `camera/`, `discovery/`) have a clear single responsibility? Are there modules doing too much or too little? Would a consumer understand the layout from the directory tree?

2. **Public API surface** — Walk `lib.rs` re-exports and `pub` items. Is the API layered so that common tasks are easy and advanced tasks are possible? Are there types exposed that should be internal, or internal types that should be exposed? Is there a clear "getting started" path?

3. **Dependency direction** — Do modules depend on each other in one direction, or are there circular or surprising cross-dependencies? Does `types/` depend on things it shouldn't? Does `client/` reach into `mqtt/` internals? Map the actual dependency graph and flag anything that feels wrong.

4. **Trait abstraction fitness** — The `io/` traits (`AsyncIo`, `TlsConnector`, `SecureConnect`, `AsyncUdpSocket`, `TimerProvider`) are the backbone of the multi-platform story. Are they at the right level of abstraction? Too fine-grained? Too coarse? Are there platform capabilities that can't be expressed through the current traits?

5. **Error type design** — `BambuError` carries the whole crate. Is the variant set right? Are there cases where callers can't distinguish errors they need to handle differently? Does `ProtocolViolation(Cow<'static, str>)` pull its weight vs. structured variants?

6. **Feature flag hygiene** — Review `Cargo.toml` feature definitions and `#[cfg(...)]` usage. Are the `std`/`alloc`/`tokio`/`embassy`/`esp-idf` gates consistent? Are there items gated that shouldn't be, or ungated items that only work on one target?

### Approach

This is a read-only review — no code changes. The output is a list of findings categorized as:
- **Fix now** — structural issues that will get harder to fix as the crate grows
- **Fix before 1.0** — things that affect the public API contract
- **Consider** — trade-offs worth thinking about but not necessarily wrong

Use `ctx_refactor` (symbols_overview, references, implementations), `ctx_callgraph`, `ctx_impact`, and `ctx_graph` to map actual dependencies rather than guessing from file names. Read `Cargo.toml` feature definitions and grep for `#[cfg(` patterns to audit feature gates.

### Design constraints

- This phase produces findings, not code. A follow-up phase (or inline fixes) will address anything flagged.
- Don't re-review correctness issues — phases 1–23 covered that. Focus on structure, not bugs.
- Evaluate against the stated goal: "a multi-platform async Rust crate for controlling Bambu Lab 3D printers over LAN." The architecture should serve that goal without over-engineering for hypotheticals.
- The CLI (`src/bin/bambino-cli/`) is out of scope — it's a consumer of the library, not part of its architecture.

---

## Progress Tracker

| Phase | Status |
|-------|--------|
| 24 | FTPS TLS Version Validation | Not Started |
| 25 | Raw MQTT Access Through PrinterClient | Complete |
| 26 | Architectural Review | Not Started |