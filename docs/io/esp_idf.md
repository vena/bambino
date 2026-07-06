**bambino > io > esp_idf**

# Module: io::esp_idf

## Contents

**Structs**

- [`EspIdfIoError`](#espidfioerror) - Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring
- [`EspIdfRawStreamFactory`](#espidfrawstreamfactory) - Raw (pre-TLS) connection factory for ESP-IDF, using raw `std::net::TcpStream` — the
- [`EspIdfTcpStream`](#espidftcpstream) - Raw (unencrypted) TCP stream, used both as the seed for `EspIdfTlsConnector::connect`'s
- [`EspIdfTimer`](#espidftimer) - Async timer utilizing the ESP-IDF high-resolution timer service.
- [`EspIdfTlsConnector`](#espidftlsconnector) - TLS connector for ESP-IDF that wraps an already-connected raw stream (FTPS's data and
- [`EspIdfUdpSocket`](#espidfudpsocket) - UDP Socket implementation designed for ESP-IDF's BSD Socket integration.
- [`EspTlsStream`](#esptlsstream) - Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`.

---

## bambino::io::esp_idf::EspIdfIoError

*Struct*

Wrapper around `std::io::Error` implementing `embedded_io_async::Error`, mirroring
`TokioIoError` (`io/tokio.rs`) — needed because `embedded-io-async` has no blanket impl
for `std::io::Error` itself, only for types that opt in explicitly.

**Tuple Struct**: `()`

**Trait Implementations:**

- **Error**
  - `fn kind(self: &Self) -> embedded_io_async::ErrorKind`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Display**
  - `fn fmt(self: &Self, f: & mut core::fmt::Formatter) -> core::fmt::Result`
- **Error**
  - `fn source(self: &Self) -> Option<&dyn std::error::Error>`



## bambino::io::esp_idf::EspIdfRawStreamFactory

*Struct*

Raw (pre-TLS) connection factory for ESP-IDF, using raw `std::net::TcpStream` — the
ESP-IDF counterpart to `TokioRawStreamFactory` (`io/tokio.rs`), used for both MQTT's
lazy connect and FTPS's passive data channel. Whether the returned stream ends up
TLS-wrapped (via `EspIdfTlsConnector`) or used directly (plaintext FTPS data-channel
models) is decided by the caller, not this factory.

**Unit Struct**

**Trait Implementations:**

- **RawStreamFactory**
  - `fn dial(self: &Self, host: &str, port: u16) -> Result<EspIdfTcpStream, SocketError>`



## bambino::io::esp_idf::EspIdfTcpStream

*Struct*

Raw (unencrypted) TCP stream, used both as the seed for `EspIdfTlsConnector::connect`'s
`EspTls::adopt()` call and directly as `RawIO` for models whose
`model.quirks().uses_plaintext_ftps_data_channel()` is true (the FTPS data channel is
then never TLS-wrapped, so its `embedded_io_async::Read`/`Write` impls below are
exercised for real, not just to satisfy the `AsyncIo` trait bound).

The underlying socket is only non-blocking transiently, during `connect()`'s own polling
loop (see that function's doc comment) — by the time a caller receives an
`EspIdfTcpStream`, it has always been switched back to blocking mode, matching
`EspIdfUdpSocket`'s approach of using `std::net::*` directly rather than inventing async
socket polling for every raw transport. Reads/writes below therefore block the calling
task/thread until data is available/sent, same as any other blocking
`std::net::TcpStream` use, unless `EspIdfTlsConnector::connect` flips the socket back to
non-blocking right before handing it to `EspTls::adopt()` (see that function) —
plaintext callers never trigger that path.

Wraps `Option<TcpStream>` rather than `TcpStream` directly so `Socket::release()` can
`.take()` the stream and hand its fd to `IntoRawFd::into_raw_fd()` — `esp_tls_conn_destroy`
closes an adopted fd itself once `release()` returns, so the Rust-side `TcpStream` must
give up ownership of the fd first or the fd would be double-closed.

**Tuple Struct**: `()`

**Methods:**

- `fn connect(host: &str, port: u16) -> Result<Self, SocketError>` - Dials a raw TCP connection to `host:port`.

**Traits:** ErrorType

**Trait Implementations:**

- **Read**
  - `fn read(self: & mut Self, buf: & mut [u8]) -> Result<usize, <Self as >::Error>`
- **Write**
  - `fn write(self: & mut Self, buf: &[u8]) -> Result<usize, <Self as >::Error>`
  - `fn flush(self: & mut Self) -> Result<(), <Self as >::Error>`
- **Socket**
  - `fn handle(self: &Self) -> i32`
  - `fn release(self: & mut Self) -> Result<(), ::esp_idf_svc::sys::EspError>`



## bambino::io::esp_idf::EspIdfTimer

*Struct*

Async timer utilizing the ESP-IDF high-resolution timer service.

Wraps `EspAsyncTimer` to provide non-blocking async sleep that integrates
with the FreeRTOS scheduler instead of blocking the task thread.

**Methods:**

- `fn new() -> Result<Self, ::esp_idf_svc::sys::EspError>` - Constructs a new timer backed by a dedicated ESP-IDF high-resolution timer service.

**Trait Implementations:**

- **TimerProvider**
  - `fn sleep(self: &Self, duration: core::time::Duration) -> Result<(), TimerError>`
  - `fn now_millis(self: &Self) -> u64`



## bambino::io::esp_idf::EspIdfTlsConnector

*Struct*

TLS connector for ESP-IDF that wraps an already-connected raw stream (FTPS's data and
control channels, and MQTT's lazy connect via `RawStreamFactory`+`TlsConnector`). Built
on `esp_idf_svc::tls::EspTls` via `EspTls::adopt()` (confirmed by Phase 3's spike: no raw
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
`model.quirks().enforce_ftps_tls_1_2()` is true — the connection is safely rejected
rather than silently downgraded — but there is currently no way to make it succeed on
ESP-IDF for those models. `io/tokio.rs` (`tokio-rustls`) and `io/embassy.rs`
(`embedded-tls`) have no equivalent gap; both expose a genuine max-protocol-version knob.

**Methods:**

- `fn new() -> Self` - Creates a connector that skips server certificate verification. The handshake
- `fn with_certs(ca_cert: Vec<u8>, client_auth: Option<(Vec<u8>, Vec<u8>)>) -> Self` - Creates a connector that verifies the server certificate against a CA cert.
- `fn with_connect_timeout(self: Self, connect_timeout: core::time::Duration) -> Self` - Overrides the default handshake deadline. Non-consuming — chain onto

**Trait Implementations:**

- **Default**
  - `fn default() -> Self`
- **TlsConnector**
  - `fn connect(self: &Self, host: &str, raw_stream: EspIdfTcpStream) -> Result<<Self as >::Stream, SocketError>` - Bounds the handshake loop by `self.connect_timeout`, tracked the same way
  - `fn negotiated_version(self: &Self, stream: &<Self as >::Stream) -> Option<TlsVersion>`



## bambino::io::esp_idf::EspIdfUdpSocket

*Struct*

UDP Socket implementation designed for ESP-IDF's BSD Socket integration.

**Trait Implementations:**

- **AsyncUdpSocket**
  - `fn send_to(self: &Self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>`
  - `fn recv_from(self: &Self, buf: & mut [u8]) -> Result<(usize, SocketAddr), SocketError>` - Non-blocking read paced with a short sleep on the WouldBlock path so this never
- **BindableUdpSocket**
  - `fn bind(addr: SocketAddr) -> Result<Self, SocketError>`



## bambino::io::esp_idf::EspTlsStream

*Struct*

Non-blocking TLS stream adapting `esp_idf_svc::tls::EspTls` to `embedded-io-async`.

`EspTls`'s own `read`/`write` are synchronous calls, but the underlying socket runs
in non-blocking mode (`Config::non_block = true`, set by `EspIdfTlsConnector`), so
each call returns immediately instead of blocking the FreeRTOS task. Retries happen
by yielding to the async executor via `EspIdfTimer::sleep` — see `TLS_POLL_INTERVAL`.

Generic over the adopted socket type `S`: `EspIdfTlsConnector` (wrap-an-existing-stream,
below) produces `EspTlsStream<EspIdfTcpStream>`.

**Generic Parameters:**
- S

**Traits:** ErrorType

**Trait Implementations:**

- **Read**
  - `fn read(self: & mut Self, buf: & mut [u8]) -> Result<usize, <Self as >::Error>`
- **Write**
  - `fn write(self: & mut Self, buf: &[u8]) -> Result<usize, <Self as >::Error>`
  - `fn flush(self: & mut Self) -> Result<(), <Self as >::Error>`



