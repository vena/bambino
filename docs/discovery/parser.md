**bambino > discovery > parser**

# Module: discovery::parser

## Contents

**Structs**

- [`SsdpDevice`](#ssdpdevice) - Normalized device details extracted directly from SSDP UDP datagram payloads.

**Functions**

- [`parse_ssdp_payload`](#parse_ssdp_payload) - Parse an incoming raw UDP datagram buffer into normalized printer credentials.

---

## bambino::discovery::parser::SsdpDevice

*Struct*

Normalized device details extracted directly from SSDP UDP datagram payloads.

**Fields:**
- `serial: String` - The unique uppercase physical hardware serial number.
- `model: crate::models::BambuModel` - Resolved printer capability profile based on prefixes and headers.
- `name: String` - Human-friendly printer name defined by the user.
- `ip: String` - Direct network target IP address extracted from the LOCATION header.
- `port: u16` - Discovery communications port parsed from the LOCATION header.
- `discovery_port: u16` - SSDP port on which the device was discovered (2021 or 1990).
- `version: String` - Device firmware target version.
- `connect_type: String` - Network connection medium (e.g. "lan", "wlan").
- `raw_model_str: String` - Unmodified hardware identifier returned by the network card.
- `signal_dbm: Option<i32>` - WiFi signal strength in dBm (e.g. -43), if reported by the device.
- `bind_state: String` - Cloud binding state (e.g. "bound", "free").
- `security_link: String` - Security link state (e.g. "secure").

**Traits:** Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> SsdpDevice`
- **PartialEq**
  - `fn eq(self: &Self, other: &SsdpDevice) -> bool`



## bambino::discovery::parser::parse_ssdp_payload

*Function*

Parse an incoming raw UDP datagram buffer into normalized printer credentials.

Under the SSDP specification, responses map to standard HTTP responses, while
advertisements map to HTTP requests. This parser automatically evaluates the envelope
and routes the payload buffer to the appropriate parsing schema of `httparse`.

```rust
fn parse_ssdp_payload(buf: &[u8]) -> Option<SsdpDevice>
```



