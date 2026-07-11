*[bambino](../../index.md) / [ams](../index.md) / [mapping](index.md)*

---

# Module `mapping`

# AMS Slicer Mapping & Filament Change Builders

Handles translation of slicer-allocated project materials into physical and
virtual printer hardware channels [REF-AMS-MAP]. Implements flat `ams_mapping` and
structured `ams_mapping2` payload arrays and enforces safety interlocks for single-nozzle
external spools [REF-AMS-USEAMS].

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsMapping2Entry`](#amsmapping2entry) | struct | Structured object detailing unit and slot coordinates within `ams_mapping2` arrays. |
| [`MaterialSource`](#materialsource) | enum | Enumeration of possible physical feed locations for loaded spools. |
| [`build_ams_mapping`](#build-ams-mapping) | fn | Builds the flat `ams_mapping` integer array from raw project allocations. |
| [`build_ams_mapping2`](#build-ams-mapping2) | fn | Builds the structured `ams_mapping2` object array from raw project allocations. |
| [`flat_channel_id_for_entry`](#flat-channel-id-for-entry) | fn | Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to. |
| [`validate_external_spool_safety`](#validate-external-spool-safety) | fn | Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle. |
| [`validate_external_spool_safety_flat`](#validate-external-spool-safety-flat) | fn | Flat-array equivalent of `validate_external_spool_safety`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`. |

## Types

### `AmsMapping2Entry`

```rust
struct AmsMapping2Entry {
    pub ams_id: u8,
    pub slot_id: u8,
}
```

Structured object detailing unit and slot coordinates within `ams_mapping2` arrays.

#### Fields

- **`ams_id`**: `u8`

  AMS unit index (0-3 for standard, 128+ for AMS-HT, 254/255 for external/unmapped).

- **`slot_id`**: `u8`

  Tray slot index within the unit (0-3 for standard AMS, 0 for single-slot units).

#### Trait Implementations

##### `impl Clone for AmsMapping2Entry`

- <span id="amsmapping2entry-clone"></span>`fn clone(&self) -> AmsMapping2Entry` — [`AmsMapping2Entry`](#amsmapping2entry)

##### `impl Debug for AmsMapping2Entry`

- <span id="amsmapping2entry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsMapping2Entry`

- <span id="amsmapping2entry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsMapping2Entry`

##### `impl Eq for AmsMapping2Entry`

##### `impl PartialEq for AmsMapping2Entry`

- <span id="amsmapping2entry-partialeq-eq"></span>`fn eq(&self, other: &AmsMapping2Entry) -> bool` — [`AmsMapping2Entry`](#amsmapping2entry)

##### `impl Serialize for AmsMapping2Entry`

- <span id="amsmapping2entry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `MaterialSource`

```rust
enum MaterialSource {
    StandardAms {
        ams_id: u8,
        slot_id: u8,
    },
    AmsHt {
        ams_id: u8,
    },
    ExternalSpool,
    ExternalSpoolLeft,
    ExternalSpoolRight,
    Unmapped,
}
```

Enumeration of possible physical feed locations for loaded spools.

#### Variants

- **`StandardAms`**

  Spool loaded inside a standard 4-slot AMS unit.

- **`AmsHt`**

  Spool loaded inside a single-slot High-Temperature (AMS-HT) dry-chamber.

- **`ExternalSpool`**

  Default virtual external spool holder (used for standard single-nozzle models).

- **`ExternalSpoolLeft`**

  Left external spool holder (specifically used on dual-nozzle IDEX systems).

- **`ExternalSpoolRight`**

  Right external spool holder (specifically used on dual-nozzle IDEX systems).

- **`Unmapped`**

  Virtual unmapped placeholder slot (indicates an unused project filament).

#### Implementations

- <span id="materialsource-flat-channel-id"></span>`fn flat_channel_id(&self) -> i32`

  Computes the flat channel integer value used in standard `ams_mapping` arrays.

- <span id="materialsource-to-mapping2-entry"></span>`fn to_mapping2_entry(&self) -> AmsMapping2Entry` — [`AmsMapping2Entry`](#amsmapping2entry)

  Converts this source location into a structured `ams_mapping2` JSON entry.

#### Trait Implementations

##### `impl Clone for MaterialSource`

- <span id="materialsource-clone"></span>`fn clone(&self) -> MaterialSource` — [`MaterialSource`](#materialsource)

##### `impl Copy for MaterialSource`

##### `impl Debug for MaterialSource`

- <span id="materialsource-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for MaterialSource`

##### `impl PartialEq for MaterialSource`

- <span id="materialsource-partialeq-eq"></span>`fn eq(&self, other: &MaterialSource) -> bool` — [`MaterialSource`](#materialsource)


---

## Functions

### `build_ams_mapping`

```rust
fn build_ams_mapping(allocations: &[(usize, MaterialSource)]) -> Vec<i32>
```

**Types:** [`MaterialSource`](#materialsource)

Builds the flat `ams_mapping` integer array from raw project allocations.

`allocations` is a slice of `(filament_id, MaterialSource)` pairs where `filament_id`
represents the 1-based index (1 to N) of the project material defined in the slicer.

**Array Length Rule [REF-AMS-MAP]:**
The length of the array is governed by the highest filament ID index present in the project,
rather than the total count of active spools. If a project uses filament 1 and filament 4,
the array must be padded to a length of 4 (elements 0 to 3), using the `-1` sentinel for
intermediate unused filament indexes.

### `build_ams_mapping2`

```rust
fn build_ams_mapping2(allocations: &[(usize, MaterialSource)]) -> Vec<AmsMapping2Entry>
```

**Types:** [`MaterialSource`](#materialsource), [`AmsMapping2Entry`](#amsmapping2entry)

Builds the structured `ams_mapping2` object array from raw project allocations.

Symmetrical to `build_ams_mapping`, this array provides detailed physical unit routing
parameters to ensure correct material transitions on multi-AMS and IDEX platforms.

### `flat_channel_id_for_entry`

```rust
fn flat_channel_id_for_entry(entry: &AmsMapping2Entry) -> i32
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry)

Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to.

Inverse of `MaterialSource::flat_channel_id`, operating on the already-structured
`ams_id`/`slot_id` pair instead of a `MaterialSource` — used by
`ProjectFileRequest::from_config` (`mqtt/commands/print_job.rs`) to derive `ams_mapping`
from `ams_mapping2` when the caller only supplied the latter via
`PrintJobConfig::with_ams_mapping2()`, so the two arrays never go out of sync (BUG-033).

### `validate_external_spool_safety`

```rust
fn validate_external_spool_safety(is_single_nozzle: bool, mapping2: &[AmsMapping2Entry]) -> bool
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry)

Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle.

**Mandatory use_ams Override on Single-Nozzle Systems [REF-AMS-USEAMS]:**
If a print job is dispatched exclusively from the external spool (meaning all active project
filaments map to `ExternalSpool` or are left `Unmapped`), single-nozzle printers require that the
`use_ams` command parameter be configured strictly to `false`. Failing to override this parameter
causes the printer's execution processor to reject the print task with error `07FF_8012`.

### `validate_external_spool_safety_flat`

```rust
fn validate_external_spool_safety_flat(is_single_nozzle: bool, ams_mapping: &[i32]) -> bool
```

Flat-array equivalent of `validate_external_spool_safety`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.

