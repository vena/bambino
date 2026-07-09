*[bambino](../index.md) / [diagnostics](index.md)*

---

# Module `diagnostics`

# Diagnostics & Calibration

Tools for interpreting printer health alerts and managing calibration data.

The [`hms`](hms/index.md#hms) submodule decodes HMS (Health Management System) fault codes and print
error registers into human-readable alerts with severity levels. The [`kprofile`](kprofile/index.md#kprofile)
submodule manages Linear Advance (K-factor) calibration profiles — querying the
printer's stored profiles, creating new ones, and deleting them (with separate
request types for standard and IDEX platforms).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`hms`](#hms) | mod | # HMS Diagnostic Telemetry Parsing & Unpacking Engine |
| [`kprofile`](#kprofile) | mod | # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders |

## Modules

- [`hms`](hms/index.md#hms) — # HMS Diagnostic Telemetry Parsing & Unpacking Engine
- [`kprofile`](kprofile/index.md#kprofile) — # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders


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

- <span id="extrusioncaligetrequest-new"></span>`fn new(sequence_id: u64) -> Self`

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

- <span id="extrusioncaliselrequest-new"></span>`fn new(ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str, sequence_id: u64) -> Self`

  Creates a request payload to bind a stored K-profile calibration entry to an AMS

  material slot.

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

- <span id="extrusioncalisetrequest-new"></span>`fn new(profiles: Vec<KProfileEntry>, sequence_id: u64) -> Result<Self, BambuError>` — [`KProfileEntry`](kprofile/index.md#kprofileentry), [`BambuError`](../error/index.md#bambuerror)

  Builds a secure write-transaction payload targeting physical EEPROM slots.

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

- <span id="idexcalidelrequest-new"></span>`fn new(target: IdexCaliDelEntry, sequence_id: u64) -> Self` — [`IdexCaliDelEntry`](kprofile/index.md#idexcalidelentry)

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
    pub nozzle_diameter: String,
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

- **`nozzle_diameter`**: `String`

  Physical orifice size matching the calibrated tool (e.g. `"0.4"`).

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

  Links K-profile to AMS tray slot (default -1).

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

  19-character setting ID of the entry being deleted, validated by [`validate_setting_id`](kprofile/index.md#validate-setting-id).

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

- <span id="standardcalidelrequest-new"></span>`fn new(target: StandardCaliDelEntry, sequence_id: u64) -> Result<Self, BambuError>` — [`StandardCaliDelEntry`](kprofile/index.md#standardcalidelentry), [`BambuError`](../error/index.md#bambuerror)

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

- <span id="hmsseverity-from-attr"></span>`fn from_attr(attr: u32) -> Self`

  Extracts the severity level from the second byte of the 32-bit `attr` value.

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

### `validate_setting_id`

```rust
fn validate_setting_id(setting_id: &str) -> bool
```

Validates whether a provided calibration profile setting ID complies with EEPROM limits.

**The Calibration Setting ID Boundary Rule [REF-DIAG-KPROF]:**
Stored EEPROM K-profiles require standard 19-character numeric formats consisting of a
`"PF"` header prefix followed by exactly 17 numeric digits. Standard alphanumeric hashes
(e.g. `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side presets.
Transmitting alphanumeric layouts inside direct database operations causes indexing halts
or table corruption on the physical mainboard.

