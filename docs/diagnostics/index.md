*[bambino](../index.md) / [diagnostics](index.md)*

---

# Module `diagnostics`

# Diagnostics & Calibration

Tools for interpreting printer health alerts and managing calibration data.

The [`hms`](hms/index.md) submodule decodes HMS (Health Management System) fault codes and print
error registers into human-readable alerts with severity levels. The [`kprofile`](kprofile/index.md)
submodule manages Linear Advance (K-factor) calibration profiles — querying the
printer's stored profiles, creating new ones, and deleting them (with separate
request types for standard and IDEX platforms).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`hms`](hms/index.md) | mod | # HMS Diagnostic Telemetry Parsing & Unpacking Engine |
| [`kprofile`](kprofile/index.md) | mod | # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders |

## Modules

- [`hms`](hms/index.md) — # HMS Diagnostic Telemetry Parsing & Unpacking Engine
- [`kprofile`](kprofile/index.md) — # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders


---

## Types

### `DecodedHmsAlert`

```rust
struct DecodedHmsAlert {
    pub wiki_key: String,
    pub short_code: String,
    pub severity: HmsSeverity,
    pub module_id: u8,
    pub is_genuine_fault: bool,
}
```

Fully decoded representation of an active diagnostic entry from the `hms` telemetry array.

#### Fields

- **`wiki_key`**: `String`

  The standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`).

- **`short_code`**: `String`

  The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).

- **`severity`**: `HmsSeverity`

  Decoded physical severity rating of the active system alert.

- **`module_id`**: `u8`

  Unique identifier of the source hardware module executing under failure.

- **`is_genuine_fault`**: `bool`

  Flags whether this alert represents a genuine hardware fault rather than a progress or state step.

#### Trait Implementations

##### `impl Clone for DecodedHmsAlert`

- <span id="decodedhmsalert-clone"></span>`fn clone(&self) -> DecodedHmsAlert` — [`DecodedHmsAlert`](hms/index.md#decodedhmsalert)

##### `impl Debug for DecodedHmsAlert`

- <span id="decodedhmsalert-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DecodedHmsAlert`

##### `impl Hash for DecodedHmsAlert`

- <span id="decodedhmsalert-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for DecodedHmsAlert`

- <span id="decodedhmsalert-partialeq-eq"></span>`fn eq(&self, other: &DecodedHmsAlert) -> bool` — [`DecodedHmsAlert`](hms/index.md#decodedhmsalert)

### `DecodedPrintError`

```rust
struct DecodedPrintError {
    pub short_code: String,
    pub module_id: u8,
    pub is_genuine_fault: bool,
}
```

Fully decoded representation of the primary system `print_error` register.

#### Fields

- **`short_code`**: `String`

  The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).

- **`module_id`**: `u8`

  Unpacked system module code where the primary print execution halted.

- **`is_genuine_fault`**: `bool`

  Flags whether this error register holds a genuine hardware failure block.

#### Trait Implementations

##### `impl Clone for DecodedPrintError`

- <span id="decodedprinterror-clone"></span>`fn clone(&self) -> DecodedPrintError` — [`DecodedPrintError`](hms/index.md#decodedprinterror)

##### `impl Debug for DecodedPrintError`

- <span id="decodedprinterror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DecodedPrintError`

##### `impl Hash for DecodedPrintError`

- <span id="decodedprinterror-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for DecodedPrintError`

- <span id="decodedprinterror-partialeq-eq"></span>`fn eq(&self, other: &DecodedPrintError) -> bool` — [`DecodedPrintError`](hms/index.md#decodedprinterror)

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

- <span id="extrusioncaligetrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../mqtt/commands/index.md#clampedtaskid)

  Builds an `extrusion_cali_get` request.
  Callers should prefer `PrinterClient::get_k_profiles()`, which handles the priming quirk
  documented above.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliGetRequest`

- <span id="extrusioncaligetrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliGetRequest` — [`ExtrusionCaliGetRequest`](kprofile/index.md#extrusioncaligetrequest)

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

- <span id="extrusioncaligetresponse-clone"></span>`fn clone(&self) -> ExtrusionCaliGetResponse` — [`ExtrusionCaliGetResponse`](kprofile/index.md#extrusioncaligetresponse)

##### `impl Debug for ExtrusionCaliGetResponse`

- <span id="extrusioncaligetresponse-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for ExtrusionCaliGetResponse`

- <span id="extrusioncaligetresponse-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for ExtrusionCaliGetResponse`

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

- <span id="extrusioncaliselrequest-new"></span>`fn new(ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../mqtt/commands/index.md#clampedtaskid)

  Creates a request payload to bind a stored K-profile calibration entry to an AMS
  material slot.

  **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
  addressing differs by command family — this rule is *not* the same one used by
  `ams_filament_setting` (filament configuration, see
  [`crate::mqtt::AmsFilamentSettingRequest::new`](../mqtt/index.md)):
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

- <span id="extrusioncaliselrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliSelRequest` — [`ExtrusionCaliSelRequest`](kprofile/index.md#extrusioncaliselrequest)

##### `impl Debug for ExtrusionCaliSelRequest`

- <span id="extrusioncaliselrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ExtrusionCaliSelRequest`

- <span id="extrusioncaliselrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="extrusioncalisetrequest-new"></span>`fn new(profiles: Vec<KProfileEntry>, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`KProfileEntry`](kprofile/index.md#kprofileentry), [`ClampedTaskId`](../mqtt/commands/index.md#clampedtaskid), [`Error`](../error/index.md#error)

  Builds a secure write-transaction payload targeting physical EEPROM slots.

  Verifies that all target profiles carry valid setting identifiers to protect local
  database health. Supports multi-profile writes for IDEX platforms.

#### Trait Implementations

##### `impl Clone for ExtrusionCaliSetRequest`

- <span id="extrusioncalisetrequest-clone"></span>`fn clone(&self) -> ExtrusionCaliSetRequest` — [`ExtrusionCaliSetRequest`](kprofile/index.md#extrusioncalisetrequest)

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

- <span id="idexcalidelentry-clone"></span>`fn clone(&self) -> IdexCaliDelEntry` — [`IdexCaliDelEntry`](kprofile/index.md#idexcalidelentry)

##### `impl Debug for IdexCaliDelEntry`

- <span id="idexcalidelentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for IdexCaliDelEntry`

- <span id="idexcalidelentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for IdexCaliDelEntry`

##### `impl PartialEq for IdexCaliDelEntry`

- <span id="idexcalidelentry-partialeq-eq"></span>`fn eq(&self, other: &IdexCaliDelEntry) -> bool` — [`IdexCaliDelEntry`](kprofile/index.md#idexcalidelentry)

##### `impl Serialize for IdexCaliDelEntry`

- <span id="idexcalidelentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="idexcalidelrequest-new"></span>`fn new(target: IdexCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`IdexCaliDelEntry`](kprofile/index.md#idexcalidelentry), [`ClampedTaskId`](../mqtt/commands/index.md#clampedtaskid)

  Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.

#### Trait Implementations

##### `impl Clone for IdexCaliDelRequest`

- <span id="idexcalidelrequest-clone"></span>`fn clone(&self) -> IdexCaliDelRequest` — [`IdexCaliDelRequest`](kprofile/index.md#idexcalidelrequest)

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

- <span id="kprofileentry-clone"></span>`fn clone(&self) -> KProfileEntry` — [`KProfileEntry`](kprofile/index.md#kprofileentry)

##### `impl Debug for KProfileEntry`

- <span id="kprofileentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for KProfileEntry`

- <span id="kprofileentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for KProfileEntry`

##### `impl PartialEq for KProfileEntry`

- <span id="kprofileentry-partialeq-eq"></span>`fn eq(&self, other: &KProfileEntry) -> bool` — [`KProfileEntry`](kprofile/index.md#kprofileentry)

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

  19-character setting ID of the entry being deleted, validated by [`is_setting_id_valid`](kprofile/index.md#is-setting-id-valid).

#### Trait Implementations

##### `impl Clone for StandardCaliDelEntry`

- <span id="standardcalidelentry-clone"></span>`fn clone(&self) -> StandardCaliDelEntry` — [`StandardCaliDelEntry`](kprofile/index.md#standardcalidelentry)

##### `impl Debug for StandardCaliDelEntry`

- <span id="standardcalidelentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for StandardCaliDelEntry`

- <span id="standardcalidelentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for StandardCaliDelEntry`

##### `impl PartialEq for StandardCaliDelEntry`

- <span id="standardcalidelentry-partialeq-eq"></span>`fn eq(&self, other: &StandardCaliDelEntry) -> bool` — [`StandardCaliDelEntry`](kprofile/index.md#standardcalidelentry)

##### `impl Serialize for StandardCaliDelEntry`

- <span id="standardcalidelentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="standardcalidelrequest-new"></span>`fn new(target: StandardCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Result<Self, Error>` — [`StandardCaliDelEntry`](kprofile/index.md#standardcalidelentry), [`ClampedTaskId`](../mqtt/commands/index.md#clampedtaskid), [`Error`](../error/index.md#error)

  Builds a single-nozzle deletion transaction keyed on the setting identifier.

#### Trait Implementations

##### `impl Clone for StandardCaliDelRequest`

- <span id="standardcalidelrequest-clone"></span>`fn clone(&self) -> StandardCaliDelRequest` — [`StandardCaliDelRequest`](kprofile/index.md#standardcalidelrequest)

##### `impl Debug for StandardCaliDelRequest`

- <span id="standardcalidelrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardCaliDelRequest`

- <span id="standardcalidelrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `HmsSeverity`

```rust
enum HmsSeverity {
    Fatal,
    Serious,
    Warning,
    Info,
    Unknown,
}
```

Numerical classification of the severity level of an HMS diagnostic alert.

#### Variants

- **`Fatal`**

  Severe operational failure requiring immediate print execution halt.

- **`Serious`**

  High-priority alert requiring user intervention before execution resumes.

- **`Warning`**

  Non-blocking warning indicating minor runtime or environment issues.

- **`Info`**

  Routine information prompt or system state confirmation event.

- **`Unknown`**

  Fallback classification for unrecognized alert bounds.

#### Implementations

- <span id="hmsseverity-from-code"></span>`fn from_code(code: u32) -> Self`

  Extracts the severity level from the high 16 bits of the 32-bit `code` value.

  Bit representation: `(code >> 16) & 0xFFFF` [REF-DIAG-HMS]. Confirmed against
  BambuStudio's `parse_hms_info` (`DevHMS.cpp:7-25`, identical in OrcaSlicer) and
  pybambu's `get_HMS_severity`, both of which derive severity from `code >> 16`.

#### Trait Implementations

##### `impl Clone for HmsSeverity`

- <span id="hmsseverity-clone"></span>`fn clone(&self) -> HmsSeverity` — [`HmsSeverity`](hms/index.md#hmsseverity)

##### `impl Copy for HmsSeverity`

##### `impl Debug for HmsSeverity`

- <span id="hmsseverity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for HmsSeverity`

##### `impl Hash for HmsSeverity`

- <span id="hmsseverity-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for HmsSeverity`

- <span id="hmsseverity-partialeq-eq"></span>`fn eq(&self, other: &HmsSeverity) -> bool` — [`HmsSeverity`](hms/index.md#hmsseverity)


---

## Functions

### `decode_hms_alert`

```rust
fn decode_hms_alert(attr: u32, code: u32) -> DecodedHmsAlert
```

**Types:** [`DecodedHmsAlert`](hms/index.md#decodedhmsalert)

Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS].

Unpacks the 32-bit `attr` and `code` parameters to reconstruct standard Wiki-slug
tracking variables, extract severity ratings, isolate module indexes, and filter
transient state updates.

### `decode_print_error`

```rust
fn decode_print_error(print_error: u32) -> Option<DecodedPrintError>
```

**Types:** [`DecodedPrintError`](hms/index.md#decodedprinterror)

Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code.

Under the over-the-wire telemetry channel, the `print_error` status is passed as a packed
decimal integer. Reconstructing this to LCD standards requires hex-string conversion
and formatting with an underscore separator [REF-DIAG-HMS].

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

