---
paths:
  - "src/client/mod.rs"
  - "src/client/connect.rs"
  - "src/client/hardware.rs"
---

`PrinterClient` is generic over three symmetric `TlsConnector`+`RawStreamFactory` trios (MQTT mandatory, FTPS/camera defaulted to dummy types) plus `Timer: TimerProvider`. Type-changing builders (`.with_timer()`, `.with_ftps()`, `.with_camera()`, `.with_attached_*()`) move the type-independent state as one `ClientCore` (`src/client/mod.rs`) plus only the slots whose types change — add a new field to `ClientCore`, not to `PrinterClient`, unless its type depends on a type parameter. Type-preserving builders (`.with_mqtt_port()`, `.with_connect_timeout()`, etc.) take and return `Self`; they consume `self`, so don't call them "non-consuming". The connect timeout (default 10s) bounds each `ensure_*()`'s dial+connect sequence — chain `.with_timer()` for it to actually fire (`DummyTimer` never elapses). Each channel's connect sequence lives once, in `dial_mqtt`/`dial_ftps`/`dial_camera` (`connect.rs`), shared by `ensure_*` and `connect_all`. On a `from_mqtt()` client (no ip/access code) a dialed FTPS/camera returns `Error::NotConfigured`; the builders don't panic.

`PrinterClient::toggle_led` was renamed to `set_led` (breaking, pre-1.0), matching the `set_*` naming used by every other hardware/thermal setter. `set_fan_speed` checks `FanTarget::is_supported_by(quirks)` and warns before clamping `speed_percent > 100`; a fan's M106 port is `FanTarget::write_port()`.
