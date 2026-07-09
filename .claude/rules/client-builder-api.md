---
paths:
  - "src/client/mod.rs"
  - "src/client/connect.rs"
  - "src/client/hardware.rs"
---

`PrinterClient` is generic over three symmetric `TlsConnector`+`RawStreamFactory` trios (MQTT mandatory, FTPS/camera defaulted to dummy types) plus `Timer: TimerProvider`. Consuming builders (`.with_timer()`, `.with_ftps()`, `.with_camera()`) change type parameters; non-consuming builders (`.with_mqtt_port()`, etc.) don't. `connect_timeout_secs` (default 10s) bounds each `ensure_*()`'s dial+connect sequence — chain `.with_timer()` for it to actually fire (`DummyTimer` never elapses). FTPS config is consumed via `.take()` on first connect — reconnecting needs a new `PrinterClient`.

`PrinterClient::toggle_led` was renamed to `set_led` (breaking, pre-1.0), matching the `set_*` naming used by every other hardware/thermal setter. `set_fan_speed` also gained guard checks for `AuxiliaryLeft`/`ChamberExhaust` fan targets against the corresponding `ModelQuirks` capability, and warns before clamping `speed_percent > 100`.
