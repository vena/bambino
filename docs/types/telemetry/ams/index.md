*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [ams](index.md)*

---

# Module `ams`

AMS telemetry types (tray slots, units, dry settings, virtual trays).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsDrySetting`](#amsdrysetting) | struct | Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER]. |
| [`AmsStatusReport`](#amsstatusreport) | struct | Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE]. |
| [`AmsTray`](#amstray) | struct | Material spool state descriptor representing a single physical tray slot. |
| [`AmsUnit`](#amsunit) | struct | Modular standard expansion unit managing up to 4 physical spool slots. |
| [`VirtualTray`](#virtualtray) | struct | Virtual/external spool holder telemetry. |

## Types

### `AmsDrySetting`

```rust
struct AmsDrySetting {
    pub dry_temperature: Option<i32>,
    pub dry_duration: Option<i32>,
    pub dry_filament: Option<String>,
}
```

Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER].

#### Fields

- **`dry_temperature`**: `Option<i32>`

  Target drying temperature in degrees Celsius.

- **`dry_duration`**: `Option<i32>`

  Configured drying duration in minutes.

- **`dry_filament`**: `Option<String>`

  Filament type string for the active drying profile (e.g. "PA-CF").

#### Trait Implementations

##### `impl Clone for AmsDrySetting`

- <span id="amsdrysetting-clone"></span>`fn clone(&self) -> AmsDrySetting` — [`AmsDrySetting`](#amsdrysetting)

##### `impl Debug for AmsDrySetting`

- <span id="amsdrysetting-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsDrySetting`

- <span id="amsdrysetting-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsDrySetting`

##### `impl Serialize for AmsDrySetting`

- <span id="amsdrysetting-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsStatusReport`

```rust
struct AmsStatusReport {
    pub ams: Vec<AmsUnit>,
    pub ams_exist_bits: Option<String>,
    pub tray_exist_bits: Option<String>,
    pub tray_is_bbl_bits: Option<String>,
    pub tray_now: Option<String>,
    pub tray_pre: Option<String>,
    pub tray_tar: Option<String>,
    pub version: Option<i32>,
    pub tray_read_done_bits: Option<String>,
    pub tray_reading_bits: Option<String>,
    pub insert_flag: Option<bool>,
    pub power_on_flag: Option<bool>,
    pub cali_id: Option<i32>,
    pub cali_stat: Option<i32>,
}
```

Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE].

On the wire, AMS telemetry is nested as `print.ams.ams[...]` — this struct represents
the intermediate `print.ams` object.

#### Fields

- **`ams`**: `Vec<AmsUnit>`

  Array of connected AMS units on the expansion bus.

- **`ams_exist_bits`**: `Option<String>`

  Hexadecimal bitmask string indicating which AMS units are physically present.

- **`tray_exist_bits`**: `Option<String>`

  Hexadecimal bitmask string indicating which tray slots contain a physical spool.

- **`tray_is_bbl_bits`**: `Option<String>`

  Hexadecimal bitmask string indicating which trays contain Bambu Lab branded spools.

- **`tray_now`**: `Option<String>`

  Index of the currently active tray feeding filament to the toolhead.

- **`tray_pre`**: `Option<String>`

  Index of the previously active tray.

- **`tray_tar`**: `Option<String>`

  Target tray index.

- **`version`**: `Option<i32>`

  AMS protocol version.

- **`tray_read_done_bits`**: `Option<String>`

  RFID read completion bitmask (hex string).

- **`tray_reading_bits`**: `Option<String>`

  Active RFID read bitmask (hex string).

- **`insert_flag`**: `Option<bool>`

  AMS insertion event flag.

- **`power_on_flag`**: `Option<bool>`

  AMS unit external power state (distinct from printer power; AMS Pro needs external power for drying).

- **`cali_id`**: `Option<i32>`

  Calibration tracking ID.

- **`cali_stat`**: `Option<i32>`

  Calibration tracking status.

#### Trait Implementations

##### `impl Clone for AmsStatusReport`

- <span id="amsstatusreport-clone"></span>`fn clone(&self) -> AmsStatusReport` — [`AmsStatusReport`](#amsstatusreport)

##### `impl Debug for AmsStatusReport`

- <span id="amsstatusreport-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsStatusReport`

- <span id="amsstatusreport-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsStatusReport`

##### `impl Serialize for AmsStatusReport`

- <span id="amsstatusreport-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsTray`

```rust
struct AmsTray {
    pub id: String,
    pub state: Option<u8>,
    pub tray_type: Option<String>,
    pub tray_color: Option<String>,
    pub tray_info_idx: Option<String>,
    pub tag_uid: Option<String>,
    pub tray_uuid: Option<String>,
    pub remain: Option<i32>,
    pub tray_sub_brands: Option<String>,
    pub nozzle_temp_max: Option<String>,
    pub nozzle_temp_min: Option<String>,
    pub tray_diameter: Option<String>,
    pub tray_weight: Option<String>,
    pub tray_id_name: Option<String>,
    pub tray_temp: Option<String>,
    pub tray_time: Option<String>,
    pub drying_temp: Option<String>,
    pub drying_time: Option<String>,
    pub bed_temp: Option<String>,
    pub bed_temp_type: Option<String>,
    pub xcam_info: Option<String>,
    pub k: Option<f64>,
    pub n: Option<i32>,
    pub cali_idx: Option<i32>,
    pub cols: Option<Vec<String>>,
    pub ctype: Option<i32>,
    pub total_len: Option<u32>,
}
```

Material spool state descriptor representing a single physical tray slot.

On the wire, AMS trays and virtual/external trays (`vt_tray`, `vir_slot`)
share the same field schema. All descriptive fields are optional — under
standard P1/A1 firmware, removing a spool truncates the JSON to only the ID key.

#### Fields

- **`id`**: `String`

  The physical index representing the slot (0 to 3). Sent as a string on the wire.

- **`state`**: `Option<u8>`

  The native state code representing filament routing status [REF-AMS-DECODE].

- **`tray_type`**: `Option<String>`

  Material class abbreviation (e.g. "PLA", "PETG", "PA-CF").

- **`tray_color`**: `Option<String>`

  RRGGBBAA hexadecimal color string defining the filament profile.

- **`tray_info_idx`**: `Option<String>`

  Short or unique customized preset index matching slicer calibrations.

- **`tag_uid`**: `Option<String>`

  16-character hexadecimal RFID tag UID, if reading a native spool.

- **`tray_uuid`**: `Option<String>`

  32-character globally unique ID of the filament spool.

- **`remain`**: `Option<i32>`

  Remaining filament volume percentage (or -1 if uncalculated).

- **`tray_sub_brands`**: `Option<String>`

  Sub-brand or variant string (e.g. "PLA Matte", "Support for PLA").

- **`nozzle_temp_max`**: `Option<String>`

  Maximum nozzle temperature for the loaded filament (sent as string).

- **`nozzle_temp_min`**: `Option<String>`

  Minimum nozzle temperature for the loaded filament (sent as string).

- **`tray_diameter`**: `Option<String>`

  Filament diameter in mm (sent as string, e.g. `"1.75"`).

- **`tray_weight`**: `Option<String>`

  Spool net weight in grams (sent as string).

- **`tray_id_name`**: `Option<String>`

  Filament preset display name (e.g. "S02-W0", "A01-K1").

- **`tray_temp`**: `Option<String>`

  Filament drying temperature (sent as string). Newer firmware uses `drying_temp`.

- **`tray_time`**: `Option<String>`

  Filament drying time (sent as string). Newer firmware uses `drying_time`.

- **`drying_temp`**: `Option<String>`

  Drying temperature on newer firmware (alias for `tray_temp`).

- **`drying_time`**: `Option<String>`

  Drying time on newer firmware (alias for `tray_time`).

- **`bed_temp`**: `Option<String>`

  Per-tray bed temperature setting (sent as string).

- **`bed_temp_type`**: `Option<String>`

  Bed temperature type/profile (sent as string).

- **`xcam_info`**: `Option<String>`

  XCam inspection info hex string.

- **`k`**: `Option<f64>`

  Flow rate calibration K factor.

- **`n`**: `Option<i32>`

  Flow rate calibration N factor.

- **`cali_idx`**: `Option<i32>`

  Calibration index (-1 if uncalibrated).

- **`cols`**: `Option<Vec<String>>`

  Multi-color columns array (e.g. `["000000FF"]`).

- **`ctype`**: `Option<i32>`

  Color type indicator.

- **`total_len`**: `Option<u32>`

  Total filament spool length in mm.

#### Implementations

- <span id="amstray-get-state"></span>`fn get_state(&self) -> u8`

  Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.

#### Trait Implementations

##### `impl Clone for AmsTray`

- <span id="amstray-clone"></span>`fn clone(&self) -> AmsTray` — [`AmsTray`](#amstray)

##### `impl Debug for AmsTray`

- <span id="amstray-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for AmsTray`

- <span id="amstray-default"></span>`fn default() -> AmsTray` — [`AmsTray`](#amstray)

##### `impl Deserialize<'de> for AmsTray`

- <span id="amstray-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsTray`

##### `impl Serialize for AmsTray`

- <span id="amstray-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsUnit`

```rust
struct AmsUnit {
    pub id: String,
    pub temp: String,
    pub humidity: String,
    pub humidity_raw: Option<String>,
    pub dry_time: Option<u32>,
    pub dry_setting: Option<AmsDrySetting>,
    pub tray: Option<Vec<AmsTray>>,
    pub info: Option<String>,
    pub dry_sf_reason: Option<Vec<i32>>,
}
```

Modular standard expansion unit managing up to 4 physical spool slots.

#### Fields

- **`id`**: `String`

  Unique index representing the unit position on the physical expansion bus (0 to 3).

- **`temp`**: `String`

  Ambient temperature inside the expansion enclosure, in degrees Celsius.

- **`humidity`**: `String`

  Enclosure climate relative humidity index (1-5 scale).

- **`humidity_raw`**: `Option<String>`

  Actual relative humidity percentage (1-100) from the onboard sensor.
  Sent as a string on the wire (e.g., `"17"`).

- **`dry_time`**: `Option<u32>`

  Remaining drying time in minutes during an active dry cycle [REF-AMS-DRYER].
  Sent as an integer on the wire but may vary by firmware.

- **`dry_setting`**: `Option<AmsDrySetting>`

  Drying configuration settings (target temperature, duration, filament type).

- **`tray`**: `Option<Vec<AmsTray>>`

  Trays / spool slots configured inside the designated unit.
  
  `None` means this push's `tray` key was absent from the wire — leave previously
  cached trays untouched. `Some(vec![])` means the key was present but empty, which
  (per `AmsUnit::merge_from`) prunes every cached tray for this unit — bambino's
  `#[serde(default)]` on `Option<Vec<_>>` gives exactly this absent-vs-present-empty
  distinction for free (absent key -> `None` via `Default`, present key -> `Some(_)`
  however short), confirmed against BambuStudio's `DevFilaSystem.cpp`
  (`ParseAmsInfo`'s `if (j_ams.contains("tray"))` gate around both the per-tray parse
  loop and the prune-absent-ids loop).

- **`info`**: `Option<String>`

  Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status, bits 8–11 = extruder assignment (IDEX routing).

- **`dry_sf_reason`**: `Option<Vec<i32>>`

  Drying failure reason codes per slot (X2D).

#### Implementations

- <span id="amsunit-parse-info"></span>`fn parse_info(&self) -> Option<u64>`

  Parses the hex-encoded `info` bitmask string into an integer.

- <span id="amsunit-ams-type"></span>`fn ams_type(&self) -> Option<u8>`

  AMS unit type from bits 0–3 (e.g. 3 = AMS Lite).

- <span id="amsunit-dry-status"></span>`fn dry_status(&self) -> Option<u8>`

  Drying status from bits 4–7.

- <span id="amsunit-extruder-assignment"></span>`fn extruder_assignment(&self) -> Option<u8>`

  Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).

  Returns `None` when `info` is absent or the value is 0xE (uninitialized).

- <span id="amsunit-dry-sub-status"></span>`fn dry_sub_status(&self) -> Option<u8>`

  Drying sub-status from bits 22–25.

#### Trait Implementations

##### `impl Clone for AmsUnit`

- <span id="amsunit-clone"></span>`fn clone(&self) -> AmsUnit` — [`AmsUnit`](#amsunit)

##### `impl Debug for AmsUnit`

- <span id="amsunit-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsUnit`

- <span id="amsunit-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsUnit`

##### `impl Serialize for AmsUnit`

- <span id="amsunit-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `VirtualTray`

```rust
struct VirtualTray {
    pub id: Option<String>,
    pub tray_type: Option<String>,
    pub tray_color: Option<String>,
    pub tray_info_idx: Option<String>,
    pub tray_sub_brands: Option<String>,
    pub nozzle_temp_max: Option<String>,
    pub nozzle_temp_min: Option<String>,
    pub tray_diameter: Option<String>,
    pub tray_weight: Option<String>,
    pub tray_temp: Option<String>,
    pub tray_time: Option<String>,
    pub bed_temp: Option<String>,
    pub bed_temp_type: Option<String>,
    pub tag_uid: Option<String>,
    pub tray_uuid: Option<String>,
    pub tray_id_name: Option<String>,
    pub xcam_info: Option<String>,
    pub remain: Option<i32>,
    pub k: Option<f64>,
    pub n: Option<i32>,
    pub cali_idx: Option<i32>,
}
```

Virtual/external spool holder telemetry.
Represents the filament loaded directly into the extruder without going through an AMS unit.

On the wire, this shares the same schema as `AmsTray` — both physical AMS trays
and virtual/external spool holders use the same field set.

#### Fields

- **`id`**: `Option<String>`

  Virtual tray ID (typically `"254"`).

- **`tray_type`**: `Option<String>`

  Material class abbreviation (e.g. "PLA", "PETG"). Empty when no filament loaded.

- **`tray_color`**: `Option<String>`

  RRGGBBAA hexadecimal color string.

- **`tray_info_idx`**: `Option<String>`

  Slicer filament preset index.

- **`tray_sub_brands`**: `Option<String>`

  Sub-brand or variant string.

- **`nozzle_temp_max`**: `Option<String>`

  Maximum nozzle temperature for the loaded filament (sent as string).

- **`nozzle_temp_min`**: `Option<String>`

  Minimum nozzle temperature for the loaded filament (sent as string).

- **`tray_diameter`**: `Option<String>`

  Filament diameter in mm (sent as string, e.g. `"1.75"`).

- **`tray_weight`**: `Option<String>`

  Spool net weight in grams (sent as string).

- **`tray_temp`**: `Option<String>`

  Filament temperature setting (sent as string).

- **`tray_time`**: `Option<String>`

  Filament print time accumulator (sent as string).

- **`bed_temp`**: `Option<String>`

  Bed temperature setting (sent as string).

- **`bed_temp_type`**: `Option<String>`

  Bed temperature type/profile (sent as string).

- **`tag_uid`**: `Option<String>`

  16-character hexadecimal RFID tag UID.

- **`tray_uuid`**: `Option<String>`

  32-character globally unique filament spool ID.

- **`tray_id_name`**: `Option<String>`

  Filament preset display name.

- **`xcam_info`**: `Option<String>`

  XCam inspection info hex string.

- **`remain`**: `Option<i32>`

  Remaining filament percentage (0–100, or 0 if unknown).

- **`k`**: `Option<f64>`

  Flow rate calibration K factor.

- **`n`**: `Option<i32>`

  Flow rate calibration N factor.

- **`cali_idx`**: `Option<i32>`

  Calibration index (-1 if uncalibrated).

#### Trait Implementations

##### `impl Clone for VirtualTray`

- <span id="virtualtray-clone"></span>`fn clone(&self) -> VirtualTray` — [`VirtualTray`](#virtualtray)

##### `impl Debug for VirtualTray`

- <span id="virtualtray-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for VirtualTray`

- <span id="virtualtray-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for VirtualTray`

##### `impl Serialize for VirtualTray`

- <span id="virtualtray-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

