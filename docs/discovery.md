**bambino > discovery**

# Module: discovery

## Contents

**Modules**

- [`parser`](#parser) - # Zero-Copy HTTP-style SSDP Parsing Engine

**Structs**

- [`DiscoveryEngine`](#discoveryengine) - Asynchronous Discovery Engine providing search orchestration and passive monitoring.

**Functions**

- [`discover_devices`](#discover_devices) - Broadcasts SSDP search queries and listens for printer responses for the given duration.

**Constants**

- [`MULTICAST_IP`](#multicast_ip) - Standard Bambu Lab multicast group target for SSDP operations.
- [`SSDP_PORT`](#ssdp_port) - Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].
- [`SSDP_PORT_ALT`](#ssdp_port_alt) - Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS].

---

## bambino::discovery::DiscoveryEngine

*Struct*

Asynchronous Discovery Engine providing search orchestration and passive monitoring.

**Generic Parameters:**
- U

**Methods:**

- `fn new(socket: U, port: u16) -> Self` - Creates a new Discovery Engine bound to a specific SSDP port.
- `fn broadcast_search(self: &Self) -> Result<(), BambuError>` - Dispatches active search queries to trigger local printer reports.
- `fn poll_next_device(self: &Self, buf: & mut [u8]) -> Result<Option<SsdpDevice>, BambuError>` - Listens on the bound socket interface and processes the next incoming SSDP packet.



## bambino::discovery::MULTICAST_IP

*Constant*: `&str`

Standard Bambu Lab multicast group target for SSDP operations.



## bambino::discovery::SSDP_PORT

*Constant*: `u16`

Primary UDP port allocated to physical Bambu Lab printer local services [REF-NET-PORTS].



## bambino::discovery::SSDP_PORT_ALT

*Constant*: `u16`

Alternative SSDP port listed in Bambu Lab documentation [REF-NET-PORTS].



## bambino::discovery::discover_devices

*Function*

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

```rust
fn discover_devices<U, T>(timeout: core::time::Duration, timer: &T) -> Result<Vec<SsdpDevice>, crate::error::BambuError>
```



## Module: parser

# Zero-Copy HTTP-style SSDP Parsing Engine

Provides utilities to parse HTTP-like headers from multicast and unicast
UDP frames on Port 2021 without performing runtime memory allocations.
Differentiates Bambu Lab printers from general UPnP devices, resolves
serial prefixes, and bypasses the H2S/H2D collision hazard.



