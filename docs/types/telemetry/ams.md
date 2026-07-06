**bambino > types > telemetry > ams**

# Module: types::telemetry::ams

## Contents

**Structs**

- [`AmsDrySetting`](#amsdrysetting) - Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER].
- [`AmsStatusReport`](#amsstatusreport) - Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE].
- [`AmsTray`](#amstray) - Material spool state descriptor representing a single physical tray slot.
- [`AmsUnit`](#amsunit) - Modular standard expansion unit managing up to 4 physical spool slots.
- [`VirtualTray`](#virtualtray) - Virtual/external spool holder telemetry. Represents the filament loaded

---

## bambino::types::telemetry::ams::AmsDrySetting

*Struct*

Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER].

**Fields:**
- `dry_temperature: Option<i32>` - Target drying temperature in degrees Celsius.
- `dry_duration: Option<i32>` - Configured drying duration in minutes.
- `dry_filament: Option<String>` - Filament type string for the active drying profile (e.g. "PA-CF").

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> AmsDrySetting`



## bambino::types::telemetry::ams::AmsStatusReport

*Struct*

Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE].

On the wire, AMS telemetry is nested as `print.ams.ams[...]` — this struct represents
the intermediate `print.ams` object.

**Fields:**
- `ams: Vec<AmsUnit>` - Array of connected AMS units on the expansion bus.
- `ams_exist_bits: Option<String>` - Hexadecimal bitmask string indicating which AMS units are physically present.
- `tray_exist_bits: Option<String>` - Hexadecimal bitmask string indicating which tray slots contain a physical spool.
- `tray_is_bbl_bits: Option<String>` - Hexadecimal bitmask string indicating which trays contain Bambu Lab branded spools.
- `tray_now: Option<String>` - Index of the currently active tray feeding filament to the toolhead.
- `tray_pre: Option<String>` - Index of the previously active tray.
- `tray_tar: Option<String>` - Target tray index.
- `version: Option<i32>` - AMS protocol version.
- `tray_read_done_bits: Option<String>` - RFID read completion bitmask (hex string).
- `tray_reading_bits: Option<String>` - Active RFID read bitmask (hex string).
- `insert_flag: Option<bool>` - AMS insertion event flag.
- `power_on_flag: Option<bool>` - AMS unit external power state (distinct from printer power; AMS Pro needs external power for drying).
- `cali_id: Option<i32>` - Calibration tracking ID.
- `cali_stat: Option<i32>` - Calibration tracking status.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsStatusReport`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## bambino::types::telemetry::ams::AmsTray

*Struct*

Material spool state descriptor representing a single physical tray slot.

On the wire, AMS trays and virtual/external trays (`vt_tray`, `vir_slot`)
share the same field schema. All descriptive fields are optional — under
standard P1/A1 firmware, removing a spool truncates the JSON to only the ID key.

**Fields:**
- `id: String` - The physical index representing the slot (0 to 3). Sent as a string on the wire.
- `state: Option<u8>` - The native state code representing filament routing status [REF-AMS-DECODE].
- `tray_type: Option<String>` - Material class abbreviation (e.g. "PLA", "PETG", "PA-CF").
- `tray_color: Option<String>` - RRGGBBAA hexadecimal color string defining the filament profile.
- `tray_info_idx: Option<String>` - Short or unique customized preset index matching slicer calibrations.
- `tag_uid: Option<String>` - 16-character hexadecimal RFID tag UID, if reading a native spool.
- `tray_uuid: Option<String>` - 32-character globally unique ID of the filament spool.
- `remain: Option<i32>` - Remaining filament volume percentage (or -1 if uncalculated).
- `tray_sub_brands: Option<String>` - Sub-brand or variant string (e.g. "PLA Matte", "Support for PLA").
- `nozzle_temp_max: Option<String>` - Maximum nozzle temperature for the loaded filament (sent as string).
- `nozzle_temp_min: Option<String>` - Minimum nozzle temperature for the loaded filament (sent as string).
- `tray_diameter: Option<String>` - Filament diameter in mm (sent as string, e.g. `"1.75"`).
- `tray_weight: Option<String>` - Spool net weight in grams (sent as string).
- `tray_id_name: Option<String>` - Filament preset display name (e.g. "S02-W0", "A01-K1").
- `tray_temp: Option<String>` - Filament drying temperature (sent as string). Newer firmware uses `drying_temp`.
- `tray_time: Option<String>` - Filament drying time (sent as string). Newer firmware uses `drying_time`.
- `drying_temp: Option<String>` - Drying temperature on newer firmware (alias for `tray_temp`).
- `drying_time: Option<String>` - Drying time on newer firmware (alias for `tray_time`).
- `bed_temp: Option<String>` - Per-tray bed temperature setting (sent as string).
- `bed_temp_type: Option<String>` - Bed temperature type/profile (sent as string).
- `xcam_info: Option<String>` - XCam inspection info hex string.
- `k: Option<f64>` - Flow rate calibration K factor.
- `n: Option<i32>` - Flow rate calibration N factor.
- `cali_idx: Option<i32>` - Calibration index (-1 if uncalibrated).
- `cols: Option<Vec<String>>` - Multi-color columns array (e.g. `["000000FF"]`).
- `ctype: Option<i32>` - Color type indicator.
- `total_len: Option<u32>` - Total filament spool length in mm.

**Methods:**

- `fn get_state(self: &Self) -> u8` - Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.

**Trait Implementations:**

- **Default**
  - `fn default() -> AmsTray`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> AmsTray`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## bambino::types::telemetry::ams::AmsUnit

*Struct*

Modular standard expansion unit managing up to 4 physical spool slots.

**Fields:**
- `id: String` - Unique index representing the unit position on the physical expansion bus (0 to 3).
- `temp: String` - Ambient temperature inside the expansion enclosure, in degrees Celsius.
- `humidity: String` - Enclosure climate relative humidity index (1-5 scale).
- `humidity_raw: Option<String>` - Actual relative humidity percentage (1-100) from the onboard sensor.
- `dry_time: Option<u32>` - Remaining drying time in minutes during an active dry cycle [REF-AMS-DRYER].
- `dry_setting: Option<AmsDrySetting>` - Drying configuration settings (target temperature, duration, filament type).
- `tray: Vec<AmsTray>` - Trays / spool slots configured inside the designated unit.
- `info: Option<String>` - Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status,
- `dry_sf_reason: Option<Vec<i32>>` - Drying failure reason codes per slot (X2D).

**Methods:**

- `fn parse_info(self: &Self) -> Option<u64>` - Parses the hex-encoded `info` bitmask string into an integer.
- `fn ams_type(self: &Self) -> Option<u8>` - AMS unit type from bits 0–3 (e.g. 3 = AMS Lite).
- `fn dry_status(self: &Self) -> Option<u8>` - Drying status from bits 4–7.
- `fn extruder_assignment(self: &Self) -> Option<u8>` - Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).
- `fn dry_sub_status(self: &Self) -> Option<u8>` - Drying sub-status from bits 22–25.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsUnit`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## bambino::types::telemetry::ams::VirtualTray

*Struct*

Virtual/external spool holder telemetry. Represents the filament loaded
directly into the extruder without going through an AMS unit.

On the wire, this shares the same schema as `AmsTray` — both physical AMS trays
and virtual/external spool holders use the same field set.

**Fields:**
- `id: Option<String>` - Virtual tray ID (typically `"254"`).
- `tray_type: Option<String>` - Material class abbreviation (e.g. "PLA", "PETG"). Empty when no filament loaded.
- `tray_color: Option<String>` - RRGGBBAA hexadecimal color string.
- `tray_info_idx: Option<String>` - Slicer filament preset index.
- `tray_sub_brands: Option<String>` - Sub-brand or variant string.
- `nozzle_temp_max: Option<String>` - Maximum nozzle temperature for the loaded filament (sent as string).
- `nozzle_temp_min: Option<String>` - Minimum nozzle temperature for the loaded filament (sent as string).
- `tray_diameter: Option<String>` - Filament diameter in mm (sent as string, e.g. `"1.75"`).
- `tray_weight: Option<String>` - Spool net weight in grams (sent as string).
- `tray_temp: Option<String>` - Filament temperature setting (sent as string).
- `tray_time: Option<String>` - Filament print time accumulator (sent as string).
- `bed_temp: Option<String>` - Bed temperature setting (sent as string).
- `bed_temp_type: Option<String>` - Bed temperature type/profile (sent as string).
- `tag_uid: Option<String>` - 16-character hexadecimal RFID tag UID.
- `tray_uuid: Option<String>` - 32-character globally unique filament spool ID.
- `tray_id_name: Option<String>` - Filament preset display name.
- `xcam_info: Option<String>` - XCam inspection info hex string.
- `remain: Option<i32>` - Remaining filament percentage (0–100, or 0 if unknown).
- `k: Option<f64>` - Flow rate calibration K factor.
- `n: Option<i32>` - Flow rate calibration N factor.
- `cali_idx: Option<i32>` - Calibration index (-1 if uncalibrated).

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> VirtualTray`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



