**bambino > types > telemetry**

# Module: types::telemetry

## Contents

**Modules**

- [`ams`](#ams) - AMS telemetry types (tray slots, units, dry settings, virtual trays).
- [`device`](#device) - Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).
- [`diagnostics`](#diagnostics) - Diagnostic telemetry types (HMS alerts, light reports).
- [`report`](#report) - Top-level telemetry report envelope (`print` and `device` wire locations).

**Structs**

- [`TelemetryReport`](#telemetryreport) - Unified top-level telemetry report received from the printer's local MQTT broker.

**Functions**

- [`decode_nozzle_temperatures`](#decode_nozzle_temperatures) - Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`] — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived.
- [`is_developer_mode`](#is_developer_mode) - Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].

---

## bambino::types::telemetry::TelemetryReport

*Struct*

Unified top-level telemetry report received from the printer's local MQTT broker.

Under the over-the-wire schema, updates are typically nested within separate
top-level domains depending on which micro-system published the frame.

**Fields:**
- `print: Option<PrinterTelemetry>` - Telemetry parameters representing the physical printer state machine.
- `device: Option<DeviceTelemetry>` - Network and hardware board capability descriptors.
- `fun: Option<String>` - Developer LAN Mode bitmask field (hex string).

**Methods:**

- `fn bed_temperatures(self: &Self) -> (u16, u16)` - Returns the bed's (actual, target) temperatures in °C.
- `fn device(self: &Self) -> Option<&DeviceTelemetry>` - Returns the `DeviceTelemetry` sub-object, checking both wire locations it can arrive at.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> TelemetryReport`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## Module: ams

AMS telemetry types (tray slots, units, dry settings, virtual trays).



## bambino::types::telemetry::decode_nozzle_temperatures

*Function*

Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`] — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived.

Returns one `(id, actual, target)` tuple per nozzle. Prefers `device.extruder.info`
(composite-packed per-nozzle temperatures, decoded via [`ExtruderInfo::temperatures()`]).
Falls back to the flat `nozzle_temper`/`nozzle_target_temper` fields when absent: a single
entry `(0, actual, target)` for a single-nozzle model, or — for a dual-nozzle (IDEX) model
with no live extruder temps yet — the wire's undocumented routing quirk: `nozzle_temper` is
nozzle 1 (left)'s actual reading and `nozzle_target_temper` is nozzle 0 (right)'s target,
each nozzle only getting half of its own reading from the flat fields.

```rust
fn decode_nozzle_temperatures(device: Option<&DeviceTelemetry>, nozzle_temper: Option<f64>, nozzle_target_temper: Option<f64>) -> Vec<(u8, u16, u16)>
```



## Module: device

Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).



## Module: diagnostics

Diagnostic telemetry types (HMS alerts, light reports).



## bambino::types::telemetry::is_developer_mode

*Function*

Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].

Returns `Some(true)` when developer mode is enabled (MQTT signature NOT required),
`Some(false)` when disabled, or `None` if the hex string is unparseable.
The `fun` field is a variable-length hex string (up to 64 bits). Bit 29
(`0x20000000`) is the `MQTT_SIGNATURE_REQUIRED` flag — when clear, developer mode is on.

```rust
fn is_developer_mode(fun_hex: &str) -> Option<bool>
```



## Module: report

Top-level telemetry report envelope (`print` and `device` wire locations).



