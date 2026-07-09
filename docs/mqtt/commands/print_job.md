**bambino > mqtt > commands > print_job**

# Module: mqtt::commands::print_job

## Contents

**Structs**

- [`PrintJobConfig`](#printjobconfig) - Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].
- [`ProjectFilePayload`](#projectfilepayload) - Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.
- [`ProjectFileRequest`](#projectfilerequest) - Submits a `.3mf` print job from the SD card for execution.

**Enums**

- [`AmsMappingTable`](#amsmappingtable) - Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

---

## bambino::mqtt::commands::print_job::AmsMappingTable

*Enum*

Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

**The Polymorphic Mapping Rule:**
* When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
* When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).

Utilizing an untagged enum ensures standard JSON compliance across all execution profiles.

**Variants:**
- `Inactive(String)` - External-spool mode: serializes to an empty string.
- `Active(Vec<i32>)` - AMS active mode: serializes to an integer slot-mapping array.

**Traits:** Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsMappingTable`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &AmsMappingTable) -> bool`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::print_job::PrintJobConfig

*Struct*

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
with named fields and sensible defaults for calibration flags.

**Fields:**
- `job_filename: String` - Filename of the `.3mf` file on SD card storage (e.g. "job.3mf").
- `plate_gcode_path: String` - Sliced plate gcode path inside the `.3mf` (e.g. "Metadata/plate_1.gcode").
- `subtask_name: String` - User-friendly label for the print queue task.
- `raw_subtask_id: u64` - Unique 32-bit tracking identifier before clamping (see `clamp_task_id`).
- `bed_type: String` - Bed plate type (e.g. "textured", "smooth").
- `bed_leveling: bool` - Whether to run automatic bed leveling before the print.
- `run_flow_calibration: bool` - Whether to run dynamic flow calibration before the print.
- `run_vibration_compensation: bool` - Whether to run vibration compensation calibration before the print.
- `timelapse: bool` - Whether timelapse capture is enabled.
- `layer_inspect: bool` - Whether to run first-layer inspection during the print.
- `nozzle_offset_cali: Option<bool>` - `None` defers to the quirks engine default in `PrinterClient::start_print()`.
- `use_ams: bool` - Whether to route filament through the AMS rather than an external spool.
- `ams_mapping: Vec<i32>` - Flat AMS slot mapping (one entry per plate object, -1 = no AMS slot).
- `ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>` - Structured per-nozzle AMS mapping; takes precedence over `ams_mapping` when set.

**Methods:**

- `fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self` - Builds a job config with calibration flags defaulted on and AMS disabled.
- `fn with_ams(self: Self, mapping: Vec<i32>) -> Self` - Enables AMS and sets the flat slot-mapping array (`ams_mapping`).
- `fn with_ams_mapping2(self: Self, mapping2: Vec<AmsMapping2Entry>) -> Self` - Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`).
- `fn bed_leveling(self: Self, enabled: bool) -> Self` - Enables or disables automatic bed leveling for this job.
- `fn flow_calibration(self: Self, enabled: bool) -> Self` - Enables or disables flow calibration for this job.
- `fn vibration_compensation(self: Self, enabled: bool) -> Self` - Enables or disables vibration compensation calibration for this job.
- `fn timelapse(self: Self, enabled: bool) -> Self` - Enables or disables timelapse capture for this job.
- `fn layer_inspect(self: Self, enabled: bool) -> Self` - Enables or disables first-layer inspection for this job.
- `fn nozzle_offset_calibration(self: Self, enabled: bool) -> Self` - Overrides the model's default nozzle-offset-calibration behavior for this job.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> PrintJobConfig`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::print_job::ProjectFilePayload

*Struct*

Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.

**Fields:**
- `command: &'static str` - Wire command name, always `"project_file"`.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.
- `param: String` - Target file path of the internal sliced plate payload (e.g. "Metadata/plate_1.gcode").
- `subtask_name: String` - User-friendly label associated with the print queue task.
- `subtask_id: String` - Unique 32-bit tracking identifier (Clamped to prevent overflow lockups).
- `file: String` - Sliced compilation container file path residing on the SD card (e.g., "job.3mf").
- `url: String` - Connection endpoint directory scheme (Must use `ftp://` for local loopback parsing) [REF-MQTT-LIFECYCLE].
- `timelapse: bool` - Whether timelapse capture is enabled for this job.
- `bed_type: String` - Bed plate type used for the print (e.g. "textured", "smooth").
- `bed_leveling: bool` - Whether to run automatic bed leveling before the print.
- `extrude_cali_flag: i32` - Controls dynamic flow calibration. Expressed as an integer: `1` for active, `0` for bypass.
- `nozzle_offset_cali: i32` - Active nozzle offset verification flag (Used primarily on IDEX and tool-changers).
- `vibration_cali: bool` - Whether vibration compensation calibration ran as part of this job.
- `layer_inspect: bool` - Whether layer inspection (first-layer scan) ran as part of this job.
- `use_ams: bool` - Triggers physical AMS multiplexer material routing. Must strictly be serialized as a boolean.
- `ams_mapping: AmsMappingTable` - Polymorphic representation enforcing empty strings on external spools vs integer arrays on standard channels.
- `ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>` - Structured sub-mappings for advanced material and multi-AMS routing.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ProjectFilePayload`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::print_job::ProjectFileRequest

*Struct*

Submits a `.3mf` print job from the SD card for execution.

**Fields:**
- `print: ProjectFilePayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn from_config(config: &PrintJobConfig, sequence_id: u64, model: BambuModel) -> Self` - Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ProjectFileRequest`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



