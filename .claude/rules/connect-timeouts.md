---
paths:
  - "src/client/connect.rs"
  - "src/io/esp_idf.rs"
  - "src/io/embassy.rs"
---

Connect-phase timeouts are two layers, not one. `PrinterClient::ensure_mqtt()`/`ensure_ftps()` bound the entire dial+TLS-connect sequence via the connect timeout (`with_connect_timeout`, default `DEFAULT_CONNECT_TIMEOUT` 10s) — the layer `PrinterClient` users rely on everywhere. `EspIdfTlsConnector` separately carries its own `connect_timeout` bounding only its handshake retry loop, for direct (non-`PrinterClient`) consumers; it defaults to disabled (`Duration::ZERO`), because a non-zero inner default silently capped the outer budget (#537) — don't give it one back. `EmbassyTlsConnector::connect` has no connector-level equivalent — a direct caller needing a bounded connect must race it against `embassy_time::with_timeout` itself.

`connect_all()` applies the same connect-timeout budget **per channel**, not as one shared deadline over the joined future, so a hung camera cannot make a healthy MQTT dial report `TimedOut`. Worst-case wall clock is still one timeout because the channels overlap. A shared deadline was considered and rejected: a single race wrapped around the join cannot express partial success — it would discard an already-completed MQTT session when a slow camera pushed the *combined* future past the deadline. Don't "simplify" it back to one outer race.

`None` disables the timeout (as it does for `set_command_timeout`); there is no `0` sentinel. `Some(Duration::ZERO)` is honored literally and fails every connect at once.
