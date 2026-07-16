*[bambino](../../index.md) / [discovery](../index.md) / [parser](index.md)*

---

# Module `parser`

# Zero-Copy HTTP-style SSDP Parsing Engine

Provides utilities to parse HTTP-like headers from multicast and unicast
UDP frames on Port 2021 without performing runtime memory allocations.
Differentiates Bambu Lab printers from general UPnP devices and resolves
serial prefixes, falling back to the `DevModel` SSDP header when the prefix
is unrecognized (see [`resolve_model`](../../models/index.md#resolve-model)).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`SsdpDevice`](#ssdpdevice) | struct | Normalized device details extracted directly from SSDP UDP datagram payloads. |
| [`parse_ssdp_payload`](#parse-ssdp-payload) | fn | Parse an incoming raw UDP datagram buffer into normalized printer credentials. |

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

- <span id="ssdpdevice-clone"></span>`fn clone(&self) -> SsdpDevice` — [`SsdpDevice`](#ssdpdevice)

##### `impl Debug for SsdpDevice`

- <span id="ssdpdevice-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for SsdpDevice`

##### `impl PartialEq for SsdpDevice`

- <span id="ssdpdevice-partialeq-eq"></span>`fn eq(&self, other: &SsdpDevice) -> bool` — [`SsdpDevice`](#ssdpdevice)


---

## Functions

### `parse_ssdp_payload`

```rust
fn parse_ssdp_payload(buf: &[u8]) -> Option<SsdpDevice>
```

**Types:** [`SsdpDevice`](#ssdpdevice)

Parse an incoming raw UDP datagram buffer into normalized printer credentials.

Under the SSDP specification, responses map to standard HTTP responses, while
advertisements map to HTTP requests. This parser automatically evaluates the envelope
and routes the payload buffer to the appropriate parsing schema of `httparse`.

