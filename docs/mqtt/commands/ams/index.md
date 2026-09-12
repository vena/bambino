*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [ams](index.md)*

---

# Module `ams`

AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).

## Contents

- [Types](#types)
  - [`AmsChangeFilamentPayload`](#amschangefilamentpayload)
  - [`AmsChangeFilamentRequest`](#amschangefilamentrequest)
  - [`AmsControlPayload`](#amscontrolpayload)
  - [`AmsControlRequest`](#amscontrolrequest)
  - [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload)
  - [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest)
  - [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload)
  - [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest)
  - [`AmsGetRfidPayload`](#amsgetrfidpayload)
  - [`AmsGetRfidRequest`](#amsgetrfidrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsChangeFilamentPayload`](#amschangefilamentpayload) | struct | Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP]. |
| [`AmsChangeFilamentRequest`](#amschangefilamentrequest) | struct | Loads or unloads filament from an AMS slot or external spool to the toolhead. |
| [`AmsControlPayload`](#amscontrolpayload) | struct | Commands standard AMS controllers to resume, pause, or reset physical material feeds. |
| [`AmsControlRequest`](#amscontrolrequest) | struct | Sends a resume, pause, or reset command to the AMS feed mechanism. |
| [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload) | struct | Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER]. |
| [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest) | struct | Starts or stops a filament drying cycle on an AMS unit with a built-in heater. |
| [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload) | struct | Overwrites physical attributes or custom slicer presets assigned to a specific tray. |
| [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest) | struct | Sets filament properties (type, color, temperature range) on an AMS tray or external spool. |
| [`AmsGetRfidPayload`](#amsgetrfidpayload) | struct | Triggers physical filament feeder movement to scan proprietary RFID tag properties. |
| [`AmsGetRfidRequest`](#amsgetrfidrequest) | struct | Requests an RFID tag scan on a specific AMS slot. |

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
    pub sequence_id: String,
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

  Load/unload destination slot (confirmed against BambuStudio's
  `command_ams_change_filament`, `DeviceManager.cpp:1602-1638`): `255` on unload, the
  `ams_id` itself for AMS-HT/external-spool units (`ams_id >= 16`), or the flat global
  tray ID (`ams_id*4 + slot_id`) for a standard unit. Only coincidentally mirrors
  `slot_id` when `ams_id == 0` — see `PrinterClient::change_filament()`, which derives
  this field so callers can't misconfigure it.

- **`curr_temp`**: `i32`

  Current nozzle temperature (-1 = let firmware decide).

- **`tar_temp`**: `i32`

  Target nozzle temperature (-1 = let firmware decide).

- **`sequence_id`**: `String`

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

### `AmsChangeFilamentRequest`

```rust
struct AmsChangeFilamentRequest {
    pub print: AmsChangeFilamentPayload,
}
```

Loads or unloads filament from an AMS slot or external spool to the toolhead.

#### Fields

- **`print`**: `AmsChangeFilamentPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amschangefilamentrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, target: i32, curr_temp: i32, tar_temp: i32, extruder_id: Option<u8>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds an `ams_change_filament` request to load or unload filament.

  Pass `extruder_id: None` on any printer without a Filament Track Switch — the wire
  payload is then byte-identical to the pre-FTS form. See
  [`AmsChangeFilamentPayload::extruder_id`](#amschangefilamentpayload) for why an FTS machine requires it.

#### Trait Implementations

##### `impl Clone for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-clone"></span>`fn clone(&self) -> AmsChangeFilamentRequest` — [`AmsChangeFilamentRequest`](#amschangefilamentrequest)

##### `impl Debug for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsControlPayload`

```rust
struct AmsControlPayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: String,
}
```

Commands standard AMS controllers to resume, pause, or reset physical material feeds.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ams_control"`.

- **`param`**: `String`

  Target physical operation (e.g., "resume", "pause").

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsControlPayload`

- <span id="amscontrolpayload-clone"></span>`fn clone(&self) -> AmsControlPayload` — [`AmsControlPayload`](#amscontrolpayload)

##### `impl Debug for AmsControlPayload`

- <span id="amscontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsControlPayload`

- <span id="amscontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsControlRequest`

```rust
struct AmsControlRequest {
    pub print: AmsControlPayload,
}
```

Sends a resume, pause, or reset command to the AMS feed mechanism.

#### Fields

- **`print`**: `AmsControlPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amscontrolrequest-new"></span>`fn new(operation: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds an `ams_control` request for the given operation ("resume", "pause", etc.).

#### Trait Implementations

##### `impl Clone for AmsControlRequest`

- <span id="amscontrolrequest-clone"></span>`fn clone(&self) -> AmsControlRequest` — [`AmsControlRequest`](#amscontrolrequest)

##### `impl Debug for AmsControlRequest`

- <span id="amscontrolrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsControlRequest`

- <span id="amscontrolrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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
    pub cooling_temp: i32,
    pub close_power_conflict: bool,
    pub sequence_id: String,
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

- **`cooling_temp`**: `i32`

  Cooling temperature applied after the drying cycle completes.

- **`close_power_conflict`**: `bool`

  Whether to override the AMS unit's power-conflict interlock.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-clone"></span>`fn clone(&self) -> AmsFilamentDryingPayload` — [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload)

##### `impl Debug for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentDryingPayload`

- <span id="amsfilamentdryingpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentDryingRequest`

```rust
struct AmsFilamentDryingRequest {
    pub print: AmsFilamentDryingPayload,
}
```

Starts or stops a filament drying cycle on an AMS unit with a built-in heater.

#### Fields

- **`print`**: `AmsFilamentDryingPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsfilamentdryingrequest-new"></span>`fn new(ams_id: i32, mode: i32, filament: &str, temp: u32, duration_hours: u32, humidity: u32, rotate_tray: bool, cooling_temp: i32, close_power_conflict: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds an `ams_filament_drying` request.

#### Trait Implementations

##### `impl Clone for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-clone"></span>`fn clone(&self) -> AmsFilamentDryingRequest` — [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest)

##### `impl Debug for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentSettingPayload`

```rust
struct AmsFilamentSettingPayload {
    pub command: &'static str,
    pub sequence_id: String,
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

- **`sequence_id`**: `String`

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
  
  Set via [`AmsFilamentSettingRequest::with_preset`](#amsfilamentsettingrequest).
  
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

### `AmsFilamentSettingRequest`

```rust
struct AmsFilamentSettingRequest {
    pub print: AmsFilamentSettingPayload,
}
```

Sets filament properties (type, color, temperature range) on an AMS tray or external spool.

#### Fields

- **`print`**: `AmsFilamentSettingPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsfilamentsettingrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Creates a request payload to update slot parameters.

  **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
  For standard physical slots, `ams_id` matches the expansion unit index (0-3). For an
  external spool, pass the virtual `ams_id` (`255` single-nozzle / Ext-R, `254` Ext-L)
  with `slot_id: 0`. An A2L-attached AMS Lite takes its physical wire id `16` with a local
  `0..=3` slot, not the normalized `6` telemetry reports it as (bambuddy
  `ams_set_filament_setting`, matching the firmware's own `ams_mapping2`).

  **`slot_id` is what you pass; `tray_id` is derived.** Both reach the wire, and they
  differ on a virtual tray: `tray_id` becomes `254` for either external `ams_id` and the
  slot index otherwise. Deriving it here rather than accepting it means a caller cannot
  send a `slot_id`/`tray_id` pair that contradicts itself — the same reasoning as
  `PrinterClient::change_filament()` deriving `target`.

  Confirmed against BambuStudio's `command_ams_filament_settings`
  (`DeviceManager.cpp:1707-1722`), whose `tag_tray_id` maps either
  `VIRTUAL_TRAY_MAIN_ID`/`VIRTUAL_TRAY_DEPUTY_ID` to `254` and whose own call sites pass
  `slot_id: 0` for a virtual tray (`:4853`, `:4877`); and against bambuddy's
  `ams_set_filament_setting`, which sends `ams_id: 255`, `tray_id: 254`, `slot_id: 0` for
  a single external slot.

  **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
  addressing differs by command family — this rule is *not* the same one used by
  `extrusion_cali_sel` (K-profile binding, see
  `crate::diagnostics::ExtrusionCaliSelRequest::new`):
  * `ams_filament_setting` (this command) — Single-Nozzle Platforms: `ams_id: 255` /
    `tray_id: 254`. Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R
    (`ams_id: 255`) require `tray_id: 254` (confirmed against
    `command_ams_filament_settings`, `DeviceManager.cpp:1667-1693` — `tag_ams_id ==
    VIRTUAL_TRAY_MAIN_ID(255) || VIRTUAL_TRAY_DEPUTY_ID(254)` always maps to
    `tag_tray_id = VIRTUAL_TRAY_DEPUTY_ID(254)`, never `0`).
  * `extrusion_cali_sel` — Single-Nozzle Platforms: `ams_id: 254` / `tray_id: 254`.
    Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`; Ext-R requires
    `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong address for
    Ext-R on IDEX machines mis-routes the pressure advance profile to the left
    carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    uncalibrated.

  Only the addressing is positional. Everything the command *describes* — the filament,
  its color, its temperature window, its preset ids — is set through the `with_*` methods
  below, following the convention [`PrintJobConfig`](../print_job/index.md#printjobconfig) already
  establishes in this crate.

  This replaced a 9-argument constructor. `nozzle_temp_min`/`nozzle_temp_max` were adjacent
  `u32`s and `ams_id`/`slot_id` adjacent `i32`s, so transposing either pair compiled
  cleanly and produced a silently wrong command — on a command whose failures are already
  silent, since the printer acks a corrupted value as `"success"`.

  Fields left unset serialize as empty strings / zero temperatures; `setting_id` is omitted
  from the wire entirely.

- <span id="amsfilamentsettingrequest-with-filament"></span>`fn with_filament(self, material_type: &str, sub_brands: Option<&str>) -> Self`

  Sets the material type and its sub-brand label.

  `sub_brands` defaults to `"{material_type} Basic"` when `None`. Case is meaningful in
  both and is left alone — unlike [`with_color`](#amsfilamentsettingrequest).

- <span id="amsfilamentsettingrequest-with-color"></span>`fn with_color(self, color_hex: &str) -> Self`

  Sets the tray color, **normalized to uppercase** with a leading `#` stripped.

  The printer parses lowercase hex letters in `tray_color` as `0` and the corruption is
  silent: the `ams_filament_setting` ack echoes the value that was sent and reports
  `result: "success"`, and only the next AMS push status reveals it (measured on a P1S
  running firmware `01.10.00.00` — `09ff00ff` stored as `09000000`, `090000FF` intact).

  The normalization lives here, at the one place the color is set, rather than in each
  caller — a caller that forgets is exactly how the original bug arrived.

- <span id="amsfilamentsettingrequest-with-temps"></span>`fn with_temps(self, min: u32, max: u32) -> Self`

  Sets the safe nozzle temperature window, in °C.

  Taking both bounds in one call is the point: as two adjacent positional `u32`s they were
  transposable without a compile error.

- <span id="amsfilamentsettingrequest-with-preset"></span>`fn with_preset(self, preset_code: &str) -> Self`

  Sets the **short-format** filament preset code, e.g. `"GFA01"` or `"GFL05"`.

  A long `"PF"`-prefixed cloud id does not belong here — pass that to
  [`with_setting_id`](#amsfilamentsettingrequest). See
  [`AmsFilamentSettingPayload::tray_info_idx`](#amsfilamentsettingpayload) for what the printer does when the two are
  conflated.

- <span id="amsfilamentsettingrequest-with-setting-id"></span>`fn with_setting_id(self, setting_id: &str) -> Self`

  Attaches the full preset identifier, which is a separate wire field from
  `tray_info_idx` and is omitted entirely when not set.

  Pass the long form here — `"GFSL05_07"`, or a `"PF"`-prefixed id — and keep the short
  code in [`with_preset`](#amsfilamentsettingrequest). See
  [`AmsFilamentSettingPayload::tray_info_idx`](#amsfilamentsettingpayload) for what the printer does when a long id is
  put in the short field instead.

#### Trait Implementations

##### `impl Clone for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-clone"></span>`fn clone(&self) -> AmsFilamentSettingRequest` — [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest)

##### `impl Debug for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsGetRfidPayload`

```rust
struct AmsGetRfidPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub slot_id: i32,
    pub sequence_id: String,
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

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-clone"></span>`fn clone(&self) -> AmsGetRfidPayload` — [`AmsGetRfidPayload`](#amsgetrfidpayload)

##### `impl Debug for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsGetRfidPayload`

- <span id="amsgetrfidpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsGetRfidRequest`

```rust
struct AmsGetRfidRequest {
    pub print: AmsGetRfidPayload,
}
```

Requests an RFID tag scan on a specific AMS slot.

#### Fields

- **`print`**: `AmsGetRfidPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsgetrfidrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds an `ams_get_rfid` request.

#### Trait Implementations

##### `impl Clone for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-clone"></span>`fn clone(&self) -> AmsGetRfidRequest` — [`AmsGetRfidRequest`](#amsgetrfidrequest)

##### `impl Debug for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

