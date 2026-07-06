**bambino > mqtt > commands > ams**

# Module: mqtt::commands::ams

## Contents

**Structs**

- [`AmsChangeFilamentPayload`](#amschangefilamentpayload) - Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP].
- [`AmsChangeFilamentRequest`](#amschangefilamentrequest) - Loads or unloads filament from an AMS slot or external spool to the toolhead.
- [`AmsControlPayload`](#amscontrolpayload) - Commands standard AMS controllers to resume, pause, or reset physical material feeds.
- [`AmsControlRequest`](#amscontrolrequest) - Sends a resume, pause, or reset command to the AMS feed mechanism.
- [`AmsFilamentDryingPayload`](#amsfilamentdryingpayload) - Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].
- [`AmsFilamentDryingRequest`](#amsfilamentdryingrequest) - Starts or stops a filament drying cycle on an AMS unit with a built-in heater.
- [`AmsFilamentSettingPayload`](#amsfilamentsettingpayload) - Overwrites physical attributes or custom slicer presets assigned to a specific tray.
- [`AmsFilamentSettingRequest`](#amsfilamentsettingrequest) - Sets filament properties (type, color, temperature range) on an AMS tray or external spool.
- [`AmsGetRfidPayload`](#amsgetrfidpayload) - Triggers physical filament feeder movement to scan proprietary RFID tag properties.
- [`AmsGetRfidRequest`](#amsgetrfidrequest) - Requests an RFID tag scan on a specific AMS slot.

---

## bambino::mqtt::commands::ams::AmsChangeFilamentPayload

*Struct*

Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP].

**Fields:**
- `command: &'static str` - Wire command name, always `"ams_change_filament"`.
- `ams_id: i32` - Target AMS unit index (or external-spool address per the caller's convention).
- `slot_id: i32` - Target slot index within the AMS unit.
- `target: i32` - Load/unload destination slot — mirrors `slot_id` in every documented wire example
- `curr_temp: i32` - Current nozzle temperature (-1 = let firmware decide).
- `tar_temp: i32` - Target nozzle temperature (-1 = let firmware decide).
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsChangeFilamentPayload`



## bambino::mqtt::commands::ams::AmsChangeFilamentRequest

*Struct*

Loads or unloads filament from an AMS slot or external spool to the toolhead.

**Fields:**
- `print: AmsChangeFilamentPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(ams_id: i32, slot_id: i32, target: i32, curr_temp: i32, tar_temp: i32, sequence_id: u64) -> Self` - Builds an `ams_change_filament` request to load or unload filament.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsChangeFilamentRequest`



## bambino::mqtt::commands::ams::AmsControlPayload

*Struct*

Commands standard AMS controllers to resume, pause, or reset physical material feeds.

**Fields:**
- `command: &'static str` - Wire command name, always `"ams_control"`.
- `param: String` - Target physical operation (e.g., "resume", "pause").
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsControlPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::ams::AmsControlRequest

*Struct*

Sends a resume, pause, or reset command to the AMS feed mechanism.

**Fields:**
- `print: AmsControlPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(operation: &str, sequence_id: u64) -> Self` - Builds an `ams_control` request for the given operation ("resume", "pause", etc.).

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsControlRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::ams::AmsFilamentDryingPayload

*Struct*

Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].

**Fields:**
- `command: &'static str` - Wire command name, always `"ams_filament_drying"`.
- `ams_id: i32` - Target AMS unit index.
- `mode: i32` - 1 = start drying, 0 = stop drying.
- `dry_temp: u32` - Drying temperature (°C).
- `dry_time: u32` - Duration in **minutes** (e.g., 8-hour cycle = 480).
- `rotate_tray: bool` - Whether to periodically rotate the tray during drying.
- `filament: String` - Filament material type being dried (e.g. "PA-CF").
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsFilamentDryingPayload`



## bambino::mqtt::commands::ams::AmsFilamentDryingRequest

*Struct*

Starts or stops a filament drying cycle on an AMS unit with a built-in heater.

**Fields:**
- `print: AmsFilamentDryingPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(ams_id: i32, mode: i32, dry_temp: u32, dry_time: u32, rotate_tray: bool, filament: &str, sequence_id: u64) -> Self` - Builds an `ams_filament_drying` request.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsFilamentDryingRequest`



## bambino::mqtt::commands::ams::AmsFilamentSettingPayload

*Struct*

Overwrites physical attributes or custom slicer presets assigned to a specific tray.

**Fields:**
- `command: &'static str` - Wire command name, always `"ams_filament_setting"`.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.
- `ams_id: i32` - Target AMS unit or external-spool address — see the addressing cheat-sheet on
- `tray_id: i32` - Target tray/slot index — see the addressing cheat-sheet on [`AmsFilamentSettingRequest::new`].
- `tray_info_idx: String` - Standard filament preset index code (e.g. "GFL05" / "PF12345678901234567") [REF-AMS-SP_CFG].
- `tray_type: String` - Material type string (e.g. "PLA", "PETG").
- `tray_sub_brands: String` - Sub-brand label (e.g. "Generic Basic"); defaults to `"{material_type} Basic"` when not given.
- `tray_color: String` - Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").
- `nozzle_temp_min: u32` - Minimum safe nozzle temperature (°C) for this filament.
- `nozzle_temp_max: u32` - Maximum safe nozzle temperature (°C) for this filament.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsFilamentSettingPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::ams::AmsFilamentSettingRequest

*Struct*

Sets filament properties (type, color, temperature range) on an AMS tray or external spool.

**Fields:**
- `print: AmsFilamentSettingPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(ams_id: i32, tray_id: i32, preset_code: &str, material_type: &str, sub_brands: Option<&str>, color_hex: &str, temp_min: u32, temp_max: u32, sequence_id: u64) -> Self` - Creates a request payload to update slot parameters.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsFilamentSettingRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::ams::AmsGetRfidPayload

*Struct*

Triggers physical filament feeder movement to scan proprietary RFID tag properties.

**Fields:**
- `command: &'static str` - Wire command name, always `"ams_get_rfid"`.
- `ams_id: i32` - Target AMS unit index.
- `slot_id: i32` - Target slot index within the AMS unit.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AmsGetRfidPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::ams::AmsGetRfidRequest

*Struct*

Requests an RFID tag scan on a specific AMS slot.

**Fields:**
- `print: AmsGetRfidPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(ams_id: i32, slot_id: i32, sequence_id: u64) -> Self` - Builds an `ams_get_rfid` request.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AmsGetRfidRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



