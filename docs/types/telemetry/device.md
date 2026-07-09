**bambino > types > telemetry > device**

# Module: types::telemetry::device

## Contents

**Structs**

- [`AirductCollection`](#airductcollection) - Climate parts collection nested within `device` parameters.
- [`AirductModeListEntry`](#airductmodelistentry) - Entry in the airduct mode availability list reported by the printer.
- [`AirductPart`](#airductpart) - Represents an individual auxiliary routing component.
- [`BedInfo`](#bedinfo) - Bed info segment with composite-packed temperature.
- [`BedTelemetry`](#bedtelemetry) - Bed telemetry sub-object from `device.bed` on new-protocol printers.
- [`DeviceTelemetry`](#devicetelemetry) - Device hardware state properties containing physical tooling descriptions.
- [`ExtToolTelemetry`](#exttooltelemetry) - Laser/cutter external tool telemetry from `device.ext_tool`.
- [`ExtruderCollection`](#extrudercollection) - IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].
- [`ExtruderInfo`](#extruderinfo) - Per-extruder thermal and routing state for IDEX platforms.
- [`NozzleCollection`](#nozzlecollection) - Wrap block holding nozzle characteristics.
- [`NozzleInfo`](#nozzleinfo) - Dynamic extruder nozzle details.

---

## bambino::types::telemetry::device::AirductCollection

*Struct*

Climate parts collection nested within `device` parameters.

**Fields:**
- `parts: Vec<AirductPart>` - Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].
- `mode_cur: Option<i32>` - Currently active airduct damper mode (0=cooling, 1=heating, 2=laser).
- `mode_list: Vec<AirductModeListEntry>` - List of airduct modes available on this model.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AirductCollection`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::AirductModeListEntry

*Struct*

Entry in the airduct mode availability list reported by the printer.

**Fields:**
- `mode_id: i32` - Mode identifier (0=cooling, 1=heating, 2=laser).

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AirductModeListEntry`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::AirductPart

*Struct*

Represents an individual auxiliary routing component.

**Fields:**
- `id: u32` - Part index matching hardware configurations (e.g., `160` for the right auxiliary fan).
- `state: Option<i32>` - The active operating speed percentage ($0$ to $100$) or damper direction flag.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AirductPart`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::BedInfo

*Struct*

Bed info segment with composite-packed temperature.

**Fields:**
- `temp: Option<u32>` - Composite-packed bed temperature [REF-THER-DECODE].

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> BedInfo`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::BedTelemetry

*Struct*

Bed telemetry sub-object from `device.bed` on new-protocol printers.

**Fields:**
- `info: Option<BedInfo>` - Bed info containing composite-packed temperature.
- `state: Option<u32>` - Bed heating state (2 = heating).

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> BedTelemetry`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::DeviceTelemetry

*Struct*

Device hardware state properties containing physical tooling descriptions.

Appears at two locations on the wire:
- Top-level `{"device": {...}}` for incremental updates (e.g., `push_alt_nozzle_info`)
- Nested inside `{"print": {"device": {...}}}` for pushall on H2/P2/X2 models

**Fields:**
- `nozzle: Option<NozzleCollection>` - Structured descriptions representing the active extruder assembly properties.
- `extruder: Option<ExtruderCollection>` - Per-extruder thermal and routing state for IDEX platforms [REF-THER-DECODE §Dual-Extruder].
- `airduct: Option<AirductCollection>` - Nested structures tracking cooling components and climate routing [REF-CLIM-FANS].
- `ctc: Option<super::diagnostics::CtcTelemetry>` - Chamber Temperature Controller telemetry [REF-THER-DECODE].
- `bed: Option<BedTelemetry>` - Composite-packed bed temperature on H2/P2/X2 models.
- `ext_tool: Option<ExtToolTelemetry>` - Laser/cutter tool mount state.
- `fire_ext: Option<serde_json::Value>` - Fire alarm/extinguisher status (H2D Pro, H2S).
- `bed_temp: Option<u32>` - Alternative top-level bed temperature in device envelope.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> DeviceTelemetry`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::ExtToolTelemetry

*Struct*

Laser/cutter external tool telemetry from `device.ext_tool`.

**Fields:**
- `mount: Option<i32>` - Mount state (0 = not mounted, 1 = mounted).
- `tool_type: Option<String>` - Tool type code (e.g. `"LB00"` = 10W laser, `"LB01"` = 40W laser, `"CP00"` = cutter).
- `calib: Option<i32>` - Calibration state.
- `low_prec: Option<bool>` - Low-precision mode flag.
- `th_temp: Option<i32>` - Thermal head temperature.
- `mount_3d: Option<i32>` - 3D mount state.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ExtToolTelemetry`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::ExtruderCollection

*Struct*

IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].

**Fields:**
- `info: Vec<ExtruderInfo>` - Per-extruder thermal and routing entries (id 0 = right/main, id 1 = left/deputy).
- `state: Option<u32>` - Bitmask: low 4 bits = extruder count, bits 4–7 = active extruder index.

**Methods:**

- `fn active_extruder_index(self: &Self) -> u8` - Returns the active extruder index extracted from the `state` bitmask.
- `fn extruder_count(self: &Self) -> u8` - Returns the extruder count extracted from the `state` bitmask.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ExtruderCollection`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::ExtruderInfo

*Struct*

Per-extruder thermal and routing state for IDEX platforms.

The `temp` field uses the same composite packing as `chamber_temper`:
values > 500 encode `(target << 16) | actual`, values <= 500 are direct actual temps.

**Fields:**
- `id: u8` - Extruder carriage index (0 = right/main, 1 = left/deputy).
- `temp: Option<u32>` - Composite-packed temperature (use `unpack_temperature()` to decode).
- `snow: Option<u32>` - Current AMS slot routing (low 4 bits = tray index, upper bits = AMS unit index).
- `spre: Option<u32>` - Previous AMS slot routing.
- `star: Option<u32>` - Target AMS slot routing.
- `hnow: Option<u8>` - Current head routing index.
- `hpre: Option<u8>` - Previous head routing index.
- `htar: Option<u8>` - Target head routing index.
- `stat: Option<u32>` - Status bitmask.
- `info: Option<u32>` - Info bitmask.
- `filam_bak: Vec<u32>` - Filament backup slot indices.
- `z_bias: Option<f64>` - Z-axis offset compensation (X2D).

**Methods:**

- `fn temperatures(self: &Self) -> (u16, u16)` - Unpacks the composite temperature into (actual, target) degrees Celsius.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ExtruderInfo`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::NozzleCollection

*Struct*

Wrap block holding nozzle characteristics.

**Fields:**
- `info: Vec<NozzleInfo>` - Polymorphic array representing active carriages and tool configurations.
- `exist: Option<u32>` - Bitmask of physically present nozzle IDs (HotendRack).
- `state: Option<u32>` - Nozzle state bitmask.
- `src_id: Option<u32>` - Tool-change source nozzle ID.
- `tar_id: Option<u32>` - Tool-change target nozzle ID.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> NozzleCollection`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::types::telemetry::device::NozzleInfo

*Struct*

Dynamic extruder nozzle details.

Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
(IDEX platforms) to provide unified schema matching.

**Fields:**
- `id: u8` - Extruder carriage index (0 = Right/Main, 1 = Left/Deputy) or storage rack index.
- `diameter: Option<f32>` - Nozzle orifice diameter in millimeters (e.g. 0.4).
- `tm: Option<u32>` - Target maximum temperature (Standard Platform abbreviated representation).
- `max_temp: Option<u32>` - Target maximum temperature (IDEX Platform verbose representation).
- `nozzle_type: Option<String>` - Core physical nozzle composition or tool type designation.
- `wear: Option<u32>` - Normalized physical wear tracker value.
- `serial_number: Option<String>` - Hotend manufacturer serial number (verbose IDEX platform representation).
- `sn: Option<String>` - Hotend manufacturer serial number (standard platform abbreviated representation).
- `filament_colour: Option<String>` - Physical filament color hex code loaded into the extruder.
- `color_m: Option<String>` - Abbreviated filament color hex code.
- `filament_id: Option<String>` - Filament preset calibration index.
- `fila_id: Option<String>` - Abbreviated filament preset calibration index.
- `stat: Option<u32>` - Nozzle status bitmask.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> NozzleInfo`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



