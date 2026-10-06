*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [ams](index.md)*

---

# Module `ams`

AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).

## Contents

- [Types](#types)
  - [`AmsChangeFilamentPayload`](#amschangefilamentpayload)
  - [`AmsControlPayload`](#amscontrolpayload)
  - [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload)
  - [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload)
  - [`AmsGetRfidPayload`](#amsgetrfidpayload)
  - [`ChangeTemps`](#changetemps)
  - [`DryingParams`](#dryingparams)
  - [`FilamentSpec`](#filamentspec)
  - [`AmsControlOp`](#amscontrolop)
  - [`AmsChangeFilamentRequest`](#amschangefilamentrequest)
  - [`AmsControlRequest`](#amscontrolrequest)
  - [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest)
  - [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest)
  - [`AmsGetRfidRequest`](#amsgetrfidrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsChangeFilamentPayload`](#amschangefilamentpayload) | struct | Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP]. |
| [`AmsControlPayload`](#amscontrolpayload) | struct | Commands standard AMS controllers to resume, pause, or reset physical material feeds. |
| [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload) | struct | Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER]. |
| [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload) | struct | Overwrites physical attributes or custom slicer presets assigned to a specific tray. |
| [`AmsGetRfidPayload`](#amsgetrfidpayload) | struct | Triggers physical filament feeder movement to scan proprietary RFID tag properties. |
| [`ChangeTemps`](#changetemps) | struct | The nozzle temperatures an `ams_change_filament` carries, °C; `-1` lets the firmware decide. |
| [`DryingParams`](#dryingparams) | struct | Everything a drying-cycle start carries besides the unit and the mode. |
| [`FilamentSpec`](#filamentspec) | struct | The description an `ams_filament_setting` can't do without. |
| [`AmsControlOp`](#amscontrolop) | enum | An `ams_control` operation on the AMS feed mechanism. |
| [`AmsChangeFilamentRequest`](#amschangefilamentrequest) | type | Loads or unloads filament from an AMS slot or external spool to the toolhead. |
| [`AmsControlRequest`](#amscontrolrequest) | type | Sends a resume, pause, or reset command to the AMS feed mechanism. |
| [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest) | type | Starts or stops a filament drying cycle on an AMS unit with a built-in heater. |
| [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest) | type | Sets filament properties (type, color, temperature range) on an AMS tray or external spool. |
| [`AmsGetRfidRequest`](#amsgetrfidrequest) | type | Requests an RFID tag scan on a specific AMS slot. |

## Types

### `AmsChangeFilamentPayload`

```rust
struct AmsChangeFilamentPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub slot_id: i32,
    pub target: i32,
    pub curr_temp: i32,
    pub tar_temp: i32,
    pub sequence_id: super::ClampedTaskId,
    pub extruder_id: Option<u8>,
}
```

Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP].

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_change_filament"`.

- **`ams_id`**: `i32`

  Target AMS unit index (or external-spool address per the caller's convention).

- **`slot_id`**: `i32`

  Target slot index within the AMS unit.

- **`target`**: `i32`

  Load/unload destination slot, derived by [`AmsChangeFilamentRequest::load`](#amschangefilamentrequest)/`unload` —
  see there.

- **`curr_temp`**: `i32`

  Current nozzle temperature (-1 = let firmware decide).

- **`tar_temp`**: `i32`

  Target nozzle temperature (-1 = let firmware decide).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`extruder_id`**: `Option<u8>`

  Which hotend to feed — `0` = right/main, `1` = left/deputy. Omitted from the wire when
  `None`, matching BambuStudio, whose `DeviceManager::command_ams_change_filament` takes
  it as an optional field and leaves it out unless a Filament Track Switch is fitted.
  
  **Required on a Filament Track Switch machine.** Without a switch each AMS is wired to
  exactly one hotend and the firmware derives the target from that binding, so naming it
  is redundant. With one fitted the situation inverts: every AMS reports its extruder as
  "not fixed" (`0xE`, see [`ExtruderInfo`](../../../types/telemetry/device/index.md#extruderinfo)) and is plumbed into
  one of the switch's two inlets, from which it can reach either hotend — so a command
  naming neither extruder is **discarded in silence**. On an H2C that presents as load
  and unload simply doing nothing.

#### Trait Implementations

##### `impl Clone for AmsChangeFilamentPayload`

- <span id="amschangefilamentpayload-clone"></span>`fn clone(&self) -> AmsChangeFilamentPayload` — [`AmsChangeFilamentPayload`](#amschangefilamentpayload)

##### `impl Debug for AmsChangeFilamentPayload`

- <span id="amschangefilamentpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsChangeFilamentPayload`

- <span id="amschangefilamentpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsControlPayload`

```rust
struct AmsControlPayload {
    pub command: &'static str,
    pub param: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Commands standard AMS controllers to resume, pause, or reset physical material feeds.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_control"`.

- **`param`**: `&'static str`

  Target operation — see [`AmsControlOp`](#amscontrolop).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsControlPayload`

- <span id="amscontrolpayload-clone"></span>`fn clone(&self) -> AmsControlPayload` — [`AmsControlPayload`](#amscontrolpayload)

##### `impl Debug for AmsControlPayload`

- <span id="amscontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsControlPayload`

- <span id="amscontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentDryingPayload`

```rust
struct AmsFilamentDryingPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub mode: i32,
    pub filament: String,
    pub temp: u32,
    pub duration: u32,
    pub humidity: u32,
    pub rotate_tray: bool,
    pub cooling_temp: u32,
    pub close_power_conflict: bool,
    pub sequence_id: super::ClampedTaskId,
}
```

Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].

Field set and shapes rewritten to match the real wire protocol — confirmed
against BambuStudio's `DevFilaSystem::CtrlAmsStartDryingHour`/`CtrlAmsStopDrying`
(`DevFilaSystemCtrl.cpp:18-53`, the sole outbound `ams_filament_drying` constructor in the
tree) and independently corroborated by bambuddy's `send_drying_command`
(`bambu_mqtt.py:4141-4171`, whose own comment cites real-hardware silent-rejection
incident #1447).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_filament_drying"`.

- **`ams_id`**: `i32`

  Target AMS unit index.

- **`mode`**: `i32`

  1 = start drying (`OnTime`), 0 = stop drying (`Off`) — `DevAms::DryCtrlMode`.

- **`filament`**: `String`

  Filament material type being dried (e.g. "PA-CF").

- **`temp`**: `u32`

  Drying temperature (°C).

- **`duration`**: `u32`

  Drying duration in **hours** (e.g., an 8-hour cycle = 8) — the wire field, unlike the
  old `dry_time`, is not in minutes.

- **`humidity`**: `u32`

  Target humidity (0 = firmware default / no target).

- **`rotate_tray`**: `bool`

  Whether to periodically rotate the tray during drying.

- **`cooling_temp`**: `u32`

  Cooling temperature applied after the drying cycle completes.

- **`close_power_conflict`**: `bool`

  Whether to override the AMS unit's power-conflict interlock.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-clone"></span>`fn clone(&self) -> AmsFilamentDryingPayload` — [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload)

##### `impl Debug for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentSettingPayload`

```rust
struct AmsFilamentSettingPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub ams_id: i32,
    pub slot_id: i32,
    pub tray_id: i32,
    pub tray_info_idx: String,
    pub tray_type: String,
    pub tray_sub_brands: String,
    pub tray_color: String,
    pub nozzle_temp_min: u32,
    pub nozzle_temp_max: u32,
    pub setting_id: Option<String>,
}
```

Overwrites physical attributes or custom slicer presets assigned to a specific tray.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_filament_setting"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`ams_id`**: `i32`

  Target AMS unit or external-spool address — see the addressing cheat-sheet on [`AmsFilamentSettingRequest::new`](#amsfilamentsettingrequest).

- **`slot_id`**: `i32`

  Slot position within the unit, as supplied by the caller.
  
  Distinct from [`tray_id`](#amsfilamentsettingpayload), and both are sent: they coincide on a standard
  AMS but not on an external spool, where this stays `0` while `tray_id` is `254`.

- **`tray_id`**: `i32`

  Derived addressing field — `254` for either external-spool `ams_id` (254/255), otherwise
  the slot index.
  
  Computed by [`AmsFilamentSettingRequest::new`](#amsfilamentsettingrequest) rather than caller-supplied, matching
  BambuStudio's `command_ams_filament_settings` (`DeviceManager.cpp:1707-1715`), so a
  caller cannot pair a `slot_id` with a `tray_id` that contradicts it.

- **`tray_info_idx`**: `String`

  **Short-format** filament preset code, e.g. `"GFA01"` or `"GFL05"` [REF-AMS-SP_CFG].
  
  Set from [`FilamentSpec::preset`](#filamentspec).
  
  This is *not* where a long `"PF"`-prefixed preset id belongs — that goes in
  [`setting_id`](#amsfilamentsettingpayload), which is a separate wire field. Putting a 19-character
  cloud id here is what produced the "truncation" an A1 was measured doing: it stored only
  the first 8 characters, uppercased, while acking the command as `"success"`, after which
  the slot resolves to Generic and drops out of the calibration table (which is keyed on
  this field).
  
  Both upstreams agree on the split: BambuStudio's `command_ams_filament_settings`
  (`DeviceManager.cpp:1723-1724`) assigns `tray_info_idx = filament_id` and
  `setting_id = setting_id` as two separate keys, and bambuddy's `ams_set_filament_setting`
  documents this parameter as "Filament ID short format (e.g. `GFL05`)" against its own
  distinct `setting_id`.

- **`tray_type`**: `String`

  Material type string (e.g. "PLA", "PETG").

- **`tray_sub_brands`**: `String`

  Sub-brand label (e.g. "Generic Basic"); defaults to `"{material_type} Basic"` when not given.

- **`tray_color`**: `String`

  Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").
  
  **Must be uppercase.** The firmware parses lowercase hex digits as `0` and stores the
  corrupted value silently — [`AmsFilamentSettingRequest::with_color`](#amsfilamentsettingrequest) normalizes for you.

- **`nozzle_temp_min`**: `u32`

  Minimum safe nozzle temperature (°C) for this filament.

- **`nozzle_temp_max`**: `u32`

  Maximum safe nozzle temperature (°C) for this filament.

- **`setting_id`**: `Option<String>`

  Full preset identifier — the long form, e.g. `"GFSL05_07"` or a `"PF"`-prefixed id.
  
  Omitted from the wire when `None`, matching both upstreams: BambuStudio always sends the
  key, bambuddy includes it only when non-empty, and the firmware accepts its absence.
  Supplying it helps the slicer resolve the correct profile for the slot.
  
  Distinct from [`tray_info_idx`](#amsfilamentsettingpayload), which takes the *short* code — see
  that field for what goes wrong when the two are conflated.

#### Trait Implementations

##### `impl Clone for AmsFilamentSettingPayload`

- <span id="amsfilamentsettingpayload-clone"></span>`fn clone(&self) -> AmsFilamentSettingPayload` — [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload)

##### `impl Debug for AmsFilamentSettingPayload`

- <span id="amsfilamentsettingpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentSettingPayload`

- <span id="amsfilamentsettingpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsGetRfidPayload`

```rust
struct AmsGetRfidPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub slot_id: i32,
    pub sequence_id: super::ClampedTaskId,
}
```

Triggers physical filament feeder movement to scan proprietary RFID tag properties.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_get_rfid"`.

- **`ams_id`**: `i32`

  Target AMS unit index.

- **`slot_id`**: `i32`

  Target slot index within the AMS unit.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-clone"></span>`fn clone(&self) -> AmsGetRfidPayload` — [`AmsGetRfidPayload`](#amsgetrfidpayload)

##### `impl Debug for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="changetemps-clone"></span>`fn clone(&self) -> ChangeTemps` — [`ChangeTemps`](#changetemps)

##### `impl Copy for ChangeTemps`

##### `impl Debug for ChangeTemps`

- <span id="changetemps-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for ChangeTemps`

##### `impl Hash for ChangeTemps`

- <span id="changetemps-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for ChangeTemps`

- <span id="changetemps-partialeq-eq"></span>`fn eq(&self, other: &ChangeTemps) -> bool` — [`ChangeTemps`](#changetemps)

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

- <span id="dryingparams-clone"></span>`fn clone(&self) -> DryingParams` — [`DryingParams`](#dryingparams)

##### `impl Debug for DryingParams`

- <span id="dryingparams-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for DryingParams`

- <span id="dryingparams-default"></span>`fn default() -> DryingParams` — [`DryingParams`](#dryingparams)

##### `impl Eq for DryingParams`

##### `impl PartialEq for DryingParams`

- <span id="dryingparams-partialeq-eq"></span>`fn eq(&self, other: &DryingParams) -> bool` — [`DryingParams`](#dryingparams)

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
  [`AmsFilamentSettingPayload::tray_info_idx`](#amsfilamentsettingpayload).

- **`material`**: `&'a str`

  Material type, e.g. `"PLA"`.

- **`nozzle_temp_min`**: `u32`

  Minimum safe nozzle temperature, °C.

- **`nozzle_temp_max`**: `u32`

  Maximum safe nozzle temperature, °C.

#### Trait Implementations

##### `impl Clone for FilamentSpec<'a>`

- <span id="filamentspec-clone"></span>`fn clone(&self) -> FilamentSpec<'a>` — [`FilamentSpec`](#filamentspec)

##### `impl Copy for FilamentSpec<'a>`

##### `impl Debug for FilamentSpec<'a>`

- <span id="filamentspec-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FilamentSpec<'a>`

##### `impl PartialEq for FilamentSpec<'a>`

- <span id="filamentspec-partialeq-eq"></span>`fn eq(&self, other: &FilamentSpec<'a>) -> bool` — [`FilamentSpec`](#filamentspec)

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

- <span id="amscontrolop-clone"></span>`fn clone(&self) -> AmsControlOp` — [`AmsControlOp`](#amscontrolop)

##### `impl Copy for AmsControlOp`

##### `impl Debug for AmsControlOp`

- <span id="amscontrolop-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsControlOp`

##### `impl Hash for AmsControlOp`

- <span id="amscontrolop-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AmsControlOp`

- <span id="amscontrolop-partialeq-eq"></span>`fn eq(&self, other: &AmsControlOp) -> bool` — [`AmsControlOp`](#amscontrolop)

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

