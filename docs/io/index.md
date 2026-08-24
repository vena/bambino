*[bambino](../index.md) / [io](index.md)*

---

# Module `io`

# Transport Abstraction Layer

Defines the async I/O traits that let the rest of the crate work without knowing
which runtime it's running on. The key traits:

- [`AsyncIo`](#asyncio) — Read + Write (blanket-implemented for anything satisfying `embedded-io-async`).
- [`TlsConnector`](#tlsconnector) — Wraps a raw stream in TLS (used by tokio/rustls and embassy/mbedtls-rs).
- [`RawStreamFactory`](#rawstreamfactory) — Dials a fresh raw (pre-TLS) stream to a host:port. Used for MQTT's
  lazy connect and FTPS's per-transfer data channel.
- [`AsyncUdpSocket`](#asyncudpsocket) — UDP send/recv for SSDP discovery.
- [`BindableUdpSocket`](#bindableudpsocket) — construct-and-bind a new UDP socket by address (std/tokio, ESP-IDF only).
- [`TimerProvider`](#timerprovider) — Async sleep and monotonic clock for platform-agnostic timeouts.

Platform implementations live in the `tokio`, `esp_idf`, and `embassy` submodules
(each gated behind its respective feature flag).
The `TokioIo` adapter (only present when the `tokio` feature is enabled) bridges Tokio's `AsyncRead`/`AsyncWrite` to `embedded-io-async`.

## Contents

- [Modules](#modules)
  - [`embassy`](embassy/index.md)
  - [`tokio`](tokio/index.md)
- [Types](#types)
  - [`SocketError`](#socketerror)
  - [`TimerError`](#timererror)
  - [`TlsVersion`](#tlsversion)
- [Traits](#traits)
  - [`AsyncIo`](#asyncio)
  - [`AsyncUdpSocket`](#asyncudpsocket)
  - [`BindableUdpSocket`](#bindableudpsocket)
  - [`RawStreamFactory`](#rawstreamfactory)
  - [`TimerProvider`](#timerprovider)
  - [`TlsConnector`](#tlsconnector)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`embassy`](embassy/index.md) | mod | # Bare-Metal Embassy Runtime Integration |
| [`tokio`](tokio/index.md) | mod | # Tokio Host Runtime Implementation |
| [`SocketError`](#socketerror) | enum | Unified transport-level Socket Errors, agnostic of runtime implementations. |
| [`TimerError`](#timererror) | enum | Unified timer/sleep errors, agnostic of runtime implementations. |
| [`TlsVersion`](#tlsversion) | enum | TLS protocol version negotiated during a handshake. |
| [`AsyncIo`](#asyncio) | trait | Consolidated Async Read + Write trait boundary. |
| [`AsyncUdpSocket`](#asyncudpsocket) | trait | Asynchronous UDP Socket trait for unicast and multicast printer discovery. |
| [`BindableUdpSocket`](#bindableudpsocket) | trait | Dynamically constructs a new UDP socket bound to a local address. |
| [`RawStreamFactory`](#rawstreamfactory) | trait | Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port. |
| [`TimerProvider`](#timerprovider) | trait | Platform-neutral asynchronous sleep controller. |
| [`TlsConnector`](#tlsconnector) | trait | Abstract TLS secure stream connector trait. |

## Modules

- [`embassy`](embassy/index.md) — # Bare-Metal Embassy Runtime Integration
- [`tokio`](tokio/index.md) — # Tokio Host Runtime Implementation


---

## Types

### `TokioIo<T>`

```rust
struct TokioIo<T>(T);
```

Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.

#### Trait Implementations

##### `impl<T> AsyncIo for TokioIo<T>`

##### `impl<T> ErrorType for TokioIo<T>`

- <span id="tokioio-errortype-type-error"></span>`type Error = TokioIoError`

##### `impl RawStreamFactory<TokioIo<TcpStream>> for TokioRawStreamFactory`

- <span id="tokiorawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError>` — [`TokioIo`](tokio/index.md#tokioio), [`SocketError`](#socketerror)

##### `impl<T: ::tokio::io::AsyncRead + Unpin> Read for TokioIo<T>`

- <span id="tokioio-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl<T> Same for TokioIo<T>`

- <span id="tokioio-same-type-output"></span>`type Output = T`

##### `impl TlsConnector<TokioIo<TcpStream>> for TokioTlsConnector`

- <span id="tokiotlsconnector-tlsconnector-type-stream"></span>`type Stream = TokioIo<TlsStream<TcpStream>>`

- <span id="tokiotlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>` — [`TokioIo`](tokio/index.md#tokioio), [`TlsConnector`](#tlsconnector), [`SocketError`](#socketerror)

- <span id="tokiotlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](#tlsconnector), [`TlsVersion`](#tlsversion)

- <span id="tokiotlsconnector-tlsconnector-peer-chain-der"></span>`fn peer_chain_der(&self, stream: &<Self as >::Stream) -> Option<Vec<Vec<u8>>>` — [`TlsConnector`](#tlsconnector)

  rustls retains the peer chain on the connection after the handshake, so this is a
  straight copy of what the server sent, in wire order (leaf first).

##### `impl<T: ::tokio::io::AsyncWrite + Unpin> Write for TokioIo<T>`

- <span id="tokioio-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="tokioio-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `TokioIoError`

```rust
struct TokioIoError(std::io::Error);
```

Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.

#### Trait Implementations

##### `impl Debug for TokioIoError`

- <span id="tokioioerror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for TokioIoError`

- <span id="tokioioerror-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Error for TokioIoError`

- <span id="tokioioerror-error-source"></span>`fn source(&self) -> Option<&dyn std::error::Error>`

##### `impl ToString for TokioIoError`

- <span id="tokioioerror-tostring-to-string"></span>`fn to_string(&self) -> String`

### `SocketError`

```rust
enum SocketError {
    ConnectionRefused,
    ConnectionAborted,
    ConnectionReset,
    NotConnected,
    TimedOut,
    AddressInUse,
    AddressNotAvailable,
    InvalidInput,
    Other(std::borrow::Cow<'static, str>),
}
```

Unified transport-level Socket Errors, agnostic of runtime implementations.

`Other` carries `Cow<'static, str>` (not `Copy`, hence the enum overall isn't either)
rather than a fixed `&'static str` so platform backends can attach dynamic content —
e.g. ESP-IDF's error mapping (`src/io/esp_idf.rs::map_esp_tls_connect_error`) formats
the actual numeric `EspError` code into the message instead of a fixed compile-time
string. Mirrors `Error::ProtocolViolation`'s existing use of the same type for the
same reason (dynamic message content in a `no_std`+`alloc`-compatible way).

#### Variants

- **`ConnectionRefused`**

  Remote socket explicitly refused connection.

- **`ConnectionAborted`**

  Connection was terminated by the local or remote software stack.

- **`ConnectionReset`**

  Connection was abruptly terminated by the remote host.

- **`NotConnected`**

  Socket is in an un-established state.

- **`TimedOut`**

  Handshake, read, or write boundaries timed out.

- **`AddressInUse`**

  Local port or address is already bound by another process.

- **`AddressNotAvailable`**

  Bound interface address is no longer valid or accessible.

- **`InvalidInput`**

  Supplied connection or routing parameters are malformed.

- **`Other`**

  Catch-all variant for atypical OS-specific networking errors.

#### Trait Implementations

##### `impl Clone for SocketError`

- <span id="socketerror-clone"></span>`fn clone(&self) -> SocketError` — [`SocketError`](#socketerror)

##### `impl Debug for SocketError`

- <span id="socketerror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for SocketError`

##### `impl PartialEq for SocketError`

- <span id="socketerror-partialeq-eq"></span>`fn eq(&self, other: &SocketError) -> bool` — [`SocketError`](#socketerror)

### `TimerError`

```rust
enum TimerError {
    Other(&'static str),
}
```

Unified timer/sleep errors, agnostic of runtime implementations.

Mirrors [`SocketError`](#socketerror)'s shape. Tokio and Embassy sleeps are infallible, so only
ESP-IDF's `EspAsyncTimer` (which can fail on FreeRTOS timer/task resource exhaustion)
ever constructs this.

#### Variants

- **`Other`**

  Catch-all for platform-specific timer scheduling failures.

#### Trait Implementations

##### `impl Clone for TimerError`

- <span id="timererror-clone"></span>`fn clone(&self) -> TimerError` — [`TimerError`](#timererror)

##### `impl Copy for TimerError`

##### `impl Debug for TimerError`

- <span id="timererror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for TimerError`

##### `impl PartialEq for TimerError`

- <span id="timererror-partialeq-eq"></span>`fn eq(&self, other: &TimerError) -> bool` — [`TimerError`](#timererror)

### `TlsVersion`

```rust
enum TlsVersion {
    Tls12,
    Tls13,
}
```

TLS protocol version negotiated during a handshake.

#### Variants

- **`Tls12`**

  TLS 1.2 negotiated.

- **`Tls13`**

  TLS 1.3 negotiated.

#### Trait Implementations

##### `impl Clone for TlsVersion`

- <span id="tlsversion-clone"></span>`fn clone(&self) -> TlsVersion` — [`TlsVersion`](#tlsversion)

##### `impl Copy for TlsVersion`

##### `impl Debug for TlsVersion`

- <span id="tlsversion-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for TlsVersion`

##### `impl PartialEq for TlsVersion`

- <span id="tlsversion-partialeq-eq"></span>`fn eq(&self, other: &TlsVersion) -> bool` — [`TlsVersion`](#tlsversion)


---

## Traits

### `AsyncIo`

```rust
trait AsyncIo: embedded_io_async::Read + embedded_io_async::Write { ... }
```


Consolidated Async Read + Write trait boundary.

Intermediates communication across all layers (MQTTS, FTPS, RTSPS, Port 6000).
Automatically implemented for any types satisfying the core `embedded-io-async` traits.

#### Implementors

- `T`

### `AsyncUdpSocket`

```rust
trait AsyncUdpSocket { ... }
```


Asynchronous UDP Socket trait for unicast and multicast printer discovery.

Interlaces with Port 2021 SSDP traffic defined in [REF-NET-DISC]. Implemented by every
platform on an already-existing socket. For constructing a *new* socket bound to an
address string, see [`BindableUdpSocket`](#bindableudpsocket) — kept separate because Embassy's network
stack cannot support it (see that trait's doc comment).

#### Required Methods

- `fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>`

  Dispatches a raw datagram payload to the given target address.

- `fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError>`

  Listens for incoming datagrams, populating the buffer and returning the source address.

  Implementations must not busy-spin: on a "no data yet" outcome, either genuinely
  block/wait for data, or internally yield for a bounded duration (e.g. via
  [`TimerProvider::sleep`](#timerprovider)) before reporting `Err`. A synchronous non-blocking read
  that returns instantly on every "would block" call defeats the caller's own pacing
  — `discover_devices` (`src/discovery/mod.rs`) polls this in a tight loop relying on
  each call to provide some wait/yield, and an implementation that returns immediately
  turns that loop into a genuine busy-spin, burning 100% CPU and potentially starving
  other tasks on single-core/cooperative-scheduler platforms.

#### Implementors

- [`EmbassyUdpSocket`](embassy/index.md#embassyudpsocket)
- [`TokioUdpSocket`](tokio/index.md#tokioudpsocket)

### `BindableUdpSocket`

```rust
trait BindableUdpSocket: AsyncUdpSocket + Sized { ... }
```


Dynamically constructs a new UDP socket bound to a local address.

Only implementable on platforms with OS-level dynamic socket creation (std/tokio,
ESP-IDF's BSD sockets). Embassy-net sockets must be constructed from pre-allocated
buffer slices supplied by the caller and bound via a typed `IpListenEndpoint` on an
already-existing socket, not a `SocketAddr` — so `EmbassyUdpSocket` does not implement
this trait. Mirrors the existing `TlsConnector`/`RawStreamFactory` split, which draws the
same boundary for TLS connection setup.

#### Required Methods

- `fn bind(addr: SocketAddr) -> Result<Self, SocketError>`

  Binds to the designated local address, constructing a new socket.

#### Implementors

- [`TokioUdpSocket`](tokio/index.md#tokioudpsocket)

### `RawStreamFactory<RawIO: AsyncIo>`

```rust
trait RawStreamFactory<RawIO: AsyncIo> { ... }
```


Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port.

Protocol-neutral by design: MQTT's lazy connect (`PrinterClient::ensure_mqtt`) and FTPS's
per-transfer passive data channel (`FtpsClient::list_directory`/`upload_file`/
`download_file`) both just need "give me a raw stream to host:port" with no
protocol-specific semantics in the trait itself — confirmed by every implementor
(`TokioRawStreamFactory`, `EspIdfRawStreamFactory`, `EmbassyRawStreamFactory`) having zero
FTP- or MQTT-specific logic. Non-consuming (`&self`) so a `PrinterClient` can hold one
persistently and call it on every lazy (re)connect, mirroring `TlsConnector::connect`.

#### Required Methods

- `fn dial(&self, host: &str, port: u16) -> Result<RawIO, SocketError>`

  Connects a raw, un-encrypted socket to the designated host and port.

#### Implementors

- [`EmbassyRawStreamFactory`](embassy/index.md#embassyrawstreamfactory)
- [`TokioRawStreamFactory`](tokio/index.md#tokiorawstreamfactory)

### `TimerProvider`

```rust
trait TimerProvider { ... }
```


Platform-neutral asynchronous sleep controller.

Required to resolve post-boot handshakes, retry throttling, and camera frame pacing
without burning processor cycles on embedded platforms.

#### Required Methods

- `fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>`

  Suspends execution of the calling task for the specified duration.

  Returns an error on platforms where sleep scheduling can genuinely fail
  (e.g. ESP-IDF's FreeRTOS timer/task resources). Infallible platforms
  (tokio, embassy) always return `Ok(())`.

- `fn now_millis(&self) -> u64`

  Returns the current monotonic clock value in milliseconds.

  The epoch is platform-specific (process start, system boot, etc.) — only
  *differences* between two calls are meaningful.

#### Provided Methods 

- `fn has_real_clock(&self) -> bool`

  Whether this timer provides genuine wall-clock timing.
  `true` (the default) for every real platform implementation (`TokioTimer`, `EmbassyTimer`,
  `EspIdfTimer`). Only `PrinterClient`'s `DummyTimer` default overrides this to `false`.

  Exists so code that races an I/O operation against
  [`sleep()`](#timerprovider) — e.g. `src/mqtt/client/mod.rs`'s `poll_wire`/
  `src/mqtt/client/frame.rs`'s `read_exact_packet` per-read deadline — can tell whether
  doing so will actually bound anything. `DummyTimer::sleep()` intentionally completes
  instantly regardless of the requested duration (so it never blocks retry/backoff loops
  that happen to be generic over `TimerProvider`); racing against it would make
  such a race resolve to "timed out" on essentially every call that doesn't also
  complete synchronously, silently turning "no wall-clock timeout configured" into
  "everything times out immediately" instead of the intended "no wall-clock
  protection here, fall back to other safety valves" (the same tradeoff
  `PrinterClient::poll_until`'s elapsed-time check already documents for
  `DummyTimer`). Callers should check this before racing against `sleep()` and
  skip the race entirely (plain unbounded await) when it's `false`.

#### Implementors

- [`EmbassyTimer`](embassy/index.md#embassytimer)
- [`TokioTimer`](tokio/index.md#tokiotimer)

### `TlsConnector<RawStream: AsyncIo>`

```rust
trait TlsConnector<RawStream: AsyncIo> { ... }
```


Abstract TLS secure stream connector trait.

Facilitates wrapping raw TCP transport interfaces inside secure SSL/TLS sessions
without enforcing a static library provider.

#### Associated Types

- `type Stream: 1`

#### Required Methods

- `fn connect(&self, host: &str, raw_stream: RawStream) -> Result<<Self as >::Stream, SocketError>`

  Negotiates a secure TLS handshake with the targeted printer.

#### Provided Methods 

- `fn negotiated_version(&self, _stream: &<Self as >::Stream) -> Option<TlsVersion>`

  Returns the TLS protocol version negotiated on the given stream.

  Platforms that cannot inspect the negotiated version return `None`. This does **not**
  mean "skip validation" — a caller enforcing a specific version (e.g.
  `FtpsClient::require_tls_1_2_if_enforced`, which P2S/X2D need) treats an
  undetermined `None` as a failure to confirm the required version and rejects the
  connection, the same as a confirmed wrong version. `None` only means "this platform
  has nothing useful to report" — whether that's fail-open or fail-closed is entirely
  up to the caller.

- `fn peer_chain_der(&self, _stream: &<Self as >::Stream) -> Option<Vec<Vec<u8>>>`

  Returns the peer's certificate chain exactly as presented during the handshake,
  DER-encoded, leaf first.

  Exists so a consumer can pin a printer's certificate — this crate ships no CA material
  and deliberately treats certificates as runtime input, so trust-on-first-use has to be
  built on top of it. Storage and policy (where a pin lives, what happens on a mismatch)
  belong to the caller; all this provides is read access to what the peer actually sent.

  Returns `None` where the platform cannot report it — a *lack of information*, never
  "the peer sent nothing" and never "skip validation". A caller enforcing a pin must treat
  `None` as a failure to confirm, exactly as `negotiated_version` documents above.

  Whether the chain contains the issuing CA or only the leaf is up to the peer, and that
  decides what pinning is possible. Confirmed on a P1S: two certificates, the `CN=<serial>`
  leaf followed by the self-signed `CN=BBL CA` root (`CA:TRUE`) — so an anchor *can* be
  captured at first contact and fed back through `with_certs(..)` for genuine chain
  verification, rather than being limited to a leaf-fingerprint comparison. Only the P1S has
  been checked; use `bambino-cli inspect-cert` to confirm any other model rather than
  assuming it generalizes.

  The returned DER is copied out of the live session: on ESP-IDF the chain is owned by the
  SSL context and is freed on drop or renegotiation, so borrowing it would dangle.

#### Implementors

- [`EmbassyTlsConnector`](embassy/index.md#embassytlsconnector)
- [`TokioTlsConnector`](tokio/index.md#tokiotlsconnector)

