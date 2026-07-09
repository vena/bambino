---
paths:
  - "src/client/connect.rs"
  - "src/io/esp_idf.rs"
  - "src/io/embassy.rs"
---

Connect-phase timeouts are two layers, not one. `PrinterClient::ensure_mqtt()`/`ensure_ftps()` bound the entire dial+TLS-connect sequence via `connect_timeout_secs` (default 10s) — the layer `PrinterClient` users rely on everywhere. `EspIdfTlsConnector` separately carries its own `connect_timeout` bounding only its handshake retry loop, for direct (non-`PrinterClient`) consumers. `EmbassyTlsConnector::connect` has no connector-level equivalent — a direct caller needing a bounded connect must race it against `embassy_time::with_timeout` itself.
