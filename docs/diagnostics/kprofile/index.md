*[bambino](../../index.md) / [diagnostics](../index.md) / [kprofile](index.md)*

---

# Module `kprofile`

# Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders

Exposes command serialization schemas and validation checks to manage stored
pressure-advance calibration profiles on the printer's onboard EEPROM [REF-DIAG-KPROF].

## Structural Guidelines & Constraints
* **Setting ID Validation**: Enforces the 19-character numeric `setting_id` boundary
  (`"PF"` followed by exactly 17 decimal digits) to prevent memory table corruption in the local
  EEPROM partition database.
* **Polymorphic Deletions**: Separates deletion schemas cleanly between standard single-nozzle
  platforms (keyed on `setting_id`) and dual-nozzle IDEX platforms (keyed on coordinate/carriage parameters).

## Contents

- [Types](#types)
  - [`ExtrusionCaliGetPayload`](#extrusioncaligetpayload)
  - [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest)
  - [`ExtrusionCaliGetResponse`](#extrusioncaligetresponse)
  - [`ExtrusionCaliGetResponsePayload`](#extrusioncaligetresponsepayload)
  - [`ExtrusionCaliSelPayload`](#extrusioncaliselpayload)
  - [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest)
  - [`ExtrusionCaliSetPayload`](#extrusioncalisetpayload)
  - [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest)
  - [`IdexCaliDelEntry`](#idexcalidelentry)
  - [`IdexCaliDelPayload`](#idexcalidelpayload)
  - [`IdexCaliDelRequest`](#idexcalidelrequest)
  - [`KProfileEntry`](#kprofileentry)
  - [`StandardCaliDelEntry`](#standardcalidelentry)
  - [`StandardCaliDelPayload`](#standardcalidelpayload)
  - [`StandardCaliDelRequest`](#standardcalidelrequest)
- [Functions](#functions)
  - [`is_setting_id_valid`](#is-setting-id-valid)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`ExtrusionCaliGetPayload`](#extrusioncaligetpayload) | struct | Inner payload for [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest). |
| [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest) | struct | JSON request wrapper to trigger a complete dump of the stored calibration database. |
| [`ExtrusionCaliGetResponse`](#extrusioncaligetresponse) | struct | JSON response wrapper containing the printer's stored calibration profile database. |
| [`ExtrusionCaliGetResponsePayload`](#extrusioncaligetresponsepayload) | struct | Payload envelope returned by the printer in response to `extrusion_cali_get`. |
| [`ExtrusionCaliSelPayload`](#extrusioncaliselpayload) | struct | Inner payload for [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest). |
| [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest) | struct | JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP]. |
| [`ExtrusionCaliSetPayload`](#extrusioncalisetpayload) | struct | Inner payload for [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest). |
| [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest) | struct | JSON request wrapper to create or overwrite calibration profile allocations. |
| [`IdexCaliDelEntry`](#idexcalidelentry) | struct | Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B). |
| [`IdexCaliDelPayload`](#idexcalidelpayload) | struct | Inner payload for [`IdexCaliDelRequest`](#idexcalidelrequest). |
| [`IdexCaliDelRequest`](#idexcalidelrequest) | struct | JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF]. |
| [`KProfileEntry`](#kprofileentry) | struct | Structured representation of a Linear Advance calibration profile entry on the printer. |
| [`StandardCaliDelEntry`](#standardcalidelentry) | struct | Deletion data fields utilized by standard single-nozzle databases (Schema A). |
| [`StandardCaliDelPayload`](#standardcalidelpayload) | struct | Inner payload for [`StandardCaliDelRequest`](#standardcalidelrequest). |
| [`StandardCaliDelRequest`](#standardcalidelrequest) | struct | JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF]. |
| [`is_setting_id_valid`](#is-setting-id-valid) | fn | Validates whether a provided calibration profile setting ID complies with EEPROM limits. |

## Types

### `ExtrusionCaliGetPayload`

```rust
struct ExtrusionCaliGetPayload {
    pub command: &'static str,
    pub sequence_id: String,
}
```

Inner payload for [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"extrusion_cali_get"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliGetPayload`

- <span id="extrusioncaligetpayload-clone"></span>`fn clone(&self) -> ExtrusionCaliGetPayload` — [`ExtrusionCaliGetPayload`](#extrusioncaligetpayload)

##### `impl Debug for ExtrusionCaliGetPayload`

- <span id="extrusioncaligetpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliGetPayload`

- <span id="extrusioncaligetpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtrusionCaliGetRequest`

```rust
struct ExtrusionCaliGetRequest {
    pub print: ExtrusionCaliGetPayload,
}
```

JSON request wrapper to trigger a complete dump of the stored calibration database.

# Firmware Quirk: Priming Required [REF-DIAG-KPROF]

The firmware ignores the first `extrusion_cali_get` command received after MQTTS
connection establishment. A dummy "priming" request must be sent first before the
real query will receive a response. `PrinterClient::get_k_profiles()` handles this
automatically — use `set_k_profile_primed(true)` to opt out if you manage priming
yourself.

#### Fields

- **`print`**: `ExtrusionCaliGetPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="extrusioncaligetrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../../mqtt/commands/index.md#clampedtaskid)

  Builds an `extrusion_cali_get` request.
  Callers should prefer `PrinterClient::get_k_profiles()`, which handles the priming quirk
  documented above.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliGetRequest`

- <span id="extrusioncaligetrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliGetRequest` — [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest)

##### `impl Debug for ExtrusionCaliGetRequest`

- <span id="extrusioncaligetrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliGetRequest`

- <span id="extrusioncaligetrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtrusionCaliGetResponse`

```rust
struct ExtrusionCaliGetResponse {
    pub print: ExtrusionCaliGetResponsePayload,
}
```

JSON response wrapper containing the printer's stored calibration profile database.

#### Fields

- **`print`**: `ExtrusionCaliGetResponsePayload`

  The `print` namespace envelope wrapping the returned calibration data.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliGetResponse`

- <span id="extrusioncaligetresponse-clone"></span>`fn clone(&self) -> ExtrusionCaliGetResponse` — [`ExtrusionCaliGetResponse`](#extrusioncaligetresponse)

##### `impl Debug for ExtrusionCaliGetResponse`

- <span id="extrusioncaligetresponse-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtrusionCaliGetResponse`

- <span id="extrusioncaligetresponse-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtrusionCaliGetResponse`

### `ExtrusionCaliGetResponsePayload`

```rust
struct ExtrusionCaliGetResponsePayload {
    pub command: String,
    pub sequence_id: String,
    pub nozzle_diameter: Option<String>,
    pub filaments: Vec<KProfileEntry>,
}
```

Payload envelope returned by the printer in response to `extrusion_cali_get`.

#### Fields

- **`command`**: `String`

  Echo of the command name (`"extrusion_cali_get"`).

- **`sequence_id`**: `String`

  Echo of the original request sequence identifier.

- **`nozzle_diameter`**: `Option<String>`

  Nozzle diameter filter applied to the returned profile set.

- **`filaments`**: `Vec<KProfileEntry>`

  Complete array of stored calibration profiles matching the active nozzle.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliGetResponsePayload`

- <span id="extrusioncaligetresponsepayload-clone"></span>`fn clone(&self) -> ExtrusionCaliGetResponsePayload` — [`ExtrusionCaliGetResponsePayload`](#extrusioncaligetresponsepayload)

##### `impl Debug for ExtrusionCaliGetResponsePayload`

- <span id="extrusioncaligetresponsepayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtrusionCaliGetResponsePayload`

- <span id="extrusioncaligetresponsepayload-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtrusionCaliGetResponsePayload`

### `ExtrusionCaliSelPayload`

```rust
struct ExtrusionCaliSelPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub tray_id: i32,
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub sequence_id: String,
}
```

Inner payload for [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"extrusion_cali_sel"`.

- **`ams_id`**: `i32`

  Target AMS/external-spool address — see the addressing cheat-sheet on [`ExtrusionCaliSelRequest::new`](#extrusioncaliselrequest).

- **`tray_id`**: `i32`

  Absolute global tray ID (not local slot index).

- **`cali_idx`**: `i32`

  Index of the calibration entry within the target's profile database (`KProfileEntry::cali_idx`).

- **`filament_id`**: `String`

  Filament preset ID this K-profile applies to (`KProfileEntry::filament_id`).

- **`nozzle_diameter`**: `String`

  Nozzle diameter this K-profile applies to (`KProfileEntry::nozzle_diameter`).

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliSelPayload`

- <span id="extrusioncaliselpayload-clone"></span>`fn clone(&self) -> ExtrusionCaliSelPayload` — [`ExtrusionCaliSelPayload`](#extrusioncaliselpayload)

##### `impl Debug for ExtrusionCaliSelPayload`

- <span id="extrusioncaliselpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliSelPayload`

- <span id="extrusioncaliselpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtrusionCaliSelRequest`

```rust
struct ExtrusionCaliSelRequest {
    pub print: ExtrusionCaliSelPayload,
}
```

JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].

The `setting_id` field is intentionally omitted from this payload to prevent
database mislinking on the motion board.

#### Fields

- **`print`**: `ExtrusionCaliSelPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="extrusioncaliselrequest-new"></span>`fn new(ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../../mqtt/commands/index.md#clampedtaskid)

  Creates a request payload to bind a stored K-profile calibration entry to an AMS
  material slot.

  **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
  addressing differs by command family — this rule is *not* the same one used by
  `ams_filament_setting` (filament configuration, see
  [`crate::mqtt::AmsFilamentSettingRequest::new`](../../mqtt/index.md)):
  * `extrusion_cali_sel` (this command) — Single-Nozzle Platforms: `ams_id: 254` /
    `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`;
    Ext-R requires `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong
    address for Ext-R on IDEX machines mis-routes the pressure advance profile to
    the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    uncalibrated.
  * `ams_filament_setting` — Single-Nozzle Platforms: `ams_id: 255` / `tray_id: 254`.
    Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R (`ams_id: 255`) require
    `tray_id: 254`, never `0` (BUG-117 / BambuStudio `DeviceManager.cpp:1667-1693`).

#### Trait Implementations

##### `impl Clone for ExtrusionCaliSelRequest`

- <span id="extrusioncaliselrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliSelRequest` — [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest)

##### `impl Debug for ExtrusionCaliSelRequest`

- <span id="extrusioncaliselrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliSelRequest`

- <span id="extrusioncaliselrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtrusionCaliSetPayload`

```rust
struct ExtrusionCaliSetPayload {
    pub command: &'static str,
    pub filaments: Vec<KProfileEntry>,
    pub sequence_id: String,
}
```

Inner payload for [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"extrusion_cali_set"`.

- **`filaments`**: `Vec<KProfileEntry>`

  Calibration profile entries to write. Multiple entries support IDEX multi-nozzle writes.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliSetPayload`

- <span id="extrusioncalisetpayload-clone"></span>`fn clone(&self) -> ExtrusionCaliSetPayload` — [`ExtrusionCaliSetPayload`](#extrusioncalisetpayload)

##### `impl Debug for ExtrusionCaliSetPayload`

- <span id="extrusioncalisetpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliSetPayload`

- <span id="extrusioncalisetpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ExtrusionCaliSetRequest`

```rust
struct ExtrusionCaliSetRequest {
    pub print: ExtrusionCaliSetPayload,
}
```

JSON request wrapper to create or overwrite calibration profile allocations.

#### Fields

- **`print`**: `ExtrusionCaliSetPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="extrusioncalisetrequest-new"></span>`fn new(profiles: Vec<KProfileEntry>, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`KProfileEntry`](#kprofileentry), [`ClampedTaskId`](../../mqtt/commands/index.md#clampedtaskid), [`Error`](../../error/index.md#error)

  Builds a secure write-transaction payload targeting physical EEPROM slots.

  Verifies that all target profiles carry valid setting identifiers to protect local
  database health. Supports multi-profile writes for IDEX platforms.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliSetRequest`

- <span id="extrusioncalisetrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliSetRequest` — [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest)

##### `impl Debug for ExtrusionCaliSetRequest`

- <span id="extrusioncalisetrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliSetRequest`

- <span id="extrusioncalisetrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IdexCaliDelEntry`

```rust
struct IdexCaliDelEntry {
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub extruder_id: u8,
}
```

Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B).

#### Fields

- **`nozzle_diameter`**: `String`

  Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).

- **`nozzle_id`**: `String`

  System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).

- **`extruder_id`**: `u8`

  Carriage index of the entry being deleted (0 = Right/Primary, 1 = Left/Deputy).

#### Trait Implementations

##### `impl Clone for IdexCaliDelEntry`

- <span id="idexcalidelentry-clone"></span>`fn clone(&self) -> IdexCaliDelEntry` — [`IdexCaliDelEntry`](#idexcalidelentry)

##### `impl Debug for IdexCaliDelEntry`

- <span id="idexcalidelentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for IdexCaliDelEntry`

- <span id="idexcalidelentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for IdexCaliDelEntry`

##### `impl PartialEq for IdexCaliDelEntry`

- <span id="idexcalidelentry-partialeq-eq"></span>`fn eq(&self, other: &IdexCaliDelEntry) -> bool` — [`IdexCaliDelEntry`](#idexcalidelentry)

##### `impl Serialize for IdexCaliDelEntry`

- <span id="idexcalidelentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IdexCaliDelPayload`

```rust
struct IdexCaliDelPayload {
    pub command: &'static str,
    pub filaments: Vec<IdexCaliDelEntry>,
    pub sequence_id: String,
}
```

Inner payload for [`IdexCaliDelRequest`](#idexcalidelrequest).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"extrusion_cali_del"`.

- **`filaments`**: `Vec<IdexCaliDelEntry>`

  Entries to delete. `IdexCaliDelRequest::new` always sends exactly one.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for IdexCaliDelPayload`

- <span id="idexcalidelpayload-clone"></span>`fn clone(&self) -> IdexCaliDelPayload` — [`IdexCaliDelPayload`](#idexcalidelpayload)

##### `impl Debug for IdexCaliDelPayload`

- <span id="idexcalidelpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for IdexCaliDelPayload`

- <span id="idexcalidelpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IdexCaliDelRequest`

```rust
struct IdexCaliDelRequest {
    pub print: IdexCaliDelPayload,
}
```

JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].

#### Fields

- **`print`**: `IdexCaliDelPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="idexcalidelrequest-new"></span>`fn new(target: IdexCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`IdexCaliDelEntry`](#idexcalidelentry), [`ClampedTaskId`](../../mqtt/commands/index.md#clampedtaskid)

  Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.

#### Trait Implementations

##### `impl Clone for IdexCaliDelRequest`

- <span id="idexcalidelrequest-clone"></span>`fn clone(&self) -> IdexCaliDelRequest` — [`IdexCaliDelRequest`](#idexcalidelrequest)

##### `impl Debug for IdexCaliDelRequest`

- <span id="idexcalidelrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for IdexCaliDelRequest`

- <span id="idexcalidelrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `KProfileEntry`

```rust
struct KProfileEntry {
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: Option<String>,
    pub nozzle_id: String,
    pub extruder_id: u8,
    pub name: String,
    pub k_value: String,
    pub n_coef: Option<String>,
    pub setting_id: String,
    pub ams_id: Option<i32>,
    pub tray_id: Option<i32>,
}
```

Structured representation of a Linear Advance calibration profile entry on the printer.

#### Fields

- **`cali_idx`**: `i32`

  Database index corresponding to the stored slot (-1 indicates a fresh write).

- **`filament_id`**: `String`

  Preset identifier associated with the base filament category (e.g. `"GFA01"`).

- **`nozzle_diameter`**: `Option<String>`

  Physical orifice size matching the calibrated tool (e.g. `"0.4"`).
  
  Single-nozzle firmware omits this field per-entry (it only sets it once at the
  `ExtrusionCaliGetResponsePayload` envelope level) — callers reading a parsed response
  must fall back to the envelope's `nozzle_diameter` when this is `None`.
  
  `skip_serializing_if` matters on the write side: this same struct is the element type
  of `extrusion_cali_set`'s `filaments` array, so round-tripping an entry read back from
  single-nozzle firmware would otherwise emit `"nozzle_diameter":null` — a shape neither
  the read side nor `reference/07_diagnostics_hms.md` §7.2 ever shows.

- **`nozzle_id`**: `String`

  System designation of the target hotend profile structure (e.g. `"HS00-0.4"`).

- **`extruder_id`**: `u8`

  Carriage layout indicator (0 = Right/Primary extruder, 1 = Left/Deputy extruder).

- **`name`**: `String`

  Custom user-defined name assigned to label the profile slot.

- **`k_value`**: `String`

  Calibrated Linear Advance constant serialized as a float string.

- **`n_coef`**: `Option<String>`

  Extrusion coefficient parameters.

- **`setting_id`**: `String`

  Secure 19-character unique setting identifier.

- **`ams_id`**: `Option<i32>`

  Links K-profile to AMS unit (default 0).

- **`tray_id`**: `Option<i32>`

  Links K-profile to AMS tray slot (default -1). At least X1C firmware spuriously
  reports `result: "fail"` for `extrusion_cali` writes using `tray_id: -1` even though
  the write still applies — don't treat that ack `result` as authoritative for a
  `tray_id: -1` write without cross-checking the profile actually landed.

#### Trait Implementations

##### `impl Clone for KProfileEntry`

- <span id="kprofileentry-clone"></span>`fn clone(&self) -> KProfileEntry` — [`KProfileEntry`](#kprofileentry)

##### `impl Debug for KProfileEntry`

- <span id="kprofileentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for KProfileEntry`

- <span id="kprofileentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for KProfileEntry`

##### `impl PartialEq for KProfileEntry`

- <span id="kprofileentry-partialeq-eq"></span>`fn eq(&self, other: &KProfileEntry) -> bool` — [`KProfileEntry`](#kprofileentry)

##### `impl Serialize for KProfileEntry`

- <span id="kprofileentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardCaliDelEntry`

```rust
struct StandardCaliDelEntry {
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub setting_id: String,
}
```

Deletion data fields utilized by standard single-nozzle databases (Schema A).

#### Fields

- **`cali_idx`**: `i32`

  Index of the calibration entry to delete (`KProfileEntry::cali_idx`).

- **`filament_id`**: `String`

  Filament preset ID of the entry being deleted (`KProfileEntry::filament_id`).

- **`nozzle_diameter`**: `String`

  Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).

- **`nozzle_id`**: `String`

  System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).

- **`setting_id`**: `String`

  19-character setting ID of the entry being deleted, validated by [`is_setting_id_valid`](#is-setting-id-valid).

#### Trait Implementations

##### `impl Clone for StandardCaliDelEntry`

- <span id="standardcalidelentry-clone"></span>`fn clone(&self) -> StandardCaliDelEntry` — [`StandardCaliDelEntry`](#standardcalidelentry)

##### `impl Debug for StandardCaliDelEntry`

- <span id="standardcalidelentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for StandardCaliDelEntry`

- <span id="standardcalidelentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for StandardCaliDelEntry`

##### `impl PartialEq for StandardCaliDelEntry`

- <span id="standardcalidelentry-partialeq-eq"></span>`fn eq(&self, other: &StandardCaliDelEntry) -> bool` — [`StandardCaliDelEntry`](#standardcalidelentry)

##### `impl Serialize for StandardCaliDelEntry`

- <span id="standardcalidelentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardCaliDelPayload`

```rust
struct StandardCaliDelPayload {
    pub command: &'static str,
    pub filaments: Vec<StandardCaliDelEntry>,
    pub sequence_id: String,
}
```

Inner payload for [`StandardCaliDelRequest`](#standardcalidelrequest).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"extrusion_cali_del"`.

- **`filaments`**: `Vec<StandardCaliDelEntry>`

  Entries to delete. `StandardCaliDelRequest::new` always sends exactly one.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for StandardCaliDelPayload`

- <span id="standardcalidelpayload-clone"></span>`fn clone(&self) -> StandardCaliDelPayload` — [`StandardCaliDelPayload`](#standardcalidelpayload)

##### `impl Debug for StandardCaliDelPayload`

- <span id="standardcalidelpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardCaliDelPayload`

- <span id="standardcalidelpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardCaliDelRequest`

```rust
struct StandardCaliDelRequest {
    pub print: StandardCaliDelPayload,
}
```

JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].

#### Fields

- **`print`**: `StandardCaliDelPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="standardcalidelrequest-new"></span>`fn new(target: StandardCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`StandardCaliDelEntry`](#standardcalidelentry), [`ClampedTaskId`](../../mqtt/commands/index.md#clampedtaskid), [`Error`](../../error/index.md#error)

  Builds a single-nozzle deletion transaction keyed on the setting identifier.

#### Trait Implementations

##### `impl Clone for StandardCaliDelRequest`

- <span id="standardcalidelrequest-clone"></span>`fn clone(&self) -> StandardCaliDelRequest` — [`StandardCaliDelRequest`](#standardcalidelrequest)

##### `impl Debug for StandardCaliDelRequest`

- <span id="standardcalidelrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardCaliDelRequest`

- <span id="standardcalidelrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`


---

## Functions

### `is_setting_id_valid`

```rust
fn is_setting_id_valid(setting_id: &str) -> bool
```

Validates whether a provided calibration profile setting ID complies with EEPROM limits.

**The Calibration Setting ID Boundary Rule [REF-DIAG-KPROF]:**
Stored EEPROM K-profiles require standard 19-character numeric formats consisting of a
`"PF"` header prefix followed by exactly 17 numeric digits. Standard alphanumeric hashes
(e.g. `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side presets.
Transmitting alphanumeric layouts inside direct database operations causes indexing halts
or table corruption on the physical mainboard.

