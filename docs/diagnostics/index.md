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
    pub code: u32,
    pub short_code: String,
    pub module_id: u8,
    pub is_genuine_fault: bool,
}
```

Fully decoded representation of the primary system `print_error` register.

#### Fields

- **`code`**: `u32`

  The raw `print_error` register value this was decoded from — what the error-dialog
  commands (`PrinterClient::ignore_error_and_resume` and friends) take.

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

### `CaliSelAddress`

```rust
struct CaliSelAddress {
    // [REDACTED: Private Fields]
}
```

The three address fields of an `extrusion_cali_sel`, derived from a unit and its local slot.

The wire `tray_id` is the global tray from
`resolve_global_tray_id`: `ams_id * 4 + slot` on a
standard unit (`reference/05_materials_ams.md` §5.3's `"ams_id": 0, "tray_id": 1` example is
unit 0 slot 1), `24 + slot` on an A2L-attached AMS Lite (BambuStudio's `GetTrayIndexMap`,
`DevFilaSystem.cpp:367-373`), the `ams_id` itself on an AMS-HT (slot 0 only) or an external
holder (slot ignored) — which gives the cheat-sheet pairs on [`ExtrusionCaliSelRequest::new`](kprofile/index.md#extrusioncaliselrequest).
`slot_id` is the unit-local slot BambuStudio and bambuddy send beside it (#315): the caller's
slot on a four-slot unit, `0` on an AMS-HT or external holder. `ams_id` is the wire form (an
A2L's AMS Lite is `16`).

#### Implementations

- <span id="caliseladdress-new"></span>`fn new(ams_id: u8, slot_id: u8) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

  Derives the address of slot `slot_id` on the unit at `ams_id` (telemetry or wire form).

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) when no tray answers to that unit and slot.

#### Trait Implementations

##### `impl Clone for CaliSelAddress`

- <span id="caliseladdress-clone"></span>`fn clone(&self) -> CaliSelAddress` — [`CaliSelAddress`](kprofile/index.md#caliseladdress)

##### `impl Copy for CaliSelAddress`

##### `impl Debug for CaliSelAddress`

- <span id="caliseladdress-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CaliSelAddress`

##### `impl Hash for CaliSelAddress`

- <span id="caliseladdress-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CaliSelAddress`

- <span id="caliseladdress-partialeq-eq"></span>`fn eq(&self, other: &CaliSelAddress) -> bool` — [`CaliSelAddress`](kprofile/index.md#caliseladdress)

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

### `IdexCaliDelEntry`

```rust
struct IdexCaliDelEntry {
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub extruder_id: u8,
}
```

Deletion data fields utilized by dual-nozzle IDEX databases (Schema B).

Serialized flat into `print`, as [`StandardCaliDelEntry`](kprofile/index.md#standardcalidelentry). `cali_idx` and `filament_id` are
what name the profile; the carriage fields alone name none (#313).

#### Fields

- **`cali_idx`**: `i32`

  Index of the calibration entry to delete (`KProfileEntry::cali_idx`).

- **`filament_id`**: `String`

  Filament preset ID of the entry being deleted (`KProfileEntry::filament_id`).

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

Every field is optional on the read side, defaulting as BambuStudio's
`from_json(PACalibResult)` does (`DevCalib.cpp:56-72`): one entry missing a key must not fail
the whole `extrusion_cali_get` reply, which `get_k_profiles` would then wait out as a timeout
(#314).

#### Fields

- **`cali_idx`**: `i32`

  Database index corresponding to the stored slot (-1 indicates a fresh write, and is the
  default when the key is absent).

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
  
  Bound permissively: firmware may send a diameter as a bare JSON number (`0.4`) rather
  than the quoted form the captures show, and a strict `Option<String>` fails the whole
  response on that shape rather than just this field.

- **`nozzle_id`**: `String`

  System designation of the target hotend profile structure (e.g. `"HS00-0.4"`).
  
  **The leading two characters are a flow code**, not a hardware id: `HH` = high flow,
  `HS` = standard. A printer can hold profiles for both against one diameter — an H2D was
  observed with 102 high-flow entries against 6 standard ones — and the same filament
  reads a different `k_value` through each, so the code is load-bearing when resolving a
  slot to a profile.
  
  Two traps follow from that:
  
  * The **fitted** nozzle reports `HH01`, not `HH00`. Compare on the first **two**
    characters only; the trailing digits are a hardware variant that the calibration table
    normalizes to `00`.
  * Some models declare this **empty on every profile** — an X1C was probed live with all
    eight entries empty here, against a four-digit `cali_idx` and a populated `setting_id`.
    Handle the emptiness directly; a model-capability flag is the wrong thing to gate on.
  
  See `reference/07_diagnostics_hms.md` §7.2 for the full slot-resolution rule, including
  why `cali_idx` alone does not identify a profile.

- **`extruder_id`**: `u8`

  Carriage layout indicator (0 = Right/Primary extruder, 1 = Left/Deputy extruder).

- **`name`**: `String`

  Custom user-defined name assigned to label the profile slot.

- **`k_value`**: `String`

  Calibrated Linear Advance constant serialized as a float string.
  
  Bound permissively for the same reason as [`nozzle_diameter`](kprofile/index.md#kprofileentry):
  firmware may send the numeric form (`0.02`) on the read side. A number is rendered back
  to its decimal text, so callers see one representation either way. `"0"` when absent,
  matching BambuStudio's `0.0` default.

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

##### `impl Default for KProfileEntry`

- <span id="kprofileentry-default"></span>`fn default() -> Self`

  The defaults BambuStudio's `from_json(PACalibResult)` applies (`DevCalib.cpp:56-72`):
  `cali_idx` `-1` (a fresh write), `k_value` `"0"`, everything else empty.

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
    pub extruder_id: u8,
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub setting_id: String,
}
```

Deletion data fields utilized by standard single-nozzle databases (Schema A).

Serialized **flat into `print`**, not inside a `filaments` array: BambuStudio, bambuddy and
OrcaSlicer all send `extrusion_cali_del` that way (`reference/07_diagnostics_hms.md` §7.2,
#313).

#### Fields

- **`extruder_id`**: `u8`

  Carriage index of the entry being deleted — `0` on a single-nozzle printer.

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

### `ExtrusionCaliGetRequest`

```rust
type ExtrusionCaliGetRequest = crate::mqtt::commands::Print<ExtrusionCaliGetPayload>;
```

JSON request wrapper to trigger a complete dump of the stored calibration database.

# Firmware Quirk: Priming Required [REF-DIAG-KPROF]

The firmware ignores the first `extrusion_cali_get` command received after MQTTS
connection establishment. A dummy "priming" request must be sent first before the
real query will receive a response. `PrinterClient::get_k_profiles()` handles this
automatically — use `set_k_profile_primed(true)` to opt out if you manage priming
yourself.

### `ExtrusionCaliSelRequest`

```rust
type ExtrusionCaliSelRequest = crate::mqtt::commands::Print<ExtrusionCaliSelPayload>;
```

JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].

The `setting_id` field is intentionally omitted from this payload to prevent
database mislinking on the motion board.

### `ExtrusionCaliSetRequest`

```rust
type ExtrusionCaliSetRequest = crate::mqtt::commands::Print<ExtrusionCaliSetPayload>;
```

JSON request wrapper to create or overwrite calibration profile allocations.

### `IdexCaliDelRequest`

```rust
type IdexCaliDelRequest = crate::mqtt::commands::Print<IdexCaliDelPayload>;
```

JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].

### `StandardCaliDelRequest`

```rust
type StandardCaliDelRequest = crate::mqtt::commands::Print<StandardCaliDelPayload>;
```

JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].


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

