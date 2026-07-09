*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [device](index.md)*

---

# Module `device`

Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).

## Contents

- [Types](#types)
  - [`AirductCollection`](#airductcollection)
  - [`AirductModeListEntry`](#airductmodelistentry)
  - [`AirductPart`](#airductpart)
  - [`BedInfo`](#bedinfo)
  - [`BedTelemetry`](#bedtelemetry)
  - [`DeviceTelemetry`](#devicetelemetry)
  - [`ExtToolTelemetry`](#exttooltelemetry)
  - [`ExtruderCollection`](#extrudercollection)
  - [`ExtruderInfo`](#extruderinfo)
  - [`NozzleCollection`](#nozzlecollection)
  - [`NozzleInfo`](#nozzleinfo)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AirductCollection`](#airductcollection) | struct | Climate parts collection nested within `device` parameters. |
| [`AirductModeListEntry`](#airductmodelistentry) | struct | Entry in the airduct mode availability list reported by the printer. |
| [`AirductPart`](#airductpart) | struct | Represents an individual auxiliary routing component. |
| [`BedInfo`](#bedinfo) | struct | Bed info segment with composite-packed temperature. |
| [`BedTelemetry`](#bedtelemetry) | struct | Bed telemetry sub-object from `device.bed` on new-protocol printers. |
| [`DeviceTelemetry`](#devicetelemetry) | struct | Device hardware state properties containing physical tooling descriptions. |
| [`ExtToolTelemetry`](#exttooltelemetry) | struct | Laser/cutter external tool telemetry from `device.ext_tool`. |
| [`ExtruderCollection`](#extrudercollection) | struct | IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder]. |
| [`ExtruderInfo`](#extruderinfo) | struct | Per-extruder thermal and routing state for IDEX platforms. |
| [`NozzleCollection`](#nozzlecollection) | struct | Wrap block holding nozzle characteristics. |
| [`NozzleInfo`](#nozzleinfo) | struct | Dynamic extruder nozzle details. |

## Types

### `AirductCollection`

```rust
struct AirductCollection {
    pub parts: Vec<AirductPart>,
    pub mode_cur: Option<i32>,
    pub mode_list: Vec<AirductModeListEntry>,
}
```

Climate parts collection nested within `device` parameters.

#### Fields

- **`parts`**: `Vec<AirductPart>`

  Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].

- **`mode_cur`**: `Option<i32>`

  Currently active airduct damper mode (0=cooling, 1=heating, 2=laser).

- **`mode_list`**: `Vec<AirductModeListEntry>`

  List of airduct modes available on this model.

#### Trait Implementations

##### `impl Clone for AirductCollection`

- <span id="airductcollection-clone"></span>`fn clone(&self) -> AirductCollection` — [`AirductCollection`](#airductcollection)

##### `impl Debug for AirductCollection`

- <span id="airductcollection-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AirductCollection`

- <span id="airductcollection-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AirductCollection`

##### `impl Serialize for AirductCollection`

- <span id="airductcollection-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirductModeListEntry`

```rust
struct AirductModeListEntry {
    pub mode_id: i32,
}
```

Entry in the airduct mode availability list reported by the printer.

#### Fields

- **`mode_id`**: `i32`

  Mode identifier (0=cooling, 1=heating, 2=laser).

#### Trait Implementations

##### `impl Clone for AirductModeListEntry`

- <span id="airductmodelistentry-clone"></span>`fn clone(&self) -> AirductModeListEntry` — [`AirductModeListEntry`](#airductmodelistentry)

##### `impl Debug for AirductModeListEntry`

- <span id="airductmodelistentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AirductModeListEntry`

- <span id="airductmodelistentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AirductModeListEntry`

##### `impl Serialize for AirductModeListEntry`

- <span id="airductmodelistentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirductPart`

```rust
struct AirductPart {
    pub id: u32,
    pub state: Option<i32>,
}
```

Represents an individual auxiliary routing component.

#### Fields

- **`id`**: `u32`

  Part index matching hardware configurations (e.g., `160` for the right auxiliary fan).

- **`state`**: `Option<i32>`

  The active operating speed percentage ($0$ to $100$) or damper direction flag.

#### Trait Implementations

##### `impl Clone for AirductPart`

- <span id="airductpart-clone"></span>`fn clone(&self) -> AirductPart` — [`AirductPart`](#airductpart)

##### `impl Debug for AirductPart`

- <span id="airductpart-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AirductPart`

- <span id="airductpart-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AirductPart`

##### `impl Serialize for AirductPart`

- <span id="airductpart-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BedInfo`

```rust
struct BedInfo {
    pub temp: Option<u32>,
}
```

Bed info segment with composite-packed temperature.

#### Fields

- **`temp`**: `Option<u32>`

  Composite-packed bed temperature [REF-THER-DECODE].

#### Trait Implementations

##### `impl Clone for BedInfo`

- <span id="bedinfo-clone"></span>`fn clone(&self) -> BedInfo` — [`BedInfo`](#bedinfo)

##### `impl Debug for BedInfo`

- <span id="bedinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for BedInfo`

- <span id="bedinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for BedInfo`

##### `impl Serialize for BedInfo`

- <span id="bedinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BedTelemetry`

```rust
struct BedTelemetry {
    pub info: Option<BedInfo>,
    pub state: Option<u32>,
}
```

Bed telemetry sub-object from `device.bed` on new-protocol printers.

#### Fields

- **`info`**: `Option<BedInfo>`

  Bed info containing composite-packed temperature.

- **`state`**: `Option<u32>`

  Bed heating state (2 = heating).

#### Trait Implementations

##### `impl Clone for BedTelemetry`

- <span id="bedtelemetry-clone"></span>`fn clone(&self) -> BedTelemetry` — [`BedTelemetry`](#bedtelemetry)

##### `impl Debug for BedTelemetry`

- <span id="bedtelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for BedTelemetry`

- <span id="bedtelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for BedTelemetry`

##### `impl Serialize for BedTelemetry`

- <span id="bedtelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `DeviceTelemetry`

```rust
struct DeviceTelemetry {
    pub nozzle: Option<NozzleCollection>,
    pub extruder: Option<ExtruderCollection>,
    pub airduct: Option<AirductCollection>,
    pub ctc: Option<super::diagnostics::CtcTelemetry>,
    pub bed: Option<BedTelemetry>,
    pub ext_tool: Option<ExtToolTelemetry>,
    pub fire_ext: Option<serde_json::Value>,
    pub bed_temp: Option<u32>,
}
```

Device hardware state properties containing physical tooling descriptions.

Appears at two locations on the wire:
- Top-level `{"device": {...}}` for incremental updates (e.g., `push_alt_nozzle_info`)
- Nested inside `{"print": {"device": {...}}}` for pushall on H2/P2/X2 models

#### Fields

- **`nozzle`**: `Option<NozzleCollection>`

  Structured descriptions representing the active extruder assembly properties.

- **`extruder`**: `Option<ExtruderCollection>`

  Per-extruder thermal and routing state for IDEX platforms [REF-THER-DECODE §Dual-Extruder].

- **`airduct`**: `Option<AirductCollection>`

  Nested structures tracking cooling components and climate routing [REF-CLIM-FANS].

- **`ctc`**: `Option<super::diagnostics::CtcTelemetry>`

  Chamber Temperature Controller telemetry [REF-THER-DECODE].

- **`bed`**: `Option<BedTelemetry>`

  Composite-packed bed temperature on H2/P2/X2 models.

- **`ext_tool`**: `Option<ExtToolTelemetry>`

  Laser/cutter tool mount state.

- **`fire_ext`**: `Option<serde_json::Value>`

  Fire alarm/extinguisher status (H2D Pro, H2S).

- **`bed_temp`**: `Option<u32>`

  Alternative top-level bed temperature in device envelope.

#### Trait Implementations

##### `impl Clone for DeviceTelemetry`

- <span id="devicetelemetry-clone"></span>`fn clone(&self) -> DeviceTelemetry` — [`DeviceTelemetry`](#devicetelemetry)

##### `impl Debug for DeviceTelemetry`

- <span id="devicetelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for DeviceTelemetry`

- <span id="devicetelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for DeviceTelemetry`

##### `impl Serialize for DeviceTelemetry`

- <span id="devicetelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtToolTelemetry`

```rust
struct ExtToolTelemetry {
    pub mount: Option<i32>,
    pub tool_type: Option<String>,
    pub calib: Option<i32>,
    pub low_prec: Option<bool>,
    pub th_temp: Option<i32>,
    pub mount_3d: Option<i32>,
}
```

Laser/cutter external tool telemetry from `device.ext_tool`.

#### Fields

- **`mount`**: `Option<i32>`

  Mount state (0 = not mounted, 1 = mounted).

- **`tool_type`**: `Option<String>`

  Tool type code (e.g. `"LB00"` = 10W laser, `"LB01"` = 40W laser, `"CP00"` = cutter).

- **`calib`**: `Option<i32>`

  Calibration state.

- **`low_prec`**: `Option<bool>`

  Low-precision mode flag.

- **`th_temp`**: `Option<i32>`

  Thermal head temperature.

- **`mount_3d`**: `Option<i32>`

  3D mount state.

#### Trait Implementations

##### `impl Clone for ExtToolTelemetry`

- <span id="exttooltelemetry-clone"></span>`fn clone(&self) -> ExtToolTelemetry` — [`ExtToolTelemetry`](#exttooltelemetry)

##### `impl Debug for ExtToolTelemetry`

- <span id="exttooltelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtToolTelemetry`

- <span id="exttooltelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtToolTelemetry`

##### `impl Serialize for ExtToolTelemetry`

- <span id="exttooltelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtruderCollection`

```rust
struct ExtruderCollection {
    pub info: Vec<ExtruderInfo>,
    pub state: Option<u32>,
}
```

IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].

#### Fields

- **`info`**: `Vec<ExtruderInfo>`

  Per-extruder thermal and routing entries (id 0 = right/main, id 1 = left/deputy).

- **`state`**: `Option<u32>`

  Bitmask: low 4 bits = extruder count, bits 4–7 = active extruder index.

#### Implementations

- <span id="extrudercollection-active-extruder-index"></span>`fn active_extruder_index(&self) -> u8`

  Returns the active extruder index extracted from the `state` bitmask.

- <span id="extrudercollection-extruder-count"></span>`fn extruder_count(&self) -> u8`

  Returns the extruder count extracted from the `state` bitmask.

#### Trait Implementations

##### `impl Clone for ExtruderCollection`

- <span id="extrudercollection-clone"></span>`fn clone(&self) -> ExtruderCollection` — [`ExtruderCollection`](#extrudercollection)

##### `impl Debug for ExtruderCollection`

- <span id="extrudercollection-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtruderCollection`

- <span id="extrudercollection-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtruderCollection`

##### `impl Serialize for ExtruderCollection`

- <span id="extrudercollection-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtruderInfo`

```rust
struct ExtruderInfo {
    pub id: u8,
    pub temp: Option<u32>,
    pub snow: Option<u32>,
    pub spre: Option<u32>,
    pub star: Option<u32>,
    pub hnow: Option<u8>,
    pub hpre: Option<u8>,
    pub htar: Option<u8>,
    pub stat: Option<u32>,
    pub info: Option<u32>,
    pub filam_bak: Vec<u32>,
    pub z_bias: Option<f64>,
}
```

Per-extruder thermal and routing state for IDEX platforms.

The `temp` field uses the same composite packing as `chamber_temper`:
values > 500 encode `(target << 16) | actual`, values <= 500 are direct actual temps.

#### Fields

- **`id`**: `u8`

  Extruder carriage index (0 = right/main, 1 = left/deputy).

- **`temp`**: `Option<u32>`

  Composite-packed temperature (use `unpack_temperature()` to decode).

- **`snow`**: `Option<u32>`

  Current AMS slot routing (low 4 bits = tray index, upper bits = AMS unit index).

- **`spre`**: `Option<u32>`

  Previous AMS slot routing.

- **`star`**: `Option<u32>`

  Target AMS slot routing.

- **`hnow`**: `Option<u8>`

  Current head routing index.

- **`hpre`**: `Option<u8>`

  Previous head routing index.

- **`htar`**: `Option<u8>`

  Target head routing index.

- **`stat`**: `Option<u32>`

  Status bitmask.

- **`info`**: `Option<u32>`

  Info bitmask.

- **`filam_bak`**: `Vec<u32>`

  Filament backup slot indices.

- **`z_bias`**: `Option<f64>`

  Z-axis offset compensation (X2D).

#### Implementations

- <span id="extruderinfo-temperatures"></span>`fn temperatures(&self) -> (u16, u16)`

  Unpacks the composite temperature into (actual, target) degrees Celsius.

#### Trait Implementations

##### `impl Clone for ExtruderInfo`

- <span id="extruderinfo-clone"></span>`fn clone(&self) -> ExtruderInfo` — [`ExtruderInfo`](#extruderinfo)

##### `impl Debug for ExtruderInfo`

- <span id="extruderinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtruderInfo`

- <span id="extruderinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtruderInfo`

##### `impl Serialize for ExtruderInfo`

- <span id="extruderinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `NozzleCollection`

```rust
struct NozzleCollection {
    pub info: Vec<NozzleInfo>,
    pub exist: Option<u32>,
    pub state: Option<u32>,
    pub src_id: Option<u32>,
    pub tar_id: Option<u32>,
}
```

Wrap block holding nozzle characteristics.

#### Fields

- **`info`**: `Vec<NozzleInfo>`

  Polymorphic array representing active carriages and tool configurations.

- **`exist`**: `Option<u32>`

  Bitmask of physically present nozzle IDs (HotendRack).

- **`state`**: `Option<u32>`

  Nozzle state bitmask.

- **`src_id`**: `Option<u32>`

  Tool-change source nozzle ID.

- **`tar_id`**: `Option<u32>`

  Tool-change target nozzle ID.

#### Trait Implementations

##### `impl Clone for NozzleCollection`

- <span id="nozzlecollection-clone"></span>`fn clone(&self) -> NozzleCollection` — [`NozzleCollection`](#nozzlecollection)

##### `impl Debug for NozzleCollection`

- <span id="nozzlecollection-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NozzleCollection`

- <span id="nozzlecollection-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NozzleCollection`

##### `impl Serialize for NozzleCollection`

- <span id="nozzlecollection-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `NozzleInfo`

```rust
struct NozzleInfo {
    pub id: u8,
    pub diameter: Option<f32>,
    pub tm: Option<u32>,
    pub max_temp: Option<u32>,
    pub nozzle_type: Option<String>,
    pub wear: Option<u32>,
    pub serial_number: Option<String>,
    pub sn: Option<String>,
    pub filament_colour: Option<String>,
    pub color_m: Option<String>,
    pub filament_id: Option<String>,
    pub fila_id: Option<String>,
    pub stat: Option<u32>,
}
```

Dynamic extruder nozzle details.

Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
(IDEX platforms) to provide unified schema matching.

#### Fields

- **`id`**: `u8`

  Extruder carriage index (0 = Right/Main, 1 = Left/Deputy) or storage rack index.

- **`diameter`**: `Option<f32>`

  Nozzle orifice diameter in millimeters (e.g. 0.4).

- **`tm`**: `Option<u32>`

  Target maximum temperature (Standard Platform abbreviated representation).

- **`max_temp`**: `Option<u32>`

  Target maximum temperature (IDEX Platform verbose representation).

- **`nozzle_type`**: `Option<String>`

  Core physical nozzle composition or tool type designation.

- **`wear`**: `Option<u32>`

  Normalized physical wear tracker value.

- **`serial_number`**: `Option<String>`

  Hotend manufacturer serial number (verbose IDEX platform representation).

- **`sn`**: `Option<String>`

  Hotend manufacturer serial number (standard platform abbreviated representation).

- **`filament_colour`**: `Option<String>`

  Physical filament color hex code loaded into the extruder.

- **`color_m`**: `Option<String>`

  Abbreviated filament color hex code.

- **`filament_id`**: `Option<String>`

  Filament preset calibration index.

- **`fila_id`**: `Option<String>`

  Abbreviated filament preset calibration index.

- **`stat`**: `Option<u32>`

  Nozzle status bitmask.

#### Trait Implementations

##### `impl Clone for NozzleInfo`

- <span id="nozzleinfo-clone"></span>`fn clone(&self) -> NozzleInfo` — [`NozzleInfo`](#nozzleinfo)

##### `impl Debug for NozzleInfo`

- <span id="nozzleinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NozzleInfo`

- <span id="nozzleinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NozzleInfo`

##### `impl Serialize for NozzleInfo`

- <span id="nozzleinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

