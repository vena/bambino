*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [print_job](index.md)*

---

# Module `print_job`

Print job dispatch (file selection, AMS material mapping, plate/timelapse config).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`PrintJobConfig`](#printjobconfig) | struct | Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE]. |
| [`ProjectFilePayload`](#projectfilepayload) | struct | Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage. |
| [`ProjectFileRequest`](#projectfilerequest) | struct | Submits a `.3mf` print job from the SD card for execution. |
| [`AmsMappingTable`](#amsmappingtable) | enum | Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE]. |
| [`CalibrationMode`](#calibrationmode) | enum | Tri-state calibration setting: force every print, skip entirely, or let the firmware decide based on whether the relevant calibration ran recently [REF-MQTT-LIFECYCLE]. |

## Types

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
    pub run_vibration_compensation: bool,
    pub timelapse: bool,
    pub layer_inspect: bool,
    pub nozzle_offset_cali: Option<CalibrationMode>,
    pub use_ams: bool,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>,
}
```

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
with named fields and sensible defaults for calibration flags.

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

- **`run_vibration_compensation`**: `bool`

  Whether to run vibration compensation calibration before the print.

- **`timelapse`**: `bool`

  Whether timelapse capture is enabled.

- **`layer_inspect`**: `bool`

  Whether to run first-layer inspection during the print.

- **`nozzle_offset_cali`**: `Option<CalibrationMode>`

  `None` defers to the quirks engine default in `PrinterClient::start_print()`.

- **`use_ams`**: `bool`

  Whether to route filament through the AMS rather than an external spool.

- **`ams_mapping`**: `Vec<i32>`

  Flat AMS slot mapping (one entry per plate object, -1 = no AMS slot).

- **`ams_mapping2`**: `Option<Vec<crate::ams::mapping::AmsMapping2Entry>>`

  Structured per-nozzle AMS mapping; takes precedence over `ams_mapping` when set.

#### Implementations

- <span id="printjobconfig-new"></span>`fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self`

  Builds a job config with calibration flags defaulted on and AMS disabled.

- <span id="printjobconfig-with-ams"></span>`fn with_ams(self, mapping: Vec<i32>) -> Self`

  Enables AMS and sets the flat slot-mapping array (`ams_mapping`).

- <span id="printjobconfig-with-ams-mapping2"></span>`fn with_ams_mapping2(self, mapping2: Vec<AmsMapping2Entry>) -> Self` — [`AmsMapping2Entry`](../../../ams/mapping/index.md#amsmapping2entry)

  Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`).

- <span id="printjobconfig-bed-leveling"></span>`fn bed_leveling(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](#calibrationmode)

  Enables or disables automatic bed leveling for this job.

- <span id="printjobconfig-flow-calibration"></span>`fn flow_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](#calibrationmode)

  Enables or disables flow calibration for this job.

- <span id="printjobconfig-vibration-compensation"></span>`fn vibration_compensation(self, enabled: bool) -> Self`

  Enables or disables vibration compensation calibration for this job.

- <span id="printjobconfig-timelapse"></span>`fn timelapse(self, enabled: bool) -> Self`

  Enables or disables timelapse capture for this job.

- <span id="printjobconfig-layer-inspect"></span>`fn layer_inspect(self, enabled: bool) -> Self`

  Enables or disables first-layer inspection for this job.

- <span id="printjobconfig-nozzle-offset-calibration"></span>`fn nozzle_offset_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](#calibrationmode)

  Overrides the model's default nozzle-offset-calibration behavior for this job.

#### Trait Implementations

##### `impl Clone for PrintJobConfig`

- <span id="printjobconfig-clone"></span>`fn clone(&self) -> PrintJobConfig` — [`PrintJobConfig`](#printjobconfig)

##### `impl Debug for PrintJobConfig`

- <span id="printjobconfig-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `ProjectFilePayload`

```rust
struct ProjectFilePayload {
    pub command: &'static str,
    pub sequence_id: String,
    pub param: String,
    pub subtask_name: String,
    pub subtask_id: String,
    pub flow_cali: bool,
    pub profile_id: String,
    pub project_id: String,
    pub task_id: String,
    pub file: String,
    pub url: String,
    pub timelapse: bool,
    pub bed_type: String,
    pub bed_leveling: bool,
    pub auto_bed_leveling: i32,
    pub extrude_cali_flag: i32,
    pub nozzle_offset_cali: i32,
    pub vibration_cali: bool,
    pub layer_inspect: bool,
    pub use_ams: bool,
    pub ams_mapping: AmsMappingTable,
    pub ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>,
}
```

Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"project_file"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

- **`param`**: `String`

  Target file path of the internal sliced plate payload (e.g. "Metadata/plate_1.gcode").

- **`subtask_name`**: `String`

  User-friendly label associated with the print queue task.

- **`subtask_id`**: `String`

  Unique 32-bit tracking identifier (Clamped to prevent overflow lockups).

- **`flow_cali`**: `bool`

  Dynamic flow (pressure advance) calibration flag, duplicating `extrude_cali_flag` under
  its own key. bambuddy cites a real production incident (#1478) where a
  consumer relying on the wrong one of these two calibration flags silently skipped
  calibration — both are sent so no observer can pick the wrong field.

- **`profile_id`**: `String`

  Slicer preset profile ID. Always `"0"` — confirmed against bambuddy and pybambu, both
  of which hardcode this value; no observed non-zero case.

- **`project_id`**: `String`

  Per-submission project tracking ID. Set equal to `subtask_id` — bambuddy's
  `send_start_print_command` (`bambu_mqtt.py:3721-3781`) mints one fresh ID per
  submission and reuses it for `subtask_id`/`project_id`/`task_id` alike; bambino's
  `subtask_id` already carries the same "fresh per submission" contract via its own doc
  comment, so reusing it here satisfies the same invariant bambuddy's fix relies on
  (avoiding the task-continuation firmware bug, #1042/#1011) without inventing a second
  ID-minting mechanism.

- **`task_id`**: `String`

  Per-submission task tracking ID. See `project_id`'s doc comment — same value, same
  reasoning.

- **`file`**: `String`

  Sliced compilation container file path residing on the SD card (e.g., "job.3mf").

- **`url`**: `String`

  Connection endpoint directory scheme (Must use `ftp://` for local loopback parsing) [REF-MQTT-LIFECYCLE].

- **`timelapse`**: `bool`

  Whether timelapse capture is enabled for this job.

- **`bed_type`**: `String`

  Bed plate type used for the print (e.g. "textured", "smooth").

- **`bed_leveling`**: `bool`

  Whether to run automatic bed leveling before the print. `true` only for `CalibrationMode::On`
  — `Auto` is carried by the companion `auto_bed_leveling` int, not by setting this `true`.

- **`auto_bed_leveling`**: `i32`

  Tri-state companion to `bed_leveling`: `0`=off, `1`=on, `2`=auto (skip if leveled recently).
  bed_leveling itself must stay a strict JSON bool on every model — real captures showed
  integer-encoding it disrupts flow calibration on H2S (see reference/03_mqtt_telemetry.md);
  this separate int field is how BambuStudio expresses Auto instead
  (`bambu_networking.hpp`'s `auto_bed_leveling` member, confirmed against bambuddy's wire capture).

- **`extrude_cali_flag`**: `i32`

  Controls dynamic flow calibration: `0`=off, `1`=on, `2`=auto (skip if calibrated recently).

- **`nozzle_offset_cali`**: `i32`

  Active nozzle offset verification flag (Used primarily on IDEX and tool-changers):
  `0`=off, `1`=on, `2`=auto (skip if calibrated recently).

- **`vibration_cali`**: `bool`

  Whether vibration compensation calibration ran as part of this job.

- **`layer_inspect`**: `bool`

  Whether layer inspection (first-layer scan) ran as part of this job.

- **`use_ams`**: `bool`

  Triggers physical AMS multiplexer material routing. Must strictly be serialized as a boolean.

- **`ams_mapping`**: `AmsMappingTable`

  Polymorphic representation enforcing empty strings on external spools vs integer arrays on standard channels.

- **`ams_mapping2`**: `Option<Vec<crate::ams::mapping::AmsMapping2Entry>>`

  Structured sub-mappings for advanced material and multi-AMS routing.

#### Trait Implementations

##### `impl Clone for ProjectFilePayload`

- <span id="projectfilepayload-clone"></span>`fn clone(&self) -> ProjectFilePayload` — [`ProjectFilePayload`](#projectfilepayload)

##### `impl Debug for ProjectFilePayload`

- <span id="projectfilepayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ProjectFilePayload`

- <span id="projectfilepayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ProjectFileRequest`

```rust
struct ProjectFileRequest {
    pub print: ProjectFilePayload,
}
```

Submits a `.3mf` print job from the SD card for execution.

#### Fields

- **`print`**: `ProjectFilePayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="projectfilerequest-from-config"></span>`fn from_config(config: &PrintJobConfig, sequence_id: impl Into<ClampedTaskId>, model: PrinterModel) -> Self` — [`PrintJobConfig`](#printjobconfig), [`ClampedTaskId`](../index.md#clampedtaskid), [`PrinterModel`](../../../models/index.md#printermodel)

  Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.

#### Trait Implementations

##### `impl Clone for ProjectFileRequest`

- <span id="projectfilerequest-clone"></span>`fn clone(&self) -> ProjectFileRequest` — [`ProjectFileRequest`](#projectfilerequest)

##### `impl Debug for ProjectFileRequest`

- <span id="projectfilerequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ProjectFileRequest`

- <span id="projectfilerequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsMappingTable`

```rust
enum AmsMappingTable {
    Inactive(String),
    Active(Vec<i32>),
}
```

Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

**The Polymorphic Mapping Rule:**
* When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
* When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).

Utilizing an untagged enum ensures standard JSON compliance across all execution profiles.

#### Variants

- **`Inactive`**

  External-spool mode: serializes to an empty string.

- **`Active`**

  AMS active mode: serializes to an integer slot-mapping array.

#### Trait Implementations

##### `impl Clone for AmsMappingTable`

- <span id="amsmappingtable-clone"></span>`fn clone(&self) -> AmsMappingTable` — [`AmsMappingTable`](#amsmappingtable)

##### `impl Debug for AmsMappingTable`

- <span id="amsmappingtable-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsMappingTable`

##### `impl PartialEq for AmsMappingTable`

- <span id="amsmappingtable-partialeq-eq"></span>`fn eq(&self, other: &AmsMappingTable) -> bool` — [`AmsMappingTable`](#amsmappingtable)

##### `impl Serialize for AmsMappingTable`

- <span id="amsmappingtable-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="calibrationmode-clone"></span>`fn clone(&self) -> CalibrationMode` — [`CalibrationMode`](#calibrationmode)

##### `impl Copy for CalibrationMode`

##### `impl Debug for CalibrationMode`

- <span id="calibrationmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for CalibrationMode`

- <span id="calibrationmode-default"></span>`fn default() -> CalibrationMode` — [`CalibrationMode`](#calibrationmode)

##### `impl Eq for CalibrationMode`

##### `impl PartialEq for CalibrationMode`

- <span id="calibrationmode-partialeq-eq"></span>`fn eq(&self, other: &CalibrationMode) -> bool` — [`CalibrationMode`](#calibrationmode)

