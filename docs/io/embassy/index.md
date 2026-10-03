*[bambino](../../index.md) / [io](../index.md) / [embassy](index.md)*

---

# Module `embassy`

# Bare-Metal Embassy Runtime Integration

Provides the concrete bindings of the abstract IO, Secure TLS transport,
and Timer interfaces for bare-metal targets utilizing the Embassy network
stack and `mbedtls-rs`.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`EmbassyRawStreamFactory`](#embassyrawstreamfactory) | struct | Raw (pre-TLS) connection factory for the Embassy network stack, dialing on an [`EmbassySocketPool`](#embassysocketpool). |
| [`EmbassySocketBuffers`](#embassysocketbuffers) | struct | Buffer storage for an [`EmbassySocketPool`](#embassysocketpool) of `N` sockets. |
| [`EmbassySocketPool`](#embassysocketpool) | struct | A fixed set of TCP sockets that every connection bambino makes over embassy-net is dialed on. |
| [`EmbassyTcpStream`](#embassytcpstream) | struct | A raw TCP connection from [`EmbassyRawStreamFactory`](#embassyrawstreamfactory), on a socket borrowed from an [`EmbassySocketPool`](#embassysocketpool). |
| [`EmbassyTimer`](#embassytimer) | struct | Timer implementation designed for the hardware microsecond clock in Embassy. |
| [`EmbassyTlsConnector`](#embassytlsconnector) | struct | TLS Secure connector wrapping an `mbedtls-rs` async `Session`. |
| [`EmbassyTlsStream`](#embassytlsstream) | struct | TLS stream returned by [`EmbassyTlsConnector::connect`](#embassytlsconnector), wrapping an `mbedtls-rs` `Session` so a failed read or write keeps its cause. |
| [`EmbassyUdpSocket`](#embassyudpsocket) | struct | UDP Socket implementation designed for the Embassy network stack. |

## Types

### `EmbassyRawStreamFactory`

```rust
struct EmbassyRawStreamFactory {
    // [REDACTED: Private Fields]
}
```

Raw (pre-TLS) connection factory for the Embassy network stack, dialing on an [`EmbassySocketPool`](#embassysocketpool).

`Copy`, so one pool can back the factories for every channel. See [`EmbassySocketPool`](#embassysocketpool) for
setup, sizing, and how a dropped connection is ended.

#### Implementations

- <span id="embassyrawstreamfactory-new"></span>`fn new(pool: &'static EmbassySocketPool) -> Self` — [`EmbassySocketPool`](#embassysocketpool)

  Creates a factory that dials on `pool`'s sockets.

#### Trait Implementations

##### `impl Clone for EmbassyRawStreamFactory`

- <span id="embassyrawstreamfactory-clone"></span>`fn clone(&self) -> EmbassyRawStreamFactory` — [`EmbassyRawStreamFactory`](#embassyrawstreamfactory)

##### `impl Copy for EmbassyRawStreamFactory`

##### `impl RawStreamFactory<EmbassyTcpStream> for EmbassyRawStreamFactory`

- <span id="embassyrawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<EmbassyTcpStream, SocketError>` — [`EmbassyTcpStream`](#embassytcpstream), [`SocketError`](../index.md#socketerror)

### `EmbassySocketBuffers<const N: usize, const TX_SZ: usize, const RX_SZ: usize>`

```rust
struct EmbassySocketBuffers<const N: usize, const TX_SZ: usize, const RX_SZ: usize> {
    // [REDACTED: Private Fields]
}
```

Buffer storage for an [`EmbassySocketPool`](#embassysocketpool) of `N` sockets.

Each socket gets a `TX_SZ`-byte send buffer and an `RX_SZ`-byte receive buffer. Plain byte
arrays, so it can be built in a `const` context and held in a `static_cell::StaticCell`;
[`EmbassySocketPool::new`](#embassysocketpool) borrows it for the rest of the program.

#### Implementations

- <span id="embassysocketbuffers-new"></span>`const fn new() -> Self`

  Creates zeroed buffers.

#### Trait Implementations

##### `impl Default for EmbassySocketBuffers<N, TX_SZ, RX_SZ>`

- <span id="embassysocketbuffers-default"></span>`fn default() -> Self`

### `EmbassySocketPool`

```rust
struct EmbassySocketPool {
    // [REDACTED: Private Fields]
}
```

A fixed set of TCP sockets that every connection bambino makes over embassy-net is dialed on.

**Why bambino owns the sockets.** embassy-net removes a `TcpSocket` from the stack when it is
dropped, and removing it sends nothing: a FIN that `close()` queued a moment earlier is lost,
and the peer is never told the connection ended. embassy-net's own `TcpClient` pool drops its
sockets exactly that way, so it cannot close a connection cleanly. This pool creates its `N`
sockets once and never removes them. A dropped [`EmbassyTcpStream`](#embassytcpstream) calls `close()` and hands
its socket back, the stack runner sends the FIN, and the socket is dialed again once the
connection has fully closed (`connect()` accepts a socket in `Closed` or `TimeWait`). Reusing
the same sockets for the program's life is also what lets their `&'static mut` buffers be
borrowed without `unsafe`.

**How a connection ends.** Always with a FIN, never an RST up front: an RST makes the printer
discard data it has acked but not yet read, such as the end of an uploaded file. The socket
counts as free only once the printer has sent its own FIN. Until then the pool reads and
discards whatever the printer still sends, so a transfer cancelled midway can finish and close
instead of stalling on a full receive window. A connection still closing 10 s after the drop
gets an RST. The pool enforces that bound itself rather than with `TcpSocket::set_timeout`: on
smoltcp 0.13.1, `close()` doesn't restart the timeout clock, so an idle connection would get
an RST in place of its FIN.

**Waiting its turn.** A dial waits while any connection of this pool is still closing, so the
printer, which caps concurrent connections, never holds an old bambino connection alongside a
new one. A dial fails at once with [`SocketError::ResourceExhausted`](../index.md#socketerror) when every socket is held
by a live stream. Size `N` as the most connections held at once: FTPS alone holds control and
data together. `N` also caps how many of the printer's connection slots bambino can take, so a
smaller `N` leaves more room for other tools. The `N` sockets stay registered for the
program's life, so the stack's `StackResources` must count them.

**The pool task.** [`run`](#embassysocketpool) settles closing connections as soon as the printer
allows; spawn it once. Without it a dial settles them itself, but a connection dropped
mid-transfer then stays half-open, holding a printer slot, until the next dial.

One pool serves every channel: hand copies of the same [`EmbassyRawStreamFactory`](#embassyrawstreamfactory) to MQTT,
FTPS and the camera.

```ignore
static SOCKET_BUFS: StaticCell<EmbassySocketBuffers<3>> = StaticCell::new();

#[embassy_executor::task]
async fn socket_pool_task(pool: &'static EmbassySocketPool) -> ! {
    pool.run().await
}

let pool = EmbassySocketPool::new(stack, SOCKET_BUFS.init(EmbassySocketBuffers::new()));
spawner.spawn(socket_pool_task(pool).unwrap());
let factory = EmbassyRawStreamFactory::new(pool);
```

Like embassy-net's `Stack`, the pool is neither `Send` nor `Sync`: use it from the executor
that runs the network stack.

#### Implementations

- <span id="embassysocketpool-new"></span>`fn new<const N: usize, const TX_SZ: usize, const RX_SZ: usize>(stack: ::embassy_net::Stack<'static>, bufs: &'static mut EmbassySocketBuffers<N, TX_SZ, RX_SZ>) -> &'static Self` — [`EmbassySocketBuffers`](#embassysocketbuffers)

  Creates one socket per buffer pair in `bufs`, registered on `stack` for good.

  The pool is leaked: dropping a socket would lose any FIN it still has to send, and
  `bufs` could never be borrowed again.

- <span id="embassysocketpool-run"></span>`async fn run(&self) -> never`

  Settles closing connections for as long as the program runs; spawn it as a task.

  Sleeps while nothing is closing, and wakes when a stream is dropped.

#### Trait Implementations

### `EmbassyTcpStream`

```rust
struct EmbassyTcpStream {
    // [REDACTED: Private Fields]
}
```

A raw TCP connection from [`EmbassyRawStreamFactory`](#embassyrawstreamfactory), on a socket borrowed from an [`EmbassySocketPool`](#embassysocketpool).

Dropping it ends the connection with a FIN and returns the socket to the pool, which keeps it
registered until the printer has closed its side too (see [`EmbassySocketPool`](#embassysocketpool)).

#### Trait Implementations

##### `impl AsyncIo for EmbassyTcpStream`

##### `impl Drop for EmbassyTcpStream`

- <span id="embassytcpstream-drop"></span>`fn drop(&mut self)`

##### `impl ErrorType for EmbassyTcpStream`

- <span id="embassytcpstream-errortype-type-error"></span>`type Error = Error`

##### `impl RawStreamFactory<EmbassyTcpStream> for EmbassyRawStreamFactory`

- <span id="embassyrawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<EmbassyTcpStream, SocketError>` — [`EmbassyTcpStream`](#embassytcpstream), [`SocketError`](../index.md#socketerror)

##### `impl Read for EmbassyTcpStream`

- <span id="embassytcpstream-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl Write for EmbassyTcpStream`

- <span id="embassytcpstream-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="embassytcpstream-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `EmbassyTimer`

```rust
struct EmbassyTimer;
```

Timer implementation designed for the hardware microsecond clock in Embassy.

#### Trait Implementations

##### `impl TimerProvider for EmbassyTimer`

- <span id="embassytimer-timerprovider-sleep"></span>`async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>` — [`TimerError`](../index.md#timererror)

- <span id="embassytimer-timerprovider-now-millis"></span>`fn now_millis(&self) -> u64`

### `EmbassyTlsConnector<'a>`

```rust
struct EmbassyTlsConnector<'a> {
    // [REDACTED: Private Fields]
}
```

TLS Secure connector wrapping an `mbedtls-rs` async `Session`.

**One global `Tls` instance.** MbedTLS only permits one active library instance
program-wide (enforced by `mbedtls-rs` itself — a second `Tls::new()` call errors while one
is already live). The caller constructs that single `::mbedtls_rs::Tls` once at startup
(e.g. behind a `static_cell::StaticCell`, mirroring `EmbassyRawStreamFactory`'s `'static`
storage convention below — see the README's Embassy setup example) and passes a
`TlsReference` — a cheap `Copy` handle, not the `Tls` itself —
into each `EmbassyTlsConnector::new()` call. This lets MQTT's connector and FTPS's
control/data connectors all share the one instance concurrently.

**No caller-supplied buffers.** `mbedtls-rs`
allocates its own SSL context/config/record buffers per `Session` (via `mbedtls_calloc`,
16 KiB in/out by default — see `Cargo.toml`'s `mbedtls-rs` dependency comment to shrink
this via the `ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` features), so `connect()`
can be called repeatedly on the same connector — there is no one-shot buffer-consumption
constraint to work around.

**`negotiated_version` reports the real negotiated version.** `mbedtls-rs` 0.3 added
`Session::tls_version()`, so `FtpsClient::connect()`'s TLS-1.2 enforcement check can be
satisfied for real on P2S/X2D rather than always failing closed — no need for
`PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` when the printer genuinely
negotiates 1.2 (see `src/ftps/CLAUDE.md` and this module's `CLAUDE.md`). This connector
still sets only `min_version`, so it cannot *cap* the peer at 1.2; a printer that
insisted on 1.3 would fail the check rather than be downgraded.

**No built-in connect timeout**, same as before: `connect()` has no retry/poll loop of its
own to bound — the hang risk lives inside `mbedtls-rs`'s handshake await. Callers that need
a bounded connect must race `EmbassyTlsConnector::connect` against
`embassy_time::with_timeout` themselves.

#### Implementations

- <span id="embassytlsconnector-new"></span>`fn new(tls: ::mbedtls_rs::TlsReference<'a>) -> Self`

  Creates a new connector against the single active `Tls` instance
  (via its `TlsReference`), defaulting to no certificate
  verification — matching this crate's existing unsafe-by-default convention on other
  platforms (`build_unsafe_client_config`), since Bambu printer certs chain to a private
  BBL CA that no OS trust store carries.

- <span id="embassytlsconnector-with-ca-chain"></span>`fn with_ca_chain(self, ca_chain: ::mbedtls_rs::Certificate<'a>) -> Self`

  Enables server certificate verification against the given CA chain. Without this,
  the connector never checks the printer's certificate.

- <span id="embassytlsconnector-with-client-credentials"></span>`fn with_client_credentials(self, creds: ::mbedtls_rs::Credentials<'a>) -> Self`

  Supplies client credentials for mutual TLS (mTLS).

#### Trait Implementations

##### `impl<RawStream> TlsConnector<RawStream> for EmbassyTlsConnector<'a>`

- <span id="embassytlsconnector-tlsconnector-type-stream"></span>`type Stream = EmbassyTlsStream<'a, RawStream>`

- <span id="embassytlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: RawStream) -> Result<<Self as >::Stream, SocketError>` — [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="embassytlsconnector-tlsconnector-close"></span>`async fn close(&self, stream: &mut <Self as >::Stream) -> Result<(), SocketError>` — [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

  Sends `close_notify` via `mbedtls-rs` 0.3's `Session::close()`.

  Idempotent for free: `Session::close` returns `Ok(())` immediately when its own
  `connected` flag is already clear, and clears that flag on success — which is also what
  silences the `Session dropped without being closed properly` warning `mbedtls-rs` emits
  from `Drop`.

  Closing does **not** release the session's record buffers; `mbedtls-rs` frees those in
  `Drop`, which on an ESP32-C6 is ~48 KB per session (GitHub issue #293). Callers that
  need the memory back must drop the stream, not merely close it.

- <span id="embassytlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

  Reports the TLS version actually negotiated, via `mbedtls-rs` 0.3's
  `Session::tls_version()`.

  `None` means the handshake has not completed (or the session was closed), never "this
  backend cannot tell": `mbedtls-rs` gates the accessor on its own `connected` flag
  because MbedTLS seeds the underlying field with the *configured maximum* version at
  setup and on every reset, which is not a version the peers have agreed on.

- <span id="embassytlsconnector-tlsconnector-peer-chain-der"></span>`fn peer_chain_der(&self, _stream: &<Self as >::Stream) -> Option<Vec<Vec<u8>>>` — [`TlsConnector`](../index.md#tlsconnector)

  `mbedtls-rs` exposes neither a peer-certificate accessor nor the raw
  `mbedtls_ssl_context` pointer that would let this crate call
  `mbedtls_ssl_get_peer_cert` itself (confirmed by reading its source: the only
  post-handshake inspectors on `Session` are `tls_verification_details` and `tls_alpn`).
  The ESP-IDF backend can do this only because `esp_tls_get_ssl_context` hands out that
  pointer. Return `None` honestly — a consumer pinning certificates cannot do so on this
  backend today, and must fail closed rather than be handed a fabricated empty chain.

### `EmbassyTlsStream<'a, T: AsyncIo>`

```rust
struct EmbassyTlsStream<'a, T: AsyncIo>();
```

TLS stream returned by [`EmbassyTlsConnector::connect`](#embassytlsconnector), wrapping an `mbedtls-rs` `Session` so a failed read or write keeps its cause.

`Session` implements `embedded_io_async` itself, but its error reports every mbedTLS failure
as `ErrorKind::Other` (`mbedtls-rs` 0.3.0, `impl embedded_io::Error for SessionError`), so
running out of memory, a peer reset, and a send failure all looked the same to a caller.
This stream reclassifies the mbedTLS code with the table the ESP-IDF backend uses, so an
allocation failure surfaces as `OutOfMemory` and from there as
[`SocketError::ResourceExhausted`](../index.md#socketerror) (GitHub issue #385). Errors from the underlying stream
keep their own kind. Reads and writes forward to `Session` unchanged otherwise, including
its lack of cancel safety.

[`session`](#embassytlsstream) and [`session_mut`](#embassytlsstream) reach the `Session` for
anything else it offers.

#### Implementations

- <span id="embassytlsstream-session"></span>`fn session(&self) -> &::mbedtls_rs::Session<'a, T>`

  Returns the underlying `mbedtls-rs` session.

- <span id="embassytlsstream-session-mut"></span>`fn session_mut(&mut self) -> &mut ::mbedtls_rs::Session<'a, T>`

  Returns the underlying `mbedtls-rs` session mutably.

  Reading or writing through it directly bypasses this stream's error classification.

#### Trait Implementations

##### `impl<T> AsyncIo for EmbassyTlsStream<'a, T>`

##### `impl<T: AsyncIo> ErrorType for EmbassyTlsStream<'_, T>`

- <span id="embassytlsstream-errortype-type-error"></span>`type Error = ErrorKind`

##### `impl<T: AsyncIo> Read for EmbassyTlsStream<'_, T>`

- <span id="embassytlsstream-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl<T: AsyncIo> Write for EmbassyTlsStream<'_, T>`

- <span id="embassytlsstream-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="embassytlsstream-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `EmbassyUdpSocket<'a>`

```rust
struct EmbassyUdpSocket<'a> {
    // [REDACTED: Private Fields]
}
```

UDP Socket implementation designed for the Embassy network stack.

Under Embassy, binding and state registration are coordinated via the stack's SocketSet
pool at boot time, so this type only implements [`AsyncUdpSocket`](../index.md#asyncudpsocket) (send/recv on an
already-existing socket) — it deliberately does not implement `BindableUdpSocket`,
since embassy-net's `UdpSocket::new()` requires pre-allocated buffer slices and its
`bind()` takes a typed `IpListenEndpoint`, not a `SocketAddr`. Construct one with
[`EmbassyUdpSocket::new()`](#embassyudpsocket) from an already-bound `embassy_net::udp::UdpSocket`.

#### Implementations

- <span id="embassyudpsocket-new"></span>`fn new(inner: ::embassy_net::udp::UdpSocket<'a>) -> Self`

  Creates a wrapper using a pre-initialized Embassy UDP socket.

#### Trait Implementations

##### `impl AsyncUdpSocket for EmbassyUdpSocket<'a>`

- <span id="embassyudpsocket-asyncudpsocket-send-to"></span>`async fn send_to(&self, buf: &[u8], target: core::net::SocketAddr) -> Result<usize, SocketError>` — [`SocketError`](../index.md#socketerror)

- <span id="embassyudpsocket-asyncudpsocket-recv-from"></span>`async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, core::net::SocketAddr), SocketError>` — [`SocketError`](../index.md#socketerror)

