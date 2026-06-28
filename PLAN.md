# Deep Code Review Plan

Phases 1–25 complete (library, CLI, reference docs, rustdoc audit, raw MQTT access through PrinterClient). Phase 24 complete (FTPS TLS version validation). Details in git history.

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
| 26 | Architectural Review | Not Started |