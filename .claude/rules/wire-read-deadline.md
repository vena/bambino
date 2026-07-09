---
paths:
  - "src/io/mod.rs"
  - "src/mqtt/client/frame.rs"
  - "src/camera/binary.rs"
  - "src/ftps/protocol.rs"
  - "src/bin/bambino-cli/monitor/**"
---

`poll_wire`/`read_exact_packet` enforce a per-read deadline against a connection that stalls with zero incoming bytes (`connect_timeout_secs` only covers the dial+connect sequence, not a connection gone silent after). Correctness hinge: on timeout, bytes already read for the in-progress frame must not be lost — `FrameReadState` persists partial-frame progress so a retry resumes mid-frame instead of desyncing the parser. `TimerProvider::has_real_clock()` (`false` for `DummyTimer`) makes this safe: without it, racing every read against `DummyTimer`'s instant-complete `sleep()` would fail every call under the default timer-less `PrinterClient` config.

`race`, `Raced`, and `read_chunk` live in `src/io/mod.rs` (`pub(crate)`) — shared by `mqtt/client` and `camera/binary.rs`. `read_exact_packet`/`FrameReadState` live in `src/mqtt/client/frame.rs` (MQTT-specific). `read_next_frame()` delegates to `read_next_frame_with_timer(&DummyTimer)` — don't give it an independent implementation; both share one `read_state` field. `ftps/protocol.rs` mirrors the same stall-timeout shape for the FTPS data channel.

`read_chunk`'s no-deadline branch (`DummyTimer`) maps a `0`-byte read to `SocketError::ConnectionReset`, matching the with-deadline branch — otherwise a legitimate EOF looks identical to "no bytes yet" to callers' fill-loops, which would spin forever instead of erroring.

`select!`-multiplexed consumers never see `poll_wire`'s 30s deadline — e.g. `bambino-cli`'s `monitor::run`, which races `poll_telemetry()` against `ping_timer.tick()`. `tokio::select!` drops the losing future's accumulated progress every time the ping branch wins (every 15s, faster than 30s), so the deadline can never fire. Only `tick_zombie_check`'s independent 60s counter (durable across cancellation) catches it. Expected, not a bug.
