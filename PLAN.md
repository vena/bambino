# Deep Code Review Plan

Module-by-module review of the `bambino` crate. Detailed write-ups belong in commit messages, not here.

When completing a phase, collapse its section into the completed summary below.

---

## Completed Phases (1–23)

Phases 1–22 built the full library (typed telemetry, MQTT commands, FTPS client, quirks engine, CLI) and aligned the `/reference` protocol docs against the implementation.

Phase 23 audited and rewrote rustdoc documentation across the crate:
- Expanded `lib.rs` crate-level docs with a quick-start example, feature flag table, compilation target matrix, and module guide with cross-links.
- Rewrote all module-level `//!` docs (13 modules including submodules) in a natural, developer-friendly voice while staying technically precise.
- Added `///` doc comments to 30+ previously undocumented public structs, enums, constants, and associated functions across MQTT commands, diagnostics/kprofile, telemetry types, and IO adapters.
- Added `#[doc(inline)]` to key re-exports (`BambuError`, `BambuModel`, `TelemetryEvent`, etc.) for better rustdoc rendering.
- Converted backtick type references to clickable `[`rustdoc links`]` in the most visible doc comments.
- Added `# Example` sections to `PrinterClient::new()`, `poll_telemetry()`, `send_gcode()`, `set_bed_temperature()`, `bed_temperatures()`, and `discover_devices()`.
- `cargo doc --no-deps` completes with zero warnings.

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

## Progress Tracker

| Phase | Module | Status |
|-------|--------|--------|
| 1–22 | Core → Reference Doc Alignment | **Complete** |
| 23 | Rustdoc Library Documentation | **Complete** |
| 24 | FTPS TLS Version Validation | Not Started |