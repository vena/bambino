*[bambino](../../index.md) / [io](../index.md) / [esp_idf](index.md)*

---

# Module `esp_idf`

# ESP-IDF (ESP32 standard library) Platform Support

Bridges native ESP-IDF services and standard BSD socket structures to
our transport-agnostic client traits under Espressif's Rust standard library.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`EspIdfIoError`](#espidfioerror) | struct | Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring `TokioIoError` (`io/tokio.rs`) — needed because `embedded-io-async` has no blanket impl for `std::io::Error` itself, only for types that opt in explicitly. |
| [`EspIdfRawStreamFactory`](#espidfrawstreamfactory) | struct | Raw (pre-TLS) connection factory for ESP-IDF, using raw `std::net::TcpStream` — the ESP-IDF counterpart to `TokioRawStreamFactory` (`io/tokio.rs`), used for both MQTT's lazy connect and FTPS's passive data channel. |
| [`EspIdfTcpStream`](#espidftcpstream) | struct | Raw (unencrypted) TCP stream, used both as the seed for `EspIdfTlsConnector::connect`'s `EspTls::adopt()` call and directly as `RawIO` for models whose `model.quirks().uses_plaintext_ftps_data_channel()` is true (the FTPS data channel is then never TLS-wrapped, so its `embedded_io_async::Read`/`Write` impls below are exercised for real, not just to satisfy the `AsyncIo` trait bound). |
| [`EspIdfTimer`](#espidftimer) | struct | Async timer utilizing the ESP-IDF high-resolution timer service. |
| [`EspIdfTlsConnector`](#espidftlsconnector) | struct | TLS connector for ESP-IDF that wraps an already-connected raw stream (FTPS's data and control channels, and MQTT's lazy connect via `RawStreamFactory`+`TlsConnector`). |
| [`EspIdfTlsStream`](#espidftlsstream) | struct | Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`. |
| [`EspIdfUdpSocket`](#espidfudpsocket) | struct | UDP Socket implementation designed for ESP-IDF's BSD Socket integration. |

## Types

### `EspIdfIoError`

```rust
struct EspIdfIoError();
```

Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring `TokioIoError` (`io/tokio.rs`) — needed because `embedded-io-async` has no blanket impl for `std::io::Error` itself, only for types that opt in explicitly.

#### Trait Implementations

##### `impl Debug for EspIdfIoError`

- <span id="espidfioerror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for EspIdfIoError`

- <span id="espidfioerror-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Error for EspIdfIoError`

- <span id="espidfioerror-error-source"></span>`fn source(&self) -> Option<&dyn std::error::Error>`

##### `impl ToString for EspIdfIoError`

- <span id="espidfioerror-tostring-to-string"></span>`fn to_string(&self) -> String`

### `EspIdfRawStreamFactory`

```rust
struct EspIdfRawStreamFactory;
```

Raw (pre-TLS) connection factory for ESP-IDF, using raw `std::net::TcpStream` — the ESP-IDF counterpart to `TokioRawStreamFactory` (`io/tokio.rs`), used for both MQTT's lazy connect and FTPS's passive data channel.
Whether the returned stream ends up TLS-wrapped (via `EspIdfTlsConnector`) or used directly
(plaintext FTPS data-channel models) is decided by the caller, not this factory.

#### Trait Implementations

##### `impl RawStreamFactory<EspIdfTcpStream> for EspIdfRawStreamFactory`

- <span id="espidfrawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<EspIdfTcpStream, SocketError>` — [`EspIdfTcpStream`](#espidftcpstream), [`SocketError`](../index.md#socketerror)

### `EspIdfTcpStream`

```rust
struct EspIdfTcpStream {
    // [REDACTED: Private Fields]
}
```

Raw (unencrypted) TCP stream, used both as the seed for `EspIdfTlsConnector::connect`'s `EspTls::adopt()` call and directly as `RawIO` for models whose `model.quirks().uses_plaintext_ftps_data_channel()` is true (the FTPS data channel is then never TLS-wrapped, so its `embedded_io_async::Read`/`Write` impls below are exercised for real, not just to satisfy the `AsyncIo` trait bound).

The underlying socket stays non-blocking for the stream's entire lifetime (not
just during `connect()`'s own polling loop) — `read()`/`write()` below retry on
`WouldBlock` by yielding to the async executor via `EspIdfTimer::sleep(TLS_POLL_INTERVAL)`,
the same pattern `EspIdfTlsStream` already uses. A genuinely blocking socket here would give a
stalled peer (network partition, printer reboot) no `.await` yield point for any outer
timeout/cancellation to preempt, indefinitely parking the FreeRTOS task — exactly the hazard
`connect()`'s own non-blocking dial already fixes one layer up.

Wraps `Option<TcpStream>` rather than `TcpStream` directly so `Socket::release()` can
`.take()` the stream and hand its fd to `IntoRawFd::into_raw_fd()` — `esp_tls_conn_destroy`
closes an adopted fd itself once `release()` returns, so the Rust-side `TcpStream` must
give up ownership of the fd first or the fd would be double-closed.

#### Implementations

- <span id="espidftcpstream-connect"></span>`async fn connect(host: &str, port: u16) -> Result<Self, SocketError>` — [`SocketError`](../index.md#socketerror)

  Dials a raw TCP connection to `host:port`.

#### Trait Implementations

##### `impl AsyncIo for EspIdfTcpStream`

##### `impl ErrorType for EspIdfTcpStream`

- <span id="espidftcpstream-errortype-type-error"></span>`type Error = EspIdfIoError`

##### `impl RawStreamFactory<EspIdfTcpStream> for EspIdfRawStreamFactory`

- <span id="espidfrawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<EspIdfTcpStream, SocketError>` — [`EspIdfTcpStream`](#espidftcpstream), [`SocketError`](../index.md#socketerror)

##### `impl Read for EspIdfTcpStream`

- <span id="espidftcpstream-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl Socket for EspIdfTcpStream`

- <span id="espidftcpstream-socket-handle"></span>`fn handle(&self) -> i32`

- <span id="espidftcpstream-socket-release"></span>`fn release(&mut self) -> Result<(), ::esp_idf_svc::sys::EspError>`

##### `impl TlsConnector<EspIdfTcpStream> for EspIdfTlsConnector`

- <span id="espidftlsconnector-tlsconnector-type-stream"></span>`type Stream = EspIdfTlsStream<EspIdfTcpStream>`

- <span id="espidftlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: EspIdfTcpStream) -> Result<<Self as >::Stream, SocketError>` — [`EspIdfTcpStream`](#espidftcpstream), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

  Bounds the handshake loop by `self.connect_timeout`, tracked the same way `poll_until` does (`src/client/mod.rs`: capture `now_millis()` before the loop, compare `saturating_sub` against a budget each iteration).

- <span id="espidftlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

##### `impl Write for EspIdfTcpStream`

- <span id="espidftcpstream-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="espidftcpstream-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `EspIdfTimer`

```rust
struct EspIdfTimer {
    // [REDACTED: Private Fields]
}
```

Async timer utilizing the ESP-IDF high-resolution timer service.

Wraps `EspAsyncTimer` to provide non-blocking async sleep that integrates
with the FreeRTOS scheduler instead of blocking the task thread.

#### Implementations

- <span id="espidftimer-new"></span>`fn new() -> Result<Self, ::esp_idf_svc::sys::EspError>`

  Constructs a new timer backed by a dedicated ESP-IDF high-resolution timer service.

#### Trait Implementations

##### `impl TimerProvider for EspIdfTimer`

- <span id="espidftimer-timerprovider-sleep"></span>`async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>` — [`TimerError`](../index.md#timererror)

- <span id="espidftimer-timerprovider-now-millis"></span>`fn now_millis(&self) -> u64`

### `EspIdfTlsConnector`

```rust
struct EspIdfTlsConnector {
    // [REDACTED: Private Fields]
}
```

TLS connector for ESP-IDF that wraps an already-connected raw stream (FTPS's data and control channels, and MQTT's lazy connect via `RawStreamFactory`+`TlsConnector`).
Built on `esp_idf_svc::tls::EspTls` via `EspTls::adopt()` (confirmed by Phase 3's spike: no raw
mbedTLS FFI needed to wrap an existing fd) instead of `EspTls::new()` + `connect()`.

**No way to force TLS 1.2.** Unlike `io/tokio.rs`'s
`build_verified_client_config_with_options(..., force_tls_1_2: bool)` /
`build_unsafe_client_config_with_options(force_tls_1_2: bool)`, this connector has no
equivalent knob: `esp_idf_svc::tls::Config` (0.52.1, as vendored) exposes no min/max TLS
version field, and the mbedTLS accessor functions that would set it
(`mbedtls_ssl_conf_min_tls_version`/`mbedtls_ssl_conf_max_tls_version`) are absent from
this ESP-IDF build's actual bindgen output (confirmed by inspecting the generated
`esp-idf-sys` bindings directly, not just the safe wrapper's public API) — the
corresponding `mbedtls_ssl_config` struct fields are present but named
`private_max_tls_version`/`private_min_tls_version` per mbedTLS's own field-privacy
convention, so writing them directly would bypass that library's documented API contract
with no ABI stability guarantee across ESP-IDF/mbedTLS version bumps. Practical impact:
if a printer's vsFTPd offers/prefers TLS 1.3, `require_tls_1_2_if_enforced`
(`ftps/client.rs`) still fails closed for models where
`model.quirks().enforces_ftps_tls_1_2()` is true — the connection is safely rejected
rather than silently downgraded — but there is currently no way to make it succeed on
ESP-IDF for those models.

**Only `io/tokio.rs` (`tokio-rustls`) exposes a genuine max-protocol-version knob.**
`io/embassy.rs` has the same gap for a different reason: its backend is `mbedtls-rs` (not
`embedded-tls`, which it replaced — see `Cargo.toml`'s dependency comment), and
`EmbassyTlsConnector::connect` sets only `min_version` while `negotiated_version` returns
`None` unconditionally, so `require_tls_1_2_if_enforced` (`ftps/client.rs`) fails closed
there too for every `enforces_ftps_tls_1_2()` model. On both embedded backends the only way
through today is `with_ftps_allow_unverified_tls_1_2(true)`, which bypasses the check
rather than satisfying it.

#### Implementations

- <span id="espidftlsconnector-new"></span>`fn new() -> Self`

  Creates a connector that skips server certificate verification.

  Requires `CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY=y` in the consuming app's sdkconfig

  (a sub-option of `CONFIG_ESP_TLS_INSECURE`; both are off by default). No library call

  can enable it — ESP-IDF compiles the no-verification branch out otherwise, and

  `set_client_config` then fails the connection with `ESP_ERR_MBEDTLS_SSL_SETUP_FAILED`.

  Failing loudly there is deliberate: this crate no longer falls back to ESP-IDF's

  public-root CA bundle, which could never validate a self-signed printer certificate

  anyway (GitHub issue #62). Prefer [`Self::with_certs`] wherever the caller can supply

  the printer's CA — it needs no sdkconfig change and actually verifies the peer.

  The handshake (this connector wraps an already-connected raw stream, so there's no TCP dial to

  bound — only the handshake itself) defaults to `DEFAULT_CONNECT_TIMEOUT`; override via

  `.with_connect_timeout(d)`.

- <span id="espidftlsconnector-with-certs"></span>`fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self`

  Creates a connector that verifies the server certificate against a CA cert.

  The supplied CA is the sole trust anchor: ESP-IDF's bundled public root CAs are

  explicitly disabled, so these bytes reach mbedTLS as `cacert_buf` rather than being

  silently overridden by the bundle (GitHub issue #62). Certificates are a runtime

  input — nothing is embedded in this crate.

- <span id="espidftlsconnector-with-connect-timeout"></span>`fn with_connect_timeout(self, connect_timeout: core::time::Duration) -> Self`

  Overrides the default handshake deadline, which bounds how long the poll loop keeps

  retrying rather than how long any single attempt may take.

  The deadline is checked *between* iterations, so it cannot preempt a stall *inside*

  one: the `EspTls::negotiate` FFI call is not interruptible from this task once entered.

  `connect` pins `Config::timeout_ms = 0` so each call is a single handshake step, which

  keeps that window near-instant and gives this deadline ~`TLS_POLL_INTERVAL` granularity

  (GitHub issue #67) — but a call that blocks internally is still unbounded regardless of

  what is passed here, and the calling task is then lost with nothing logged (observed

  once on ESP32-P4, GitHub issue #66). Consumers running printer I/O on a dedicated task

  should subscribe it to the ESP-IDF Task Watchdog, which is the only layer that can

  recover from that; no in-crate timeout can, and this one does not claim to.

  Passing `Duration::ZERO` disables the

  deadline entirely, matching `set_command_timeout`'s "0 disables" convention

  and `client::connect::with_connect_timeout`'s precedent — otherwise the very

  first would-block poll would immediately exceed a zero-length budget.

  Non-consuming — chain onto `new()`/`with_certs()`.

#### Trait Implementations

##### `impl Default for EspIdfTlsConnector`

- <span id="espidftlsconnector-default"></span>`fn default() -> Self`

##### `impl TlsConnector<EspIdfTcpStream> for EspIdfTlsConnector`

- <span id="espidftlsconnector-tlsconnector-type-stream"></span>`type Stream = EspIdfTlsStream<EspIdfTcpStream>`

- <span id="espidftlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: EspIdfTcpStream) -> Result<<Self as >::Stream, SocketError>` — [`EspIdfTcpStream`](#espidftcpstream), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

  Bounds the handshake loop by `self.connect_timeout`, tracked the same way `poll_until` does (`src/client/mod.rs`: capture `now_millis()` before the loop, compare `saturating_sub` against a budget each iteration).

- <span id="espidftlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

### `EspIdfTlsStream<S>`

```rust
struct EspIdfTlsStream<S>
where
    S: ::esp_idf_svc::tls::Socket {
    // [REDACTED: Private Fields]
}
```

Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`.

`EspTls`'s own `read`/`write` are synchronous calls, but the underlying fd runs
in non-blocking mode (`O_NONBLOCK`, set by `EspIdfTlsConnector::connect`), so
each call returns immediately instead of blocking the FreeRTOS task. Retries happen
by yielding to the async executor via `EspIdfTimer::sleep` — see `TLS_POLL_INTERVAL`.

Generic over the adopted socket type `S`: `EspIdfTlsConnector` (wrap-an-existing-stream,
below) produces `EspIdfTlsStream<EspIdfTcpStream>`.

#### Trait Implementations

##### `impl AsyncIo for EspIdfTlsStream<S>`

##### `impl<S: ::esp_idf_svc::tls::Socket> ErrorType for EspIdfTlsStream<S>`

- <span id="espidftlsstream-errortype-type-error"></span>`type Error = ErrorKind`

##### `impl<S: ::esp_idf_svc::tls::Socket> Read for EspIdfTlsStream<S>`

- <span id="espidftlsstream-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl<S: ::esp_idf_svc::tls::Socket> Write for EspIdfTlsStream<S>`

- <span id="espidftlsstream-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="espidftlsstream-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `EspIdfUdpSocket`

```rust
struct EspIdfUdpSocket {
    // [REDACTED: Private Fields]
}
```

UDP Socket implementation designed for ESP-IDF's BSD Socket integration.

#### Trait Implementations

##### `impl AsyncUdpSocket for EspIdfUdpSocket`

- <span id="espidfudpsocket-asyncudpsocket-send-to"></span>`async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>` — [`SocketError`](../index.md#socketerror)

  Non-blocking send that reports transient lwIP buffer exhaustion as `TimedOut` rather than a terminal fault.

- <span id="espidfudpsocket-asyncudpsocket-recv-from"></span>`async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError>` — [`SocketError`](../index.md#socketerror)

  Non-blocking read paced with a short sleep on the WouldBlock path so this never busy-spins a caller polling in a tight loop — see `UDP_RECV_POLL_INTERVAL`'s doc comment.

  `TokioUdpSocket::recv_from` achieves the same pacing via a 100ms timeout wrapping a

  genuinely-blocking OS call; this platform has no async socket-readiness primitive for an

  arbitrary fd (see `TLS_POLL_INTERVAL`'s doc comment for why), so pacing is applied explicitly

  here instead.

##### `impl BindableUdpSocket for EspIdfUdpSocket`

- <span id="espidfudpsocket-bindableudpsocket-bind"></span>`async fn bind(addr: SocketAddr) -> Result<Self, SocketError>` — [`SocketError`](../index.md#socketerror)

