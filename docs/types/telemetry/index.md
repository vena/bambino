*[bambino](../../index.md) / [types](../index.md) / [telemetry](index.md)*

---

# Module `telemetry`

# State Telemetry Payload Schemas

Provides structured, allocation-friendly deserialization models for the
local MQTTS Port 8883 state telemetry streams [REF-MQTT-ENV].

Supports permissive parsing for platform discrepancies (such as the variable
types of `sdcard` presence markers) and implements binary unpacking helpers
for composite packed temperatures, home/status flags, and door sensors.

## Architectural Alignment
* **Quirks Integration:** Raw elements (e.g., `device.airduct.parts` or `ctc.info.temp`)
  are fully parsed into clean schemas to allow model-specific behaviors to be evaluated
  via the quirks engine.

## Contents

- [Modules](#modules)
  - [`ams`](#ams)
  - [`device`](#device)
  - [`diagnostics`](#diagnostics)
  - [`report`](#report)
- [Types](#types)
  - [`TelemetryReport`](#telemetryreport)
- [Functions](#functions)
  - [`decode_nozzle_temperatures`](#decode-nozzle-temperatures)
  - [`is_developer_mode`](#is-developer-mode)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`ams`](#ams) | mod | AMS telemetry types (tray slots, units, dry settings, virtual trays). |
| [`device`](#device) | mod | Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras). |
| [`diagnostics`](#diagnostics) | mod | Diagnostic telemetry types (HMS alerts, light reports). |
| [`report`](#report) | mod | Top-level telemetry report envelope (`print` and `device` wire locations). |
| [`TelemetryReport`](#telemetryreport) | struct | Unified top-level telemetry report received from the printer's local MQTT broker. |
| [`decode_nozzle_temperatures`](#decode-nozzle-temperatures) | fn | Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`] — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived. |
| [`is_developer_mode`](#is-developer-mode) | fn | Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1]. |

## Modules

- [`ams`](ams/index.md#ams) — AMS telemetry types (tray slots, units, dry settings, virtual trays).
- [`device`](device/index.md#device) — Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).
- [`diagnostics`](diagnostics/index.md#diagnostics) — Diagnostic telemetry types (HMS alerts, light reports).
- [`report`](report/index.md#report) — Top-level telemetry report envelope (`print` and `device` wire locations).


---

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

##### `impl<E> AsTaggedExplicit<'a, E> for AmsDrySetting`

##### `impl<E> AsTaggedImplicit<'a, E> for AmsDrySetting`

##### `impl Clone for AmsDrySetting`

- <span id="amsdrysetting-clone"></span>`fn clone(&self) -> AmsDrySetting` — [`AmsDrySetting`](ams/index.md#amsdrysetting)

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
    pub calibrate_remain_flag: Option<bool>,
    pub cfs: Option<Vec<AmsFilamentStep>>,
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

- **`calibrate_remain_flag`**: `Option<bool>`

  Whether AMS-side remaining-filament detection is enabled. Confirmed
  independently by `bambu-printer-manager` (`bambucommands.py:180`, `bambutools.py:90`)
  and `OpenBambuAPI/local-printer-api.md:317` (community protocol spec).

- **`cfs`**: `Option<Vec<AmsFilamentStep>>`

  Per-slot filament-change step codes. Confirmed against BambuStudio's
  `DevFilaSystem.cpp:507-508` (`GetVal<std::vector<DevFilamentStep>>(jj["ams"], "cfs")`);
  consistent with pybambu's `MOCK-X2D.json:184-189` fixture (`"cfs": [2, 9, 5, 7]`).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for AmsStatusReport`

##### `impl<E> AsTaggedImplicit<'a, E> for AmsStatusReport`

##### `impl Clone for AmsStatusReport`

- <span id="amsstatusreport-clone"></span>`fn clone(&self) -> AmsStatusReport` — [`AmsStatusReport`](ams/index.md#amsstatusreport)

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
    pub remain_g: Option<i32>,
    pub filament_setting_id: Option<String>,
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

- **`remain_g`**: `Option<i32>`

  Accurate remaining weight in grams, when firmware can resolve it. Distinct
  from `remain`'s coarse percentage estimate. Confirmed against BambuStudio's
  `DevFilaSystem.cpp:800`/`.h:73` (`remain_g`, introduced in commit `31637e013`,
  "ENH: support accurate filament remain weight", 2026-06-12) — firmware sends `-1` for
  "not provided", preserved here as the raw wire value; use `remaining_weight_grams()`
  for the sentinel-translated `Option<u32>`.

- **`filament_setting_id`**: `Option<String>`

  Filament preset ID BambuStudio resolves and prefers for print-preset auto-matching,
  distinct from `tray_info_idx`. Wire key is `setting_id`; renamed here to
  avoid confusion with `tray_info_idx`'s own doc name collision. Confirmed against
  BambuStudio's `DevFilaSystem.cpp:801` (`filament_setting_id`) and `DevMapping.cpp`
  (commit `d1f121d26`, 2026-06-09), which prefers this field over the coarser
  `filament_id` when auto-matching a spool to a slicer preset.

#### Implementations

- <span id="amstray-state"></span>`fn state(&self) -> u8`

  Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.

- <span id="amstray-remaining-weight-grams"></span>`fn remaining_weight_grams(&self) -> Option<u32>`

  Accurate remaining weight in grams, translating `remain_g`'s raw wire

  sentinel to `None`. Mirrors BambuStudio's `DevAmsTray::get_filament_remain_weight()`

  (`DevFilaSystem.cpp:116-124`): `remain_g < 0` means "not provided by firmware" and

  `remain_g == 0` means "confirmed empty," both `None` here; only a positive value is

  returned. Does not replicate BambuStudio's percentage-based fallback (`weight * remain

  / 100`) when `remain_g` is absent — callers needing that estimate already have

  `tray_weight`/`remain` to compute it themselves.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for AmsTray`

##### `impl<E> AsTaggedImplicit<'a, E> for AmsTray`

##### `impl Clone for AmsTray`

- <span id="amstray-clone"></span>`fn clone(&self) -> AmsTray` — [`AmsTray`](ams/index.md#amstray)

##### `impl Debug for AmsTray`

- <span id="amstray-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for AmsTray`

- <span id="amstray-default"></span>`fn default() -> AmsTray` — [`AmsTray`](ams/index.md#amstray)

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

  Drying sub-status from bits 22–23. Bits 24–25 belong to the unrelated `bind_switch_in` field.

- <span id="amsunit-dry-fan1-status"></span>`fn dry_fan1_status(&self) -> Option<u8>`

  Dry-fan 1 status from bits 18–19. Confirmed against BambuStudio's

  `DevFilaSystem.cpp:696` (`get_flag_bits(info, 18, 2)`) and independently by

  `bambu-printer-manager`'s `bambutools.py:685`, an exact match.

- <span id="amsunit-dry-fan2-status"></span>`fn dry_fan2_status(&self) -> Option<u8>`

  Dry-fan 2 status from bits 20–21. Confirmed against BambuStudio's

  `DevFilaSystem.cpp:697` (`get_flag_bits(info, 20, 2)`) and independently by

  `bambu-printer-manager`'s `bambutools.py:686`, an exact match.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for AmsUnit`

##### `impl<E> AsTaggedImplicit<'a, E> for AmsUnit`

##### `impl Clone for AmsUnit`

- <span id="amsunit-clone"></span>`fn clone(&self) -> AmsUnit` — [`AmsUnit`](ams/index.md#amsunit)

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

##### `impl<E> AsTaggedExplicit<'a, E> for VirtualTray`

##### `impl<E> AsTaggedImplicit<'a, E> for VirtualTray`

##### `impl Clone for VirtualTray`

- <span id="virtualtray-clone"></span>`fn clone(&self) -> VirtualTray` — [`VirtualTray`](ams/index.md#virtualtray)

##### `impl Debug for VirtualTray`

- <span id="virtualtray-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for VirtualTray`

- <span id="virtualtray-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for VirtualTray`

##### `impl Serialize for VirtualTray`

- <span id="virtualtray-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirductCollection`

```rust
struct AirductCollection {
    pub parts: Option<Vec<AirductPart>>,
    pub mode_cur: Option<i32>,
    pub mode_list: Option<Vec<AirductModeListEntry>>,
}
```

Climate parts collection nested within `device` parameters.

#### Fields

- **`parts`**: `Option<Vec<AirductPart>>`

  Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].
  
  `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
  — see its doc comment.

- **`mode_cur`**: `Option<i32>`

  Currently active airduct damper mode (0=cooling, 1=heating, 2=laser).

- **`mode_list`**: `Option<Vec<AirductModeListEntry>>`

  List of airduct modes available on this model.
  
  `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
  — see its doc comment.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for AirductCollection`

##### `impl<E> AsTaggedImplicit<'a, E> for AirductCollection`

##### `impl Clone for AirductCollection`

- <span id="airductcollection-clone"></span>`fn clone(&self) -> AirductCollection` — [`AirductCollection`](device/index.md#airductcollection)

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

##### `impl<E> AsTaggedExplicit<'a, E> for AirductModeListEntry`

##### `impl<E> AsTaggedImplicit<'a, E> for AirductModeListEntry`

##### `impl Clone for AirductModeListEntry`

- <span id="airductmodelistentry-clone"></span>`fn clone(&self) -> AirductModeListEntry` — [`AirductModeListEntry`](device/index.md#airductmodelistentry)

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

##### `impl<E> AsTaggedExplicit<'a, E> for AirductPart`

##### `impl<E> AsTaggedImplicit<'a, E> for AirductPart`

##### `impl Clone for AirductPart`

- <span id="airductpart-clone"></span>`fn clone(&self) -> AirductPart` — [`AirductPart`](device/index.md#airductpart)

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

##### `impl<E> AsTaggedExplicit<'a, E> for BedInfo`

##### `impl<E> AsTaggedImplicit<'a, E> for BedInfo`

##### `impl Clone for BedInfo`

- <span id="bedinfo-clone"></span>`fn clone(&self) -> BedInfo` — [`BedInfo`](device/index.md#bedinfo)

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

##### `impl<E> AsTaggedExplicit<'a, E> for BedTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for BedTelemetry`

##### `impl Clone for BedTelemetry`

- <span id="bedtelemetry-clone"></span>`fn clone(&self) -> BedTelemetry` — [`BedTelemetry`](device/index.md#bedtelemetry)

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

  Composite-packed bed temperature mirroring `bed.info.temp`; confirmed redundant, not a fallback.
  
  A fixture payload carries the identical value in both fields, and both
  pybambu (`models.py`, reads only `device.bed.info.temp`) and bambuddy independently
  never consult this field either. Parsed for wire-format completeness only —
  `decode_bed_temperatures()` deliberately does not read it.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for DeviceTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for DeviceTelemetry`

##### `impl Clone for DeviceTelemetry`

- <span id="devicetelemetry-clone"></span>`fn clone(&self) -> DeviceTelemetry` — [`DeviceTelemetry`](device/index.md#devicetelemetry)

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

##### `impl<E> AsTaggedExplicit<'a, E> for ExtToolTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for ExtToolTelemetry`

##### `impl Clone for ExtToolTelemetry`

- <span id="exttooltelemetry-clone"></span>`fn clone(&self) -> ExtToolTelemetry` — [`ExtToolTelemetry`](device/index.md#exttooltelemetry)

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
    pub info: Option<Vec<ExtruderInfo>>,
    pub state: Option<u32>,
}
```

IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].

#### Fields

- **`info`**: `Option<Vec<ExtruderInfo>>`

  Per-extruder thermal and routing entries (id 0 = right/main, id 1 = left/deputy).
  
  `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
  — see its doc comment.

- **`state`**: `Option<u32>`

  Bitmask: low 4 bits = extruder count, bits 4–7 = active extruder index.

#### Implementations

- <span id="extrudercollection-active-extruder-index"></span>`fn active_extruder_index(&self) -> u8`

  Returns the active extruder index extracted from the `state` bitmask.

- <span id="extrudercollection-extruder-count"></span>`fn extruder_count(&self) -> u8`

  Returns the extruder count extracted from the `state` bitmask.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for ExtruderCollection`

##### `impl<E> AsTaggedImplicit<'a, E> for ExtruderCollection`

##### `impl Clone for ExtruderCollection`

- <span id="extrudercollection-clone"></span>`fn clone(&self) -> ExtruderCollection` — [`ExtruderCollection`](device/index.md#extrudercollection)

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

  Current AMS slot routing (confirmed against BambuStudio's `DevExterSystemParser::ParseV2_0`, `DevExtruderSystem.cpp:369-372`): low 8 bits (0–7) = slot_id, next 8 bits (8–15) = ams_id. Sentinel `0xFFFF` on a single-extruder system means unmapped.

- **`spre`**: `Option<u32>`

  Previous AMS slot routing. Same 8/8 (slot_id/ams_id) bit split as `snow`.

- **`star`**: `Option<u32>`

  Target AMS slot routing. Same 8/8 (slot_id/ams_id) bit split as `snow`.

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

- <span id="extruderinfo-current-ams-slot"></span>`fn current_ams_slot(&self) -> Option<(u8, u8)>`

  Currently routed `(ams_id, slot_id)`, decoded from `snow` — the preferred source for

  resolving which physical tray is feeding this extruder right now, confirmed

  against BambuStudio's `DevExterSystem::ParseV2_0` (`DevExtderSystem.cpp:318-386`), which

  decodes `snow` directly with no extruder-map inversion needed.

- <span id="extruderinfo-previous-ams-slot"></span>`fn previous_ams_slot(&self) -> Option<(u8, u8)>`

  Previously routed `(ams_id, slot_id)`, decoded from `spre`. See

  [`ExtruderInfo::current_ams_slot`]'s doc comment for the shared bit layout.

- <span id="extruderinfo-target-ams-slot"></span>`fn target_ams_slot(&self) -> Option<(u8, u8)>`

  Target `(ams_id, slot_id)` for an in-progress filament change, decoded from `star`. See

  [`ExtruderInfo::current_ams_slot`]'s doc comment for the shared bit layout.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for ExtruderInfo`

##### `impl<E> AsTaggedImplicit<'a, E> for ExtruderInfo`

##### `impl Clone for ExtruderInfo`

- <span id="extruderinfo-clone"></span>`fn clone(&self) -> ExtruderInfo` — [`ExtruderInfo`](device/index.md#extruderinfo)

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
    pub info: Option<Vec<NozzleInfo>>,
    pub exist: Option<u32>,
    pub state: Option<u32>,
    pub src_id: Option<u32>,
    pub tar_id: Option<u32>,
}
```

Wrap block holding nozzle characteristics.

#### Fields

- **`info`**: `Option<Vec<NozzleInfo>>`

  Polymorphic array representing active carriages and tool configurations.
  
  `None` means this push's `info` key was absent from the wire — leave previously cached
  entries untouched. `Some(vec![])` means the key was present but empty, which (per
  `NozzleCollection::merge_from`) replaces the cached entries with an empty list.
  Confirmed against BambuStudio's `json_diff::restore_objects` (`src/slic3r/Utils/
  json_diff.cpp`) — its generic recursive JSON-delta merge treats a present array
  differing from the last-known value as the new authoritative value (including an empty
  array replacing a non-empty one), and only an absent key as "carry the old value
  forward." `#[serde(default)]` on `Option<Vec<_>>` gives this distinction for free
  (absent key -> `None`, present key -> `Some(_)` however short) — previously both
  collapsed to the same empty `Vec` (same shape as the `AmsTray` fix).

- **`exist`**: `Option<u32>`

  Bitmask of physically present nozzle IDs (HotendRack).

- **`state`**: `Option<u32>`

  Nozzle state bitmask.

- **`src_id`**: `Option<u32>`

  Tool-change source nozzle ID.

- **`tar_id`**: `Option<u32>`

  Tool-change target nozzle ID.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for NozzleCollection`

##### `impl<E> AsTaggedImplicit<'a, E> for NozzleCollection`

##### `impl Clone for NozzleCollection`

- <span id="nozzlecollection-clone"></span>`fn clone(&self) -> NozzleCollection` — [`NozzleCollection`](device/index.md#nozzlecollection)

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

  Extruder carriage index (0 = Right/Main, 1 = Left/Deputy), or on H2C, a packed rack
  slot: high nibble (bits 4–7) `1` flags a rack-stored spare nozzle, low nibble (bits
  0–3) is the slot index within the rack — see [`NozzleInfo::is_rack_stored()`].

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

#### Implementations

- <span id="nozzleinfo-is-rack-stored"></span>`fn is_rack_stored(&self) -> bool`

  Returns whether this entry is a rack-stored spare nozzle rather than an installed one.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for NozzleInfo`

##### `impl<E> AsTaggedImplicit<'a, E> for NozzleInfo`

##### `impl Clone for NozzleInfo`

- <span id="nozzleinfo-clone"></span>`fn clone(&self) -> NozzleInfo` — [`NozzleInfo`](device/index.md#nozzleinfo)

##### `impl Debug for NozzleInfo`

- <span id="nozzleinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NozzleInfo`

- <span id="nozzleinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NozzleInfo`

##### `impl Serialize for NozzleInfo`

- <span id="nozzleinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CtcInfo`

```rust
struct CtcInfo {
    pub temp: Option<u32>,
    pub target: Option<u32>,
}
```

Controller information segment detailing current temperature coordinates.

#### Fields

- **`temp`**: `Option<u32>`

  Composite-packed integer temperature value [REF-THER-DECODE].
  Use `PrinterTelemetry::unpack_temperature()` on this value cast to `f64`.

- **`target`**: `Option<u32>`

  Explicit CTC target temperature (authoritative on new-gen models).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for CtcInfo`

##### `impl<E> AsTaggedImplicit<'a, E> for CtcInfo`

##### `impl Clone for CtcInfo`

- <span id="ctcinfo-clone"></span>`fn clone(&self) -> CtcInfo` — [`CtcInfo`](diagnostics/index.md#ctcinfo)

##### `impl Debug for CtcInfo`

- <span id="ctcinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for CtcInfo`

- <span id="ctcinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for CtcInfo`

##### `impl Serialize for CtcInfo`

- <span id="ctcinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CtcTelemetry`

```rust
struct CtcTelemetry {
    pub info: Option<CtcInfo>,
    pub state: Option<u32>,
}
```

Chamber Temperature Controller (CTC) telemetry sub-object.

#### Fields

- **`info`**: `Option<CtcInfo>`

  Controller info containing thermal actuals and targets.

- **`state`**: `Option<u32>`

  CTC controller state (0 = idle, 2 = heating).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for CtcTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for CtcTelemetry`

##### `impl Clone for CtcTelemetry`

- <span id="ctctelemetry-clone"></span>`fn clone(&self) -> CtcTelemetry` — [`CtcTelemetry`](diagnostics/index.md#ctctelemetry)

##### `impl Debug for CtcTelemetry`

- <span id="ctctelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for CtcTelemetry`

- <span id="ctctelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for CtcTelemetry`

##### `impl Serialize for CtcTelemetry`

- <span id="ctctelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `HmsEntry`

```rust
struct HmsEntry {
    pub attr: u32,
    pub code: u32,
    pub ts_boot: Option<u64>,
    pub ts_unix: Option<String>,
}
```

Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].

Each entry represents an active hardware fault or status indication. Use
`diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.

#### Fields

- **`attr`**: `u32`

  Packed attribute word encoding module ID, severity, and subsystem address.

- **`code`**: `u32`

  Packed code word encoding fault category and error index.

- **`ts_boot`**: `Option<u64>`

  Seconds since boot when the alert was raised (confirmed present on X2 only; unverified on H2/P2).

- **`ts_unix`**: `Option<String>`

  UTC timestamp string when the alert was raised (e.g. `"20260426002648"`).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for HmsEntry`

##### `impl<E> AsTaggedImplicit<'a, E> for HmsEntry`

##### `impl Clone for HmsEntry`

- <span id="hmsentry-clone"></span>`fn clone(&self) -> HmsEntry` — [`HmsEntry`](diagnostics/index.md#hmsentry)

##### `impl Debug for HmsEntry`

- <span id="hmsentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for HmsEntry`

- <span id="hmsentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for HmsEntry`

##### `impl Serialize for HmsEntry`

- <span id="hmsentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IpcamTelemetry`

```rust
struct IpcamTelemetry {
    pub ipcam_dev: Option<String>,
    pub ipcam_record: Option<String>,
    pub timelapse: Option<String>,
    pub mode_bits: Option<u32>,
    pub resolution: Option<String>,
    pub tutk_server: Option<String>,
    pub rtsp_url: Option<String>,
}
```

Camera and recording state telemetry, nested as `print.ipcam` on the wire.

#### Fields

- **`ipcam_dev`**: `Option<String>`

  Internal identifier or state of the hardware camera module.

- **`ipcam_record`**: `Option<String>`

  Camera live feed recording status (`"enable"` or `"disable"`).

- **`timelapse`**: `Option<String>`

  Frame-by-layer timelapse recording status (`"enable"` or `"disable"`).

- **`mode_bits`**: `Option<u32>`

  Camera mode bitmask.

- **`resolution`**: `Option<String>`

  Camera resolution setting.

- **`tutk_server`**: `Option<String>`

  TUTK server status (`"enable"` or `"disable"`).

- **`rtsp_url`**: `Option<String>`

  RTSP streaming URL (e.g. `"rtsps://192.168.1.64/streaming/live/1"`).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for IpcamTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for IpcamTelemetry`

##### `impl Clone for IpcamTelemetry`

- <span id="ipcamtelemetry-clone"></span>`fn clone(&self) -> IpcamTelemetry` — [`IpcamTelemetry`](diagnostics/index.md#ipcamtelemetry)

##### `impl Debug for IpcamTelemetry`

- <span id="ipcamtelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for IpcamTelemetry`

- <span id="ipcamtelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for IpcamTelemetry`

##### `impl Serialize for IpcamTelemetry`

- <span id="ipcamtelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `LightReport`

```rust
struct LightReport {
    pub node: String,
    pub mode: String,
}
```

Chamber/work/heatbed light state entry from the `lights_report` array.

#### Fields

- **`node`**: `String`

  Light identifier (e.g. "chamber_light", "work_light").

- **`mode`**: `String`

  Current state (e.g. "on", "off", "flashing").

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for LightReport`

##### `impl<E> AsTaggedImplicit<'a, E> for LightReport`

##### `impl Clone for LightReport`

- <span id="lightreport-clone"></span>`fn clone(&self) -> LightReport` — [`LightReport`](report/index.md#lightreport)

##### `impl Debug for LightReport`

- <span id="lightreport-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for LightReport`

- <span id="lightreport-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for LightReport`

##### `impl Serialize for LightReport`

- <span id="lightreport-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `NetInfo`

```rust
struct NetInfo {
    pub conf: Option<u32>,
}
```

Network interface state from `print.net` [REF-NET-PORTS].

#### Fields

- **`conf`**: `Option<u32>`

  Bitmask; bit 0 (`0x1`) set means wired Ethernet is the active connection.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for NetInfo`

##### `impl<E> AsTaggedImplicit<'a, E> for NetInfo`

##### `impl Clone for NetInfo`

- <span id="netinfo-clone"></span>`fn clone(&self) -> NetInfo` — [`NetInfo`](report/index.md#netinfo)

##### `impl Debug for NetInfo`

- <span id="netinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NetInfo`

- <span id="netinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NetInfo`

##### `impl Serialize for NetInfo`

- <span id="netinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrinterTelemetry`

```rust
struct PrinterTelemetry {
    pub gcode_state: Option<String>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub subtask_id: Option<String>,
    pub layer_num: Option<i32>,
    pub total_layers: Option<i32>,
    pub mc_remaining_time: Option<i32>,
    pub spd_lvl: Option<u8>,
    pub spd_mag: Option<u16>,
    pub mc_percent: Option<i32>,
    pub mc_print_sub_stage: Option<i32>,
    pub mc_print_stage: Option<String>,
    pub home_flag: Option<u32>,
    pub stat: Option<String>,
    pub stg_cur: Option<i32>,
    pub print_error: Option<u32>,
    pub hms: Option<Vec<super::diagnostics::HmsEntry>>,
    pub sdcard: bool,
    pub wifi_signal: Option<String>,
    pub net: Option<NetInfo>,
    pub cooling_fan_speed: Option<String>,
    pub big_fan1_speed: Option<String>,
    pub big_fan2_speed: Option<String>,
    pub heatbreak_fan_speed: Option<String>,
    pub nozzle_target_temper: Option<f64>,
    pub nozzle_temper: Option<f64>,
    pub bed_temper: Option<f64>,
    pub bed_target_temper: Option<f64>,
    pub chamber_temper: Option<f64>,
    pub tray_exist_bits: Option<String>,
    pub power_on_flag: Option<bool>,
    pub ipcam: Option<super::diagnostics::IpcamTelemetry>,
    pub xcam: Option<serde_json::Value>,
    pub ams: Option<super::ams::AmsStatusReport>,
    pub ams_status: Option<i32>,
    pub ams_mapping: Vec<i32>,
    pub vt_tray: Option<super::ams::VirtualTray>,
    pub vir_slot: Option<Vec<super::ams::VirtualTray>>,
    pub device: Option<super::device::DeviceTelemetry>,
    pub fun: Option<String>,
    pub print_type: Option<String>,
    pub lights_report: Option<Vec<LightReport>>,
    pub gcode_file_prepare_percent: Option<String>,
    pub hw_switch_state: Option<i32>,
    pub s_obj: Option<Vec<i32>>,
    pub nozzle_type: Option<String>,
    pub nozzle_diameter: Option<String>,
    pub fan_gear: Option<u32>,
    pub print_gcode_action: Option<i32>,
    pub print_real_action: Option<i32>,
    pub task_id: Option<String>,
    pub job_id: Option<String>,
    pub remain_time: Option<i32>,
    pub cfg: Option<String>,
    pub stg: Option<Vec<i32>>,
    pub mapping: Option<Vec<i32>>,
    pub gcode_start_time: Option<String>,
    pub cali_version: Option<i32>,
    pub err: Option<String>,
    pub fail_reason: Option<String>,
    pub canvas_id: Option<String>,
    pub design_id: Option<String>,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
    pub project_id: Option<String>,
    pub batch_id: Option<String>,
}
```

Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

#### Fields

- **`gcode_state`**: `Option<String>`

  High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").

- **`gcode_file`**: `Option<String>`

  Path or parent project file currently loaded for execution.

- **`subtask_name`**: `Option<String>`

  User-assigned name of the active print queue task.

- **`subtask_id`**: `Option<String>`

  Hardware-enforced unique 32-bit transaction identifier tracking active jobs.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.
  Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.

- **`mc_remaining_time`**: `Option<i32>`

  Estimated remaining duration of the active layer sequence, in seconds.

- **`spd_lvl`**: `Option<u8>`

  Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).

- **`spd_mag`**: `Option<u16>`

  Speed magnitude as a percentage of the nominal feedrate.

- **`mc_percent`**: `Option<i32>`

  Motion controller progress percentage (0–100).

- **`mc_print_sub_stage`**: `Option<i32>`

  Print sub-stage identifier tracking granular execution phases within the active print stage.

- **`mc_print_stage`**: `Option<String>`

  Motion controller print stage string.

- **`home_flag`**: `Option<u32>`

  Kinematics flag field tracking homing states, networking interfaces, and door nodes.

- **`stat`**: `Option<String>`

  State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).

- **`stg_cur`**: `Option<i32>`

  Active print stage. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].

- **`print_error`**: `Option<u32>`

  Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].

- **`hms`**: `Option<Vec<super::diagnostics::HmsEntry>>`

  Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].

- **`sdcard`**: `bool`

  Permissive indicator tracking physical MicroSD card insertion.
  
  Evaluated via custom deserializer to absorb structural variations between firmwares.

- **`wifi_signal`**: `Option<String>`

  Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").

- **`net`**: `Option<NetInfo>`

  Network interface state, nested as `print.net` on the wire.

- **`cooling_fan_speed`**: `Option<String>`

  On-board part cooling fan speed (represented as discrete steps 0 to 15) [REF-CLIM-FANS].

- **`big_fan1_speed`**: `Option<String>`

  On-board left-side auxiliary fan speed (represented as discrete steps 0 to 15).

- **`big_fan2_speed`**: `Option<String>`

  On-board filtration or chamber exhaust fan speed (represented as discrete steps 0 to 15).

- **`heatbreak_fan_speed`**: `Option<String>`

  On-board toolhead heatbreak fan speed (represented as discrete steps 0 to 15).

- **`nozzle_target_temper`**: `Option<f64>`

  Hotend target temperature register.
  
  Wire sends both integers and floats depending on model. Never composite-packed —
  unlike `chamber_temper`, no `unpack_temperature()` call is needed here.

- **`nozzle_temper`**: `Option<f64>`

  Hotend actual temperature register.
  
  Wire sends both integers and floats depending on model [REF-THER-DECODE].

- **`bed_temper`**: `Option<f64>`

  Heated build-plate temperature register (actual value; never composite-packed).

- **`bed_target_temper`**: `Option<f64>`

  Explicit bed target temperature. Separate from composite-packed `bed_temper`.

- **`chamber_temper`**: `Option<f64>`

  Active chamber heater or sensor telemetry (actual, target, or composite packed).

- **`tray_exist_bits`**: `Option<String>`

  Hexadecimal bitmask string representing the physical presence of loaded spools.

- **`power_on_flag`**: `Option<bool>`

  Power status of the printer core logic board.

- **`ipcam`**: `Option<super::diagnostics::IpcamTelemetry>`

  Camera and recording telemetry. Nested as `print.ipcam` on the wire.

- **`xcam`**: `Option<serde_json::Value>`

  AI detection settings (spaghetti detection, first-layer inspection, etc.).

- **`ams`**: `Option<super::ams::AmsStatusReport>`

  AMS expansion bus status container [REF-AMS-DECODE].

- **`ams_status`**: `Option<i32>`

  Combined AMS state bitmask (lower 8 bits = sub status, bits 8–15 = main status).

- **`ams_mapping`**: `Vec<i32>`

  Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].

- **`vt_tray`**: `Option<super::ams::VirtualTray>`

  Virtual/external spool holder state on single-nozzle platforms (P1S, P1P, A1, X1C, H2S).
  Dual-nozzle IDEX platforms (H2D, H2D Pro, X2D) report `vir_slot` instead [REF-AMS-DECODE].

- **`vir_slot`**: `Option<Vec<super::ams::VirtualTray>>`

  IDEX external spool holder array. Each entry uses the same schema as `VirtualTray`.

- **`device`**: `Option<super::device::DeviceTelemetry>`

  Device sub-object nested inside pushall `print` envelope on H2/P2/X2 models.
  Contains CTC, nozzle, and airduct telemetry for enclosed printers.

- **`fun`**: `Option<String>`

  Developer LAN Mode bitmask field (hex string) nested inside `print` [REF-MQTT-ENV §3.2.1].

- **`print_type`**: `Option<String>`

  Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).

- **`lights_report`**: `Option<Vec<LightReport>>`

  Chamber/work/heatbed light states array.

- **`gcode_file_prepare_percent`**: `Option<String>`

  File download progress percentage (sent as string).

- **`hw_switch_state`**: `Option<i32>`

  Extruder filament sensor state (1 = filament present).

- **`s_obj`**: `Option<Vec<i32>>`

  Skipped object IDs during selective printing.

- **`nozzle_type`**: `Option<String>`

  Legacy single-nozzle type string (pre-IDEX models).

- **`nozzle_diameter`**: `Option<String>`

  Legacy single-nozzle diameter string (pre-IDEX models).

- **`fan_gear`**: `Option<u32>`

  Fan gear composite bitmask.

- **`print_gcode_action`**: `Option<i32>`

  G-code action state (H2/X2 models).

- **`print_real_action`**: `Option<i32>`

  Real action state (H2/X2 models).

- **`task_id`**: `Option<String>`

  Cloud task identifier.

- **`job_id`**: `Option<String>`

  Cloud job identifier.

- **`remain_time`**: `Option<i32>`

  Alternative remaining time field (minutes).

- **`cfg`**: `Option<String>`

  Hex config bitmask string (bit 18 = AMS Filament Backup).

- **`stg`**: `Option<Vec<i32>>`

  Calibration stage list.

- **`mapping`**: `Option<Vec<i32>>`

  IDEX AMS-to-extruder mapping array.

- **`gcode_start_time`**: `Option<String>`

  Print start timestamp string.

- **`cali_version`**: `Option<i32>`

  Calibration version identifier.

- **`err`**: `Option<String>`

  Error string field.

- **`fail_reason`**: `Option<String>`

  Failure reason description.

- **`canvas_id`**: `Option<String>`

  Cloud canvas project ID.

- **`design_id`**: `Option<String>`

  Cloud design ID.

- **`model_id`**: `Option<String>`

  Cloud model ID.

- **`profile_id`**: `Option<String>`

  Cloud profile ID.

- **`project_id`**: `Option<String>`

  Cloud project ID.

- **`batch_id`**: `Option<String>`

  Cloud batch ID.

#### Implementations

- <span id="printertelemetry-unpack-temperature"></span>`fn unpack_temperature(raw_val: f64) -> (u16, u16)`

  Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].

- <span id="printertelemetry-is-ethernet-active"></span>`fn is_ethernet_active(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].

- <span id="printertelemetry-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet using the `wifi_signal` sentinel value [REF-NET-PORTS], as a fallback for firmware that doesn't populate `print.net.conf`.

- <span id="printertelemetry-is-220v-power"></span>`fn is_220v_power(&self) -> bool`

  Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.

- <span id="printertelemetry-sdcard-state"></span>`fn sdcard_state(&self) -> Option<SdcardState>` — [`SdcardState`](report/index.md#sdcardstate)

  Evaluates the SD-card presence/health state from `home_flag` bits 8–9. See

  [`SdcardState`](report/index.md#sdcardstate)'s doc comment for verification sources. Returns `None` before any

  telemetry carrying `home_flag` has been observed — distinct from `Some(NoSdcard)`.

- <span id="printertelemetry-is-door-open-from-home-flag"></span>`fn is_door_open_from_home_flag(&self) -> bool`

  Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].

- <span id="printertelemetry-is-door-open-from-stat"></span>`fn is_door_open_from_stat(&self) -> bool`

  Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for PrinterTelemetry`

##### `impl<E> AsTaggedImplicit<'a, E> for PrinterTelemetry`

##### `impl Clone for PrinterTelemetry`

- <span id="printertelemetry-clone"></span>`fn clone(&self) -> PrinterTelemetry` — [`PrinterTelemetry`](report/index.md#printertelemetry)

##### `impl Debug for PrinterTelemetry`

- <span id="printertelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrinterTelemetry`

- <span id="printertelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrinterTelemetry`

##### `impl Serialize for PrinterTelemetry`

- <span id="printertelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `TelemetryReport`

```rust
struct TelemetryReport {
    pub print: Option<PrinterTelemetry>,
    pub device: Option<DeviceTelemetry>,
    pub fun: Option<String>,
}
```

Unified top-level telemetry report received from the printer's local MQTT broker.

Under the over-the-wire schema, updates are typically nested within separate
top-level domains depending on which micro-system published the frame.

#### Fields

- **`print`**: `Option<PrinterTelemetry>`

  Telemetry parameters representing the physical printer state machine.

- **`device`**: `Option<DeviceTelemetry>`

  Network and hardware board capability descriptors.

- **`fun`**: `Option<String>`

  Developer LAN Mode bitmask field (hex string).
  Drifts between top-level and `print.fun` depending on firmware version [REF-MQTT-ENV §3.2.1].

#### Implementations

- <span id="telemetryreport-bed-temperatures"></span>`fn bed_temperatures(&self) -> (u16, u16)`

  Returns the bed's (actual, target) temperatures in °C.

  

  Handles the different wire formats across printer generations automatically:

  new-gen composite-packed `device.bed`, pushall-nested `print.device.bed`, and

  old-gen direct `bed_temper`/`bed_target_temper` fields. Returns (0, 0) if absent.

  

  # Example

  

  ```rust,ignore

  let (actual, target) = report.bed_temperatures();

  println!("Bed: {}°C (target {}°C)", actual, target);

  ```

- <span id="telemetryreport-device"></span>`fn device(&self) -> Option<&DeviceTelemetry>` — [`DeviceTelemetry`](device/index.md#devicetelemetry)

  Returns the `DeviceTelemetry` sub-object, checking both wire locations it can arrive at.

- <span id="telemetryreport-fun"></span>`fn fun(&self) -> Option<&str>`

  Returns the `fun` Developer LAN Mode bitmask, checking both wire locations it can

  arrive at.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for TelemetryReport`

##### `impl<E> AsTaggedImplicit<'a, E> for TelemetryReport`

##### `impl Clone for TelemetryReport`

- <span id="telemetryreport-clone"></span>`fn clone(&self) -> TelemetryReport` — [`TelemetryReport`](#telemetryreport)

##### `impl Debug for TelemetryReport`

- <span id="telemetryreport-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for TelemetryReport`

- <span id="telemetryreport-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for TelemetryReport`

##### `impl Serialize for TelemetryReport`

- <span id="telemetryreport-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentStep`

```rust
enum AmsFilamentStep {
    Idle,
    Pause,
    HeatNozzle,
    CutFilament,
    PullCurrFilament,
    PushNewFilament,
    GrabNewFilament,
    PurgeOldFilament,
    CheckPosition,
    SwitchExtruder,
    SwitchHotend,
    AmsFilaCooling,
    PushSwitcherFila,
    PullSwitcherFila,
    SwitcherSwitch,
    Unknown(i64),
}
```

Per-slot filament-change step code. Mirrors BambuStudio's `DevFilamentStep` enum
(`DevDefs.h:64`) — used to type `AmsStatusReport.cfs`. `CheckPosition` covers both `0x08`
wire values (`STEP_CHECK_POSITION`/`STEP_CONFIRM_EXTRUDED` share the same discriminant in
the source enum). `Unknown` preserves any other raw value rather than failing to decode.

#### Variants

- **`Idle`**

  No filament-change activity in progress.

- **`Pause`**

  Change sequence paused.

- **`HeatNozzle`**

  Heating the nozzle before the change.

- **`CutFilament`**

  Cutting the current filament.

- **`PullCurrFilament`**

  Retracting the current filament out of the toolhead.

- **`PushNewFilament`**

  Feeding the new filament toward the toolhead.

- **`GrabNewFilament`**

  Grabbing the new filament at the AMS slot.

- **`PurgeOldFilament`**

  Purging leftover old filament from the nozzle.

- **`CheckPosition`**

  Verifying filament position (wire value `0x08`, shared with `STEP_CONFIRM_EXTRUDED`).

- **`SwitchExtruder`**

  Switching to a different extruder (IDEX).

- **`SwitchHotend`**

  Switching to a different hotend (tool-changer).

- **`AmsFilaCooling`**

  Cooling the filament inside the AMS unit.

- **`PushSwitcherFila`**

  Pushing filament into the tool-changer switcher.

- **`PullSwitcherFila`**

  Pulling filament out of the tool-changer switcher.

- **`SwitcherSwitch`**

  Switching the tool-changer's active position.

- **`Unknown`**

  Any wire value not covered by a named variant, preserved verbatim.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for AmsFilamentStep`

##### `impl<E> AsTaggedImplicit<'a, E> for AmsFilamentStep`

##### `impl Clone for AmsFilamentStep`

- <span id="amsfilamentstep-clone"></span>`fn clone(&self) -> AmsFilamentStep` — [`AmsFilamentStep`](ams/index.md#amsfilamentstep)

##### `impl Copy for AmsFilamentStep`

##### `impl Debug for AmsFilamentStep`

- <span id="amsfilamentstep-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsFilamentStep`

- <span id="amsfilamentstep-deserialize"></span>`fn deserialize<D>(deserializer: D) -> Result<Self, <D as >::Error>`

##### `impl DeserializeOwned for AmsFilamentStep`

##### `impl Eq for AmsFilamentStep`

##### `impl Hash for AmsFilamentStep`

- <span id="amsfilamentstep-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AmsFilamentStep`

- <span id="amsfilamentstep-partialeq-eq"></span>`fn eq(&self, other: &AmsFilamentStep) -> bool` — [`AmsFilamentStep`](ams/index.md#amsfilamentstep)

##### `impl Serialize for AmsFilamentStep`

- <span id="amsfilamentstep-serialize"></span>`fn serialize<S>(&self, serializer: S) -> Result<<S as >::Ok, <S as >::Error>`

##### `impl StructuralPartialEq for AmsFilamentStep`

### `SdcardState`

```rust
enum SdcardState {
    NoSdcard,
    Normal,
    Abnormal,
    ReadOnly,
}
```

SD-card presence/health state, decoded from `home_flag` bits 8–9.

Confirmed against BambuStudio's `MachineObject::parse_json` (`DeviceManager.cpp:1092`:
`m_storage->set_sdcard_state(get_flag_bits(flag, 8, 2))`) and corroborated by pybambu's
`const.py:265-266`/`models.py:3408-3412` (same bits). The `sdcard` boolean field can never
report a degraded state — only this bitmask distinguishes "no card," "normal," "abnormal,"
and "read-only."

#### Variants

- **`NoSdcard`**

  No SD card physically present.

- **`Normal`**

  SD card present and functioning normally.

- **`Abnormal`**

  SD card present but reporting an abnormal/error condition.

- **`ReadOnly`**

  SD card present but mounted read-only.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for SdcardState`

##### `impl<E> AsTaggedImplicit<'a, E> for SdcardState`

##### `impl Clone for SdcardState`

- <span id="sdcardstate-clone"></span>`fn clone(&self) -> SdcardState` — [`SdcardState`](report/index.md#sdcardstate)

##### `impl Copy for SdcardState`

##### `impl Debug for SdcardState`

- <span id="sdcardstate-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for SdcardState`

##### `impl Hash for SdcardState`

- <span id="sdcardstate-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for SdcardState`

- <span id="sdcardstate-partialeq-eq"></span>`fn eq(&self, other: &SdcardState) -> bool` — [`SdcardState`](report/index.md#sdcardstate)

##### `impl StructuralPartialEq for SdcardState`


---

## Functions

### `decode_nozzle_temperatures`

```rust
fn decode_nozzle_temperatures(device: Option<&DeviceTelemetry>, nozzle_temper: Option<f64>, nozzle_target_temper: Option<f64>) -> Vec<(u8, u16, u16)>
```

**Types:** [`DeviceTelemetry`](device/index.md#devicetelemetry)

Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`] — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived.

Returns one `(id, actual, target)` tuple per nozzle. Prefers `device.extruder.info`
(composite-packed per-nozzle temperatures, decoded via [`ExtruderInfo::temperatures()`]).
Falls back to the flat `nozzle_temper`/`nozzle_target_temper` fields when absent: a single
entry `(0, actual, target)` for a single-nozzle model, or — for a dual-nozzle (IDEX) model
with no live extruder temps yet — the wire's undocumented routing quirk: `nozzle_temper` is
nozzle 1 (left)'s actual reading and `nozzle_target_temper` is nozzle 0 (right)'s target,
each nozzle only getting half of its own reading from the flat fields.

### `is_developer_mode`

```rust
fn is_developer_mode(fun_hex: &str) -> Option<bool>
```

Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].

Returns `Some(true)` when developer mode is enabled (MQTT signature NOT required),
`Some(false)` when disabled, or `None` if the hex string is unparseable.
The `fun` field is a variable-length hex string (up to 64 bits). Bit 29
(`0x20000000`) is the `MQTT_SIGNATURE_REQUIRED` flag — when clear, developer mode is on.

