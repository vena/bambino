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
- `Inactive(String)`
- `Active(Vec<i32>)`

**Traits:** Eq

**Trait Implementations:**

- **PartialEq**
  - `fn eq(self: &Self, other: &AmsMappingTable) -> bool`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsMappingTable`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::print_job::PrintJobConfig

*Struct*

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
with named fields and sensible defaults for calibration flags.

**Fields:**
- `job_filename: String`
- `plate_gcode_path: String`
- `subtask_name: String`
- `raw_subtask_id: u64`
- `bed_type: String`
- `bed_leveling: bool`
- `run_flow_calibration: bool`
- `run_vibration_compensation: bool`
- `timelapse: bool`
- `layer_inspect: bool`
- `nozzle_offset_cali: Option<bool>` - `None` defers to the quirks engine default in `PrinterClient::start_print()`.
- `use_ams: bool`
- `ams_mapping: Vec<i32>`
- `ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>`

**Methods:**

- `fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self`
- `fn with_ams(self: Self, mapping: Vec<i32>) -> Self`
- `fn with_ams_mapping2(self: Self, mapping2: Vec<AmsMapping2Entry>) -> Self`
- `fn bed_leveling(self: Self, enabled: bool) -> Self`
- `fn flow_calibration(self: Self, enabled: bool) -> Self`
- `fn vibration_compensation(self: Self, enabled: bool) -> Self`
- `fn timelapse(self: Self, enabled: bool) -> Self`
- `fn layer_inspect(self: Self, enabled: bool) -> Self`
- `fn nozzle_offset_calibration(self: Self, enabled: bool) -> Self`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> PrintJobConfig`



## bambino::mqtt::commands::print_job::ProjectFilePayload

*Struct*

Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.

**Fields:**
- `command: &'static str`
- `sequence_id: String`
- `param: String` - Target file path of the internal sliced plate payload (e.g. "Metadata/plate_1.gcode").
- `subtask_name: String` - User-friendly label associated with the print queue task.
- `subtask_id: String` - Unique 32-bit tracking identifier (Clamped to prevent overflow lockups).
- `file: String` - Sliced compilation container file path residing on the SD card (e.g., "job.3mf").
- `url: String` - Connection endpoint directory scheme (Must use `ftp://` for local loopback parsing) [REF-MQTT-LIFECYCLE].
- `timelapse: bool`
- `bed_type: String`
- `bed_leveling: bool`
- `extrude_cali_flag: i32` - Controls dynamic flow calibration. Expressed as an integer: `1` for active, `0` for bypass.
- `nozzle_offset_cali: i32` - Active nozzle offset verification flag (Used primarily on IDEX and tool-changers).
- `vibration_cali: bool`
- `layer_inspect: bool`
- `use_ams: bool` - Triggers physical AMS multiplexer material routing. Must strictly be serialized as a boolean.
- `ams_mapping: AmsMappingTable` - Polymorphic representation enforcing empty strings on external spools vs integer arrays on standard channels.
- `ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>` - Structured sub-mappings for advanced material and multi-AMS routing.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ProjectFilePayload`



## bambino::mqtt::commands::print_job::ProjectFileRequest

*Struct*

Submits a `.3mf` print job from the SD card for execution.

**Fields:**
- `print: ProjectFilePayload`

**Methods:**

- `fn from_config(config: &PrintJobConfig, sequence_id: u64, model: BambuModel) -> Self` - Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ProjectFileRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



