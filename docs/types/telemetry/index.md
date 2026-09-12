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
  - [`ams`](ams/index.md)
  - [`device`](device/index.md)
  - [`diagnostics`](diagnostics/index.md)
  - [`report`](report/index.md)
  - [`stage`](stage/index.md)
  - [`xcam`](xcam/index.md)
- [Types](#types)
  - [`TelemetryReport`](#telemetryreport)
- [Functions](#functions)
  - [`decode_nozzle_temperatures`](#decode-nozzle-temperatures)
  - [`fun2_bit`](#fun2-bit)
  - [`is_developer_mode`](#is-developer-mode)
- [Constants](#constants)
  - [`FUN2_REMOTE_DRY_BIT`](#fun2-remote-dry-bit)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`ams`](ams/index.md) | mod | AMS telemetry types (tray slots, units, dry settings, virtual trays). |
| [`device`](device/index.md) | mod | Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras). |
| [`diagnostics`](diagnostics/index.md) | mod | Diagnostic telemetry types (HMS alerts, light reports). |
| [`report`](report/index.md) | mod | Top-level telemetry report envelope (`print` and `device` wire locations). |
| [`stage`](stage/index.md) | mod | Typed decoding for the `stg_cur` / `stg` stage-ID space. |
| [`xcam`](xcam/index.md) | mod | AI failure-detection and print-option settings (`print.xcam`). |
| [`TelemetryReport`](#telemetryreport) | struct | Unified top-level telemetry report received from the printer's local MQTT broker. |
| [`decode_nozzle_temperatures`](#decode-nozzle-temperatures) | fn | Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`](../../client/index.md#printerclient) — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived. |
| [`fun2_bit`](#fun2-bit) | fn | Reads one bit of a `fun2` capability hex string, LSB-first from the right. |
| [`is_developer_mode`](#is-developer-mode) | fn | Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1]. |
| [`FUN2_REMOTE_DRY_BIT`](#fun2-remote-dry-bit) | const | `fun2` bit reporting the printer's own remote-dry support (`DeviceManager.cpp:4469`). |

## Modules

- [`ams`](ams/index.md) — AMS telemetry types (tray slots, units, dry settings, virtual trays).
- [`device`](device/index.md) — Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).
- [`diagnostics`](diagnostics/index.md) — Diagnostic telemetry types (HMS alerts, light reports).
- [`report`](report/index.md) — Top-level telemetry report envelope (`print` and `device` wire locations).
- [`stage`](stage/index.md) — Typed decoding for the `stg_cur` / `stg` stage-ID space.
- [`xcam`](xcam/index.md) — AI failure-detection and print-option settings (`print.xcam`).


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

  This handles symmetrical empty slots safely on standard P1S and A1 Mini lines.

- <span id="amstray-remaining-weight-grams"></span>`fn remaining_weight_grams(&self) -> Option<u32>`

  Accurate remaining weight in grams, translating `remain_g`'s raw wire
  sentinel to `None`. Mirrors BambuStudio's `DevAmsTray::get_filament_remain_weight()`
  (`DevFilaSystem.cpp:116-124`): `remain_g < 0` means "not provided by firmware" and
  `remain_g == 0` means "confirmed empty," both `None` here; only a positive value is
  returned. Does not replicate BambuStudio's percentage-based fallback (`weight * remain
  / 100`) when `remain_g` is absent — callers needing that estimate already have
  `tray_weight`/`remain` to compute it themselves.

#### Trait Implementations

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

  Unique index representing the unit position on the physical expansion bus.
  
  Standard AMS units report 0-3 and AMS-HT units 128-135, both verbatim. The A2L's AMS
  Lite reports physical id **16** on the wire and is normalized to **6** here, so that
  `tray_exist_bits` (whose bit base for this unit is 24 = `6 * 4`), `resolve_global_tray_id`
  and the mapping builders all agree; `MaterialSource::AmsLite` puts the physical 16 back
  on the outbound `ams_mapping2`.

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

  Raw AMS unit type from bits 0–3 — e.g. `3` is an AMS 2 Pro, **not** an AMS Lite (`2`).

  Prefer [`unit_model`](ams/index.md#amsunit), which decodes this into [`AmsUnitModel`](ams/index.md#amsunitmodel) and
  carries the capability accessors. This stays for the one case that cannot serve: reading
  a unit type newer than this crate knows about.

- <span id="amsunit-unit-model"></span>`fn unit_model(&self) -> Option<AmsUnitModel>` — [`AmsUnitModel`](ams/index.md#amsunitmodel)

  Which physical AMS accessory this unit is, decoded from `info` bits 0–3.

  Use this rather than [`ams_type`](ams/index.md#amsunit) to ask whether the unit can dry, how
  many slots it has, or what temperature range its heater accepts — see [`AmsUnitModel`](ams/index.md#amsunitmodel).

  `None` when `info` is absent from the payload (older firmware omits it entirely) or when
  it carries a unit type this crate doesn't know. Both cases mean "don't assume a
  capability", which is the safe reading. This accessor is deliberately payload-local: the
  `info` module list carries the unit type a second time as a module-name prefix
  (`ams_f1/0`, `n3f/0`, `n3s/0`) and BambuStudio falls back to it when the bitmask is
  missing, but that lives in a different payload than this one.

- <span id="amsunit-dry-status"></span>`fn dry_status(&self) -> Option<u8>`

  Drying status from bits 4–7.

- <span id="amsunit-extruder-assignment"></span>`fn extruder_assignment(&self) -> Option<u8>`

  Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).
  Returns `None` when `info` is absent or the value is 0xE (uninitialized).

- <span id="amsunit-filament-switch-inlet"></span>`fn filament_switch_inlet(&self) -> Option<FilamentSwitchInlet>` — [`FilamentSwitchInlet`](ams/index.md#filamentswitchinlet)

  Filament Track Switch inlet this unit feeds, decoded from `bind_switch_in` (bits 24–27).

  Returns [`FilamentSwitchInlet::InB`](ams/index.md#filamentswitchinlet) for `0` and [`FilamentSwitchInlet::InA`](ams/index.md#filamentswitchinlet) for `1`;
  `None` for `info` absent, or any other value, which upstream treats as "not bound".

  **Only meaningful when [`extruder_assignment`](ams/index.md#amsunit) returns `None`
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
  follows BambuStudio's `DevFilaSystem.cpp:598-609`, corroborated by bambuddy (`c5e00558`,
  `7a42e0a7`). See issue #137.

- <span id="amsunit-has-unfixed-extruder"></span>`fn has_unfixed_extruder(&self) -> bool`

  True when this unit reports `0xE` ("not wired to a fixed extruder") in bits 8–11.

  Distinguishes the two cases [`extruder_assignment`](ams/index.md#amsunit) folds into
  `None`: a unit routed through a Filament Track Switch, versus one whose assignment the
  firmware simply has not initialized. Pair with
  [`filament_switch_inlet`](ams/index.md#amsunit) to tell them apart — an unbound
  `bind_switch_in` alongside `0xE` means uninitialized.

- <span id="amsunit-dry-sub-status"></span>`fn dry_sub_status(&self) -> Option<u8>`

  Drying sub-status from bits 22–23.

- <span id="amsunit-dry-fan1-status"></span>`fn dry_fan1_status(&self) -> Option<u8>`

  Dry-fan 1 status from bits 18–19. Confirmed against BambuStudio's
  `DevFilaSystem.cpp:696` (`get_flag_bits(info, 18, 2)`) and independently by
  `bambu-printer-manager`'s `bambutools.py:685`, an exact match.

- <span id="amsunit-dry-fan2-status"></span>`fn dry_fan2_status(&self) -> Option<u8>`

  Dry-fan 2 status from bits 20–21. Confirmed against BambuStudio's
  `DevFilaSystem.cpp:697` (`get_flag_bits(info, 20, 2)`) and independently by
  `bambu-printer-manager`'s `bambutools.py:686`, an exact match.

- <span id="amsunit-dry-block-reasons"></span>`fn dry_block_reasons(&self) -> Option<Vec<DryBlockReason>>` — [`DryBlockReason`](ams/index.md#dryblockreason)

  Decodes [`dry_sf_reason`](ams/index.md#amsunit) into typed reasons, in reported order.

  A layer over the raw `Vec<i32>` rather than a replacement for it — the same relationship
  [`parse_info`](ams/index.md#amsunit) has with the typed `info` accessors. Unrecognized codes
  survive as [`DryBlockReason::Other`](ams/index.md#dryblockreason).

- <span id="amsunit-primary-dry-block-reason"></span>`fn primary_dry_block_reason(&self) -> Option<DryBlockReason>` — [`DryBlockReason`](ams/index.md#dryblockreason)

  The single reason worth showing a user when the firmware reports several at once.

  Mirrors bambuddy's `primary_reason_code`: a reason the user has to act on outranks one
  that clears on its own, because that is the only case where showing a message beats
  retrying silently. Ties break on reported order.

#### Trait Implementations

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

  Part index matching hardware configurations (e.g., `160` for the second left-side
  auxiliary fan on X2D/P2S — despite the wire port number suggesting a "right" fan).

- **`state`**: `Option<i32>`

  The active operating speed percentage (`0` to `100`) or damper direction flag.

#### Trait Implementations

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
  
  Three bits are known, decoded by BambuStudio's `DevExtruderSystem.cpp:354-356` via
  `DevUtil::get_flag_bits(info, N)` (which reads a single bit at position `N`, its `count`
  defaulting to 1):
  
  | Bit | Mask | Meaning |
  |---|---|---|
  | 1 | `0b0010` | The extruder holds filament |
  | 2 | `0b0100` | The buffer holds filament |
  | 3 | `0b1000` | A nozzle is fitted |
  
  Read out of BambuStudio's parser rather than from a wire capture; bit 1 is independently
  corroborated by bambuddy's `ExtruderSlot`, which computes `has_filament` as
  `bool(flags & 0b10)`. Bits 0 and 4+ have no recorded meaning.

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
  [`ExtruderInfo::current_ams_slot`](device/index.md#extruderinfo)'s doc comment for the shared bit layout.

- <span id="extruderinfo-target-ams-slot"></span>`fn target_ams_slot(&self) -> Option<(u8, u8)>`

  Target `(ams_id, slot_id)` for an in-progress filament change, decoded from `star`. See
  [`ExtruderInfo::current_ams_slot`](device/index.md#extruderinfo)'s doc comment for the shared bit layout.

#### Trait Implementations

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
    pub p_t: Option<u64>,
}
```

Dynamic extruder nozzle details.

Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
(IDEX platforms) to provide unified schema matching.

#### Fields

- **`id`**: `u8`

  Extruder carriage index (0 = Right/Main, 1 = Left/Deputy), or on H2C, a packed rack
  slot: high nibble (bits 4–7) `1` flags a rack-stored spare nozzle, low nibble (bits
  0–3) is the slot index within the rack — see [`NozzleInfo::is_rack_stored()`](device/index.md#nozzleinfo).

- **`diameter`**: `Option<f32>`

  Nozzle orifice diameter in millimeters (e.g. 0.4).
  
  **Can be stale.** An empty hotend is still reported in `nozzle_info`, carrying the
  diameter of the nozzle it last held — measured on an idle H2C, where an unoccupied
  position read `diameter: 0.4` alongside `max_temp: 0` and serial `"N/A"`. Establish
  presence with [`NozzleInfo::is_installed()`](device/index.md#nozzleinfo) before trusting this.

- **`tm`**: `Option<u32>`

  Target maximum temperature (Standard Platform abbreviated representation).

- **`max_temp`**: `Option<u32>`

  Target maximum temperature (IDEX Platform verbose representation).

- **`nozzle_type`**: `Option<String>`

  Core physical nozzle composition or tool type designation.
  
  **Two vocabularies by generation.** Legacy printers report the nozzle *material* here
  (e.g. `"hardened_steel"`, `"stainless_steel"`); H2-generation printers report a *flow
  code* instead (`"HH"` = high flow, `"HS"` = standard, followed by a hardware-variant
  digit pair). Do not assume one and parse the other. The related but distinct
  `nozzle_id` on a K-profile entry uses the flow-code vocabulary only — see
  `crate::diagnostics::KProfileEntry::nozzle_id`.

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
  
  **Not a presence indicator.** It read `0` on every entry of an H2C's `nozzle_info`,
  occupied and empty alike — use [`NozzleInfo::is_installed()`](device/index.md#nozzleinfo) instead.

- **`p_t`**: `Option<u64>`

  Cumulative print time for this individual hotend.
  
  A wear/usage counter tied to the physical hotend rather than the position it sits in,
  which is what makes it meaningful on a rack machine where hotends are swapped between
  slots. Reported by H2C Vortek rack hotends; absent elsewhere — BambuStudio guards it with
  `if (njon.contains("p_t"))` and a `/*maybe not contains*/` note
  (`DevNozzleSystem.cpp:789-791`, parsing the same `device.nozzle` push this field comes
  from).
  
  **Units are seconds.** BambuStudio's nozzle-rack panel names the value `usedSeconds` and
  formats it as `usedSeconds / 3600` hours, falling back to `usedSeconds / 60` minutes
  under an hour and displaying `"0 h"` below a minute
  (`wgtDeviceNozzleRackUpdate.cpp:669-679`). ha-bambulab agrees independently, dividing by
  3600 for an hours sensor (`definitions.py:951`).

#### Implementations

- <span id="nozzleinfo-is-rack-stored"></span>`fn is_rack_stored(&self) -> bool`

  Returns whether this entry is a rack-stored spare nozzle rather than an installed one.

  Confirmed directly against BambuStudio's source
  (`DevNozzleSystem.cpp:769`, `DevNozzleSystemParser::ParseV2_0`) — rack-stored spare
  nozzles are appended to the *same* `nozzle.info` array as installed ones, distinguished
  by `DevUtil::get_hex_bits(id, 1) == 1`. `get_hex_bits(num, pos, base=10)` extracts the
  4-bit **nibble** at `pos*4` (`(num >> (pos*4)) & 0xF`), not a single bit — so this
  checks the *high* nibble (bits 4–7) of `id`, matching `reference/04_toolhead_thermal_
  motion.md`'s independently-documented H2C rack range of ids `16`-`21` (all of which
  have high nibble `1`; the low nibble `id & 0xF` is the rack slot index). Reachable on
  real hardware: H2C ("2 Slots, up to 7 active nozzles" per `MODEL_MATRIX.csv`) is a
  currently-modeled printer with existing rack-aware code elsewhere
  (`src/client/thermal.rs`'s H2C nozzle-ID validation, `src/quirks/mod.rs`).

- <span id="nozzleinfo-is-installed"></span>`fn is_installed(&self) -> bool`

  Returns whether a hotend is physically mounted in this position.

  An empty hotend is **not** omitted from `nozzle_info` — it is still reported, keeping
  the [`diameter`](device/index.md#nozzleinfo) of whatever it last held. Measured on an idle H2C, an
  unoccupied position read `diameter: 0.4`, `max_temp: 0`, serial `"N/A"`. So presence has
  to be *stated*, and neither field states it alone:

  * the serial must be the firmware's explicit `"N/A"` sentinel, **and**
  * the temperature rating must be absent or zero.

  Either check on its own gives a wrong answer on some model, because a firmware that
  reports neither field normalizes to the same shape as an empty hotend. Requiring both
  means a not-reported entry reads as installed, which is the safe direction: it defers to
  whatever the caller does with a present-but-unknown hotend rather than silently hiding
  one.

  [`stat`](device/index.md#nozzleinfo) is not usable for this — it read `0` on every entry, occupied and
  empty alike. An empty **rack dock** is a different case: it is absent from the payload
  entirely, so an id in `16..=21` already implies a nozzle is in it (see
  [`is_rack_stored()`](device/index.md#nozzleinfo)).

#### Trait Implementations

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

##### `impl Clone for NetInfo`

- <span id="netinfo-clone"></span>`fn clone(&self) -> NetInfo` — [`NetInfo`](report/index.md#netinfo)

##### `impl Debug for NetInfo`

- <span id="netinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NetInfo`

- <span id="netinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NetInfo`

##### `impl Serialize for NetInfo`

- <span id="netinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintPauseList`

```rust
struct PrintPauseList {
    pub total: Option<i32>,
    pub list: Option<Vec<PrintPausePoint>>,
}
```

The pause schedule for a running job, reported as `print.p_list`.

Lets a consumer see upcoming pauses (typically scheduled filament swaps) before they happen.
Nothing in bambino acts on this — it is surfaced so callers can.

**Field names and semantics come from BambuStudio's parser
(`DevPrintTaskInfo.cpp::parsePauseList`), not from a capture taken here.** The abbreviated
wire keys in particular have not been confirmed against a real `push_status` from a printer
running a job with scheduled pauses — see issue #139.

#### Fields

- **`total`**: `Option<i32>`

  Total number of pauses scheduled for the job.

- **`list`**: `Option<Vec<PrintPausePoint>>`

  The scheduled pauses themselves. Absent and empty are distinct on the wire; both mean
  "nothing to show" to a caller.

#### Implementations

- <span id="printpauselist-next-pause"></span>`fn next_pause(&self) -> Option<&PrintPausePoint>` — [`PrintPausePoint`](report/index.md#printpausepoint)

  Returns the next pending pause — the point with the lowest `pause_index`.

  Points with no `pause_index` are skipped rather than treated as index 0, which would make
  a malformed entry masquerade as the next pause. Returns `None` when the list is absent,
  empty, or entirely unindexed.

#### Trait Implementations

##### `impl Clone for PrintPauseList`

- <span id="printpauselist-clone"></span>`fn clone(&self) -> PrintPauseList` — [`PrintPauseList`](report/index.md#printpauselist)

##### `impl Debug for PrintPauseList`

- <span id="printpauselist-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrintPauseList`

- <span id="printpauselist-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrintPauseList`

##### `impl Eq for PrintPauseList`

##### `impl PartialEq for PrintPauseList`

- <span id="printpauselist-partialeq-eq"></span>`fn eq(&self, other: &PrintPauseList) -> bool` — [`PrintPauseList`](report/index.md#printpauselist)

##### `impl Serialize for PrintPauseList`

- <span id="printpauselist-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintPausePoint`

```rust
struct PrintPausePoint {
    pub progress_percent: Option<i32>,
    pub remaining_time_secs: Option<i32>,
    pub pause_index: Option<i32>,
    pub layer: Option<i32>,
}
```

One scheduled pause in a running job's pause list.

Wire keys are single letters (`p`/`t`/`i`/`l`), renamed here to something readable. Every
field is `Option` per this module's convention even though BambuStudio's parser requires all
four and discards the whole schedule if any is missing — bambino keeps what it can parse and
lets the caller decide, rather than dropping a pause list because one point is malformed.

#### Fields

- **`progress_percent`**: `Option<i32>`

  Percent complete at which this pause occurs.

- **`remaining_time_secs`**: `Option<i32>`

  Remaining print time at this pause, in seconds.

- **`pause_index`**: `Option<i32>`

  Index of this pause within the job's schedule.

- **`layer`**: `Option<i32>`

  Layer number at which this pause occurs.

#### Trait Implementations

##### `impl Clone for PrintPausePoint`

- <span id="printpausepoint-clone"></span>`fn clone(&self) -> PrintPausePoint` — [`PrintPausePoint`](report/index.md#printpausepoint)

##### `impl Debug for PrintPausePoint`

- <span id="printpausepoint-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrintPausePoint`

- <span id="printpausepoint-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrintPausePoint`

##### `impl Eq for PrintPausePoint`

##### `impl PartialEq for PrintPausePoint`

- <span id="printpausepoint-partialeq-eq"></span>`fn eq(&self, other: &PrintPausePoint) -> bool` — [`PrintPausePoint`](report/index.md#printpausepoint)

##### `impl Serialize for PrintPausePoint`

- <span id="printpausepoint-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrinterTelemetry`

```rust
struct PrinterTelemetry {
    pub gcode_state: Option<String>,
    pub command: Option<String>,
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
    pub ipcam: Option<super::diagnostics::IpcamTelemetry>,
    pub xcam: Option<super::xcam::XcamTelemetry>,
    pub ams: Option<super::ams::AmsStatusReport>,
    pub p_list: Option<PrintPauseList>,
    pub ams_status: Option<i32>,
    pub ams_mapping: Vec<i32>,
    pub vt_tray: Option<super::ams::VirtualTray>,
    pub vir_slot: Option<Vec<super::ams::VirtualTray>>,
    pub device: Option<super::device::DeviceTelemetry>,
    pub fun: Option<String>,
    pub fun2: Option<String>,
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
    pub aux: Option<String>,
    pub flag3: Option<u32>,
    pub stg: Option<Vec<i32>>,
    pub mapping: Option<Vec<i32>>,
    pub gcode_start_time: Option<String>,
    pub cali_version: Option<i32>,
    pub err: Option<String>,
    pub fail_reason: Option<String>,
    pub canvas_id: Option<String>,
    pub design_id: Option<String>,
    pub model_id: Option<String>,
    pub plate_idx: Option<i32>,
    pub profile_id: Option<String>,
    pub project_id: Option<String>,
    pub batch_id: Option<String>,
}
```

Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

#### Fields

- **`gcode_state`**: `Option<String>`

  High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").

- **`command`**: `Option<String>`

  Wire command name this frame arrived under (`"push_status"`/`"pushall"` for genuine
  telemetry pushes; a command-echo response — e.g. `"extrusion_cali_get"` — shares this
  same `print` envelope and can otherwise deserialize as an emptyish telemetry report
  [see poll_telemetry's command-echo filter].

- **`gcode_file`**: `Option<String>`

  Path or parent project file currently loaded for execution.

- **`subtask_name`**: `Option<String>`

  User-assigned name of the active print queue task.

- **`subtask_id`**: `Option<String>`

  Hardware-enforced unique 32-bit transaction identifier tracking active jobs.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.
  
  Permissive: bambuddy's `_probe_number` coerces this because "firmware is inconsistent
  about whether these arrive as ints or as numeric strings". BambuStudio reads it as a
  bare `get<int>()` and does not corroborate the string form, so the permissive binding is
  defensive rather than confirmed — it costs nothing and cannot fail a frame.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.
  Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.
  
  Permissive on the same single-source basis as `layer_num`.

- **`mc_remaining_time`**: `Option<i32>`

  Estimated remaining print duration, in **minutes**.
  
  The wire unit is minutes, not seconds — BambuStudio multiplies by 60 on both parse arms
  to reach its own seconds-based `mc_left_time` (`DeviceManager.cpp:3081-3086`), and
  bambuddy does the same (`notification_service.py:1163-1169`, "in minutes, convert to
  seconds"). Callers wanting seconds must multiply.
  
  Permissive: BambuStudio branches on `is_string()` here, so the quoted form is real.

- **`spd_lvl`**: `Option<u8>`

  Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).

- **`spd_mag`**: `Option<u16>`

  Speed magnitude as a percentage of the nominal feedrate.

- **`mc_percent`**: `Option<i32>`

  Motion controller progress percentage (0–100).
  
  Permissive: BambuStudio branches on `is_string()` (`DeviceManager.cpp:3060-3065`) and
  bambuddy coerces via `float()`/`_probe_number`, so the quoted form is confirmed.

- **`mc_print_sub_stage`**: `Option<i32>`

  Print sub-stage identifier tracking granular execution phases within the active print stage.

- **`mc_print_stage`**: `Option<String>`

  Motion controller print stage.
  
  Captures show the quoted form (`"2"`), but BambuStudio parses both an `is_string()` and
  an `is_number()` arm, so a bare number must not fail the frame. A numeric wire value is
  normalized to its decimal text.

- **`home_flag`**: `Option<u32>`

  Kinematics flag field tracking homing states, networking interfaces, and door nodes.
  
  Transmitted as a signed 32-bit int on the wire [REF-HOMEFLAG]; bit 31 set produces a
  negative JSON number that a bare `u32` target rejects, failing the whole telemetry
  message's deserialize. Masked into `u32` via `deserialize_signed_as_u32`.

- **`stat`**: `Option<String>`

  State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).

- **`stg_cur`**: `Option<i32>`

  Stage currently executing, drawn from the same ID space as [`Self::stg`](report/index.md#printertelemetry). Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].
  
  Emitted in incremental pushes, so it is usable for real-time stage tracking subject to
  the [REF-MQTT-IDLEBUG] `gcode_state` gate — A1/P1 firmware reports `0` ("printing") while
  genuinely idle, so the value means nothing unless `gcode_state` is `RUNNING` or `PAUSE`.

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

- **`ipcam`**: `Option<super::diagnostics::IpcamTelemetry>`

  Camera and recording telemetry. Nested as `print.ipcam` on the wire.

- **`xcam`**: `Option<super::xcam::XcamTelemetry>`

  AI detection settings (spaghetti detection, first-layer inspection, etc.).
  
  Appears to be pushall-only — no incremental `msg: 1` frame in this repo's captures
  carries it. Prefer [`xcam`](xcam/index.md), which caches and merges it.

- **`ams`**: `Option<super::ams::AmsStatusReport>`

  AMS expansion bus status container [REF-AMS-DECODE].

- **`p_list`**: `Option<PrintPauseList>`

  Schedule of pauses the firmware plans for the running job. Nested as `print.p_list`.
  
  Present only while a job with scheduled pauses (e.g. filament swaps) is loaded; absent
  otherwise, which is not an error.

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

- **`fun2`**: `Option<String>`

  Second capability bitfield (hex string), distinct from [`fun`](report/index.md#printertelemetry).
  
  Carries the printer's own firmware capability flags — most importantly bit 5,
  remote-dry support. Read via [`fun2_bit`](#telemetryreport) rather than directly:
  BambuStudio notes this string "may have infinite length" (`DeviceManager.cpp:4464`) and
  reads it with a no-border bit extractor, so it must not be parsed into a fixed-width
  integer the way `fun` is.

- **`print_type`**: `Option<String>`

  Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).

- **`lights_report`**: `Option<Vec<LightReport>>`

  Chamber/work/heatbed light states array.

- **`gcode_file_prepare_percent`**: `Option<String>`

  File download progress percentage (sent as string).

- **`hw_switch_state`**: `Option<i32>`

  Legacy main-extruder filament sensor state -- **not** a boolean, and not
  per-extruder.
  
  BambuStudio assigns this value unmodified to `MAIN_EXTRUDER_ID` only
  (`DeviceManager.cpp`, `parse_json`) and never bitmask-decodes it, so no
  interpretation beyond "non-zero means the main extruder reports filament" is
  confirmed. Dual-nozzle hardware (H2S/P2S/X2D-class) is observed sending values
  above 1 (`2` and `3` in captured telemetry), so a `== 1` comparison misreads
  those models.
  
  For per-extruder filament state on dual-nozzle models, read
  [`ExtruderCollection`](device/index.md) /
  [`ExtruderInfo`](device/index.md) instead, which model the V2
  per-extruder `info` bit field BambuStudio actually uses for the deputy extruder
  (`DevExtruderSystem.cpp`, `ExterSystemParser::ParseV2_0`).

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

  Hex config bitmask string of user-facing printer settings [REF-MQTT-TELEMETRY].
  
  Bit 18 is AMS Filament Backup (auto-refill), confirmed against both upstreams:
  BambuStudio decodes it as `SetAutoRefillEnabled(get_flag_bits(cfg, 18))`
  (`DeviceManager.cpp`, the `/*cfg*/` block), and `DevFilaSystem::CanShowFilamentBackup()`
  gates the "Filament Backup" UI on that same `IsAutoRefillEnabled()` — the auto-refill
  flag and the feature's user-facing name are the same thing. bambuddy reads the identical
  position in `parse_ams_filament_backup_from_cfg` (`services/bambu_mqtt.py`).
  
  A1 / A1 Mini omit `cfg` entirely, so absent is not "off" — hence `Option`.

- **`aux`**: `Option<String>`

  Auxiliary state hex string, sent only by firmware using BambuStudio's "np" payload format.
  
  Its *presence* is one quarter of BambuStudio's `check_enable_np` probe
  (`DeviceManager.cpp:4338-4346`) — see [`Self::reports_np_format`](report/index.md#printertelemetry). BambuStudio reads it
  as a string (`DeviceManager.cpp:4492`).

- **`flag3`**: `Option<u32>`

  Third capability bitfield.
  
  Bit 9 is BambuStudio's `is_enable_ams_np`, the AMS-side "np" flag
  (`DeviceManager.cpp:3111`), read alongside the `cfg`/`fun`/`aux`/`stat` probe — see
  [`Self::reports_np_format`](report/index.md#printertelemetry). Masked into `u32` like [`home_flag`](report/index.md#printertelemetry).

- **`stg`**: `Option<Vec<i32>>`

  Stage queue for the run in progress — the stages still to execute, emptied to `[]` at
  `FINISH`.
  
  Emitted in incremental (`msg: 1`) pushes, not only in `pushall` — see [REF-MQTT-IDLEBUG],
  which corrects an earlier claim to the contrary. For a standalone `calibration` command
  the queue tracks the option bitmask: bed-leveling alone gives `[14, 1]`, bed-leveling
  plus vibration compensation gives `[14, 1, 3]` (P1S, firmware `01.10.00.00`). Stage IDs
  follow BambuStudio's `get_stage_string` table; decode them with [`Self::stage_queue`](report/index.md#printertelemetry).
  
  Reflects what the firmware actually accepted: unsupported option bits are dropped from
  the queue without an error or a failed ack. [`start_calibration()`](../../client/index.md#printerclient)
  masks those bits up front so a caller does not have to diff this against the request.

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

- **`plate_idx`**: `Option<i32>`

  Which plate of a multi-plate 3MF the current job was sliced for.
  
  Needed to pull the right plate's metadata — thumbnail, filament list, bed temperature —
  out of the project file, since a 3MF's per-plate data is indexed on exactly this. It is
  also authoritative over the 3MF's own `slice_info`, which can name a different plate on
  a retained or reused archive.
  
  Firmware sends this as **either a number or a decimal string** — BambuStudio branches on
  `is_number()` / `is_string()` for exactly this field (`DeviceManager.cpp:2617-2626`), so
  the permissive deserializer is load-bearing rather than defensive: a bare `Option<i32>`
  would fail the entire telemetry frame on the string form.

- **`profile_id`**: `Option<String>`

  Cloud profile ID.

- **`project_id`**: `Option<String>`

  Cloud project ID.

- **`batch_id`**: `Option<String>`

  Cloud batch ID.

#### Implementations

- <span id="printertelemetry-reports-np-format"></span>`fn reports_np_format(&self) -> bool`

  Returns true if this frame shows the firmware uses BambuStudio's "np" payload format.

  This is not the MQTT version — the transport is MQTT 3.1.1 on every printer. It is a
  firmware-side difference in which JSON fields and commands the printer understands.
  BambuStudio tracks it as `is_enable_np` / `is_enable_ams_np` and never expands "np"; the
  name here follows it rather than guessing.

  Mirrors BambuStudio's selector (`StatusPanel.cpp:5376`,
  `obj->is_enable_np || obj->is_enable_ams_np`): either `cfg`, `fun`, `aux` and `stat` are
  all present (`check_enable_np`, `DeviceManager.cpp:4338-4346`), or `flag3` bit 9 is set
  (`DeviceManager.cpp:3111`). `false` means this frame didn't show it, which on a partial
  frame is not proof the firmware lacks it.

- <span id="printertelemetry-current-stage"></span>`fn current_stage(&self) -> Option<PrintStage>` — [`PrintStage`](stage/index.md#printstage)

  Returns the stage currently executing, decoded, or `None` when it cannot be trusted.

  Applies the [REF-MQTT-IDLEBUG] gate: A1/P1 firmware reports `stg_cur = 0` ("printing")
  while genuinely idle, so this returns `None` unless `gcode_state` is `RUNNING` or `PAUSE`.
  That gate matters more once the value is typed than it did when it was a bare `i32` — a
  [`PrintStage::Printing`](stage/index.md#printstage) rendered in a UI reads as authoritative. Use
  [`Self::current_stage_ungated`](report/index.md#printertelemetry) only when you are applying your own gate.

  A `Some(PrintStage::Idle)` during a run is not a bug and not completion: after the last
  queued stage finishes, `stg_cur` reads idle for the tail of the run.

- <span id="printertelemetry-current-stage-ungated"></span>`fn current_stage_ungated(&self) -> Option<PrintStage>` — [`PrintStage`](stage/index.md#printstage)

  Decodes `stg_cur` with no [REF-MQTT-IDLEBUG] gate applied.

  Prefer [`Self::current_stage`](report/index.md#printertelemetry). This exists for callers applying their own state gate;
  on an A1 or P1 the raw value is `0` ("printing") when the machine is idle.

- <span id="printertelemetry-stage-queue"></span>`fn stage_queue(&self) -> Vec<PrintStage>` — [`PrintStage`](stage/index.md#printstage)

  Decodes the queued stage list, in wire order.

  Needs no state gate — the idle-bug anomaly is specific to `stg_cur`, and an empty or
  absent queue is unambiguous. Returns an empty `Vec` when `stg` is absent; the queue also
  legitimately empties to `[]` at `FINISH`.

- <span id="printertelemetry-unpack-temperature"></span>`fn unpack_temperature(raw_val: f64) -> (u16, u16)`

  Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].

  Accepts `f64` because the wire sends both integers and floats depending on model.
  Values ≤ 500 are direct temperatures (target assumed 0°C). Values > 500 are
  composite-packed: upper 16 bits = target, lower 16 bits = actual.

- <span id="printertelemetry-is-ethernet-active"></span>`fn is_ethernet_active(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].

  Previously inspected bit 18 (`0x00040000`) of `home_flag`, following a
  pybambu-sourced heuristic. Both first-party clients (BambuStudio's
  `DevPrintOptions.cpp:26`, OrcaSlicer identically) actually decode that bit as
  `is_support_prompt_sound_detection`, unrelated to networking — confirmed wrong, not
  merely disputed. Real wired-ethernet state comes from `print.net.conf` bit 0
  (`DeviceManager.cpp:3053`: `network_wired = (net.conf & 0x1) != 0`). Returns `false`
  (not `None`) when `net`/`net.conf` haven't been observed yet, matching
  `is_ethernet_active_via_wifi_signal()`'s existing no-signal-observed convention.

- <span id="printertelemetry-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet using the `wifi_signal` sentinel value [REF-NET-PORTS], as a fallback for firmware that doesn't populate `print.net.conf`.

  A printer with no wifi signal to report (i.e. running wired-only) sends a fixed
  `wifi_signal` of `"-90dBm"`. Prefer `is_ethernet_active()` — this heuristic is kept
  only as a fallback for firmware that doesn't send `net.conf`.

- <span id="printertelemetry-is-220v-power"></span>`fn is_220v_power(&self) -> bool`

  Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.

  Used by [`crate::quirks::ModelQuirks::bed_temp_max`](../../quirks/index.md#modelquirks) on X1C, where the safe bed
  temperature ceiling is genuinely voltage-dependent (110°C @220V, 120°C @110V per the
  official spec sheet.

- <span id="printertelemetry-sdcard-state"></span>`fn sdcard_state(&self) -> Option<SdcardState>` — [`SdcardState`](report/index.md#sdcardstate)

  Evaluates the SD-card presence/health state from `home_flag` bits 8–9. See
  [`SdcardState`](report/index.md#sdcardstate)'s doc comment for verification sources. Returns `None` before any
  telemetry carrying `home_flag` has been observed — distinct from `Some(NoSdcard)`.

- <span id="printertelemetry-is-door-open-from-home-flag"></span>`fn is_door_open_from_home_flag(&self) -> bool`

  Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].

  Used by X1 series models where the door sensor is wired to the home_flag bitmask.

- <span id="printertelemetry-is-door-open-from-stat"></span>`fn is_door_open_from_stat(&self) -> bool`

  Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].

  Used by H2, P2, and X2 series models where the door sensor state is encoded in the `stat` string.

#### Trait Implementations

##### `impl Clone for PrinterTelemetry`

- <span id="printertelemetry-clone"></span>`fn clone(&self) -> PrinterTelemetry` — [`PrinterTelemetry`](report/index.md#printertelemetry)

##### `impl Debug for PrinterTelemetry`

- <span id="printertelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrinterTelemetry`

- <span id="printertelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrinterTelemetry`

##### `impl Serialize for PrinterTelemetry`

- <span id="printertelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `XcamDetector`

```rust
struct XcamDetector {
    pub enabled: bool,
    pub sensitivity: Option<XcamSensitivity>,
}
```

One AI failure detector's decoded state.

#### Fields

- **`enabled`**: `bool`

  Whether the firmware is running this detector.

- **`sensitivity`**: `Option<XcamSensitivity>`

  How eagerly it halts the print, or `None` if the field holds the unassigned value `3`.

#### Trait Implementations

##### `impl Clone for XcamDetector`

- <span id="xcamdetector-clone"></span>`fn clone(&self) -> XcamDetector` — [`XcamDetector`](xcam/index.md#xcamdetector)

##### `impl Copy for XcamDetector`

##### `impl Debug for XcamDetector`

- <span id="xcamdetector-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for XcamDetector`

##### `impl PartialEq for XcamDetector`

- <span id="xcamdetector-partialeq-eq"></span>`fn eq(&self, other: &XcamDetector) -> bool` — [`XcamDetector`](xcam/index.md#xcamdetector)

### `XcamTelemetry`

```rust
struct XcamTelemetry {
    pub cfg: Option<u32>,
    pub printing_monitor: Option<bool>,
    pub spaghetti_detector: Option<bool>,
    pub print_halt: Option<bool>,
    pub halt_print_sensitivity: Option<String>,
    pub first_layer_inspector: Option<bool>,
    pub buildplate_marker_detector: Option<bool>,
    pub allow_skip_parts: Option<bool>,
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}
```

AI detection and print-option settings, nested as `print.xcam` on the wire.

Every field is `Option` because which keys arrive depends on both firmware generation and
model, and because `xcam` appears to be pushall-only — an incremental `msg: 1` frame carries
none of it. Use [`merge_from`](xcam/index.md#xcamtelemetry) rather than replacing a cached copy wholesale.

Unmodeled keys survive in [`extra`](xcam/index.md#xcamtelemetry): this wire object is still largely uncharted
and model-dependent, so round-tripping a report must not silently drop what it carries.

#### Fields

- **`cfg`**: `Option<u32>`

  New-gen packed detector bitmask. Decode via the detector accessors, not by hand.
  
  Its presence is also what BambuStudio uses to decide the printer supports AI monitoring
  at all — see [`supports_ai_monitoring`](xcam/index.md#xcamtelemetry).

- **`printing_monitor`**: `Option<bool>`

  Mid-generation AI-monitoring master switch, superseded by `cfg`.

- **`spaghetti_detector`**: `Option<bool>`

  Oldest-generation AI-monitoring master switch, superseded by `printing_monitor`.

- **`print_halt`**: `Option<bool>`

  Whether a detected failure halts the print rather than only warning.

- **`halt_print_sensitivity`**: `Option<String>`

  AI-monitoring sensitivity as a bare string (`"low"`/`"medium"`/`"high"`).
  
  Old-gen only, and bambuddy reports it as reliably stale (`bambu_mqtt.py:2723`, "it's always
  stale"). Prefer the per-detector sensitivity off `cfg` whenever `cfg` is present.

- **`first_layer_inspector`**: `Option<bool>`

  Whether first-layer inspection is enabled.

- **`buildplate_marker_detector`**: `Option<bool>`

  Whether buildplate-marker detection is enabled.

- **`allow_skip_parts`**: `Option<bool>`

  Per-job or runtime flag whose exact semantics are **not settled** — do not gate anything
  on it.
  
  It is not a model-capability flag (Bambu documents every current model as supporting
  skip-objects, yet this reads `false` in all twelve upstream fixtures carrying it) and not a
  user setting (no such setting exists in Bambu Studio or on the printer). Gating
  [`crate::client::PrinterClient::skip_objects`](../../client/index.md#printerclient) on it would break skip-objects outright on
  hardware that supports the feature.

- **`extra`**: `std::collections::BTreeMap<String, serde_json::Value>`

  Every `xcam` key this struct does not model, preserved verbatim.
  
  `auto_recovery_step_loss` and `filament_tangle_detect` land here deliberately: bambuddy
  reads them out of `xcam` (`bambu_mqtt.py:2792-2795`, itself commented "tracked locally
  only"), but BambuStudio sources both from `home_flag` instead (bits 4 and 20), and no
  capture shows either inside `xcam`. Same for `ipcam_record`/`timelapse`, which bambino
  models under [`IpcamTelemetry`](diagnostics/index.md#ipcamtelemetry) from `print.ipcam`.

#### Implementations

- <span id="xcamtelemetry-supports-ai-monitoring"></span>`fn supports_ai_monitoring(&self) -> bool`

  Returns whether this printer supports on-device AI failure monitoring.

  Presence of `cfg` is the signal, matching BambuStudio's `is_support_detect` assignment.

- <span id="xcamtelemetry-ai-monitoring-enabled"></span>`fn ai_monitoring_enabled(&self) -> Option<bool>`

  Returns whether AI print monitoring is enabled, across all three protocol generations.

  Checks `cfg`'s spaghetti-detection bit first, then `printing_monitor`, then
  `spaghetti_detector` — the same precedence BambuStudio applies. `None` means no
  generation's key was present.

- <span id="xcamtelemetry-spaghetti-detection"></span>`fn spaghetti_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](xcam/index.md#xcamdetector)

  Returns spaghetti-detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-purge-chute-pileup-detection"></span>`fn purge_chute_pileup_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](xcam/index.md#xcamdetector)

  Returns purge-chute-pileup detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-nozzle-clumping-detection"></span>`fn nozzle_clumping_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](xcam/index.md#xcamdetector)

  Returns nozzle-clumping detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-air-printing-detection"></span>`fn air_printing_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](xcam/index.md#xcamdetector)

  Returns air-printing detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-buildplate-align-detection"></span>`fn buildplate_align_detection(&self) -> Option<bool>`

  Returns whether buildplate-alignment detection is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-fod-check"></span>`fn fod_check(&self) -> Option<bool>`

  Returns whether the foreign-object-detection check is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-displacement-detection"></span>`fn displacement_detection(&self) -> Option<bool>`

  Returns whether displacement detection is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-merge-from"></span>`fn merge_from(&mut self, incoming: &XcamTelemetry)` — [`XcamTelemetry`](xcam/index.md#xcamtelemetry)

  Merges a freshly-parsed `XcamTelemetry` into `self` field-by-field.

  Mirrors `IpcamTelemetry::merge_from` and exists for the same reason:
  a frame that carries only part of the object must not blank the rest of a cached copy.
  Present fields overwrite; absent ones leave the cached value alone. `extra` merges per key
  rather than being replaced, so an unmodeled key seen once survives later partial frames.

#### Trait Implementations

##### `impl Clone for XcamTelemetry`

- <span id="xcamtelemetry-clone"></span>`fn clone(&self) -> XcamTelemetry` — [`XcamTelemetry`](xcam/index.md#xcamtelemetry)

##### `impl Debug for XcamTelemetry`

- <span id="xcamtelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for XcamTelemetry`

- <span id="xcamtelemetry-default"></span>`fn default() -> XcamTelemetry` — [`XcamTelemetry`](xcam/index.md#xcamtelemetry)

##### `impl Deserialize<'de> for XcamTelemetry`

- <span id="xcamtelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for XcamTelemetry`

##### `impl Serialize for XcamTelemetry`

- <span id="xcamtelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `TelemetryReport`

```rust
struct TelemetryReport {
    pub print: Option<PrinterTelemetry>,
    pub device: Option<DeviceTelemetry>,
    pub fun: Option<String>,
    pub fun2: Option<String>,
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

- **`fun2`**: `Option<String>`

  Second capability bitfield (hex string) — see [`PrinterTelemetry::fun2`](report/index.md#printertelemetry).
  
  Accepted at the top level as well as inside `print` on the same first-found-wins terms as
  [`fun`](#telemetryreport). BambuStudio itself reads only `print.fun2`
  (`DeviceManager.cpp:4459`); the top-level slot mirrors `fun`'s documented drift rather
  than a location observed carrying `fun2`.

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

  Mirrors `bed_temperatures()`'s first-found-wins fallback: top-level `device` (incremental
  updates) is checked first, falling back to pushall-nested `print.device` (H2/P2/X2
  models). Returns `None` if neither location is present. Use this instead of manually
  checking both locations for nozzle/extruder/airduct/ctc/ext_tool sub-telemetry.

- <span id="telemetryreport-fun"></span>`fn fun(&self) -> Option<&str>`

  Returns the `fun` Developer LAN Mode bitmask, checking both wire locations it can
  arrive at.

  Mirrors `device()`'s fallback order — top-level `fun` is checked first,
  falling back to `print.fun` [REF-MQTT-ENV §3.2.1]. Prefer this over reading `self.fun`
  directly, the same way `device()` is preferred over `self.device`.

- <span id="telemetryreport-fun2"></span>`fn fun2(&self) -> Option<&str>`

  Returns the `fun2` capability bitfield, checking both wire locations.

  Same first-found-wins order as [`fun`](#telemetryreport). Prefer [`fun2_bit`](#telemetryreport)
  over parsing this yourself — see that method for why the string can't go through
  `u64::from_str_radix`.

- <span id="telemetryreport-fun2-bit"></span>`fn fun2_bit(&self, bit: u32) -> Option<bool>`

  Reads a single bit of the `fun2` capability bitfield.

  `None` only when `fun2` is absent or carries no hex digits at all — "the printer didn't
  say", which is distinct from a bit that is present and clear. A bit index past the end of
  the string reads `false`, matching BambuStudio's extractor, which returns `0` rather than
  failing (`DevUtil.cpp:53`).

  Known bits (`DeviceManager.cpp:4466-4477`): `0` print with eMMC, `3` PA mode,
  **`5` remote dry supported** (see [`supports_remote_dry`](#telemetryreport)),
  `6` update-remain hide display, `7` print TPU from left extruder (model-gated),
  `8` active arc fitting, `17` model internal storage, `19` check track-switch matches
  sliced printer, `21`-`22` AMS preload version, `23` filament manual multi-color.

- <span id="telemetryreport-supports-remote-dry"></span>`fn supports_remote_dry(&self) -> Option<bool>`

  Whether the printer reports its own support for remote AMS drying — `fun2` bit 5.

  This is the printer-side half of the drying gate; the attached unit's heater is the other
  half (see [`AmsUnitModel::supports_drying`](ams/index.md#amsunitmodel)). BambuStudio requires both
  (`Widgets/AMSControl.cpp:348`).

  `None` means the printer never reported `fun2`, which is not the same as reporting `0` —
  older firmware omits the field entirely, and treating that as "unsupported" would refuse
  drying on hardware that has always accepted it.

#### Trait Implementations

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
discarding it (`ModelQuirks::supports_ams_remote_drying`). BambuStudio writes the same pair out
longhand at `Widgets/AMSControl.cpp:348`.

Do not infer any of this from `ams_id`: `0..=3` is shared by the original AMS, the AMS Lite and
the AMS 2 Pro, and only the last of those can dry.

Wire numbering matches BambuStudio's `DevAmsType` (`DevDefs.h:54-62`), which casts these four
bits straight to it (`DevFilaSystem.cpp:598`). bambuddy reaches the same taxonomy by an
independent route — the `info` module-name prefix, `"ams"`/`"n3f"`/`"n3s"`
(`bambu_mqtt.py:2492`) — and ha-bambulab spells out the full prefix map (`ams/N`,
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
  added unit types before ([`AmsLiteMixed`](ams/index.md#amsunitmodel) being the most recent), and
  guessing a capability for one is how a drying command reaches a unit that can't dry. Read
  [`AmsUnit::ams_type`](ams/index.md#amsunit) for the raw value when this returns `None`.

- <span id="amsunitmodel-supports-drying"></span>`fn supports_drying(self) -> bool`

  Returns true if this unit has a drying chamber at all.

  True for [`Ams2Pro`](ams/index.md#amsunitmodel) and [`AmsHt`](ams/index.md#amsunitmodel) only. The original AMS and
  both AMS Lite variants have no heater, so a drying command addressed to one cannot do
  anything. Confirmed by BambuStudio (`Widgets/AMSItem.hpp:255`,
  `support_drying() { return ams_type == N3S || ams_type == N3F; }`) and independently by
  bambuddy (`print_scheduler.py:3976`, `if module_type not in ("n3f", "n3s"): skip`).

- <span id="amsunitmodel-dry-temp-range"></span>`fn dry_temp_range(self) -> Option<(u32, u32)>`

  Inclusive `(min, max)` drying-chamber temperature range in °C, or `None` if this unit
  cannot dry.

  `(45, 65)` for the AMS 2 Pro and `(45, 85)` for the AMS-HT. **Both bounds are real** —
  BambuStudio refuses a temperature below the minimum just as it refuses one above the
  maximum (`AMSDryControl.cpp:1186-1199`), so a caller clamping only the ceiling still
  publishes values the vendor's own client rejects.

- <span id="amsunitmodel-slot-count"></span>`fn slot_count(self) -> Option<u8>`

  Spool slots this unit type has, or `None` where the type alone doesn't determine it.

  `1` for the AMS-HT, `4` for the original AMS, AMS Lite and AMS 2 Pro. `None` for
  [`ExternalSpool`](ams/index.md#amsunitmodel) and [`AmsLiteMixed`](ams/index.md#amsunitmodel): upstream
  has no static answer for those either and falls back to the observed tray count
  (BambuStudio `DevAms::GetSlotCount`), so count [`AmsUnit::tray`](ams/index.md#amsunit) rather than trusting a
  number invented here.

#### Trait Implementations

##### `impl Clone for AmsUnitModel`

- <span id="amsunitmodel-clone"></span>`fn clone(&self) -> AmsUnitModel` — [`AmsUnitModel`](ams/index.md#amsunitmodel)

##### `impl Copy for AmsUnitModel`

##### `impl Debug for AmsUnitModel`

- <span id="amsunitmodel-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsUnitModel`

##### `impl PartialEq for AmsUnitModel`

- <span id="amsunitmodel-partialeq-eq"></span>`fn eq(&self, other: &AmsUnitModel) -> bool` — [`AmsUnitModel`](ams/index.md#amsunitmodel)

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

Codes from BambuStudio's `DevAms::CannotDryReason` (`DevFilaSystem.h:167-179`), which has ten
members; bambuddy's `DRY_SF_REASON_MESSAGES` (`backend/app/services/drying_preflight.py`)
agrees on `0`-`8` and omits `10`. The user-action split is bambuddy's — see
[`needs_user_action`](ams/index.md#dryblockreason).

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
  unload (`AMSDryControl.cpp:1355-1357`), unlike `3`, where Studio offers an unload button.

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
  surface a message. True for [`InsufficientPower`](ams/index.md#dryblockreason) and
  [`ExternalPowerRequired`](ams/index.md#dryblockreason) (bambuddy's
  `POWER_REASON_CODES = {1, 8}`), for [`FilamentAtOutlet`](ams/index.md#dryblockreason)
  (`RETRACT_REASON_CODE = 3`), and for
  [`FilamentAtOutletManualUnload`](ams/index.md#dryblockreason), which bambuddy doesn't
  know; every other known reason clears on its own.

  [`Other`](ams/index.md#dryblockreason) returns `false` — an unknown reason is reported as transient
  because that is the reading that keeps a caller retrying rather than permanently refusing
  on a code that may be benign.

#### Trait Implementations

##### `impl Clone for DryBlockReason`

- <span id="dryblockreason-clone"></span>`fn clone(&self) -> DryBlockReason` — [`DryBlockReason`](ams/index.md#dryblockreason)

##### `impl Copy for DryBlockReason`

##### `impl Debug for DryBlockReason`

- <span id="dryblockreason-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DryBlockReason`

##### `impl PartialEq for DryBlockReason`

- <span id="dryblockreason-partialeq-eq"></span>`fn eq(&self, other: &DryBlockReason) -> bool` — [`DryBlockReason`](ams/index.md#dryblockreason)

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

Deliberately not `Copy`-cheap-`u8` — the wire values (`0` = In-B, `1` = In-A) are inverted
relative to how the inlets read alphabetically, and every prior attempt to remember that from
a bare integer is a bug waiting to happen.

#### Variants

- **`InA`**

  Inlet In-A. Wire value `1`.

- **`InB`**

  Inlet In-B. Wire value `0`.

#### Trait Implementations

##### `impl Clone for FilamentSwitchInlet`

- <span id="filamentswitchinlet-clone"></span>`fn clone(&self) -> FilamentSwitchInlet` — [`FilamentSwitchInlet`](ams/index.md#filamentswitchinlet)

##### `impl Copy for FilamentSwitchInlet`

##### `impl Debug for FilamentSwitchInlet`

- <span id="filamentswitchinlet-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FilamentSwitchInlet`

##### `impl PartialEq for FilamentSwitchInlet`

- <span id="filamentswitchinlet-partialeq-eq"></span>`fn eq(&self, other: &FilamentSwitchInlet) -> bool` — [`FilamentSwitchInlet`](ams/index.md#filamentswitchinlet)

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

### `PrintStage`

```rust
enum PrintStage {
    Idle,
    Printing,
    AutoBedLeveling,
    HeatbedPreheating,
    VibrationCompensation,
    ChangingFilament,
    M400Pause,
    PausedFilamentRunout,
    HeatingNozzle,
    CalibratingDynamicFlow,
    ScanningBedSurface,
    InspectingFirstLayer,
    IdentifyingBuildPlateType,
    CalibratingMicroLidar,
    HomingToolhead,
    CleaningNozzleTip,
    CheckingExtruderTemperature,
    PausedByUser,
    PausedFrontCoverFallOff,
    CalibratingMicroLidarAlt,
    CalibratingFlowRatio,
    PausedNozzleTemperatureMalfunction,
    PausedHeatbedTemperatureMalfunction,
    FilamentUnloading,
    PausedStepLoss,
    FilamentLoading,
    MotorNoiseCancellation,
    PausedAmsOffline,
    PausedLowHeatbreakFanSpeed,
    PausedChamberTemperatureControlProblem,
    CoolingChamber,
    PausedUserGcode,
    MotorNoiseShowoff,
    PausedNozzleClumping,
    PausedCutterError,
    PausedFirstLayerError,
    PausedNozzleClog,
    MeasuringMotionPrecision,
    EnhancingMotionPrecision,
    MeasureMotionAccuracy,
    NozzleOffsetCalibration,
    HighTemperatureAutoBedLeveling,
    AutoCheckQuickReleaseLever,
    AutoCheckDoorAndUpperCover,
    LaserCalibration,
    AutoCheckPlatform,
    ConfirmingBirdsEyeCameraLocation,
    CalibratingBirdsEyeCamera,
    AutoBedLevelingPhase1,
    AutoBedLevelingPhase2,
    HeatingChamber,
    AdjustingHeatbedTemperature,
    PrintingCalibrationLines,
    AutoCheckMaterial,
    LiveViewCameraCalibration,
    WaitingForHeatbedTemperature,
    AutoCheckMaterialPosition,
    CuttingModuleOffsetCalibration,
    MeasuringSurface,
    ThermalPreconditioning,
    HomingBladeHolder,
    CalibratingCameraOffset,
    CalibratingBladeHolderPosition,
    HotendPickAndPlaceTest,
    WaitingForChamberTemperatureEqualize,
    PreparingHotend,
    CalibratingNozzleClumpingDetection,
    PurifyingChamberAir,
    MeasuringRotaryAttachment,
    ToolheadMovingAbovePurgeChute,
    CoolingNozzle,
    ToolheadMovingToHeatbedCenter,
    ActiveArcFitting,
    HotendTypeDetection,
    BuildPlateAlignmentDetection,
    HeatbedSurfaceForeignObjectDetection,
    HeatbedUndersideForeignObjectDetection,
    PreExtrusionBeforePrinting,
    PreparingAms,
    Unknown(i32),
}
```

A stage the printer reports in `stg_cur` (currently executing) or `stg` (queued).

The wire carries bare integers. [`PrintStage::from_wire`](stage/index.md#printstage) maps them to variants and leaves
anything unrecognized in [`PrintStage::Unknown`](stage/index.md#printstage) rather than failing, so a firmware that adds
a stage id does not break decoding.

# Read this before trusting a decoded value

Decoding does not make `stg_cur` trustworthy on its own. A1 and P1 firmware reports
`stg_cur = 0` ([`PrintStage::Printing`](stage/index.md#printstage)) while genuinely idle [REF-MQTT-IDLEBUG], so the value
means nothing unless `gcode_state` is `RUNNING` or `PAUSE`. Gate on that before displaying a
stage, or use a helper that does it for you — a typed [`PrintStage::Printing`](stage/index.md#printstage) handed to a UI
looks authoritative in a way the raw `0` did not, which makes ignoring the gate *more*
dangerous here, not less.

[`PrintStage::Idle`](stage/index.md#printstage) returning from a run in progress is also normal: once the last queued
stage finishes, `stg_cur` reads idle for the remainder of the run while `mc_percent` keeps
climbing. Completion is `gcode_state`/`mc_percent`, never this.

# Verification

Ids `0`–`77` come from BambuStudio's own `get_stage_string` (`DeviceManager.cpp`), the vendor
client's table. bambuddy's independent `STAGE_NAMES` corroborates 69 of the 78 and contradicts
none of them except id `74`, where bambuddy carries a self-described guess ("Preparing", noted
upstream as seen on H2D) and BambuStudio is taken as authoritative. Ids `67`–`73`, `75` and
`76` appear in BambuStudio alone.

Directly observed on hardware here (P1S, firmware `01.10.00.00`): `0`, `1`, `3`, `14`, `25`
and the `255` idle encoding. Everything else is upstream-sourced, not locally captured.

Idle is reported as `-1` on X1 and `255` on P1; both normalize to [`PrintStage::Idle`](stage/index.md#printstage), which
is the split a raw `i32` would otherwise leak to every consumer.

#### Variants

- **`Idle`**

  Idle. Wire sends `-1` on X1 and `255` on P1; both map here.

- **`Printing`**

  Printing.

- **`AutoBedLeveling`**

  Auto bed leveling.

- **`HeatbedPreheating`**

  Heatbed preheating.

- **`VibrationCompensation`**

  Resonance sweep. Upstream's machine name for this is `sweeping_xy_mech_mode`.

- **`ChangingFilament`**

  Changing filament.

- **`M400Pause`**

  M400 pause.

- **`PausedFilamentRunout`**

  Paused (filament ran out).

- **`HeatingNozzle`**

  Heating nozzle.

- **`CalibratingDynamicFlow`**

  Calibrating dynamic flow.

- **`ScanningBedSurface`**

  Scanning bed surface.

- **`InspectingFirstLayer`**

  Inspecting first layer.

- **`IdentifyingBuildPlateType`**

  Identifying build plate type.

- **`CalibratingMicroLidar`**

  Calibrating Micro Lidar.

- **`HomingToolhead`**

  Homing toolhead.

- **`CleaningNozzleTip`**

  Cleaning nozzle tip.

- **`CheckingExtruderTemperature`**

  Checking extruder temperature.

- **`PausedByUser`**

  Paused by the user.

- **`PausedFrontCoverFallOff`**

  Pause (front cover fall off).

- **`CalibratingMicroLidarAlt`**

  Second micro-lidar calibration id. Upstream marks `12` and `18` as duplicated.

- **`CalibratingFlowRatio`**

  Calibrating flow ratio.

- **`PausedNozzleTemperatureMalfunction`**

  Pause (nozzle temperature malfunction).

- **`PausedHeatbedTemperatureMalfunction`**

  Pause (heatbed temperature malfunction).

- **`FilamentUnloading`**

  Filament unloading.

- **`PausedStepLoss`**

  Pause (step loss).

- **`FilamentLoading`**

  Filament loading.

- **`MotorNoiseCancellation`**

  Motor noise cancellation.

- **`PausedAmsOffline`**

  Pause (AMS offline).

- **`PausedLowHeatbreakFanSpeed`**

  Pause (low speed of the heatbreak fan).

- **`PausedChamberTemperatureControlProblem`**

  Pause (chamber temperature control problem).

- **`CoolingChamber`**

  Cooling chamber.

- **`PausedUserGcode`**

  Pause (Gcode inserted by user).

- **`MotorNoiseShowoff`**

  Motor noise showoff.

- **`PausedNozzleClumping`**

  Pause (nozzle clumping).

- **`PausedCutterError`**

  Pause (cutter error).

- **`PausedFirstLayerError`**

  Pause (first layer error).

- **`PausedNozzleClog`**

  Pause (nozzle clog).

- **`MeasuringMotionPrecision`**

  Measuring motion precision.

- **`EnhancingMotionPrecision`**

  Enhancing motion precision.

- **`MeasureMotionAccuracy`**

  Measure motion accuracy.

- **`NozzleOffsetCalibration`**

  Nozzle offset calibration.

- **`HighTemperatureAutoBedLeveling`**

  High temperature auto bed leveling.

- **`AutoCheckQuickReleaseLever`**

  Auto Check: Quick Release Lever.

- **`AutoCheckDoorAndUpperCover`**

  Auto Check: Door and Upper Cover.

- **`LaserCalibration`**

  Laser Calibration.

- **`AutoCheckPlatform`**

  Auto Check: Platform.

- **`ConfirmingBirdsEyeCameraLocation`**

  Confirming BirdsEye Camera location.

- **`CalibratingBirdsEyeCamera`**

  Calibrating BirdsEye Camera.

- **`AutoBedLevelingPhase1`**

  Auto bed leveling - phase 1.

- **`AutoBedLevelingPhase2`**

  Auto bed leveling - phase 2.

- **`HeatingChamber`**

  Heating chamber.

- **`AdjustingHeatbedTemperature`**

  Adjusting heatbed temperature.

- **`PrintingCalibrationLines`**

  Printing calibration lines.

- **`AutoCheckMaterial`**

  Auto Check: Material.

- **`LiveViewCameraCalibration`**

  Live View Camera Calibration.

- **`WaitingForHeatbedTemperature`**

  Waiting for heatbed to reach target temperature.

- **`AutoCheckMaterialPosition`**

  Auto Check: Material Position.

- **`CuttingModuleOffsetCalibration`**

  Cutting Module Offset Calibration.

- **`MeasuringSurface`**

  Measuring Surface.

- **`ThermalPreconditioning`**

  Thermal Preconditioning for first layer optimization.

- **`HomingBladeHolder`**

  Homing Blade Holder.

- **`CalibratingCameraOffset`**

  Calibrating Camera Offset.

- **`CalibratingBladeHolderPosition`**

  Calibrating Blade Holder Position.

- **`HotendPickAndPlaceTest`**

  Hotend Pick and Place Test.

- **`WaitingForChamberTemperatureEqualize`**

  Waiting for the Chamber temperature to equalize.

- **`PreparingHotend`**

  Preparing Hotend.

- **`CalibratingNozzleClumpingDetection`**

  Calibrating the detection position of nozzle clumping.

- **`PurifyingChamberAir`**

  Purifying the chamber air.

- **`MeasuringRotaryAttachment`**

  Measuring Rotary Attachment.

- **`ToolheadMovingAbovePurgeChute`**

  The toolhead moves above the purge chute.

- **`CoolingNozzle`**

  Cooling down the nozzle.

- **`ToolheadMovingToHeatbedCenter`**

  The toolhead moves to the center of the heatbed.

- **`ActiveArcFitting`**

  Active Arc Fitting.

- **`HotendTypeDetection`**

  Hotend Type Detection.

- **`BuildPlateAlignmentDetection`**

  Build plate alignment detection.

- **`HeatbedSurfaceForeignObjectDetection`**

  Heatbed surface foreign object detection.

- **`HeatbedUndersideForeignObjectDetection`**

  Heatbed underside foreign object detection.

- **`PreExtrusionBeforePrinting`**

  Pre-extrusion before printing.

- **`PreparingAms`**

  Preparing AMS.

- **`Unknown`**

  A stage id no upstream table covers. Carries the raw wire value.

#### Implementations

- <span id="printstage-from-wire"></span>`fn from_wire(value: i32) -> Self`

  Decodes a raw `stg_cur` / `stg` wire value.

- <span id="printstage-is-idle"></span>`fn is_idle(self) -> bool`

  Returns true for the idle encodings (`-1` on X1, `255` on P1).

  Not a completion test: `stg_cur` reads idle for the tail of a calibration run that is
  still in progress. Check `gcode_state` for that.

- <span id="printstage-is-paused"></span>`fn is_paused(self) -> bool`

  Returns true if this stage is one of the paused states.

- <span id="printstage-label"></span>`fn label(self) -> Option<&'static str>`

  Human-readable label, matching BambuStudio's own wording.

  Returns `None` for [`PrintStage::Unknown`](stage/index.md#printstage) — the caller decides how to render an id no
  upstream table covers, rather than getting a fabricated label.

#### Trait Implementations

##### `impl Clone for PrintStage`

- <span id="printstage-clone"></span>`fn clone(&self) -> PrintStage` — [`PrintStage`](stage/index.md#printstage)

##### `impl Copy for PrintStage`

##### `impl Debug for PrintStage`

- <span id="printstage-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintStage`

##### `impl Hash for PrintStage`

- <span id="printstage-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintStage`

- <span id="printstage-partialeq-eq"></span>`fn eq(&self, other: &PrintStage) -> bool` — [`PrintStage`](stage/index.md#printstage)

### `XcamSensitivity`

```rust
enum XcamSensitivity {
    Low,
    Medium,
    High,
}
```

Sensitivity level attached to an AI failure detector.

Applies only to the four camera-based failure detectors (spaghetti, purge-chute pileup,
nozzle clumping, air printing). Unrelated to skip-objects or to `allow_skip_parts`.

#### Variants

- **`Low`**

  Least eager to halt the print.

- **`Medium`**

  Firmware default on every capture observed so far.

- **`High`**

  Most eager to halt the print.

#### Implementations

- <span id="xcamsensitivity-as-str"></span>`fn as_str(&self) -> &'static str`

  Returns the wire spelling BambuStudio uses for this level (`"low"`/`"medium"`/`"high"`).

#### Trait Implementations

##### `impl Clone for XcamSensitivity`

- <span id="xcamsensitivity-clone"></span>`fn clone(&self) -> XcamSensitivity` — [`XcamSensitivity`](xcam/index.md#xcamsensitivity)

##### `impl Copy for XcamSensitivity`

##### `impl Debug for XcamSensitivity`

- <span id="xcamsensitivity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for XcamSensitivity`

- <span id="xcamsensitivity-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for XcamSensitivity`

##### `impl Eq for XcamSensitivity`

##### `impl PartialEq for XcamSensitivity`

- <span id="xcamsensitivity-partialeq-eq"></span>`fn eq(&self, other: &XcamSensitivity) -> bool` — [`XcamSensitivity`](xcam/index.md#xcamsensitivity)

##### `impl Serialize for XcamSensitivity`

- <span id="xcamsensitivity-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`


---

## Functions

### `decode_nozzle_temperatures`

```rust
fn decode_nozzle_temperatures(device: Option<&DeviceTelemetry>, nozzle_temper: Option<f64>, nozzle_target_temper: Option<f64>) -> Vec<(u8, u16, u16)>
```

**Types:** [`DeviceTelemetry`](device/index.md#devicetelemetry)

Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`](../../client/index.md#printerclient) — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived.

Returns one `(id, actual, target)` tuple per nozzle. Prefers `device.extruder.info`
(composite-packed per-nozzle temperatures, decoded via [`ExtruderInfo::temperatures()`](device/index.md#extruderinfo)).
Falls back to the flat `nozzle_temper`/`nozzle_target_temper` fields when absent: a single
entry `(0, actual, target)` for a single-nozzle model, or — for a dual-nozzle (IDEX) model
with no live extruder temps yet — the wire's undocumented routing quirk: `nozzle_temper` is
nozzle 1 (left)'s actual reading and `nozzle_target_temper` is nozzle 0 (right)'s target,
each nozzle only getting half of its own reading from the flat fields.

### `fun2_bit`

```rust
fn fun2_bit(hex: &str, bit: u32) -> Option<bool>
```

Reads one bit of a `fun2` capability hex string, LSB-first from the right.

The counterpart to [`is_developer_mode`](#is-developer-mode) for the second capability field, and the free
function behind [`fun2_bit`](#fun2-bit) — use this when holding a `fun2` string on its
own rather than a whole report.

`fun2` "may have infinite length" per BambuStudio's own comment
(`DeviceManager.cpp:4464`), which is why this walks hex digits from the right instead of
going through `u64::from_str_radix` the way [`is_developer_mode`](#is-developer-mode) does for `fun` — a string
longer than 16 digits would fail that parse outright and report every capability as absent.

Mirrors `DevUtil::get_flag_bits_no_border` (`DevUtil.cpp:27-90`): a `0x` prefix and any
non-hex characters are ignored, and an index past the end of the string reads `false` rather
than failing. Returns `None` only when no hex digits remain after filtering.

### `is_developer_mode`

```rust
fn is_developer_mode(fun_hex: &str) -> Option<bool>
```

Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].

Returns `Some(true)` when developer mode is enabled (MQTT signature NOT required),
`Some(false)` when disabled, or `None` if the hex string is unparseable.
The `fun` field is a variable-length hex string (up to 64 bits). Bit 29
(`0x20000000`) is the `MQTT_SIGNATURE_REQUIRED` flag — when clear, developer mode is on.


---

## Constants

### `FUN2_REMOTE_DRY_BIT`
```rust
const FUN2_REMOTE_DRY_BIT: u32 = 5u32;
```

`fun2` bit reporting the printer's own remote-dry support (`DeviceManager.cpp:4469`).

