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
| [`resolve_rack_nozzle_mapping`](#resolve-rack-nozzle-mapping) | fn | Translates a per-slot extruder mapping into an H2C `nozzle_mapping` of physical nozzle IDs. |

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
    pub run_vibration_compensation: CalibrationMode,
    pub timelapse: bool,
    pub layer_inspect: bool,
    pub nozzle_offset_cali: Option<CalibrationMode>,
    pub use_ams: bool,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>,
    pub nozzle_slot_extruders: Option<Vec<i32>>,
    pub rack_nozzle_id: Option<i32>,
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

- **`run_vibration_compensation`**: `CalibrationMode`

  Whether to run vibration compensation calibration before the print. No tri-state
  companion field exists on the wire for this one (`reference/03_mqtt_telemetry.md:334`),
  so `Auto` serializes identically to `Off` via `as_wire_bool()`.

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

- **`nozzle_slot_extruders`**: `Option<Vec<i32>>`

  Extruder index per filament slot for tool-changer models, negative for unprinted slots.
  
  Only consulted on a model whose quirks report [`uses_nozzle_rack`]. Set together with
  `rack_nozzle_id` via [`PrintJobConfig::with_nozzle_rack`]; either one alone resolves to no
  `nozzle_mapping` on the wire, which is the safe outcome.

- **`rack_nozzle_id`**: `Option<i32>`

  Physical nozzle ID of the rack position the printer currently reports as live (`16..=21`).
  
  The caller must supply this because the mounted hotend can change between slicing and
  dispatch, and bambino does not model rack telemetry.

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

- <span id="printjobconfig-vibration-compensation"></span>`fn vibration_compensation(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](#calibrationmode)

  Enables or disables vibration compensation calibration for this job. No tri-state

  companion field exists on the wire for this one, so `CalibrationMode::Auto` serializes

  identically to `Off`.

- <span id="printjobconfig-timelapse"></span>`fn timelapse(self, enabled: bool) -> Self`

  Enables or disables timelapse capture for this job.

- <span id="printjobconfig-layer-inspect"></span>`fn layer_inspect(self, enabled: bool) -> Self`

  Enables or disables first-layer inspection for this job.

- <span id="printjobconfig-with-nozzle-rack"></span>`fn with_nozzle_rack(self, slot_extruders: Vec<i32>, rack_nozzle_id: i32) -> Self`

  Supplies the tool-changer rack routing for this job (H2C only).

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
    pub nozzle_mapping: Option<Vec<i32>>,
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

- **`nozzle_mapping`**: `Option<Vec<i32>>`

  Physical nozzle ID per filament slot on tool-changer models, `-1` for unprinted slots.
  
  Omitted from the wire entirely when `None`, which is the case for every non-rack model and
  for a rack model whose routing could not be resolved with confidence — firmware then picks
  the nozzle itself. See [`resolve_rack_nozzle_mapping`](#resolve-rack-nozzle-mapping) for why omission beats guessing.

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

Returns a [`RACK_WIRE_SLOTS`]-long vector of physical IDs, or `None` when the mapping cannot
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

