**bambino > io**

# Module: io

## Contents

**Modules**

- [`tokio`](#tokio) - # Tokio Host Runtime Implementation

**Enums**

- [`SocketError`](#socketerror) - Unified transport-level Socket Errors, agnostic of runtime implementations.
- [`TimerError`](#timererror) - Unified timer/sleep errors, agnostic of runtime implementations.
- [`TlsVersion`](#tlsversion) - TLS protocol version negotiated during a handshake.

**Traits**

- [`AsyncIo`](#asyncio) - Consolidated Async Read + Write trait boundary.
- [`AsyncUdpSocket`](#asyncudpsocket) - Asynchronous UDP Socket trait for unicast and multicast printer discovery.
- [`BindableUdpSocket`](#bindableudpsocket) - Dynamically constructs a new UDP socket bound to a local address.
- [`RawStreamFactory`](#rawstreamfactory) - Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port.
- [`TimerProvider`](#timerprovider) - Platform-neutral asynchronous sleep controller.
- [`TlsConnector`](#tlsconnector) - Abstract TLS secure stream connector trait.

---

## bambino::io::AsyncIo

*Trait*

Consolidated Async Read + Write trait boundary.

Intermediates communication across all layers (MQTTS, FTPS, RTSPS, Port 6000).
Automatically implemented for any types satisfying the core `embedded-io-async` traits.



## bambino::io::AsyncUdpSocket

*Trait*

Asynchronous UDP Socket trait for unicast and multicast printer discovery.

Interlaces with Port 2021 SSDP traffic defined in [REF-NET-DISC]. Implemented by every
platform on an already-existing socket. For constructing a *new* socket bound to an
address string, see [`BindableUdpSocket`] — kept separate because Embassy's network
stack cannot support it (see that trait's doc comment).

**Methods:**

- `send_to`: Dispatches a raw datagram payload to the given target address.
- `recv_from`: Listens for incoming datagrams, populating the buffer and returning the source address.



## bambino::io::BindableUdpSocket

*Trait*

Dynamically constructs a new UDP socket bound to a local address.

Only implementable on platforms with OS-level dynamic socket creation (std/tokio,
ESP-IDF's BSD sockets). Embassy-net sockets must be constructed from pre-allocated
buffer slices supplied by the caller and bound via a typed `IpListenEndpoint` on an
already-existing socket, not a `SocketAddr` — so `EmbassyUdpSocket` does not implement
this trait. Mirrors the existing `TlsConnector`/`RawStreamFactory` split, which draws the
same boundary for TLS connection setup.

**Methods:**

- `bind`: Binds to the designated local address, constructing a new socket.



## bambino::io::RawStreamFactory

*Trait*

Dials a fresh, un-encrypted (pre-TLS) raw stream to a host:port.

Protocol-neutral by design: MQTT's lazy connect (`PrinterClient::ensure_mqtt`) and FTPS's
per-transfer passive data channel (`BambuFtpsClient::list_directory`/`upload_file`/
`download_file`) both just need "give me a raw stream to host:port" with no
protocol-specific semantics in the trait itself — confirmed by every implementor
(`TokioRawStreamFactory`, `EspIdfRawStreamFactory`, `EmbassyRawStreamFactory`) having zero
FTP- or MQTT-specific logic. Non-consuming (`&self`) so a `PrinterClient` can hold one
persistently and call it on every lazy (re)connect, mirroring `TlsConnector::connect`.

**Methods:**

- `dial`: Connects a raw, un-encrypted socket to the designated host and port.



## bambino::io::SocketError

*Enum*

Unified transport-level Socket Errors, agnostic of runtime implementations.

`Other` carries `Cow<'static, str>` (not `Copy`, hence the enum overall isn't either)
rather than a fixed `&'static str` so platform backends can attach dynamic content —
e.g. ESP-IDF's error mapping (`src/io/esp_idf.rs::map_esp_tls_connect_error`) formats
the actual numeric `EspError` code into the message instead of a fixed compile-time
string. Mirrors `BambuError::ProtocolViolation`'s existing use of the same type for the
same reason (dynamic message content in a `no_std`+`alloc`-compatible way).

**Variants:**
- `ConnectionRefused` - Remote socket explicitly refused connection.
- `ConnectionAborted` - Connection was terminated by the local or remote software stack.
- `ConnectionReset` - Connection was abruptly terminated by the remote host.
- `NotConnected` - Socket is in an un-established state.
- `TimedOut` - Handshake, read, or write boundaries timed out.
- `AddressInUse` - Local port or address is already bound by another process.
- `AddressNotAvailable` - Bound interface address is no longer valid or accessible.
- `InvalidInput` - Supplied connection or routing parameters are malformed.
- `Other(std::borrow::Cow<'static, str>)` - Catch-all variant for atypical OS-specific networking errors.

**Traits:** Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &SocketError) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> SocketError`



## bambino::io::TimerError

*Enum*

Unified timer/sleep errors, agnostic of runtime implementations.

Mirrors [`SocketError`]'s shape. Tokio and Embassy sleeps are infallible, so only
ESP-IDF's `EspAsyncTimer` (which can fail on FreeRTOS timer/task resource exhaustion)
ever constructs this.

**Variants:**
- `Other(&'static str)` - Catch-all for platform-specific timer scheduling failures.

**Traits:** Copy, Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> TimerError`
- **PartialEq**
  - `fn eq(self: &Self, other: &TimerError) -> bool`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::io::TimerProvider

*Trait*

Platform-neutral asynchronous sleep controller.

Required to resolve post-boot handshakes, retry throttling, and camera frame pacing
without burning processor cycles on embedded platforms.

**Methods:**

- `sleep`: Suspends execution of the calling task for the specified duration.
- `now_millis`: Returns the current monotonic clock value in milliseconds.
- `has_real_clock`: Whether this timer provides genuine wall-clock timing. `true` (the default) for



## bambino::io::TlsConnector

*Trait*

Abstract TLS secure stream connector trait.

Facilitates wrapping raw TCP transport interfaces inside secure SSL/TLS sessions
without enforcing a static library provider.

**Methods:**

- `Stream`: The resulting encrypted socket stream type.
- `connect`: Negotiates a secure TLS handshake with the targeted printer.
- `negotiated_version`: Returns the TLS protocol version negotiated on the given stream.



## bambino::io::TlsVersion

*Enum*

TLS protocol version negotiated during a handshake.

**Variants:**
- `Tls12`
- `Tls13`

**Traits:** Copy, Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &TlsVersion) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> TlsVersion`



## Module: tokio

# Tokio Host Runtime Implementation

Provides the concrete bindings of the abstract IO, Secure TLS transport,
and Timer interfaces for standard operating systems using the Tokio runtime
and the Rustls TLS stack.



