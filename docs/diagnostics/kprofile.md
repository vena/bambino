**bambino > diagnostics > kprofile**

# Module: diagnostics::kprofile

## Contents

**Structs**

- [`ExtrusionCaliGetPayload`](#extrusioncaligetpayload) - Inner payload for [`ExtrusionCaliGetRequest`].
- [`ExtrusionCaliGetRequest`](#extrusioncaligetrequest) - JSON request wrapper to trigger a complete dump of the stored calibration database.
- [`ExtrusionCaliGetResponse`](#extrusioncaligetresponse) - JSON response wrapper containing the printer's stored calibration profile database.
- [`ExtrusionCaliGetResponsePayload`](#extrusioncaligetresponsepayload) - Payload envelope returned by the printer in response to `extrusion_cali_get`.
- [`ExtrusionCaliSelPayload`](#extrusioncaliselpayload) - Inner payload for [`ExtrusionCaliSelRequest`].
- [`ExtrusionCaliSelRequest`](#extrusioncaliselrequest) - JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
- [`ExtrusionCaliSetPayload`](#extrusioncalisetpayload) - Inner payload for [`ExtrusionCaliSetRequest`].
- [`ExtrusionCaliSetRequest`](#extrusioncalisetrequest) - JSON request wrapper to create or overwrite calibration profile allocations.
- [`IdexCaliDelEntry`](#idexcalidelentry) - Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B).
- [`IdexCaliDelPayload`](#idexcalidelpayload) - Inner payload for [`IdexCaliDelRequest`].
- [`IdexCaliDelRequest`](#idexcalidelrequest) - JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].
- [`KProfileEntry`](#kprofileentry) - Structured representation of a Linear Advance calibration profile entry on the printer.
- [`StandardCaliDelEntry`](#standardcalidelentry) - Deletion data fields utilized by standard single-nozzle databases (Schema A).
- [`StandardCaliDelPayload`](#standardcalidelpayload) - Inner payload for [`StandardCaliDelRequest`].
- [`StandardCaliDelRequest`](#standardcalidelrequest) - JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].

**Functions**

- [`validate_setting_id`](#validate_setting_id) - Validates whether a provided calibration profile setting ID complies with EEPROM limits.

---

## bambino::diagnostics::kprofile::ExtrusionCaliGetPayload

*Struct*

Inner payload for [`ExtrusionCaliGetRequest`].

**Fields:**
- `command: &'static str` - Wire command name, always `"extrusion_cali_get"`.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliGetPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::diagnostics::kprofile::ExtrusionCaliGetRequest

*Struct*

JSON request wrapper to trigger a complete dump of the stored calibration database.

# Firmware Quirk: Priming Required [REF-DIAG-KPROF]

The firmware ignores the first `extrusion_cali_get` command received after MQTTS
connection establishment. A dummy "priming" request must be sent first before the
real query will receive a response. `PrinterClient::get_k_profiles()` handles this
automatically — use `set_k_profile_primed(true)` to opt out if you manage priming
yourself.

**Fields:**
- `print: ExtrusionCaliGetPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(sequence_id: u64) -> Self` - Builds an `extrusion_cali_get` request. Callers should prefer

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliGetRequest`



## bambino::diagnostics::kprofile::ExtrusionCaliGetResponse

*Struct*

JSON response wrapper containing the printer's stored calibration profile database.

**Fields:**
- `print: ExtrusionCaliGetResponsePayload` - The `print` namespace envelope wrapping the returned calibration data.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliGetResponse`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::diagnostics::kprofile::ExtrusionCaliGetResponsePayload

*Struct*

Payload envelope returned by the printer in response to `extrusion_cali_get`.

**Fields:**
- `command: String` - Echo of the command name (`"extrusion_cali_get"`).
- `sequence_id: String` - Echo of the original request sequence identifier.
- `nozzle_diameter: Option<String>` - Nozzle diameter filter applied to the returned profile set.
- `filaments: Vec<KProfileEntry>` - Complete array of stored calibration profiles matching the active nozzle.

**Trait Implementations:**

- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliGetResponsePayload`



## bambino::diagnostics::kprofile::ExtrusionCaliSelPayload

*Struct*

Inner payload for [`ExtrusionCaliSelRequest`].

**Fields:**
- `command: &'static str` - Wire command name, always `"extrusion_cali_sel"`.
- `ams_id: i32` - Target AMS/external-spool address — see the addressing cheat-sheet on
- `tray_id: i32` - Absolute global tray ID (not local slot index).
- `cali_idx: i32` - Index of the calibration entry within the target's profile database (`KProfileEntry::cali_idx`).
- `filament_id: String` - Filament preset ID this K-profile applies to (`KProfileEntry::filament_id`).
- `nozzle_diameter: String` - Nozzle diameter this K-profile applies to (`KProfileEntry::nozzle_diameter`).
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliSelPayload`



## bambino::diagnostics::kprofile::ExtrusionCaliSelRequest

*Struct*

JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].

The `setting_id` field is intentionally omitted from this payload to prevent
database mislinking on the motion board.

**Fields:**
- `print: ExtrusionCaliSelPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str, sequence_id: u64) -> Self` - Creates a request payload to bind a stored K-profile calibration entry to an AMS

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliSelRequest`



## bambino::diagnostics::kprofile::ExtrusionCaliSetPayload

*Struct*

Inner payload for [`ExtrusionCaliSetRequest`].

**Fields:**
- `command: &'static str` - Wire command name, always `"extrusion_cali_set"`.
- `filaments: Vec<KProfileEntry>` - Calibration profile entries to write. Multiple entries support IDEX multi-nozzle writes.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliSetPayload`



## bambino::diagnostics::kprofile::ExtrusionCaliSetRequest

*Struct*

JSON request wrapper to create or overwrite calibration profile allocations.

**Fields:**
- `print: ExtrusionCaliSetPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(profiles: Vec<KProfileEntry>, sequence_id: u64) -> Result<Self, BambuError>` - Builds a secure write-transaction payload targeting physical EEPROM slots.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> ExtrusionCaliSetRequest`



## bambino::diagnostics::kprofile::IdexCaliDelEntry

*Struct*

Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B).

**Fields:**
- `nozzle_diameter: String` - Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).
- `nozzle_id: String` - System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).
- `extruder_id: u8` - Carriage index of the entry being deleted (0 = Right/Primary, 1 = Left/Deputy).

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **PartialEq**
  - `fn eq(self: &Self, other: &IdexCaliDelEntry) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> IdexCaliDelEntry`



## bambino::diagnostics::kprofile::IdexCaliDelPayload

*Struct*

Inner payload for [`IdexCaliDelRequest`].

**Fields:**
- `command: &'static str` - Wire command name, always `"extrusion_cali_del"`.
- `filaments: Vec<IdexCaliDelEntry>` - Entries to delete. `IdexCaliDelRequest::new` always sends exactly one.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> IdexCaliDelPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::diagnostics::kprofile::IdexCaliDelRequest

*Struct*

JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].

**Fields:**
- `print: IdexCaliDelPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(target: IdexCaliDelEntry, sequence_id: u64) -> Self` - Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> IdexCaliDelRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::diagnostics::kprofile::KProfileEntry

*Struct*

Structured representation of a Linear Advance calibration profile entry on the printer.

**Fields:**
- `cali_idx: i32` - Database index corresponding to the stored slot (-1 indicates a fresh write).
- `filament_id: String` - Preset identifier associated with the base filament category (e.g. `"GFA01"`).
- `nozzle_diameter: String` - Physical orifice size matching the calibrated tool (e.g. `"0.4"`).
- `nozzle_id: String` - System designation of the target hotend profile structure (e.g. `"HS00-0.4"`).
- `extruder_id: u8` - Carriage layout indicator (0 = Right/Primary extruder, 1 = Left/Deputy extruder).
- `name: String` - Custom user-defined name assigned to label the profile slot.
- `k_value: String` - Calibrated Linear Advance constant serialized as a float string.
- `n_coef: Option<String>` - Extrusion coefficient parameters.
- `setting_id: String` - Secure 19-character unique setting identifier.
- `ams_id: Option<i32>` - Links K-profile to AMS unit (default 0).
- `tray_id: Option<i32>` - Links K-profile to AMS tray slot (default -1).

**Trait Implementations:**

- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **PartialEq**
  - `fn eq(self: &Self, other: &KProfileEntry) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> KProfileEntry`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::diagnostics::kprofile::StandardCaliDelEntry

*Struct*

Deletion data fields utilized by standard single-nozzle databases (Schema A).

**Fields:**
- `cali_idx: i32` - Index of the calibration entry to delete (`KProfileEntry::cali_idx`).
- `filament_id: String` - Filament preset ID of the entry being deleted (`KProfileEntry::filament_id`).
- `nozzle_diameter: String` - Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).
- `nozzle_id: String` - System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).
- `setting_id: String` - 19-character setting ID of the entry being deleted, validated by [`validate_setting_id`].

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **PartialEq**
  - `fn eq(self: &Self, other: &StandardCaliDelEntry) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> StandardCaliDelEntry`



## bambino::diagnostics::kprofile::StandardCaliDelPayload

*Struct*

Inner payload for [`StandardCaliDelRequest`].

**Fields:**
- `command: &'static str` - Wire command name, always `"extrusion_cali_del"`.
- `filaments: Vec<StandardCaliDelEntry>` - Entries to delete. `StandardCaliDelRequest::new` always sends exactly one.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> StandardCaliDelPayload`



## bambino::diagnostics::kprofile::StandardCaliDelRequest

*Struct*

JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].

**Fields:**
- `print: StandardCaliDelPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(target: StandardCaliDelEntry, sequence_id: u64) -> Result<Self, BambuError>` - Builds a single-nozzle deletion transaction keyed on the setting identifier.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> StandardCaliDelRequest`



## bambino::diagnostics::kprofile::validate_setting_id

*Function*

Validates whether a provided calibration profile setting ID complies with EEPROM limits.

**The Calibration Setting ID Boundary Rule [REF-DIAG-KPROF]:**
Stored EEPROM K-profiles require standard 19-character numeric formats consisting of a
`"PF"` header prefix followed by exactly 17 numeric digits. Standard alphanumeric hashes
(e.g. `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side presets.
Transmitting alphanumeric layouts inside direct database operations causes indexing halts
or table corruption on the physical mainboard.

```rust
fn validate_setting_id(setting_id: &str) -> bool
```



