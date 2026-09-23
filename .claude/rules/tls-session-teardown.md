---
paths:
  - "src/io/mod.rs"
  - "src/io/tokio.rs"
  - "src/io/embassy.rs"
  - "src/io/esp_idf.rs"
  - "src/client/connect.rs"
  - "src/client/camera.rs"
  - "src/client/storage.rs"
  - "src/ftps/client.rs"
  - "src/mqtt/client/mod.rs"
  - "src/camera/binary.rs"
---

Every path that tears a TLS session down must **close it and then drop it** — both, in that order. `TlsConnector::close()` (`src/io/mod.rs`) sends `close_notify`; dropping the stream is what returns its memory. Neither substitutes for the other: MbedTLS frees a session's buffers in `Drop`, not in `close()` (~48 KB per session on an ESP32-C6, against ~29 KB of headroom at the two-session peak — see the `mbedtls-rs` comment in `Cargo.toml`), and a drop without a close leaves the peer seeing a truncated connection, which is indistinguishable from a truncation attack to anyone reading the wire.

The current call sites are `PrinterClient::disconnect_mqtt`, `disconnect_camera`, `disconnect_storage` (via `FtpsClient::disconnect`), `FtpsClient`'s per-transfer data channel, and the three `attach_*` methods, which are async so they can close an occupied slot before replacing it. A new one added anywhere else has to do the same. The sync type-changing builders (`with_ftps`, `with_camera`, `with_attached_camera`, `with_attached_storage`) cannot close; their docs tell the caller to disconnect first. A camera installed by `attach_camera` with no connector configured is dropped without a close, since there is nothing to close through — `with_attached_camera` exists so a `from_mqtt()` client can supply one.

Two traps this has already sprung (GitHub issue #293, found only because `mbedtls-rs` logs `Session dropped without being closed properly`):

- **`close()` defaults to a no-op**, so a teardown path that never calls it compiles, passes, and logs nothing. Integration tests count the calls with `CloseCountingTlsConnector` (`tests/integration/common/io.rs`) rather than trusting the wiring. That mock proves the *call*, never the wire — whether a `close_notify` reaches a real peer is `embassy-hw-probe`'s job (see `.claude/rules/wire-framing-hardware-verification.md`).
- **Closing needs the connector, so don't discard it.** `ensure_camera()` used to clear `camera_config` after a successful connect even though nothing was moved out of it, which left `disconnect_camera()` with no connector to close through. `ftps_config` is the opposite case and is genuinely consumed — its connector moves into the `FtpsClient`, which is why `FtpsClient` owns a `tls_connector` field at all.

Error paths deliberately skip the close: a transfer that failed mid-stream has already poisoned the client, and there is no orderly shutdown to negotiate with a peer that may itself be the failure. Close failures on the success path are logged and swallowed — the connection is going away either way.
