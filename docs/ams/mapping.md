**bambino > ams > mapping**

# Module: ams::mapping

## Contents

**Structs**

- [`AmsMapping2Entry`](#amsmapping2entry) - Structured object detailing unit and slot coordinates within `ams_mapping2` arrays.

**Enums**

- [`MaterialSource`](#materialsource) - Enumeration of possible physical feed locations for loaded spools.

**Functions**

- [`build_ams_mapping`](#build_ams_mapping) - Builds the flat `ams_mapping` integer array from raw project allocations.
- [`build_ams_mapping2`](#build_ams_mapping2) - Builds the structured `ams_mapping2` object array from raw project allocations.
- [`validate_external_spool_safety`](#validate_external_spool_safety) - Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle.
- [`validate_external_spool_safety_flat`](#validate_external_spool_safety_flat) - Flat-array equivalent of `validate_external_spool_safety`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.

---

## bambino::ams::mapping::AmsMapping2Entry

*Struct*

Structured object detailing unit and slot coordinates within `ams_mapping2` arrays.

**Fields:**
- `ams_id: u8` - AMS unit index (0-3 for standard, 128+ for AMS-HT, 254/255 for external/unmapped).
- `slot_id: u8` - Tray slot index within the unit (0-3 for standard AMS, 0 for single-slot units).

**Traits:** Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **PartialEq**
  - `fn eq(self: &Self, other: &AmsMapping2Entry) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> AmsMapping2Entry`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::ams::mapping::MaterialSource

*Enum*

Enumeration of possible physical feed locations for loaded spools.

**Variants:**
- `StandardAms{ ams_id: u8, slot_id: u8 }` - Spool loaded inside a standard 4-slot AMS unit.
- `AmsHt{ ams_id: u8 }` - Spool loaded inside a single-slot High-Temperature (AMS-HT) dry-chamber.
- `ExternalSpool` - Default virtual external spool holder (used for standard single-nozzle models).
- `ExternalSpoolLeft` - Left external spool holder (specifically used on dual-nozzle IDEX systems).
- `ExternalSpoolRight` - Right external spool holder (specifically used on dual-nozzle IDEX systems).
- `Unmapped` - Virtual unmapped placeholder slot (indicates an unused project filament).

**Methods:**

- `fn flat_channel_id(self: &Self) -> i32` - Computes the flat channel integer value used in standard `ams_mapping` arrays.
- `fn to_mapping2_entry(self: &Self) -> AmsMapping2Entry` - Converts this source location into a structured `ams_mapping2` JSON entry.

**Traits:** Eq, Copy

**Trait Implementations:**

- **PartialEq**
  - `fn eq(self: &Self, other: &MaterialSource) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> MaterialSource`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::ams::mapping::build_ams_mapping

*Function*

Builds the flat `ams_mapping` integer array from raw project allocations.

`allocations` is a slice of `(filament_id, MaterialSource)` pairs where `filament_id`
represents the 1-based index (1 to N) of the project material defined in the slicer.

**Array Length Rule [REF-AMS-MAP]:**
The length of the array is governed by the highest filament ID index present in the project,
rather than the total count of active spools. If a project uses filament 1 and filament 4,
the array must be padded to a length of 4 (elements 0 to 3), using the `-1` sentinel for
intermediate unused filament indexes.

```rust
fn build_ams_mapping(allocations: &[(usize, MaterialSource)]) -> Vec<i32>
```



## bambino::ams::mapping::build_ams_mapping2

*Function*

Builds the structured `ams_mapping2` object array from raw project allocations.

Symmetrical to `build_ams_mapping`, this array provides detailed physical unit routing
parameters to ensure correct material transitions on multi-AMS and IDEX platforms.

```rust
fn build_ams_mapping2(allocations: &[(usize, MaterialSource)]) -> Vec<AmsMapping2Entry>
```



## bambino::ams::mapping::validate_external_spool_safety

*Function*

Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle.

**Mandatory use_ams Override on Single-Nozzle Systems [REF-AMS-USEAMS]:**
If a print job is dispatched exclusively from the external spool (meaning all active project
filaments map to `ExternalSpool` or are left `Unmapped`), single-nozzle printers require that the
`use_ams` command parameter be configured strictly to `false`. Failing to override this parameter
causes the printer's execution processor to reject the print task with error `07FF_8012`.

```rust
fn validate_external_spool_safety(is_single_nozzle: bool, mapping2: &[AmsMapping2Entry]) -> bool
```



## bambino::ams::mapping::validate_external_spool_safety_flat

*Function*

Flat-array equivalent of `validate_external_spool_safety`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.

```rust
fn validate_external_spool_safety_flat(is_single_nozzle: bool, ams_mapping: &[i32]) -> bool
```



