*[bambino](../index.md) / [discovery](index.md)*

---

# Module `discovery`

# Printer Discovery (SSDP)

Find Bambu Lab printers on the local network using SSDP (Simple Service Discovery Protocol).

[`DiscoveryEngine`](#discoveryengine) sends M-SEARCH queries on UDP port 2021 (and the alternate port 1990)
and parses incoming NOTIFY/response packets into [`SsdpDevice`](parser/index.md#ssdpdevice) records.
[`DiscoveryEngine`](#discoveryengine) itself works across std, ESP-IDF, and Embassy via the
[`AsyncUdpSocket`](../io/index.md#asyncudpsocket) trait. The [`discover_devices()`] convenience function runs a timed
broadcast-and-listen sweep and returns all unique printers found, but is std-only
(`BindableUdpSocket` isn't implemented on Embassy — see
`.claude/rules/udp-socket-binding.md`); Embassy callers must drive `DiscoveryEngine`
directly instead.

## Contents

- [Modules](#modules)
  - [`parser`](#parser)
- [Types](#types)
  - [`DiscoveryEngine`](#discoveryengine)
- [Functions](#functions)
  - [`discover_devices`](#discover-devices)
- [Constants](#constants)
  - [`MULTICAST_ADDR`](#multicast-addr)
  - [`SSDP_PORT`](#ssdp-port)
  - [`SSDP_PORT_ALT`](#ssdp-port-alt)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`parser`](#parser) | mod | # Zero-Copy HTTP-style SSDP Parsing Engine |
| [`DiscoveryEngine`](#discoveryengine) | struct | Asynchronous Discovery Engine providing search orchestration and passive monitoring. |
| [`discover_devices`](#discover-devices) | fn | Broadcasts SSDP search queries and listens for printer responses for the given duration. |
| [`MULTICAST_ADDR`](#multicast-addr) | const | Standard Bambu Lab multicast group target for SSDP operations. |
| [`SSDP_PORT`](#ssdp-port) | const | Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS]. |
| [`SSDP_PORT_ALT`](#ssdp-port-alt) | const | Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS]. |

## Modules

- [`parser`](parser/index.md#parser) — # Zero-Copy HTTP-style SSDP Parsing Engine


---

## Types

### `SsdpDevice`

```rust
struct SsdpDevice {
    pub serial: String,
    pub model: crate::models::BambuModel,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub discovery_port: u16,
    pub version: String,
    pub connect_type: String,
    pub raw_model_str: String,
    pub signal_dbm: Option<i32>,
    pub bind_state: String,
    pub security_link: String,
}
```

Normalized device details extracted directly from SSDP UDP datagram payloads.

#### Fields

- **`serial`**: `String`

  The unique uppercase physical hardware serial number.

- **`model`**: `crate::models::BambuModel`

  Resolved printer capability profile based on prefixes and headers.

- **`name`**: `String`

  Human-friendly printer name defined by the user.

- **`ip`**: `String`

  Direct network target IP address extracted from the LOCATION header.

- **`port`**: `u16`

  Discovery communications port parsed from the LOCATION header.

- **`discovery_port`**: `u16`

  SSDP port on which the device was discovered (2021 or 1990).

- **`version`**: `String`

  Device firmware target version.

- **`connect_type`**: `String`

  Network connection medium (e.g. "lan", "wlan").

- **`raw_model_str`**: `String`

  Hardware identifier from the `DevModel.bambu.com` header, or the NT/ST URN-derived fallback string when that header is absent/empty (see `effective_dev_model`).

- **`signal_dbm`**: `Option<i32>`

  WiFi signal strength in dBm (e.g. -43), if reported by the device.

- **`bind_state`**: `String`

  Cloud binding state (e.g. "bound", "free").

- **`security_link`**: `String`

  Security link state (e.g. "secure").

#### Trait Implementations

##### `impl Clone for SsdpDevice`

- <span id="ssdpdevice-clone"></span>`fn clone(&self) -> SsdpDevice` — [`SsdpDevice`](parser/index.md#ssdpdevice)

##### `impl Debug for SsdpDevice`

- <span id="ssdpdevice-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for SsdpDevice`

##### `impl PartialEq for SsdpDevice`

- <span id="ssdpdevice-partialeq-eq"></span>`fn eq(&self, other: &SsdpDevice) -> bool` — [`SsdpDevice`](parser/index.md#ssdpdevice)

### `DiscoveryEngine<U: AsyncUdpSocket>`

```rust
struct DiscoveryEngine<U: AsyncUdpSocket> {
    // [REDACTED: Private Fields]
}
```

Asynchronous Discovery Engine providing search orchestration and passive monitoring.

#### Implementations

- <span id="discoveryengine-new"></span>`fn new(socket: U, port: u16) -> Self`

  Creates a new Discovery Engine bound to a specific SSDP port.

- <span id="discoveryengine-broadcast-search"></span>`async fn broadcast_search(&self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches active search queries to trigger local printer reports.

- <span id="discoveryengine-poll-next-device"></span>`async fn poll_next_device(&self, buf: &mut [u8]) -> Result<Option<SsdpDevice>, Error>` — [`SsdpDevice`](parser/index.md#ssdpdevice), [`Error`](../error/index.md#error)

  Listens on the bound socket interface and processes the next incoming SSDP packet.

#### Trait Implementations


---

## Functions

### `parse_ssdp_payload`

```rust
fn parse_ssdp_payload(buf: &[u8]) -> Option<SsdpDevice>
```

**Types:** [`SsdpDevice`](parser/index.md#ssdpdevice)

Parse an incoming raw UDP datagram buffer into normalized printer credentials.

Under the SSDP specification, responses map to standard HTTP responses, while
advertisements map to HTTP requests. This parser automatically evaluates the envelope
and routes the payload buffer to the appropriate parsing schema of `httparse`.

### `discover_devices`

```rust
async fn discover_devices<U, T>(timeout: core::time::Duration, timer: &T) -> Result<Vec<SsdpDevice>, crate::error::Error>
where
    U: BindableUdpSocket,
    T: TimerProvider
```

**Types:** [`SsdpDevice`](parser/index.md#ssdpdevice), [`Error`](../error/index.md#error)

Broadcasts SSDP search queries and listens for printer responses for the given duration.

Returns a deduplicated list of all printers found. The timer parameter drives sleep
timing, making this work across std, ESP-IDF, and Embassy.

# Example

```ignore
use bambino::discovery::discover_devices;
use bambino::io::tokio::{TokioUdpSocket, TokioTimer};

let timer = TokioTimer::new();
let printers = discover_devices::<TokioUdpSocket, _>(
    std::time::Duration::from_secs(5),
    &timer,
).await?;

for printer in &printers {
    println!("{} ({:?}) at {}", printer.name, printer.model, printer.ip);
}
```


---

## Constants

### `MULTICAST_ADDR`
```rust
const MULTICAST_ADDR: core::net::Ipv4Addr;
```

Standard Bambu Lab multicast group target for SSDP operations.

### `SSDP_PORT`
```rust
const SSDP_PORT: u16 = 2_021u16;
```

Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].

### `SSDP_PORT_ALT`
```rust
const SSDP_PORT_ALT: u16 = 1_990u16;
```

Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS].

