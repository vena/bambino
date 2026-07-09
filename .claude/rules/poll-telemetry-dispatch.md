---
paths:
  - "src/client/telemetry.rs"
  - "src/mqtt/client/mod.rs"
---

`PrinterClient::poll_telemetry()` returns `TelemetryEvent` (discriminated enum), not raw `MqttMessage`. Use `poll_raw()` or `BambuMqttClient::poll_telemetry()` for raw access. The message buffer lives on `BambuMqttClient` — command-response methods like `get_version()` stash non-matching messages there, and `poll_telemetry()` drains them first.
