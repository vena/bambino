*[bambino](../../index.md) / [mqtt](../index.md) / [commands](index.md)*

---

# Module `commands`

# MQTT Command Payloads & Serialization Builders

Provides the concrete data structures and serialization wrappers required to control
physical Bambu Lab printers over MQTTS Port 8883 [REF-MQTT-LIFECYCLE].

Handles complex polymorphic rules such as the string-vs-array mapping schemas for the
`ams_mapping` parameter, and enforces safety bounds on task identities.

## Architectural Alignment
* **Polymorphic Mapping Rules [REF-MQTT-LIFECYCLE]:** Handles conditional typing for
  material mappings, where inactive AMS sessions must present as empty strings while active
  sessions require integer arrays.
* **Task-ID Overflow Prevention [REF-MQTT-ENV]:** Clamps all generated sequence identifiers
  to 32-bit signed integer limits to prevent memory allocation overflows on hardware boards.

## Contents

- [Modules](#modules)
  - [`ams`](ams/index.md)
  - [`control`](control/index.md)
  - [`gcode`](gcode/index.md)
  - [`hardware`](hardware/index.md)
  - [`print_job`](print_job/index.md)
  - [`status`](status/index.md)
- [Types](#types)
  - [`ClampedTaskId`](#clampedtaskid)
  - [`Info`](#info)
  - [`Print`](#print)
  - [`Pushing`](#pushing)
  - [`System`](#system)
- [Functions](#functions)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`ams`](ams/index.md) | mod | AMS-related MQTT command payloads (filament change, drying, RFID scan, settings). |
| [`control`](control/index.md) | mod | Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration). |
| [`gcode`](gcode/index.md) | mod | G-code dispatch command payload. |
| [`hardware`](hardware/index.md) | mod | Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound). |
| [`print_job`](print_job/index.md) | mod | Print job dispatch (file selection, AMS material mapping, plate/timelapse config). |
| [`status`](status/index.md) | mod | Status query commands (pushall, get_version, get_access_code). |
| [`ClampedTaskId`](#clampedtaskid) | struct | A task or sequence id already reduced into the range firmware accepts (below `i32::MAX`). |
| [`Info`](#info) | struct | The `info` namespace envelope a command payload is published in. |
| [`Print`](#print) | struct | The `print` namespace envelope a command payload is published in. |
| [`Pushing`](#pushing) | struct | The `pushing` namespace envelope a command payload is published in. |
| [`System`](#system) | struct | The `system` namespace envelope a command payload is published in. |

## Modules

- [`ams`](ams/index.md) — AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).
- [`control`](control/index.md) — Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).
- [`gcode`](gcode/index.md) — G-code dispatch command payload.
- [`hardware`](hardware/index.md) — Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).
- [`print_job`](print_job/index.md) — Print job dispatch (file selection, AMS material mapping, plate/timelapse config).
- [`status`](status/index.md) — Status query commands (pushall, get_version, get_access_code).


---

## Types

### `ChangeTemps`

```rust
struct ChangeTemps {
    pub current: i32,
    pub target: i32,
}
```

The nozzle temperatures an `ams_change_filament` carries, °C; `-1` lets the firmware decide.

#### Fields

- **`current`**: `i32`

  Current nozzle temperature (`curr_temp`).

- **`target`**: `i32`

  Target nozzle temperature (`tar_temp`).

#### Implementations

- <span id="changetemps-const-firmware"></span>`const FIRMWARE: Self`

#### Trait Implementations

##### `impl Clone for ChangeTemps`

- <span id="changetemps-clone"></span>`fn clone(&self) -> ChangeTemps` — [`ChangeTemps`](ams/index.md#changetemps)

##### `impl Copy for ChangeTemps`

##### `impl Debug for ChangeTemps`

- <span id="changetemps-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for ChangeTemps`

##### `impl Hash for ChangeTemps`

- <span id="changetemps-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for ChangeTemps`

- <span id="changetemps-partialeq-eq"></span>`fn eq(&self, other: &ChangeTemps) -> bool` — [`ChangeTemps`](ams/index.md#changetemps)

### `DryingParams`

```rust
struct DryingParams {
    pub filament: String,
    pub temp: u32,
    pub duration_hours: u32,
    pub humidity: u32,
    pub rotate_tray: bool,
    pub cooling_temp: u32,
    pub close_power_conflict: bool,
}
```

Everything a drying-cycle start carries besides the unit and the mode.

`Default` is the all-zero/empty set BambuStudio sends to stop a cycle; a start needs at
least `temp` and `duration_hours`.

#### Fields

- **`filament`**: `String`

  Filament material type being dried (e.g. "PA-CF").

- **`temp`**: `u32`

  Drying temperature (°C).

- **`duration_hours`**: `u32`

  Drying duration in whole hours.

- **`humidity`**: `u32`

  Target humidity (0 = firmware default / no target).

- **`rotate_tray`**: `bool`

  Whether to periodically rotate the tray during drying.

- **`cooling_temp`**: `u32`

  Cooling temperature applied after the cycle; BambuStudio sends the filament's
  softening temperature here.

- **`close_power_conflict`**: `bool`

  Whether to override the AMS unit's power-conflict interlock.

#### Trait Implementations

##### `impl Clone for DryingParams`

- <span id="dryingparams-clone"></span>`fn clone(&self) -> DryingParams` — [`DryingParams`](ams/index.md#dryingparams)

##### `impl Debug for DryingParams`

- <span id="dryingparams-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for DryingParams`

- <span id="dryingparams-default"></span>`fn default() -> DryingParams` — [`DryingParams`](ams/index.md#dryingparams)

##### `impl Eq for DryingParams`

##### `impl PartialEq for DryingParams`

- <span id="dryingparams-partialeq-eq"></span>`fn eq(&self, other: &DryingParams) -> bool` — [`DryingParams`](ams/index.md#dryingparams)

### `FilamentSpec<'a>`

```rust
struct FilamentSpec<'a> {
    pub preset: &'a str,
    pub material: &'a str,
    pub nozzle_temp_min: u32,
    pub nozzle_temp_max: u32,
}
```

The description an `ams_filament_setting` can't do without.

Required by [`AmsFilamentSettingRequest::new`]: sending the command without them writes an
empty material with a 0–0 °C nozzle window to the tray, and the printer acks it as success.

#### Fields

- **`preset`**: `&'a str`

  **Short-format** filament preset code, e.g. `"GFA01"` — see
  [`AmsFilamentSettingPayload::tray_info_idx`](ams/index.md#amsfilamentsettingpayload).

- **`material`**: `&'a str`

  Material type, e.g. `"PLA"`.

- **`nozzle_temp_min`**: `u32`

  Minimum safe nozzle temperature, °C.

- **`nozzle_temp_max`**: `u32`

  Maximum safe nozzle temperature, °C.

#### Trait Implementations

##### `impl Clone for FilamentSpec<'a>`

- <span id="filamentspec-clone"></span>`fn clone(&self) -> FilamentSpec<'a>` — [`FilamentSpec`](ams/index.md#filamentspec)

##### `impl Copy for FilamentSpec<'a>`

##### `impl Debug for FilamentSpec<'a>`

- <span id="filamentspec-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FilamentSpec<'a>`

##### `impl PartialEq for FilamentSpec<'a>`

- <span id="filamentspec-partialeq-eq"></span>`fn eq(&self, other: &FilamentSpec<'a>) -> bool` — [`FilamentSpec`](ams/index.md#filamentspec)

### `FlashTiming`

```rust
struct FlashTiming {
    pub on_ms: u32,
    pub off_ms: u32,
    pub loops: u32,
    pub interval_ms: u32,
}
```

Flash cycle timing for [`LedCtrlRequest::new_flashing`](hardware/index.md#ledctrlrequest); every field is in milliseconds except `loops`.

#### Fields

- **`on_ms`**: `u32`

  Time lit per cycle, in ms.

- **`off_ms`**: `u32`

  Time dark per cycle, in ms.

- **`loops`**: `u32`

  Number of cycles.

- **`interval_ms`**: `u32`

  Pause between cycles, in ms.

#### Trait Implementations

##### `impl Clone for FlashTiming`

- <span id="flashtiming-clone"></span>`fn clone(&self) -> FlashTiming` — [`FlashTiming`](hardware/index.md#flashtiming)

##### `impl Copy for FlashTiming`

##### `impl Debug for FlashTiming`

- <span id="flashtiming-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for FlashTiming`

- <span id="flashtiming-default"></span>`fn default() -> FlashTiming` — [`FlashTiming`](hardware/index.md#flashtiming)

##### `impl Eq for FlashTiming`

##### `impl Hash for FlashTiming`

- <span id="flashtiming-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FlashTiming`

- <span id="flashtiming-partialeq-eq"></span>`fn eq(&self, other: &FlashTiming) -> bool` — [`FlashTiming`](hardware/index.md#flashtiming)

### `NozzleRack`

```rust
struct NozzleRack {
    pub slot_extruders: Vec<i32>,
    pub rack_nozzle_id: i32,
}
```

Tool-changer rack routing for a print job: both inputs [`resolve_rack_nozzle_mapping`](print_job/index.md#resolve-rack-nozzle-mapping) needs.

#### Fields

- **`slot_extruders`**: `Vec<i32>`

  Extruder index per filament slot, negative for unprinted slots.

- **`rack_nozzle_id`**: `i32`

  Physical nozzle ID of the rack position the printer currently reports as live, in
  `RACK_NOZZLE_ID_MIN..=RACK_NOZZLE_ID_MAX` (16..=21). The caller must supply this because
  the mounted hotend can change between slicing and dispatch, and bambino does not model
  rack telemetry.

#### Trait Implementations

##### `impl Clone for NozzleRack`

- <span id="nozzlerack-clone"></span>`fn clone(&self) -> NozzleRack` — [`NozzleRack`](print_job/index.md#nozzlerack)

##### `impl Debug for NozzleRack`

- <span id="nozzlerack-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for NozzleRack`

##### `impl PartialEq for NozzleRack`

- <span id="nozzlerack-partialeq-eq"></span>`fn eq(&self, other: &NozzleRack) -> bool` — [`NozzleRack`](print_job/index.md#nozzlerack)

### `PrintJobConfig`

```rust
struct PrintJobConfig {
    pub job_filename: String,
    pub plate_gcode_path: String,
    pub subtask_name: String,
    pub raw_subtask_id: u64,
    pub bed_type: String,
    pub bed_leveling: CalibrationMode,
    pub run_flow_calibration: CalibrationMode,
    pub run_vibration_compensation: CalibrationMode,
    pub timelapse: bool,
    pub layer_inspect: bool,
    pub nozzle_offset_cali: Option<CalibrationMode>,
    pub ams: Option<AmsSource>,
    pub nozzle_rack: Option<NozzleRack>,
}
```

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` with named fields and sensible
defaults for calibration flags.

#### Fields

- **`job_filename`**: `String`

  Filename of the `.3mf` file on SD card storage (e.g. "job.3mf").

- **`plate_gcode_path`**: `String`

  Sliced plate gcode path inside the `.3mf` (e.g. "Metadata/plate_1.gcode").

- **`subtask_name`**: `String`

  User-friendly label for the print queue task.

- **`raw_subtask_id`**: `u64`

  Unique 32-bit tracking identifier before clamping (see `ClampedTaskId`).

- **`bed_type`**: `String`

  Bed plate type (e.g. "textured", "smooth").

- **`bed_leveling`**: `CalibrationMode`

  Whether to run automatic bed leveling before the print.

- **`run_flow_calibration`**: `CalibrationMode`

  Whether to run dynamic flow calibration before the print.

- **`run_vibration_compensation`**: `CalibrationMode`

  Whether to run vibration compensation calibration before the print. Defaults to `Off`
  on every model, matching BambuStudio, which always sends `false` (see
  `reference/03_mqtt_telemetry.md`, "Default (`vibration_cali`)"). No tri-state companion
  field exists on the wire for this one, so `Auto` serializes identically to `Off` via
  `as_wire_bool()`.

- **`timelapse`**: `bool`

  Whether timelapse capture is enabled.

- **`layer_inspect`**: `bool`

  Whether to run first-layer inspection during the print.

- **`nozzle_offset_cali`**: `Option<CalibrationMode>`

  `None` defers to the quirks engine default in `PrinterClient::start_print()`.

- **`ams`**: `Option<AmsSource>`

  AMS routing; `None` prints from the external spool (`use_ams: false`).

- **`nozzle_rack`**: `Option<NozzleRack>`

  Tool-changer rack routing, set via [`PrintJobConfig::with_nozzle_rack`](print_job/index.md#printjobconfig).
  
  Only consulted on a model whose quirks report `has_nozzle_rack`.

#### Implementations

- <span id="printjobconfig-new"></span>`fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self`

  Builds a job config with these defaults: bed leveling and flow calibration on,
  **timelapse recording and first-layer inspection on**, vibration compensation off, AMS
  disabled, and the model's own nozzle-offset-calibration default.

- <span id="printjobconfig-with-ams"></span>`fn with_ams(self, mapping: Vec<i32>) -> Self`

  Enables AMS and sets the flat slot-mapping array (`ams_mapping`).

  Values outside the documented flat channel space (`0..=15` standard AMS, `128..=135`
  AMS-HT, or `-1` unmapped) are folded to `-1` with a `log::warn!` — firmware rejects
  out-of-range values (254/255 in particular) with a visible error (`0700_8012`/
  `07FF_8012`, `reference/05_materials_ams.md:151`). The `with_ams_mapping2`-derived path
  already sanitizes via `flat_channel_id_for_entry`; this mirrors it for the raw path
  (issue #56).

  This is a convenience, not the enforcement point: `ams` is a public field, so
  `ProjectFileRequest::from_config` re-runs the same sanitization at serialization time
  (issue #120). Bypassing this builder cannot produce an out-of-range flat channel on the
  wire.

  Replaces any mapping set by [`with_ams_mapping2`](print_job/index.md#printjobconfig).

- <span id="printjobconfig-with-ams-mapping2"></span>`fn with_ams_mapping2(self, mapping2: Vec<AmsMapping2Entry>) -> Self` — [`AmsMapping2Entry`](../../ams/mapping/index.md#amsmapping2entry)

  Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`); the flat array is
  derived from them.

  Replaces any mapping set by [`with_ams`](print_job/index.md#printjobconfig).

- <span id="printjobconfig-bed-leveling"></span>`fn bed_leveling(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](print_job/index.md#calibrationmode)

  Enables or disables automatic bed leveling for this job.

- <span id="printjobconfig-flow-calibration"></span>`fn flow_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](print_job/index.md#calibrationmode)

  Enables or disables flow calibration for this job.

- <span id="printjobconfig-vibration-compensation"></span>`fn vibration_compensation(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](print_job/index.md#calibrationmode)

  Enables or disables vibration compensation calibration for this job. No tri-state
  companion field exists on the wire for this one, so `CalibrationMode::Auto` serializes
  identically to `Off`.

- <span id="printjobconfig-timelapse"></span>`fn timelapse(self, enabled: bool) -> Self`

  Enables or disables timelapse capture for this job.

- <span id="printjobconfig-layer-inspect"></span>`fn layer_inspect(self, enabled: bool) -> Self`

  Enables or disables first-layer inspection for this job.

- <span id="printjobconfig-with-nozzle-rack"></span>`fn with_nozzle_rack(self, slot_extruders: Vec<i32>, rack_nozzle_id: i32) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

  Supplies the tool-changer rack routing for this job (H2C only).

  `slot_extruders` is one extruder index per filament slot (negative = slot not printed);
  `rack_nozzle_id` is the physical ID of the live rack position. See
  [`resolve_rack_nozzle_mapping`](print_job/index.md#resolve-rack-nozzle-mapping) for how they combine and when the resulting
  `nozzle_mapping` is deliberately omitted. Ignored entirely on non-rack models.

  # Errors

  [`Error::InvalidArgument`](../../error/index.md#error) when `rack_nozzle_id` is outside the rack's physical IDs
  (`RACK_NOZZLE_ID_MIN..=RACK_NOZZLE_ID_MAX`), rather than silently sending no mapping.

- <span id="printjobconfig-nozzle-offset-calibration"></span>`fn nozzle_offset_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](print_job/index.md#calibrationmode)

  Overrides the model's default nozzle-offset-calibration behavior for this job.

#### Trait Implementations

##### `impl Clone for PrintJobConfig`

- <span id="printjobconfig-clone"></span>`fn clone(&self) -> PrintJobConfig` — [`PrintJobConfig`](print_job/index.md#printjobconfig)

##### `impl Debug for PrintJobConfig`

- <span id="printjobconfig-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `ClampedTaskId`

```rust
struct ClampedTaskId();
```

A task or sequence id already reduced into the range firmware accepts (below `i32::MAX`).

Every request constructor takes `impl Into<ClampedTaskId>`; the only way to make one is the
clamping `From<u64>`, so an out-of-range id can't reach the wire. Serializes as a decimal
string, the form the printer expects.

#### Implementations

- <span id="clampedtaskid-get"></span>`const fn get(self) -> u32`

  The clamped value.

#### Trait Implementations

##### `impl Clone for ClampedTaskId`

- <span id="clampedtaskid-clone"></span>`fn clone(&self) -> ClampedTaskId` — [`ClampedTaskId`](#clampedtaskid)

##### `impl Copy for ClampedTaskId`

##### `impl Debug for ClampedTaskId`

- <span id="clampedtaskid-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for ClampedTaskId`

- <span id="clampedtaskid-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for ClampedTaskId`

##### `impl Hash for ClampedTaskId`

- <span id="clampedtaskid-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for ClampedTaskId`

- <span id="clampedtaskid-partialeq-eq"></span>`fn eq(&self, other: &ClampedTaskId) -> bool` — [`ClampedTaskId`](#clampedtaskid)

##### `impl Serialize for ClampedTaskId`

- <span id="clampedtaskid-serialize"></span>`fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<<S as >::Ok, <S as >::Error>`

##### `impl ToString for ClampedTaskId`

- <span id="clampedtaskid-tostring-to-string"></span>`fn to_string(&self) -> String`

### `Info<P>`

```rust
struct Info<P> {
    pub info: P,
}
```

The `info` namespace envelope a command payload is published in.

#### Fields

- **`info`**: `P`

  The payload, serialized under `info`.

#### Implementations

- <span id="info-const-command"></span>`const COMMAND: &'static str`

- <span id="info-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `get_version` request.

#### Trait Implementations

##### `impl<P: clone::Clone> Clone for Info<P>`

- <span id="info-clone"></span>`fn clone(&self) -> Info<P>` — [`Info`](#info)

##### `impl<P: fmt::Debug> Debug for Info<P>`

- <span id="info-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl<P> Serialize for Info<P>`

- <span id="info-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `Print<P>`

```rust
struct Print<P> {
    pub print: P,
}
```

The `print` namespace envelope a command payload is published in.

#### Fields

- **`print`**: `P`

  The payload, serialized under `print`.

#### Implementations

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(filament_id: Option<&str>, nozzle_diameter: Option<&str>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds an `extrusion_cali_get` request.

  `filament_id` and `nozzle_diameter` scope the query; both are omitted from the wire when
  `None`, which reproduces the bare request shape exactly. Whether to scope by diameter
  depends on the machine — see [`ExtrusionCaliGetPayload::nozzle_diameter`](../../diagnostics/kprofile/index.md#extrusioncaligetpayload).

  Callers should prefer `PrinterClient::get_k_profiles()`, which handles the priming quirk
  documented above.

- <span id="print-with-extruder-id"></span>`fn with_extruder_id(self, extruder_id: u8) -> Self`

  Scopes the query to one hotend, for a dual-nozzle machine where a `cali_idx` is not
  unique across extruders. `0` = right/main, `1` = left/deputy.

- <span id="print-with-nozzle-id"></span>`fn with_nozzle_id(self, nozzle_id: &str) -> Self`

  Scopes the query to one flow type, e.g. `"HS00-0.4"` (standard) or `"HH00-0.4"` (high
  flow) — see [`ExtrusionCaliGetPayload::nozzle_id`](../../diagnostics/kprofile/index.md#extrusioncaligetpayload).

- <span id="print-with-nozzle-rack-position"></span>`fn with_nozzle_rack_position(self, nozzle_pos: i32, nozzle_sn: &str) -> Self`

  Names a specific physical hotend by rack position and serial. BambuStudio sends these
  two together and only for a non-negative position.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(profiles: Vec<KProfileEntry>, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`KProfileEntry`](../../diagnostics/kprofile/index.md#kprofileentry), [`ClampedTaskId`](#clampedtaskid), [`Error`](../../error/index.md#error)

  Builds a secure write-transaction payload targeting physical EEPROM slots.

  Verifies that all target profiles carry valid setting identifiers to protect local
  database health. Supports multi-profile writes for IDEX platforms.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(address: CaliSelAddress, cali_idx: i32, filament_id: &str, nozzle_diameter: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`CaliSelAddress`](../../diagnostics/kprofile/index.md#caliseladdress), [`ClampedTaskId`](#clampedtaskid)

  Creates a request payload to bind a stored K-profile calibration entry to an AMS
  material slot.

  **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
  addressing differs by command family — this rule is *not* the same one used by
  `ams_filament_setting` (filament configuration, see
  [`crate::mqtt::AmsFilamentSettingRequest::new`](../index.md)):
  * `extrusion_cali_sel` (this command) — Single-Nozzle Platforms: `ams_id: 254` /
    `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`;
    Ext-R requires `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong
    address for Ext-R on IDEX machines mis-routes the pressure advance profile to
    the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    uncalibrated.
  * `ams_filament_setting` — Single-Nozzle Platforms: `ams_id: 255` / `tray_id: 254`.
    Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R (`ams_id: 255`) require
    `tray_id: 254`, never `0` (BUG-117 / BambuStudio `DeviceManager.cpp:1667-1693`).

  The address is a [`CaliSelAddress`](../../diagnostics/kprofile/index.md#caliseladdress), which derives the wire `ams_id`, global `tray_id` and
  local `slot_id` from a unit and slot, so the three can't disagree (#397).

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(target: StandardCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`StandardCaliDelEntry`](../../diagnostics/kprofile/index.md#standardcalidelentry), [`ClampedTaskId`](#clampedtaskid), [`Error`](../../error/index.md#error)

  Builds a single-nozzle deletion transaction keyed on the setting identifier.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(target: IdexCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`IdexCaliDelEntry`](../../diagnostics/kprofile/index.md#idexcalidelentry), [`ClampedTaskId`](#clampedtaskid)

  Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(ams_id: i32, slot_id: i32, filament: FilamentSpec<'_>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`FilamentSpec`](ams/index.md#filamentspec), [`ClampedTaskId`](#clampedtaskid)

  Creates a request payload to update slot parameters.

  **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
  For standard physical slots, `ams_id` matches the expansion unit index (0-3). For an
  external spool, pass the virtual `ams_id` (`255` single-nozzle / Ext-R, `254` Ext-L)
  with `slot_id: 0`. An A2L-attached AMS Lite takes its physical wire id `16` with a local
  `0..=3` slot, not the normalized `6` telemetry reports it as (bambuddy
  `ams_set_filament_setting`, matching the firmware's own `ams_mapping2`).

  **`slot_id` is what you pass; `tray_id` is derived.** Both reach the wire, and they
  differ on a virtual tray: `tray_id` becomes `254` for either external `ams_id` and the
  slot index otherwise. Deriving it here rather than accepting it means a caller cannot
  send a `slot_id`/`tray_id` pair that contradicts itself — the same reasoning as
  `PrinterClient::change_filament()` deriving `target`.

  Confirmed against BambuStudio's `command_ams_filament_settings`
  (`DeviceManager.cpp:1707-1722`), whose `tag_tray_id` maps either
  `VIRTUAL_TRAY_MAIN_ID`/`VIRTUAL_TRAY_DEPUTY_ID` to `254` and whose own call sites pass
  `slot_id: 0` for a virtual tray (`:4853`, `:4877`); and against bambuddy's
  `ams_set_filament_setting`, which sends `ams_id: 255`, `tray_id: 254`, `slot_id: 0` for
  a single external slot.

  **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
  addressing differs by command family — this rule is *not* the same one used by
  `extrusion_cali_sel` (K-profile binding, see
  `crate::diagnostics::ExtrusionCaliSelRequest::new`):
  * `ams_filament_setting` (this command) — Single-Nozzle Platforms: `ams_id: 255` /
    `tray_id: 254`. Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R
    (`ams_id: 255`) require `tray_id: 254` (confirmed against
    `command_ams_filament_settings`, `DeviceManager.cpp:1667-1693` — `tag_ams_id ==
    VIRTUAL_TRAY_MAIN_ID(255) || VIRTUAL_TRAY_DEPUTY_ID(254)` always maps to
    `tag_tray_id = VIRTUAL_TRAY_DEPUTY_ID(254)`, never `0`).
  * `extrusion_cali_sel` — Single-Nozzle Platforms: `ams_id: 254` / `tray_id: 254`.
    Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`; Ext-R requires
    `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong address for
    Ext-R on IDEX machines mis-routes the pressure advance profile to the left
    carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    uncalibrated.

  The filament's essential description is the named [`FilamentSpec`](ams/index.md#filamentspec); the genuinely
  optional fields (color, sub-brand, long setting id) are `with_*` methods. Named fields
  rather than positional arguments because the temperature bounds were adjacent `u32`s,
  transposable without a compile error on a command whose failures are already silent.

  Unset optional fields: an empty color, a `"{material} Basic"` sub-brand, and no
  `setting_id` on the wire.

- <span id="print-with-sub-brands"></span>`fn with_sub_brands(self, sub_brands: &str) -> Self`

  Overrides the sub-brand label (default `"{material} Basic"`). Case is meaningful and is left alone.

- <span id="print-with-color"></span>`fn with_color(self, color_hex: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

  Sets the tray color from `RRGGBB` or `RRGGBBAA` hex, optionally `#`-prefixed.

  Normalized to the 8-digit uppercase form the firmware stores: a 6-digit color gets an
  opaque `FF` alpha, and lowercase digits are uppercased. The printer parses lowercase hex
  letters in `tray_color` as `0` and the corruption is silent: the `ams_filament_setting`
  ack echoes the value that was sent and reports `result: "success"`, and only the next
  AMS push status reveals it (measured on a P1S running firmware `01.10.00.00` —
  `09ff00ff` stored as `09000000`, `090000FF` intact).

  # Errors

  [`Error::InvalidArgument`](../../error/index.md#error) for anything that isn't 6 or 8 hex digits.

- <span id="print-with-setting-id"></span>`fn with_setting_id(self, setting_id: &str) -> Self`

  Attaches the full preset identifier, which is a separate wire field from
  `tray_info_idx` and is omitted entirely when not set.

  Pass the long form here — `"GFSL05_07"`, or a `"PF"`-prefixed id — and keep the short
  code in [`FilamentSpec::preset`](ams/index.md#filamentspec). See
  [`AmsFilamentSettingPayload::tray_info_idx`](ams/index.md#amsfilamentsettingpayload) for what the printer does when a long id is
  put in the short field instead.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(operation: AmsControlOp, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`AmsControlOp`](ams/index.md#amscontrolop), [`ClampedTaskId`](#clampedtaskid)

  Builds an `ams_control` request for `operation`.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds an `ams_get_rfid` request.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-load"></span>`fn load(ams_id: u8, slot_id: u8, temps: ChangeTemps, extruder_id: Option<u8>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ChangeTemps`](ams/index.md#changetemps), [`ClampedTaskId`](#clampedtaskid)

  Builds a request loading slot `slot_id` of the unit at wire address `ams_id`.

  `target` is derived, never caller-supplied, per BambuStudio's
  `command_ams_change_filament` (`DeviceManager.cpp:1602-1638`): the `ams_id` itself for
  any unit at wire address 16 or above (an A2L's AMS Lite, AMS-HT, an external spool), or
  the flat global tray (`ams_id * 4 + slot_id`) for a standard unit. A caller-supplied
  `target` that didn't match was a real hardware misconfiguration risk (`07FF_8012` class);
  it mirrored `slot_id` only coincidentally, for `ams_id: 0`.

  `ams_id` is the *wire* address — an A2L's AMS Lite is `16` here, not the `6` telemetry
  reports. `PrinterClient::change_filament` validates the address and converts it.

  Pass `extruder_id: None` on any printer without a Filament Track Switch — see
  [`AmsChangeFilamentPayload::extruder_id`](ams/index.md#amschangefilamentpayload) for why an FTS machine requires it.

- <span id="print-unload"></span>`fn unload(ams_id: u8, temps: ChangeTemps, extruder_id: Option<u8>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ChangeTemps`](ams/index.md#changetemps), [`ClampedTaskId`](#clampedtaskid)

  Builds a request unloading (retracting) the filament fed from the unit at wire address `ams_id`.

  `slot_id` and `target` are both the `255` unload sentinel, as in BambuStudio.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-start"></span>`fn start(ams_id: i32, params: DryingParams, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`DryingParams`](ams/index.md#dryingparams), [`ClampedTaskId`](#clampedtaskid)

  Builds a request starting a drying cycle on the unit at `ams_id`.

- <span id="print-stop"></span>`fn stop(ams_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a request stopping the drying cycle on the unit at `ams_id`.

  Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:40-53`): every field
  but the unit and mode zeroed.

- <span id="print-new"></span>`fn new(command: StandardCommand, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`StandardCommand`](control/index.md#standardcommand), [`ClampedTaskId`](#clampedtaskid)

  Builds a request for `command`.

- <span id="print-pause"></span>`fn pause(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `pause` request.

- <span id="print-resume"></span>`fn resume(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `resume` request.

- <span id="print-stop"></span>`fn stop(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `stop` request.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(object_indices: Vec<u32>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `skip_objects` request from a list of object indices to skip.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `clean_print_error` request.

- <span id="print-ignore"></span>`fn ignore(error_code: u32, job_id: Option<&str>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds an `ignore` request: skip the next re-check of `error_code` and resume.

  Unlike a plain `resume` ("fixed it, re-check"), this stops a fault such as a wrong build
  plate from being re-detected and re-pausing the print a second later (bambuddy #1869).

  `job_id` is the running job's id, or `None` when no telemetry has carried one yet.

- <span id="print-resume"></span>`fn resume(error_code: u32, job_id: Option<&str>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds an error-aware `resume` request, the form BambuStudio's error dialog sends.

- <span id="print-stop"></span>`fn stop(error_code: u32, job_id: Option<&str>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds an error-aware `stop` request, the form BambuStudio's error dialog sends.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(error_code: u32, scope: IdleIgnoreScope, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`IdleIgnoreScope`](control/index.md#idleignorescope), [`ClampedTaskId`](#clampedtaskid)

  Builds an `idle_ignore` request dismissing `error_code` for `scope`.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(option_bitmask: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `calibration` request from a capability option bitmask.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(speed: PrintSpeed, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`PrintSpeed`](../../types/control/index.md#printspeed), [`ClampedTaskId`](#clampedtaskid)

  Builds a `print_speed` request for `speed`.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(gcode_line: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Creates a request envelope wrapping a raw G-code payload.

  Ensures the line ends with a newline (`\n`), appending one only if missing, so the
  printer's stream parser sees the end-of-command boundary.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(mode: AirductMode, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`AirductMode`](hardware/index.md#airductmode), [`ClampedTaskId`](#clampedtaskid)

  Builds a `set_airduct` request for the given damper mode.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `print_option` request enabling or disabling notification sounds.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-new"></span>`fn new(mode: BuzzerMode, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`BuzzerMode`](../../types/control/index.md#buzzermode), [`ClampedTaskId`](#clampedtaskid)

  Builds a `buzzer_ctrl` request for the given alarm mode.

- <span id="print-const-command"></span>`const COMMAND: &'static str`

- <span id="print-from-config"></span>`fn from_config(config: &PrintJobConfig, sequence_id: impl Into<ClampedTaskId>, model: PrinterModel) -> Self` — [`PrintJobConfig`](print_job/index.md#printjobconfig), [`ClampedTaskId`](#clampedtaskid), [`PrinterModel`](../../models/index.md#printermodel)

  Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.

  `nozzle_offset_cali` is gated on the model's `supports_nozzle_offset_calibration()`
  quirk as a hard ceiling, not a default: it is enabled automatically on IDEX and
  tool-changer platforms when the caller left it `None`, and forced off on every
  single-nozzle model even when the caller explicitly asked for it — the printer has no
  second carriage to calibrate.

  **Polymorphic Warning [REF-MQTT-LIFECYCLE]:**
  `use_ams` is serialized strictly as a JSON boolean. On dual-nozzle IDEX systems,
  serializing this field as an integer (e.g., `1` / `0`) causes the printer's JSON engine
  to treat the value as the physical carriage index (Target nozzle 1) instead of material
  routing parameters.

#### Trait Implementations

##### `impl<P: clone::Clone> Clone for Print<P>`

- <span id="print-clone"></span>`fn clone(&self) -> Print<P>` — [`Print`](#print)

##### `impl<P: fmt::Debug> Debug for Print<P>`

- <span id="print-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl<P> Serialize for Print<P>`

- <span id="print-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `Pushing<P>`

```rust
struct Pushing<P> {
    pub pushing: P,
}
```

The `pushing` namespace envelope a command payload is published in.

#### Fields

- **`pushing`**: `P`

  The payload, serialized under `pushing`.

#### Implementations

- <span id="pushing-const-command"></span>`const COMMAND: &'static str`

- <span id="pushing-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `pushall` request.

#### Trait Implementations

##### `impl<P: clone::Clone> Clone for Pushing<P>`

- <span id="pushing-clone"></span>`fn clone(&self) -> Pushing<P>` — [`Pushing`](#pushing)

##### `impl<P: fmt::Debug> Debug for Pushing<P>`

- <span id="pushing-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl<P> Serialize for Pushing<P>`

- <span id="pushing-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `System<P>`

```rust
struct System<P> {
    pub system: P,
}
```

The `system` namespace envelope a command payload is published in.

#### Fields

- **`system`**: `P`

  The payload, serialized under `system`.

#### Implementations

- <span id="system-const-command"></span>`const COMMAND: &'static str`

- <span id="system-close-print-error"></span>`fn close_print_error(error_code: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `uiop` request closing the dialog for `error_code`.

- <span id="system-const-command"></span>`const COMMAND: &'static str`

- <span id="system-new"></span>`fn new(node: LedNode, turn_on: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`LedNode`](../../types/control/index.md#lednode), [`ClampedTaskId`](#clampedtaskid)

  Builds a simple on/off `ledctrl` request for `node`.

- <span id="system-new-flashing"></span>`fn new_flashing(node: LedNode, timing: FlashTiming, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`LedNode`](../../types/control/index.md#lednode), [`FlashTiming`](hardware/index.md#flashtiming), [`ClampedTaskId`](#clampedtaskid)

  Builds a flashing-mode request (`led_mode: "flashing"`) with explicit timing [REF-MQTT-LIFECYCLE].

- <span id="system-const-command"></span>`const COMMAND: &'static str`

- <span id="system-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](#clampedtaskid)

  Builds a `get_access_code` request.

#### Trait Implementations

##### `impl<P: clone::Clone> Clone for System<P>`

- <span id="system-clone"></span>`fn clone(&self) -> System<P>` — [`System`](#system)

##### `impl<P: fmt::Debug> Debug for System<P>`

- <span id="system-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl<P> Serialize for System<P>`

- <span id="system-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsControlOp`

```rust
enum AmsControlOp {
    Resume,
    Pause,
    Reset,
}
```

An `ams_control` operation on the AMS feed mechanism.

#### Variants

- **`Resume`**

  Resume feeding (`resume`).

- **`Pause`**

  Pause feeding (`pause`).

- **`Reset`**

  Reset the feed state (`reset`).

#### Implementations

- <span id="amscontrolop-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The wire `param` value.

#### Trait Implementations

##### `impl Clone for AmsControlOp`

- <span id="amscontrolop-clone"></span>`fn clone(&self) -> AmsControlOp` — [`AmsControlOp`](ams/index.md#amscontrolop)

##### `impl Copy for AmsControlOp`

##### `impl Debug for AmsControlOp`

- <span id="amscontrolop-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsControlOp`

##### `impl Hash for AmsControlOp`

- <span id="amscontrolop-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AmsControlOp`

- <span id="amscontrolop-partialeq-eq"></span>`fn eq(&self, other: &AmsControlOp) -> bool` — [`AmsControlOp`](ams/index.md#amscontrolop)

### `IdleIgnoreScope`

```rust
enum IdleIgnoreScope {
    Once,
    Permanent,
}
```

How long an `idle_ignore` dismissal lasts.

#### Variants

- **`Once`**

  Dismiss this occurrence only (`type: 0`).

- **`Permanent`**

  Never show this warning again (`type: 1`).

#### Trait Implementations

##### `impl Clone for IdleIgnoreScope`

- <span id="idleignorescope-clone"></span>`fn clone(&self) -> IdleIgnoreScope` — [`IdleIgnoreScope`](control/index.md#idleignorescope)

##### `impl Copy for IdleIgnoreScope`

##### `impl Debug for IdleIgnoreScope`

- <span id="idleignorescope-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for IdleIgnoreScope`

##### `impl Hash for IdleIgnoreScope`

- <span id="idleignorescope-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for IdleIgnoreScope`

- <span id="idleignorescope-partialeq-eq"></span>`fn eq(&self, other: &IdleIgnoreScope) -> bool` — [`IdleIgnoreScope`](control/index.md#idleignorescope)

### `StandardCommand`

```rust
enum StandardCommand {
    Pause,
    Resume,
    Stop,
    RefreshNozzle,
    CloseAirFilter,
    AutoStopAmsDry,
}
```

A print-lifecycle command that carries nothing but its name and `sequence_id` [REF-MQTT-LIFECYCLE].

#### Variants

- **`Pause`**

  Pause the running job (`pause`).

- **`Resume`**

  Resume a paused job (`resume`).

- **`Stop`**

  Stop the job (`stop`).

- **`RefreshNozzle`**

  Re-read the nozzle information (`refresh_nozzle`).

- **`CloseAirFilter`**

  Turn off air purification (`close_air_filt`).

- **`AutoStopAmsDry`**

  The error dialog's "stop drying" (`auto_stop_ams_dry`).

#### Implementations

- <span id="standardcommand-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The wire command name.

#### Trait Implementations

##### `impl Clone for StandardCommand`

- <span id="standardcommand-clone"></span>`fn clone(&self) -> StandardCommand` — [`StandardCommand`](control/index.md#standardcommand)

##### `impl Copy for StandardCommand`

##### `impl Debug for StandardCommand`

- <span id="standardcommand-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for StandardCommand`

##### `impl Hash for StandardCommand`

- <span id="standardcommand-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for StandardCommand`

- <span id="standardcommand-partialeq-eq"></span>`fn eq(&self, other: &StandardCommand) -> bool` — [`StandardCommand`](control/index.md#standardcommand)

### `AirductMode`

```rust
enum AirductMode {
    Cooling,
    Heating,
    Laser,
}
```

Airduct damper operating mode [REF-MQTT-LIFECYCLE].

`Cooling` (0): closes internal recirculation dampers, routes hot air out through exhaust.
`Heating` (1): closes exhaust flaps, seals enclosure for heat retention.
`Laser` (2): configuration for laser engraving module operation.

#### Variants

- **`Cooling`**

  Closes internal recirculation dampers, routes hot air out through exhaust.

- **`Heating`**

  Seals enclosure, closes exhaust flaps for heat retention.

- **`Laser`**

  Laser engraving module configuration.

#### Trait Implementations

##### `impl Clone for AirductMode`

- <span id="airductmode-clone"></span>`fn clone(&self) -> AirductMode` — [`AirductMode`](hardware/index.md#airductmode)

##### `impl Copy for AirductMode`

##### `impl Debug for AirductMode`

- <span id="airductmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirductMode`

##### `impl PartialEq for AirductMode`

- <span id="airductmode-partialeq-eq"></span>`fn eq(&self, other: &AirductMode) -> bool` — [`AirductMode`](hardware/index.md#airductmode)

### `AmsMappingTable`

```rust
enum AmsMappingTable {
    Inactive,
    Active(Vec<i32>),
}
```

Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

**The Polymorphic Mapping Rule:**
* When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
* When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).

#### Variants

- **`Inactive`**

  External-spool mode: serializes to an empty string.

- **`Active`**

  AMS active mode: serializes to an integer slot-mapping array.

#### Trait Implementations

##### `impl Clone for AmsMappingTable`

- <span id="amsmappingtable-clone"></span>`fn clone(&self) -> AmsMappingTable` — [`AmsMappingTable`](print_job/index.md#amsmappingtable)

##### `impl Debug for AmsMappingTable`

- <span id="amsmappingtable-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsMappingTable`

##### `impl PartialEq for AmsMappingTable`

- <span id="amsmappingtable-partialeq-eq"></span>`fn eq(&self, other: &AmsMappingTable) -> bool` — [`AmsMappingTable`](print_job/index.md#amsmappingtable)

##### `impl Serialize for AmsMappingTable`

- <span id="amsmappingtable-serialize"></span>`fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<<S as >::Ok, <S as >::Error>`

### `AmsSource`

```rust
enum AmsSource {
    Flat(Vec<i32>),
    Structured(Vec<crate::ams::mapping::AmsMapping2Entry>),
}
```

Where a print job's AMS routing comes from: one mapping form or the other, never both.

#### Variants

- **`Flat`**

  A flat `ams_mapping` channel array (one entry per project filament, `-1` = unmapped).
  No `ams_mapping2` is sent.

- **`Structured`**

  Structured `ams_mapping2` entries. The flat array is derived from them, so the two
  wire arrays always agree index for index [REF-AMS-MAP].

#### Trait Implementations

##### `impl Clone for AmsSource`

- <span id="amssource-clone"></span>`fn clone(&self) -> AmsSource` — [`AmsSource`](print_job/index.md#amssource)

##### `impl Debug for AmsSource`

- <span id="amssource-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsSource`

##### `impl PartialEq for AmsSource`

- <span id="amssource-partialeq-eq"></span>`fn eq(&self, other: &AmsSource) -> bool` — [`AmsSource`](print_job/index.md#amssource)

### `CalibrationMode`

```rust
enum CalibrationMode {
    Off,
    On,
    Auto,
}
```

Tri-state calibration setting: force every print, skip entirely, or let the firmware decide
based on whether the relevant calibration ran recently [REF-MQTT-LIFECYCLE].

Mirrors BambuStudio's own `getValueInt()` encoding for these fields (confirmed in
`bambu_networking.hpp`'s `auto_bed_leveling` member and `SelectMachine.cpp`'s
`ops_auto`-driven checkboxes): `Off` = 0, `On` = 1, `Auto` = 2 (skip if not needed recently).

#### Variants

- **`Off`**

  Never run this calibration.

- **`On`**

  Always run this calibration.

- **`Auto`**

  Let the firmware run it only if it wasn't done recently.

#### Trait Implementations

##### `impl Clone for CalibrationMode`

- <span id="calibrationmode-clone"></span>`fn clone(&self) -> CalibrationMode` — [`CalibrationMode`](print_job/index.md#calibrationmode)

##### `impl Copy for CalibrationMode`

##### `impl Debug for CalibrationMode`

- <span id="calibrationmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for CalibrationMode`

- <span id="calibrationmode-default"></span>`fn default() -> CalibrationMode` — [`CalibrationMode`](print_job/index.md#calibrationmode)

##### `impl Eq for CalibrationMode`

##### `impl PartialEq for CalibrationMode`

- <span id="calibrationmode-partialeq-eq"></span>`fn eq(&self, other: &CalibrationMode) -> bool` — [`CalibrationMode`](print_job/index.md#calibrationmode)

### `AmsChangeFilamentRequest`

```rust
type AmsChangeFilamentRequest = super::Print<AmsChangeFilamentPayload>;
```

Loads or unloads filament from an AMS slot or external spool to the toolhead.

### `AmsControlRequest`

```rust
type AmsControlRequest = super::Print<AmsControlPayload>;
```

Sends a resume, pause, or reset command to the AMS feed mechanism.

### `AmsFilamentDryingRequest`

```rust
type AmsFilamentDryingRequest = super::Print<AmsFilamentDryingPayload>;
```

Starts or stops a filament drying cycle on an AMS unit with a built-in heater.

### `AmsFilamentSettingRequest`

```rust
type AmsFilamentSettingRequest = super::Print<AmsFilamentSettingPayload>;
```

Sets filament properties (type, color, temperature range) on an AMS tray or external spool.

### `AmsGetRfidRequest`

```rust
type AmsGetRfidRequest = super::Print<AmsGetRfidPayload>;
```

Requests an RFID tag scan on a specific AMS slot.

### `CalibrationRequest`

```rust
type CalibrationRequest = super::Print<CalibrationPayload>;
```

Kicks off a calibration routine (vibration compensation, bed leveling, etc.).

### `CleanPrintErrorRequest`

```rust
type CleanPrintErrorRequest = super::Print<CleanPrintErrorPayload>;
```

Clears the printer's current error state so it can resume operation.

### `HmsActionRequest`

```rust
type HmsActionRequest = super::Print<HmsActionPayload>;
```

Answers a paused print's error dialog: ignore the fault and resume, or resume/stop naming it.

### `IdleIgnoreRequest`

```rust
type IdleIgnoreRequest = super::Print<IdleIgnorePayload>;
```

Dismisses a non-pausing warning, once or permanently (BambuStudio `command_hms_idle_ignore`).

### `PrintSpeedRequest`

```rust
type PrintSpeedRequest = super::Print<PrintSpeedPayload>;
```

Changes the active print speed profile (silent, standard, sport, ludicrous).

### `SkipObjectsRequest`

```rust
type SkipObjectsRequest = super::Print<SkipObjectsPayload>;
```

Tells the printer to skip specific objects in a multi-object print.

### `StandardControlRequest`

```rust
type StandardControlRequest = super::Print<StandardControlPayload>;
```

Sends a name-only print lifecycle command (pause, resume, stop, ...) to the printer.

### `UiopRequest`

```rust
type UiopRequest = super::System<UiopPayload>;
```

Closes the error dialog on the printer's screen (BambuStudio `command_clean_print_error_uiop`).

Separate from [`CleanPrintErrorRequest`](control/index.md#cleanprinterrorrequest), which clears the error latch: BambuStudio sends
this once whenever its own copy of the dialog closes.

### `GCodeRequest`

```rust
type GCodeRequest = super::Print<GCodePayload>;
```

Sends a raw G-code line to the printer for immediate execution.

### `AirductRequest`

```rust
type AirductRequest = super::Print<AirductPayload>;
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

### `BuzzerRequest`

```rust
type BuzzerRequest = super::Print<BuzzerPayload>;
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

### `LedCtrlRequest`

```rust
type LedCtrlRequest = super::System<LedCtrlPayload>;
```

Turns chamber or toolhead LEDs on or off.

### `PromptSoundRequest`

```rust
type PromptSoundRequest = super::Print<PromptSoundPayload>;
```

Enables or disables the printer's notification sounds.

### `ProjectFileRequest`

```rust
type ProjectFileRequest = super::Print<ProjectFilePayload>;
```

Submits a `.3mf` print job from the SD card for execution.

### `GetAccessCodeRequest`

```rust
type GetAccessCodeRequest = super::System<GetAccessCodePayload>;
```

Queries the printer for its own current LAN access code.

Distinct from the access code the caller supplies to authenticate: this re-reads the value
from the printer over an already-authenticated session, which is how a client notices that a
rotated code has invalidated its cached credential.

The reply is `system`-wrapped and echoes the request's `sequence_id`, alongside
`access_code`, `result`, and `reason` — confirmed on a P1S via `bambino-cli ack-probe`
(issue #140); see `reference/03_mqtt_telemetry.md` for the observed shape.

Treat the returned code as a credential: it must never be logged or written to disk.

### `GetVersionRequest`

```rust
type GetVersionRequest = super::Info<GetVersionPayload>;
```

Queries the printer for its hardware and firmware version info.

### `PushAllRequest`

```rust
type PushAllRequest = super::Pushing<PushAllPayload>;
```

Requests a full state dump from the printer (all telemetry fields at once).


---

## Functions

### `resolve_rack_nozzle_mapping`

```rust
fn resolve_rack_nozzle_mapping(slot_extruders: &[i32], rack_nozzle_id: i32) -> Option<Vec<i32>>
```

Translates a per-slot extruder mapping into an H2C `nozzle_mapping` of physical nozzle IDs.

`slot_extruders` holds one extruder index per filament slot, with any negative value meaning
"this slot is not printed". `rack_nozzle_id` is the physical ID of the rack position the
printer currently reports as live, which only the caller can know — the mounted hotend can
change between slicing and dispatch.

Returns a 32-slot vector of physical IDs (the fixed wire length), or `None` when it cannot
be resolved with confidence. **`None` means "omit the field entirely" and is the deliberate
failure mode, not an error path.** Omitting it returns the firmware to its own nozzle pick,
which is merely suboptimal; a *wrong* physical ID makes the printer level with one nozzle and
print with another millimetres off the bed. Upstream reached that failure twice, so this
declines rather than guesses when any of the following holds:

- the slot list is empty, or longer than the wire format carries;
- `rack_nozzle_id` is not a real rack position;
- no slot actually needs the rack — BambuStudio omits `nozzle_mapping` for a fixed-hotend-only
  plate, so this matches rather than naming a nozzle it need not name;
- a slot names a carriage an H2C does not have, meaning the file was mapped for another
  machine and forwarding the value raw would name a physical nozzle by a foreign index.

# The two namespaces

Extruder indices and physical nozzle IDs overlap numerically and mean different things. On an
H2C the *fixed* hotend is extruder index `1` and physical ID `1`; the *rack* is extruder index
`0` and physical IDs `16..=21`. Passing an index where an ID belongs is the entire bug class
this function exists to prevent.

**Unverified on hardware here** — no H2C is available. Every value above is taken from
bambuddy's hardware-measured constants; see `reference/03_mqtt_telemetry.md` for the
measurements and the two corrections upstream made to them.

