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

## Progress Tracker

| Phase | Status |
|-------|--------|
| 24 | FTPS TLS Version Validation | Not Started |
| 25 | Raw MQTT Access Through PrinterClient | Not Started |