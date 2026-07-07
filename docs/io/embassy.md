**bambino > io > embassy**

# Module: io::embassy

## Contents

**Structs**

- [`EmbassyRawStreamFactory`](#embassyrawstreamfactory) - Raw (pre-TLS) connection factory for the Embassy network stack.
- [`EmbassyTimer`](#embassytimer) - Timer implementation designed for the hardware microsecond clock in Embassy.
- [`EmbassyTlsConnector`](#embassytlsconnector) - TLS Secure connector wrapping the `embedded-tls` engine over caller-supplied buffers.
- [`EmbassyTlsStream`](#embassytlsstream) - Wrapper around an `embedded-tls` connection over an Embassy-supplied buffer pair.
- [`EmbassyUdpSocket`](#embassyudpsocket) - UDP Socket implementation designed for the Embassy network stack.

---

## bambino::io::embassy::EmbassyRawStreamFactory

*Struct*

Raw (pre-TLS) connection factory for the Embassy network stack.

Unlike Tokio's `TokioRawStreamFactory` (which dials a fresh `TcpStream` per call),
`embassy_net::tcp::TcpSocket` needs pre-allocated rx/tx buffer slices at construction —
there's no way to dial a raw connection without them. `RawStreamFactory::dial` is called
repeatedly from `&self` (MQTT's lazy reconnect, and FTPS's control channel once plus one
data-channel connect per transfer — `list_directory`, `upload_file`, `download_file` each
open and close their own), so a single buffer pair handed out once (Phase 2's
`EmbassyTlsConnector` pattern) isn't enough here.

Instead of hand-rolling a buffer pool, this wraps `embassy_net::tcp::client::TcpClient` —
embassy-net's own built-in connection pool (`embassy_net::tcp::client` module), which
solves exactly this problem: `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates N buffer
pairs, `TcpClient::connect()` checks one out and returns a `TcpConnection` that
automatically returns its slot to the pool on `Drop` — no unsafe code needed on our side,
and no risk of the panic-based mutual exclusion Phase 2 removed from `EmbassyTlsConnector`
(a pool with `N` slots simply fails a `connect()` call with `Error::ConnectionReset` if
all `N` are checked out, rather than panicking or aliasing memory).

**Why `&'static TcpClient`, not an owned one:** `RawStreamFactory<RawIO>`'s `RawIO`
is a fixed type for the whole trait impl, not parameterized per call — so the returned
`TcpConnection<'x, ...>`'s lifetime `'x` must be a *constant*, chosen once, not tied to
however long any individual `dial` call happens to borrow `&self` for.
Storing an *owned* `TcpClient<'d, ...>` field can't satisfy that: borrowing a field out of
`&self` can never outlive that particular call's borrow of `self`. Storing a `&'static`
*reference* sidesteps the problem entirely — copying a `&'static` reference out from
behind an arbitrarily short `&self` borrow yields an independent value that is itself
still valid for `'static`, so `TcpConnection<'static, ...>` comes out clean regardless of
how briefly any given call borrowed the factory. This pushes the actual `'static` storage
question (a `static` item, `static_cell::StaticCell`, or similar) to application setup
code, matching Phase 2's "caller supplies the buffer storage" philosophy — see the
README's Embassy section for a worked example.

**Generic Parameters:**
- const N
- const TX_SZ
- const RX_SZ

**Methods:**

- `fn new(client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>) -> Self` - `client` must be `'static` (e.g. built from a `static`/`StaticCell`-held `TcpClientState<N, TX_SZ, RX_SZ>`) — see this type's doc comment for why.

**Trait Implementations:**

- **RawStreamFactory**
  - `fn dial(self: &Self, host: &str, port: u16) -> Result<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>, SocketError>`



## bambino::io::embassy::EmbassyTimer

*Struct*

Timer implementation designed for the hardware microsecond clock in Embassy.

**Unit Struct**

**Trait Implementations:**

- **TimerProvider**
  - `fn sleep(self: &Self, duration: core::time::Duration) -> Result<(), TimerError>`
  - `fn now_millis(self: &Self) -> u64`



## bambino::io::embassy::EmbassyTlsConnector

*Struct*

TLS Secure connector wrapping the `embedded-tls` engine over caller-supplied buffers.

Generic over `Rng`: callers must provide a platform-appropriate RNG implementation
(e.g., hardware TRNG peripheral). The RNG must implement the legacy `rand_core` v0.6
traits expected by `embedded-tls` v0.19.

**Buffer ownership.** `embedded-tls` needs a read and write scratch buffer for the
lifetime of a TLS session (16KB apiece is a reasonable default, matching TLS's max
record size). Earlier versions of this connector hid two such buffers behind
process-wide statics, which meant a second concurrent connection (e.g. FTPS's control
and data channels, opened at the same time) would panic. There is no such thing as a
concurrency-safe *global* buffer pair, so this connector takes its buffers from the
caller instead: construct one `EmbassyTlsConnector` per concurrent connection you need,
each with its own `&'a mut [u8]` pair, and the caller's board-RAM budget decides how
many can exist at once. `connect()` takes the buffers out of the connector on first use
(`Option::take`) — calling `connect()` again on the same connector without a fresh one
returns `SocketError::Other` instead of a second, aliased borrow.

**No built-in connect timeout.** Unlike `EspIdfTlsConnector` (which bounds its handshake
loop behind a `connect_timeout`), `connect()` here has no retry/poll loop of its own to bound:
it calls `TlsConnection::open(context).await` once, and the hang risk lives entirely
inside `embedded-tls`'s handshake await, which this crate doesn't control. Callers that
need a bounded connect must race `EmbassyTlsConnector::connect` against
`embassy_time::with_timeout` themselves — `embassy-time` (already a dependency of the
`embassy` feature) provides exactly that combinator.

**Generic Parameters:**
- 'a
- CipherSuite
- Rng

**Methods:**

- `fn new(config: &'a ::embedded_tls::TlsConfig<'a>, rng: Rng, read_buf: &'a  mut [u8], write_buf: &'a  mut [u8]) -> Self` - Creates a new Embassy secure connector with a caller-provided RNG and TLS scratch buffers.

**Trait Implementations:**

- **TlsConnector**
  - `fn connect(self: &Self, _host: &str, raw_stream: RawStream) -> Result<<Self as >::Stream, SocketError>`
  - `fn negotiated_version(self: &Self, _stream: &<Self as >::Stream) -> Option<TlsVersion>` - `embedded-tls` 0.19 is a TLS 1.3-only client (confirmed against its docs — it has no TLS 1.2 handshake support and exposes no version-query method, since there is only ever one possible answer).



## bambino::io::embassy::EmbassyTlsStream

*Struct*

Wrapper around an `embedded-tls` connection over an Embassy-supplied buffer pair.

No longer guards a process-wide static — the read/write buffers are owned by the
[`EmbassyTlsConnector`] that produced this stream (see that type's doc comment).

**Generic Parameters:**
- 'a
- RawStream
- CipherSuite

**Traits:** ErrorType

**Trait Implementations:**

- **Read**
  - `fn read(self: & mut Self, buf: & mut [u8]) -> Result<usize, <Self as >::Error>`
- **Write**
  - `fn write(self: & mut Self, buf: &[u8]) -> Result<usize, <Self as >::Error>`
  - `fn flush(self: & mut Self) -> Result<(), <Self as >::Error>`



## bambino::io::embassy::EmbassyUdpSocket

*Struct*

UDP Socket implementation designed for the Embassy network stack.

Under Embassy, binding and state registration are coordinated via the stack's SocketSet
pool at boot time, so this type only implements [`AsyncUdpSocket`] (send/recv on an
already-existing socket) — it deliberately does not implement `BindableUdpSocket`,
since embassy-net's `UdpSocket::new()` requires pre-allocated buffer slices and its
`bind()` takes a typed `IpListenEndpoint`, not a `SocketAddr`. Construct one with
[`EmbassyUdpSocket::new()`] from an already-bound `embassy_net::udp::UdpSocket`.

**Generic Parameters:**
- 'a

**Methods:**

- `fn new(inner: ::embassy_net::udp::UdpSocket<'a>) -> Self` - Creates a wrapper using a pre-initialized Embassy UDP socket.

**Trait Implementations:**

- **AsyncUdpSocket**
  - `fn send_to(self: &Self, buf: &[u8], target: core::net::SocketAddr) -> Result<usize, SocketError>`
  - `fn recv_from(self: &Self, buf: & mut [u8]) -> Result<(usize, core::net::SocketAddr), SocketError>`



