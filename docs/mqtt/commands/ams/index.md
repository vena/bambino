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

- <span id="amschangefilamentrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, target: i32, curr_temp: i32, tar_temp: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds an `ams_change_filament` request to load or unload filament.

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
    pub tray_id: i32,
    pub tray_info_idx: String,
    pub tray_type: String,
    pub tray_sub_brands: String,
    pub tray_color: String,
    pub nozzle_temp_min: u32,
    pub nozzle_temp_max: u32,
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

- **`tray_id`**: `i32`

  Target tray/slot index — see the addressing cheat-sheet on [`AmsFilamentSettingRequest::new`](#amsfilamentsettingrequest).

- **`tray_info_idx`**: `String`

  Standard filament preset index code (e.g. "GFL05" / "PF12345678901234567") [REF-AMS-SP_CFG].

- **`tray_type`**: `String`

  Material type string (e.g. "PLA", "PETG").

- **`tray_sub_brands`**: `String`

  Sub-brand label (e.g. "Generic Basic"); defaults to `"{material_type} Basic"` when not given.

- **`tray_color`**: `String`

  Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").

- **`nozzle_temp_min`**: `u32`

  Minimum safe nozzle temperature (°C) for this filament.

- **`nozzle_temp_max`**: `u32`

  Maximum safe nozzle temperature (°C) for this filament.

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

- <span id="amsfilamentsettingrequest-new"></span>`fn new(ams_id: i32, tray_id: i32, preset_code: &str, material_type: &str, sub_brands: Option<&str>, color_hex: &str, temp_min: u32, temp_max: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Creates a request payload to update slot parameters.

  **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
  For standard physical slots, `ams_id` matches the expansion unit index (0-3).
  For the single-nozzle external spool slot, `ams_id` must strictly be set to `255`
  and `tray_id` must strictly be set to `254` to prevent command rejection.

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

