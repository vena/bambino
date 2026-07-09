---
paths:
  - "src/io/mod.rs"
  - "src/io/embassy.rs"
  - "src/discovery/mod.rs"
---

`AsyncUdpSocket` vs `BindableUdpSocket`: `AsyncUdpSocket` (`send_to`/`recv_from`) is implemented by every platform including Embassy. `BindableUdpSocket: AsyncUdpSocket` adds `bind()` — only implementable where the OS supports dynamic socket creation (tokio, ESP-IDF); `EmbassyUdpSocket` doesn't implement it since embassy-net's `bind()` needs pre-allocated buffers and a typed `IpListenEndpoint`. Functions that auto-bind sockets (e.g. `discover_devices`) must bound their socket type on `BindableUdpSocket`, not `AsyncUdpSocket`, so an Embassy call site is a compile error, not a runtime one.
