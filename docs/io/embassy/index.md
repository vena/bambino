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
| [`EmbassyRawStreamFactory`](#embassyrawstreamfactory) | struct | Raw (pre-TLS) connection factory for the Embassy network stack. |
| [`EmbassyTimer`](#embassytimer) | struct | Timer implementation designed for the hardware microsecond clock in Embassy. |
| [`EmbassyTlsConnector`](#embassytlsconnector) | struct | TLS Secure connector wrapping an `mbedtls-rs` async [`Session`](::mbedtls_rs::Session). |
| [`EmbassyUdpSocket`](#embassyudpsocket) | struct | UDP Socket implementation designed for the Embassy network stack. |

## Types

### `EmbassyRawStreamFactory<const N: usize, const TX_SZ: usize, const RX_SZ: usize>`

```rust
struct EmbassyRawStreamFactory<const N: usize, const TX_SZ: usize, const RX_SZ: usize> {
    // [REDACTED: Private Fields]
}
```

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

#### Implementations

- <span id="embassyrawstreamfactory-new"></span>`fn new(client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>) -> Self`

  `client` must be `'static` (e.g. built from a `static`/`StaticCell`-held `TcpClientState<N, TX_SZ, RX_SZ>`) — see this type's doc comment for why.

#### Trait Implementations

##### `impl RawStreamFactory<TcpConnection<'static, N, TX_SZ, RX_SZ>> for EmbassyRawStreamFactory<N, TX_SZ, RX_SZ>`

- <span id="embassyrawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>, SocketError>` — [`SocketError`](../index.md#socketerror)

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

TLS Secure connector wrapping an `mbedtls-rs` async [`Session`](::mbedtls_rs::Session).

**One global `Tls` instance.** MbedTLS only permits one active library instance
program-wide (enforced by `mbedtls-rs` itself — a second `Tls::new()` call errors while one
is already live). The caller constructs that single `::mbedtls_rs::Tls` once at startup
(e.g. behind a `static_cell::StaticCell`, mirroring `EmbassyRawStreamFactory`'s `'static`
storage convention below — see the README's Embassy setup example) and passes a
[`TlsReference`](::mbedtls_rs::TlsReference) — a cheap `Copy` handle, not the `Tls` itself —
into each `EmbassyTlsConnector::new()` call. This lets MQTT's connector and FTPS's
control/data connectors all share the one instance concurrently.

**No caller-supplied buffers, unlike the old `embedded-tls` connector.** `mbedtls-rs`
allocates its own SSL context/config/record buffers per `Session` (via `mbedtls_calloc`,
16 KiB in/out by default — see `Cargo.toml`'s `mbedtls-rs` dependency comment to shrink
this via the `ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` features), so `connect()`
can be called repeatedly on the same connector — there is no one-shot buffer-consumption
constraint to work around, unlike the old `embedded-tls`-backed connector.

**`negotiated_version` always returns `None`, honestly.** `mbedtls-rs` exposes no public
API to read back the TLS version actually negotiated (see
`EMBASSY_TLS_ESCAPE_HATCH_PLAN.md`'s Problem section — confirmed by reading its source, not
assumed) — unlike the old `embedded-tls` connector, which hard-coded a wrong `Some(Tls13)`
answer. This means `BambuFtpsClient::connect()`'s TLS-1.2 enforcement check still fails
closed for P2S/X2D even after this backend swap; use
`PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` to opt out of that check when
needed (see `EMBASSY_TLS_ESCAPE_HATCH_PLAN.md` Track A).

**No built-in connect timeout**, same as before: `connect()` has no retry/poll loop of its
own to bound — the hang risk lives inside `mbedtls-rs`'s handshake await. Callers that need
a bounded connect must race `EmbassyTlsConnector::connect` against
`embassy_time::with_timeout` themselves.

#### Implementations

- <span id="embassytlsconnector-new"></span>`fn new(tls: ::mbedtls_rs::TlsReference<'a>) -> Self`

  Creates a new connector against the single active [`Tls`](::mbedtls_rs::Tls) instance

  (via its [`TlsReference`](::mbedtls_rs::TlsReference)), defaulting to no certificate

  verification — matching this crate's existing unsafe-by-default convention on other

  platforms (`build_unsafe_client_config`), since Bambu printers use self-signed certs.

- <span id="embassytlsconnector-with-ca-chain"></span>`fn with_ca_chain(self, ca_chain: ::mbedtls_rs::Certificate<'a>) -> Self`

  Enables server certificate verification against the given CA chain. Without this,

  the connector never checks the printer's certificate.

- <span id="embassytlsconnector-with-client-credentials"></span>`fn with_client_credentials(self, creds: ::mbedtls_rs::Credentials<'a>) -> Self`

  Supplies client credentials for mutual TLS (mTLS).

#### Trait Implementations

##### `impl<RawStream> TlsConnector<RawStream> for EmbassyTlsConnector<'a>`

- <span id="embassytlsconnector-tlsconnector-type-stream"></span>`type Stream = Session<'a, RawStream>`

- <span id="embassytlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: RawStream) -> Result<<Self as >::Stream, SocketError>` — [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="embassytlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, _stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

  `mbedtls-rs` exposes no API to read back the negotiated TLS version — see this type's

  doc comment and `EMBASSY_TLS_ESCAPE_HATCH_PLAN.md`'s Problem section. Return `None`

  honestly rather than hard-coding a guess (the anti-pattern the old `embedded-tls`

  connector had, just wrong in the other direction).

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
[`EmbassyUdpSocket::new()`] from an already-bound `embassy_net::udp::UdpSocket`.

#### Implementations

- <span id="embassyudpsocket-new"></span>`fn new(inner: ::embassy_net::udp::UdpSocket<'a>) -> Self`

  Creates a wrapper using a pre-initialized Embassy UDP socket.

#### Trait Implementations

##### `impl AsyncUdpSocket for EmbassyUdpSocket<'a>`

- <span id="embassyudpsocket-asyncudpsocket-send-to"></span>`async fn send_to(&self, buf: &[u8], target: core::net::SocketAddr) -> Result<usize, SocketError>` — [`SocketError`](../index.md#socketerror)

- <span id="embassyudpsocket-asyncudpsocket-recv-from"></span>`async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, core::net::SocketAddr), SocketError>` — [`SocketError`](../index.md#socketerror)

