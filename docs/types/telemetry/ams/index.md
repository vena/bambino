*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [ams](index.md)*

---

# Module `ams`

AMS telemetry types (tray slots, units, dry settings, virtual trays).

## Contents

- [Types](#types)
  - [`AmsDrySetting`](#amsdrysetting)
  - [`AmsStatusReport`](#amsstatusreport)
  - [`AmsTray`](#amstray)
  - [`AmsUnit`](#amsunit)
  - [`AmsDryFanStatus`](#amsdryfanstatus)
  - [`AmsDryStatus`](#amsdrystatus)
  - [`AmsDrySubStatus`](#amsdrysubstatus)
  - [`AmsFilamentStep`](#amsfilamentstep)
  - [`AmsUnitModel`](#amsunitmodel)
  - [`DryBlockReason`](#dryblockreason)
  - [`FilamentSwitchInlet`](#filamentswitchinlet)
  - [`VirtualTray`](#virtualtray)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsDrySetting`](#amsdrysetting) | struct | Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER]. |
| [`AmsStatusReport`](#amsstatusreport) | struct | Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE]. |
| [`AmsTray`](#amstray) | struct | Material spool state descriptor representing a single physical tray slot. |
| [`AmsUnit`](#amsunit) | struct | Modular standard expansion unit managing up to 4 physical spool slots. |
| [`AmsDryFanStatus`](#amsdryfanstatus) | enum | State of one drying fan from `info` bits 18–19 or 20–21, BambuStudio's `DevAms::DryFanStatus` (`DevFilaSystem.h`). |
| [`AmsDryStatus`](#amsdrystatus) | enum | Drying-cycle state from `info` bits 4–7, BambuStudio's `DevAms::DryStatus` (`DevFilaSystem.h`). |
| [`AmsDrySubStatus`](#amsdrysubstatus) | enum | Drying sub-state from `info` bits 22–23, BambuStudio's `DevAms::DrySubStatus` (`DevFilaSystem.h`). |
| [`AmsFilamentStep`](#amsfilamentstep) | enum | Per-slot filament-change step code. |
| [`AmsUnitModel`](#amsunitmodel) | enum | Which physical AMS accessory is attached, decoded from `info` bits 0–3. |
| [`DryBlockReason`](#dryblockreason) | enum | Why the firmware will not, or did not, start a drying cycle — one entry of `dry_sf_reason`. |
| [`FilamentSwitchInlet`](#filamentswitchinlet) | enum | Which Filament Track Switch inlet an AMS unit feeds through. |
| [`VirtualTray`](#virtualtray) | type | External spool holder: `vt_tray` on single-nozzle models, each `vir_slot` entry on IDEX. |

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

  Configured drying duration in hours (firmware range 1-24), not minutes.
  
  Confirmed against BambuStudio, which declares the same field as `dry_hour`
  (`DeviceCore/DevFilaSystem.h`, comment "hours") and parses it unscaled
  (`DevFilaSystem.cpp`), with a "1-24 h" UI input hint (`AMSDryControl.cpp`);
  ha-bambulab likewise exposes it as a `UnitOfTime.HOURS` sensor with the value
  taken unmodified off the wire.

- **`dry_filament`**: `Option<String>`

  Filament type string for the active drying profile (e.g. "PA-CF").

#### Trait Implementations

##### `impl Clone for AmsDrySetting`

- <span id="amsdrysetting-clone"></span>`fn clone(&self) -> AmsDrySetting` — [`AmsDrySetting`](#amsdrysetting)

##### `impl Debug for AmsDrySetting`

- <span id="amsdrysetting-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for AmsDrySetting`

- <span id="amsdrysetting-default"></span>`fn default() -> AmsDrySetting` — [`AmsDrySetting`](#amsdrysetting)

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

  Active RFID read bitmask (hex string), in the same bit layout as `tray_exist_bits`
  ([REF-AMS-DECODE]).

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
  independently by `bambu-printer-manager` (`bambucommands.py`, `bambutools.py`)
  and `OpenBambuAPI/local-printer-api.md` (community protocol spec).

- **`cfs`**: `Option<Vec<AmsFilamentStep>>`

  Per-slot filament-change step codes. Confirmed against BambuStudio's
  `DevFilaSystem.cpp` (`GetVal<std::vector<DevFilamentStep>>(jj["ams"], "cfs")`);
  consistent with pybambu's `MOCK-X2D.json:184-189` fixture (`"cfs": [2, 9, 5, 7]`).

#### Implementations

- <span id="amsstatusreport-unit"></span>`fn unit(&self, ams_id: u8) -> Option<&AmsUnit>` — [`AmsUnit`](#amsunit)

  The unit at bus id `ams_id`, as normalized on ingest (an A2L's AMS Lite is `6`).

- <span id="amsstatusreport-tray"></span>`fn tray(&self, ams_id: u8, slot: u8) -> Option<&AmsTray>` — [`AmsTray`](#amstray)

  Slot `slot` of the unit at `ams_id`, if both were reported.

#### Trait Implementations

##### `impl Clone for AmsStatusReport`

- <span id="amsstatusreport-clone"></span>`fn clone(&self) -> AmsStatusReport` — [`AmsStatusReport`](#amsstatusreport)

##### `impl Debug for AmsStatusReport`

- <span id="amsstatusreport-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for AmsStatusReport`

- <span id="amsstatusreport-default"></span>`fn default() -> AmsStatusReport` — [`AmsStatusReport`](#amsstatusreport)

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

  The physical index representing the slot (0 to 3), or an external holder's address
  (`"254"`/`"255"`) for a [`VirtualTray`](#virtualtray). Sent as a string on the wire; empty if omitted.

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

  Calibration index; -1 means no K profile is selected.
  
  -1 is not only "never calibrated": an X1C power-cycled mid-print came back with every
  tray at -1 while the spools were unchanged (bambuddy #3219).

- **`cols`**: `Option<Vec<String>>`

  Multi-color columns array (e.g. `["000000FF"]`).

- **`ctype`**: `Option<i32>`

  Color type indicator.

- **`total_len`**: `Option<u32>`

  Total filament spool length in mm.

- **`remain_g`**: `Option<i32>`

  Accurate remaining weight in grams, when firmware can resolve it. Distinct
  from `remain`'s coarse percentage estimate. Confirmed against BambuStudio's
  `DevFilaSystem.cpp`/`.h` (`remain_g`, introduced in commit `31637e013`,
  "ENH: support accurate filament remain weight", 2026-06-12) — firmware sends `-1` for
  "not provided", preserved here as the raw wire value; use `remaining_weight_grams()`
  for the sentinel-translated `Option<u32>`.

- **`filament_setting_id`**: `Option<String>`

  Filament preset ID BambuStudio resolves and prefers for print-preset auto-matching,
  distinct from `tray_info_idx`. Wire key is `setting_id`; renamed here to
  avoid confusion with `tray_info_idx`'s own doc name collision. Confirmed against
  BambuStudio's `DevFilaSystem.cpp` (`filament_setting_id`) and `DevMapping.cpp`
  (commit `d1f121d26`, 2026-06-09), which prefers this field over the coarser
  `filament_id` when auto-matching a spool to a slicer preset.

#### Implementations

- <span id="amstray-slot"></span>`fn slot(&self) -> Option<u8>`

  The slot index parsed from [`id`](#amstray), or `None` if it isn't a number.

- <span id="amstray-material"></span>`fn material(&self) -> Option<&str>`

  The material abbreviation (`"PLA"`, `"PETG"`, ...), or `None` when `tray_type` is
  absent or explicitly blank (empty or `"Empty"`).

- <span id="amstray-color-rgba"></span>`fn color_rgba(&self) -> Option<[u8; 4]>`

  The `RRGGBBAA` `tray_color` decoded to `[r, g, b, a]`, or `None` if absent or malformed.

- <span id="amstray-nozzle-temp-range"></span>`fn nozzle_temp_range(&self) -> Option<(u16, u16)>`

  `(min, max)` nozzle temperature in °C for the loaded filament, if both were reported.

- <span id="amstray-remain-percent"></span>`fn remain_percent(&self) -> Option<u8>`

  Remaining filament in percent, or `None` for the firmware's `-1` "not calculated"
  sentinel or any other out-of-range value.

- <span id="amstray-state"></span>`fn state(&self) -> u8`

  Retrieves the raw status code of the spool, defaulting to `9` (Empty) if omitted.

  **Not a loaded/empty answer on its own:** some firmware sends a fully populated tray
  with no `state` key, which reads as `9` here. Use [`is_loaded`](#amstray) to ask
  whether a spool is loaded.

- <span id="amstray-is-loaded"></span>`fn is_loaded(&self, ams_id: u8) -> bool`

  True when this tray, in the unit at `ams_id`, holds a loaded spool.

  The same rule `clean_stale_tray_data` applies before
  keeping a tray's material data: a missing `state` with filament metadata is loaded,
  states `9`/`10` mean empty except on AMS-HT units (`ams_id` 128-135, where they don't),
  and an explicitly blank `tray_type` means empty.

- <span id="amstray-remaining-weight-grams"></span>`fn remaining_weight_grams(&self) -> Option<u32>`

  Accurate remaining weight in grams, translating `remain_g`'s raw wire
  sentinel to `None`. Mirrors BambuStudio's `DevAmsTray::get_filament_remain_weight()`
  (`DevFilaSystem.cpp`): `remain_g < 0` means "not provided by firmware" and
  `remain_g == 0` means "confirmed empty," both `None` here; only a positive value is
  returned. Does not replicate BambuStudio's percentage-based fallback (`weight * remain
  / 100`) when `remain_g` is absent — callers needing that estimate already have
  `tray_weight`/`remain` to compute it themselves.

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
    pub temp: Option<String>,
    pub humidity: Option<String>,
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

  Unique index representing the unit position on the physical expansion bus.
  
  Standard AMS units report 0-3 and AMS-HT units 128-135, both verbatim. The A2L's AMS
  Lite reports physical id **16** on the wire and is normalized to **6** here, so that
  `tray_exist_bits` (whose bit base for this unit is 24 = `6 * 4`), `resolve_global_tray_id`
  and the mapping builders all agree; `MaterialSource::AmsLite` puts the physical 16 back
  on the outbound `ams_mapping2`.

- **`temp`**: `Option<String>`

  Ambient temperature inside the expansion enclosure, in degrees Celsius.
  
  Optional because BambuStudio reads it only when present (`ParseAmsInfo`,
  `DevFilaSystem.cpp`): a partial unit push without it must not fail the frame.

- **`humidity`**: `Option<String>`

  Enclosure climate relative humidity index (1-5 scale). Optional, as for `temp`.

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
  (per `AmsUnit::merge_from`) prunes every cached tray for this unit — `Option` gives
  exactly this absent-vs-present-empty distinction for free (absent key -> `None`,
  present key -> `Some(_)` however short), confirmed against BambuStudio's `DevFilaSystem.cpp`
  (`ParseAmsInfo`'s `if (j_ams.contains("tray"))` gate around both the per-tray parse
  loop and the prune-absent-ids loop).

- **`info`**: `Option<String>`

  Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status, bits 8–11 = extruder assignment (IDEX routing).

- **`dry_sf_reason`**: `Option<Vec<i32>>`

  Drying failure reason codes per slot (X2D).

#### Implementations

- <span id="amsunit-ams-id"></span>`fn ams_id(&self) -> Option<u8>`

  The unit's bus id parsed from [`id`](#amsunit), or `None` if it isn't a number.

- <span id="amsunit-temperature-c"></span>`fn temperature_c(&self) -> Option<f32>`

  Enclosure temperature in °C, from the `temp` string.

- <span id="amsunit-humidity-percent"></span>`fn humidity_percent(&self) -> Option<u8>`

  Relative humidity in percent, from `humidity_raw`.

- <span id="amsunit-humidity-level"></span>`fn humidity_level(&self) -> Option<u8>`

  Coarse humidity level `1..=5` from `humidity`, where **`1` is wettest and `5` driest**
  (`reference/05_materials_ams.md`, "Per-Unit Humidity").

- <span id="amsunit-parse-info"></span>`fn parse_info(&self) -> Option<u64>`

  Parses the hex-encoded `info` bitmask string into an integer.

- <span id="amsunit-ams-type"></span>`fn ams_type(&self) -> Option<u8>`

  Raw AMS unit type from bits 0–3 — e.g. `3` is an AMS 2 Pro, **not** an AMS Lite (`2`).

  Prefer [`unit_model`](#amsunit), which decodes this into [`AmsUnitModel`](#amsunitmodel) and
  carries the capability accessors. This stays for the one case that cannot serve: reading
  a unit type newer than this crate knows about.

- <span id="amsunit-unit-model"></span>`fn unit_model(&self) -> Option<AmsUnitModel>` — [`AmsUnitModel`](#amsunitmodel)

  Which physical AMS accessory this unit is, decoded from `info` bits 0–3.

  Use this rather than [`ams_type`](#amsunit) to ask whether the unit can dry, how
  many slots it has, or what temperature range its heater accepts — see [`AmsUnitModel`](#amsunitmodel).

  `None` when `info` is absent from the payload (older firmware omits it entirely) or when
  it carries a unit type this crate doesn't know. Both cases mean "don't assume a
  capability", which is the safe reading. This accessor is deliberately payload-local: the
  `info` module list carries the unit type a second time as a module-name prefix
  (`ams_f1/0`, `n3f/0`, `n3s/0`) and BambuStudio falls back to it when the bitmask is
  missing, but that lives in a different payload than this one.

- <span id="amsunit-dry-status"></span>`fn dry_status(&self) -> Option<AmsDryStatus>` — [`AmsDryStatus`](#amsdrystatus)

  Drying status from bits 4–7.

- <span id="amsunit-supports-drying"></span>`fn supports_drying(&self) -> Option<bool>`

  Whether this unit can dry: `None` when its type is unknown (no `info`, or a type newer
  than this crate), which is not the same as "can't dry".

- <span id="amsunit-dry-temp-range"></span>`fn dry_temp_range(&self) -> Option<(u32, u32)>`

  Inclusive drying temperature range in °C, or `None` if the unit can't dry or its type
  is unknown — see [`AmsUnitModel::dry_temp_range`](#amsunitmodel).

- <span id="amsunit-extruder-assignment"></span>`fn extruder_assignment(&self) -> Option<u8>`

  Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).
  Returns `None` when `info` is absent or the value is 0xE (uninitialized).

- <span id="amsunit-filament-switch-inlet"></span>`fn filament_switch_inlet(&self) -> Option<FilamentSwitchInlet>` — [`FilamentSwitchInlet`](#filamentswitchinlet)

  Filament Track Switch inlet this unit feeds, decoded from `bind_switch_in` (bits 24–27).

  Returns [`FilamentSwitchInlet::InB`](#filamentswitchinlet) for `0` and [`FilamentSwitchInlet::InA`](#filamentswitchinlet) for `1`;
  `None` for `info` absent, or any other value, which upstream treats as "not bound".

  **Only meaningful when [`extruder_assignment`](#amsunit) returns `None`
  because the raw field is `0xE`.** An AMS wired to a fixed extruder reports that extruder
  directly and this field carries nothing; `0xE` means "not fixed", and when a Filament
  Track Switch is installed, this is the only way to recover which physical nozzle the unit
  actually feeds. That matters beyond display: BambuStudio uses the resolved inlet to pick
  the K-profile for the feeding nozzle. Note `extruder_assignment` collapses `0xE` into
  `None` and cannot distinguish "uninitialized" from "routed through a switch", so a caller
  wanting that distinction must consult this method as well.

  The field is four bits, not the two this crate documented before BUG-136 — a 2-bit read
  aliases values 4–15 into 0–3 and reports a valid inlet for a unit that has none.

  **Unverified against hardware.** No Filament Track Switch has been available; the decode
  follows BambuStudio's `DevFilaSystem.cpp`, corroborated by bambuddy (`c5e00558`,
  `7a42e0a7`). See issue #137.

- <span id="amsunit-has-unfixed-extruder"></span>`fn has_unfixed_extruder(&self) -> bool`

  True when this unit reports `0xE` ("not wired to a fixed extruder") in bits 8–11.

  Distinguishes the two cases [`extruder_assignment`](#amsunit) folds into
  `None`: a unit routed through a Filament Track Switch, versus one whose assignment the
  firmware simply has not initialized. Pair with
  [`filament_switch_inlet`](#amsunit) to tell them apart — an unbound
  `bind_switch_in` alongside `0xE` means uninitialized.

- <span id="amsunit-dry-sub-status"></span>`fn dry_sub_status(&self) -> Option<AmsDrySubStatus>` — [`AmsDrySubStatus`](#amsdrysubstatus)

  Drying sub-status from bits 22–23.

- <span id="amsunit-dry-fan1-status"></span>`fn dry_fan1_status(&self) -> Option<AmsDryFanStatus>` — [`AmsDryFanStatus`](#amsdryfanstatus)

  Dry-fan 1 status from bits 18–19. Confirmed against BambuStudio's
  `DevFilaSystem.cpp` (`get_flag_bits(info, 18, 2)`) and independently by
  `bambu-printer-manager`'s `bambutools.py`, an exact match.

- <span id="amsunit-dry-fan2-status"></span>`fn dry_fan2_status(&self) -> Option<AmsDryFanStatus>` — [`AmsDryFanStatus`](#amsdryfanstatus)

  Dry-fan 2 status from bits 20–21. Confirmed against BambuStudio's
  `DevFilaSystem.cpp` (`get_flag_bits(info, 20, 2)`) and independently by
  `bambu-printer-manager`'s `bambutools.py`, an exact match.

- <span id="amsunit-dry-block-reasons"></span>`fn dry_block_reasons(&self) -> Option<Vec<DryBlockReason>>` — [`DryBlockReason`](#dryblockreason)

  Decodes [`dry_sf_reason`](#amsunit) into typed reasons, in reported order.

  A layer over the raw `Vec<i32>` rather than a replacement for it — the same relationship
  [`parse_info`](#amsunit) has with the typed `info` accessors. Unrecognized codes
  survive as [`DryBlockReason::Other`](#dryblockreason).

- <span id="amsunit-primary-dry-block-reason"></span>`fn primary_dry_block_reason(&self) -> Option<DryBlockReason>` — [`DryBlockReason`](#dryblockreason)

  The single reason worth showing a user when the firmware reports several at once.

  Mirrors bambuddy's `primary_reason_code`: a reason the user has to act on outranks one
  that clears on its own, because that is the only case where showing a message beats
  retrying silently. Ties break on reported order.

#### Trait Implementations

##### `impl Clone for AmsUnit`

- <span id="amsunit-clone"></span>`fn clone(&self) -> AmsUnit` — [`AmsUnit`](#amsunit)

##### `impl Debug for AmsUnit`

- <span id="amsunit-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for AmsUnit`

- <span id="amsunit-default"></span>`fn default() -> AmsUnit` — [`AmsUnit`](#amsunit)

##### `impl Deserialize<'de> for AmsUnit`

- <span id="amsunit-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsUnit`

##### `impl Serialize for AmsUnit`

- <span id="amsunit-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsDryFanStatus`

```rust
enum AmsDryFanStatus {
    Off,
    On,
    Other(u8),
}
```

State of one drying fan from `info` bits 18–19 or 20–21, BambuStudio's
`DevAms::DryFanStatus` (`DevFilaSystem.h`).

#### Variants

- **`Off`**

  `0` — off.

- **`On`**

  `1` — on.

- **`Other`**

  A value this crate doesn't know (`2` or `3`), preserved verbatim.

#### Trait Implementations

##### `impl Clone for AmsDryFanStatus`

- <span id="amsdryfanstatus-clone"></span>`fn clone(&self) -> AmsDryFanStatus` — [`AmsDryFanStatus`](#amsdryfanstatus)

##### `impl Copy for AmsDryFanStatus`

##### `impl Debug for AmsDryFanStatus`

- <span id="amsdryfanstatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsDryFanStatus`

##### `impl PartialEq for AmsDryFanStatus`

- <span id="amsdryfanstatus-partialeq-eq"></span>`fn eq(&self, other: &AmsDryFanStatus) -> bool` — [`AmsDryFanStatus`](#amsdryfanstatus)

### `AmsDryStatus`

```rust
enum AmsDryStatus {
    Off,
    Checking,
    Drying,
    Cooling,
    Stopping,
    Error,
    HeaterOutOfControl,
    ProductionTest,
    Other(u8),
}
```

Drying-cycle state from `info` bits 4–7, BambuStudio's `DevAms::DryStatus`
(`DevFilaSystem.h`).

#### Variants

- **`Off`**

  `0` — not drying.

- **`Checking`**

  `1` — checking conditions before starting.

- **`Drying`**

  `2` — drying.

- **`Cooling`**

  `3` — cooling down after a cycle.

- **`Stopping`**

  `4` — stopping.

- **`Error`**

  `5` — the cycle hit an error.

- **`HeaterOutOfControl`**

  `6` — the heater could not be stopped (BambuStudio `CannotStopHeatOutofControl`).

- **`ProductionTest`**

  `7` — factory production test (BambuStudio `PrdTesting`).

- **`Other`**

  A value this crate doesn't know, preserved verbatim.

#### Trait Implementations

##### `impl Clone for AmsDryStatus`

- <span id="amsdrystatus-clone"></span>`fn clone(&self) -> AmsDryStatus` — [`AmsDryStatus`](#amsdrystatus)

##### `impl Copy for AmsDryStatus`

##### `impl Debug for AmsDryStatus`

- <span id="amsdrystatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsDryStatus`

##### `impl PartialEq for AmsDryStatus`

- <span id="amsdrystatus-partialeq-eq"></span>`fn eq(&self, other: &AmsDryStatus) -> bool` — [`AmsDryStatus`](#amsdrystatus)

### `AmsDrySubStatus`

```rust
enum AmsDrySubStatus {
    Off,
    Heating,
    Dehumidifying,
    Other(u8),
}
```

Drying sub-state from `info` bits 22–23, BambuStudio's `DevAms::DrySubStatus`
(`DevFilaSystem.h`).

#### Variants

- **`Off`**

  `0` — idle.

- **`Heating`**

  `1` — heating.

- **`Dehumidifying`**

  `2` — dehumidifying.

- **`Other`**

  A value this crate doesn't know (`3`), preserved verbatim.

#### Trait Implementations

##### `impl Clone for AmsDrySubStatus`

- <span id="amsdrysubstatus-clone"></span>`fn clone(&self) -> AmsDrySubStatus` — [`AmsDrySubStatus`](#amsdrysubstatus)

##### `impl Copy for AmsDrySubStatus`

##### `impl Debug for AmsDrySubStatus`

- <span id="amsdrysubstatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsDrySubStatus`

##### `impl PartialEq for AmsDrySubStatus`

- <span id="amsdrysubstatus-partialeq-eq"></span>`fn eq(&self, other: &AmsDrySubStatus) -> bool` — [`AmsDrySubStatus`](#amsdrysubstatus)

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
(`DevDefs.h`) — used to type `AmsStatusReport.cfs`. `CheckPosition` covers both `0x08`
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

##### `impl Clone for AmsFilamentStep`

- <span id="amsfilamentstep-clone"></span>`fn clone(&self) -> AmsFilamentStep` — [`AmsFilamentStep`](#amsfilamentstep)

##### `impl Copy for AmsFilamentStep`

##### `impl Debug for AmsFilamentStep`

- <span id="amsfilamentstep-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsFilamentStep`

- <span id="amsfilamentstep-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsFilamentStep`

##### `impl Eq for AmsFilamentStep`

##### `impl Hash for AmsFilamentStep`

- <span id="amsfilamentstep-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AmsFilamentStep`

- <span id="amsfilamentstep-partialeq-eq"></span>`fn eq(&self, other: &AmsFilamentStep) -> bool` — [`AmsFilamentStep`](#amsfilamentstep)

##### `impl Serialize for AmsFilamentStep`

- <span id="amsfilamentstep-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsUnitModel`

```rust
enum AmsUnitModel {
    ExternalSpool,
    Ams,
    AmsLite,
    Ams2Pro,
    AmsHt,
    AmsLiteMixed,
}
```

Which physical AMS accessory is attached, decoded from `info` bits 0–3.

**A property of the accessory, not of the host printer.** The quirks engine answers questions
about the printer; this answers questions about the box plugged into it, and the two are
orthogonal. Remote drying in particular needs *both* gates to pass: an AMS that physically has
a heater (here) and a printer whose firmware acts on the command rather than acking and
discarding it (`ModelQuirks::ams_remote_drying_support`). BambuStudio writes the same pair out
longhand at `Widgets/AMSControl.cpp`.

Do not infer any of this from `ams_id`: `0..=3` is shared by the original AMS, the AMS Lite and
the AMS 2 Pro, and only the last of those can dry.

Wire numbering matches BambuStudio's `DevAmsType` (`DevDefs.h`), which casts these four
bits straight to it (`DevFilaSystem.cpp`). bambuddy reaches the same taxonomy by an
independent route — the `info` module-name prefix, `"ams"`/`"n3f"`/`"n3s"`
(`bambu_mqtt.py`) — and ha-bambulab spells out the full prefix map (`ams/N`,
`ams_f1/N`, `n3f/N`, `n3s/N`).

#### Variants

- **`ExternalSpool`**

  External spool / no unit. Wire value `0` (BambuStudio `EXT_SPOOL`).

- **`Ams`**

  The original 4-slot AMS. Wire value `1`. **No drying chamber.**

- **`AmsLite`**

  AMS Lite, as shipped with the A1 series. Wire value `2`. No drying chamber.

- **`Ams2Pro`**

  AMS 2 Pro. Wire value `3` (BambuStudio `N3F`). 4 slots, dries.

- **`AmsHt`**

  AMS-HT. Wire value `4` (BambuStudio `N3S`). Single slot, dries, higher ceiling.

- **`AmsLiteMixed`**

  AMS Lite variant for N9. Wire value `5` (BambuStudio `AMS_LITE_MIXED`). No drying chamber.

#### Implementations

- <span id="amsunitmodel-from-wire"></span>`fn from_wire(value: u8) -> Option<Self>`

  Decodes a raw `info` bits 0–3 value, or `None` for a unit type this crate doesn't know.

  An unknown value is deliberately not folded onto a neighbouring variant — firmware has
  added unit types before ([`AmsLiteMixed`](#amsunitmodel) being the most recent), and
  guessing a capability for one is how a drying command reaches a unit that can't dry. Read
  [`AmsUnit::ams_type`](#amsunit) for the raw value when this returns `None`.

- <span id="amsunitmodel-supports-drying"></span>`fn supports_drying(self) -> bool`

  Returns true if this unit has a drying chamber at all.

  True for [`Ams2Pro`](#amsunitmodel) and [`AmsHt`](#amsunitmodel) only. The original AMS and
  both AMS Lite variants have no heater, so a drying command addressed to one cannot do
  anything. Confirmed by BambuStudio (`Widgets/AMSItem.hpp`,
  `support_drying() { return ams_type == N3S || ams_type == N3F; }`) and independently by
  bambuddy (`print_scheduler.py`, `if module_type not in ("n3f", "n3s"): skip`).

- <span id="amsunitmodel-dry-temp-range"></span>`fn dry_temp_range(self) -> Option<(u32, u32)>`

  Inclusive `(min, max)` drying-chamber temperature range in °C, or `None` if this unit
  cannot dry.

  `(45, 65)` for the AMS 2 Pro and `(45, 85)` for the AMS-HT. **Both bounds are real** —
  BambuStudio refuses a temperature below the minimum just as it refuses one above the
  maximum (`AMSDryControl.cpp`), so a caller clamping only the ceiling still
  publishes values the vendor's own client rejects.

- <span id="amsunitmodel-slot-count"></span>`fn slot_count(self) -> Option<u8>`

  Spool slots this unit type has, or `None` where the type alone doesn't determine it.

  `1` for the AMS-HT, `4` for the original AMS, AMS Lite and AMS 2 Pro. `None` for
  [`ExternalSpool`](#amsunitmodel) and [`AmsLiteMixed`](#amsunitmodel): upstream
  has no static answer for those either and falls back to the observed tray count
  (BambuStudio `DevAms::GetSlotCount`), so count [`AmsUnit::tray`](#amsunit) rather than trusting a
  number invented here.

#### Trait Implementations

##### `impl Clone for AmsUnitModel`

- <span id="amsunitmodel-clone"></span>`fn clone(&self) -> AmsUnitModel` — [`AmsUnitModel`](#amsunitmodel)

##### `impl Copy for AmsUnitModel`

##### `impl Debug for AmsUnitModel`

- <span id="amsunitmodel-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsUnitModel`

##### `impl PartialEq for AmsUnitModel`

- <span id="amsunitmodel-partialeq-eq"></span>`fn eq(&self, other: &AmsUnitModel) -> bool` — [`AmsUnitModel`](#amsunitmodel)

### `DryBlockReason`

```rust
enum DryBlockReason {
    PrinterBusy,
    InsufficientPower,
    AmsBusy,
    FilamentAtOutlet,
    AlreadyStarting,
    Unsupported2dMode,
    AlreadyDrying,
    FirmwareUpgrading,
    ExternalPowerRequired,
    FilamentAtOutletManualUnload,
    Other(i32),
}
```

Why the firmware will not, or did not, start a drying cycle — one entry of `dry_sf_reason`.

**`dry_sf_reason` is a list of independent codes, not a bitmask.** `reference/05_materials_ams.md`
described it as one for a while and listed only `1` and `8`, whose reading as bit positions was
a coincidence; the field is an enumerated code list, which is why it deserializes as
`Vec<i32>`.

Codes from BambuStudio's `DevAms::CannotDryReason` (`DevFilaSystem.h`), which has ten
members; bambuddy's `DRY_SF_REASON_MESSAGES` (`backend/app/services/drying_preflight.py`)
agrees on `0`-`8` and omits `10`. The user-action split is bambuddy's — see
[`needs_user_action`](#dryblockreason).

#### Variants

- **`PrinterBusy`**

  `0` — the printer is busy.

- **`InsufficientPower`**

  `1` — insufficient power: too many AMS units drying at once, or an external PSU is
  required. Needs the user to change something.

- **`AmsBusy`**

  `2` — the AMS is busy.

- **`FilamentAtOutlet`**

  `3` — filament is sitting at the AMS outlet and must be retracted first. Needs the user.

- **`AlreadyStarting`**

  `4` — a drying cycle on this AMS is already starting.

- **`Unsupported2dMode`**

  `5` — not supported in 2D mode.

- **`AlreadyDrying`**

  `6` — the AMS is already drying.

- **`FirmwareUpgrading`**

  `7` — the AMS firmware is upgrading.

- **`ExternalPowerRequired`**

  `8` — the external AMS power adapter must be plugged in. Needs the user.

- **`FilamentAtOutletManualUnload`**

  `10` — filament is at the AMS outlet and must be unloaded by hand before drying.
  
  Needs the user. BambuStudio's `FilamentAtAmsOutletManualUnload`, whose message asks for a manual
  unload (`AMSDryControl.cpp`), unlike `3`, where Studio offers an unload button.

- **`Other`**

  A code this crate doesn't know — newer firmware may add reasons, and folding one onto a
  neighbouring variant would report a wrong cause with full confidence.

#### Implementations

- <span id="dryblockreason-from-code"></span>`fn from_code(code: i32) -> Self`

  Decodes one raw `dry_sf_reason` entry.

- <span id="dryblockreason-code"></span>`fn code(self) -> i32`

  The raw wire code this reason decodes from.

- <span id="dryblockreason-needs-user-action"></span>`fn needs_user_action(self) -> bool`

  Returns true if clearing this needs the user to physically do something.

  This is the distinction that decides a caller's behavior: retry in a moment, or stop and
  surface a message. True for [`InsufficientPower`](#dryblockreason) and
  [`ExternalPowerRequired`](#dryblockreason) (bambuddy's
  `POWER_REASON_CODES = {1, 8}`), for [`FilamentAtOutlet`](#dryblockreason)
  (`RETRACT_REASON_CODE = 3`), and for
  [`FilamentAtOutletManualUnload`](#dryblockreason), which bambuddy doesn't
  know; every other known reason clears on its own.

  [`Other`](#dryblockreason) returns `false` — an unknown reason is reported as transient
  because that is the reading that keeps a caller retrying rather than permanently refusing
  on a code that may be benign.

#### Trait Implementations

##### `impl Clone for DryBlockReason`

- <span id="dryblockreason-clone"></span>`fn clone(&self) -> DryBlockReason` — [`DryBlockReason`](#dryblockreason)

##### `impl Copy for DryBlockReason`

##### `impl Debug for DryBlockReason`

- <span id="dryblockreason-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DryBlockReason`

##### `impl PartialEq for DryBlockReason`

- <span id="dryblockreason-partialeq-eq"></span>`fn eq(&self, other: &DryBlockReason) -> bool` — [`DryBlockReason`](#dryblockreason)

### `FilamentSwitchInlet`

```rust
enum FilamentSwitchInlet {
    InA,
    InB,
}
```

Which Filament Track Switch inlet an AMS unit feeds through.

The FTS is an accessory that lets one AMS feed either printer nozzle through a shared switch,
instead of being wired to a fixed extruder. A unit routed this way reports `0xE`
("not fixed") for its extruder assignment, and the inlet below is the only thing that says
which physical nozzle it actually reaches.

An enum rather than a bare `u8` because the wire values (`0` = In-B, `1` = In-A) are inverted
relative to how the inlets read alphabetically, and every prior attempt to remember that from
a bare integer is a bug waiting to happen.

#### Variants

- **`InA`**

  Inlet In-A. Wire value `1`.

- **`InB`**

  Inlet In-B. Wire value `0`.

#### Trait Implementations

##### `impl Clone for FilamentSwitchInlet`

- <span id="filamentswitchinlet-clone"></span>`fn clone(&self) -> FilamentSwitchInlet` — [`FilamentSwitchInlet`](#filamentswitchinlet)

##### `impl Copy for FilamentSwitchInlet`

##### `impl Debug for FilamentSwitchInlet`

- <span id="filamentswitchinlet-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FilamentSwitchInlet`

##### `impl PartialEq for FilamentSwitchInlet`

- <span id="filamentswitchinlet-partialeq-eq"></span>`fn eq(&self, other: &FilamentSwitchInlet) -> bool` — [`FilamentSwitchInlet`](#filamentswitchinlet)

### `VirtualTray`

```rust
type VirtualTray = AmsTray;
```

External spool holder: `vt_tray` on single-nozzle models, each `vir_slot` entry on IDEX.

Its wire schema is an AMS tray's, so it is the same type, with the same fields, accessors and
merge. `id` is the holder's address (`"254"`/`"255"`), or empty if a push omitted it.

