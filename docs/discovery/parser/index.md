*[bambino](../../index.md) / [discovery](../index.md) / [parser](index.md)*

---

# Module `parser`

# HTTP-style SSDP Parsing

Parses HTTP-like headers from multicast and unicast UDP frames on port 2021. Header
slicing is zero-copy and a non-Bambu packet is rejected without allocating; an accepted
packet allocates the owned [`SsdpDevice`](#ssdpdevice) it returns.
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
    pub model: crate::models::PrinterModel,
    pub name: String,
    pub ip: core::net::IpAddr,
    pub location_port: u16,
    pub discovery_port: Option<u16>,
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

- **`model`**: `crate::models::PrinterModel`

  Resolved printer capability profile based on prefixes and headers.

- **`name`**: `String`

  Human-friendly printer name defined by the user.

- **`ip`**: `core::net::IpAddr`

  Printer IP address from the LOCATION header. A packet whose LOCATION host isn't an IP
  literal is rejected, so this is always safe to dial or interpolate into a URL.

- **`location_port`**: `u16`

  Port of the LOCATION URI (80 when absent). This is an inert HTTP endpoint, **not** the
  MQTT, FTPS or camera port — see [REF-NET-DISC] Protocol Violation #2.

- **`discovery_port`**: `Option<u16>`

  SSDP port on which the device was discovered (2021 or 1990), or `None` if unknown.
  
  The port is not carried in the payload, so [`parse_ssdp_payload`](#parse-ssdp-payload) — which sees only the
  datagram bytes — always leaves this `None`. It is filled in by
  [`DiscoveryEngine::poll_next_device`](../index.md#discoveryengine),
  which knows which socket the datagram arrived on.

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

#### Implementations

- <span id="ssdpdevice-into-identity"></span>`fn into_identity(self, access_code: impl Into<String>) -> crate::identity::PrinterIdentity` — [`PrinterIdentity`](../../identity/index.md#printeridentity)

  The identity to connect to this printer with, keeping the model discovery resolved.

  Unlike `PrinterIdentity::new(dev.ip, dev.serial, ..)`, which re-resolves the model from
  the serial alone, this keeps [`model`](#ssdpdevice) — resolved from the serial *and* the
  `DevModel`/NT/ST headers — so a printer with an unrecognized serial prefix doesn't fall
  back to the conservative `Unknown` quirks.

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

