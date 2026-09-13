*[bambino](../../index.md) / [ams](../index.md) / [mapping](index.md)*

---

# Module `mapping`

# AMS Slicer Mapping & Filament Change Builders

Handles translation of slicer-allocated project materials into physical and
virtual printer hardware channels [REF-AMS-MAP]. Implements flat `ams_mapping` and
structured `ams_mapping2` payload arrays and enforces safety interlocks for single-nozzle
external spools [REF-AMS-USEAMS].

## Contents

- [Types](#types)
  - [`AmsMapping2Entry`](#amsmapping2entry)
  - [`AmsEntryKind`](#amsentrykind)
  - [`AmsLiteSlot`](#amsliteslot)
  - [`AmsPoolComposition`](#amspoolcomposition)
  - [`MaterialSource`](#materialsource)
- [Functions](#functions)
  - [`build_ams_mapping`](#build-ams-mapping)
  - [`build_ams_mapping2`](#build-ams-mapping2)
  - [`classify_mapping2_entry`](#classify-mapping2-entry)
  - [`flat_channel_id_for_entry`](#flat-channel-id-for-entry)
  - [`is_ams_pool_composition_valid`](#is-ams-pool-composition-valid)
  - [`is_external_spool_safety_valid`](#is-external-spool-safety-valid)
  - [`is_external_spool_safety_valid_flat`](#is-external-spool-safety-valid-flat)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsMapping2Entry`](#amsmapping2entry) | struct | Structured object detailing unit and slot coordinates within `ams_mapping2` arrays. |
| [`AmsEntryKind`](#amsentrykind) | enum | What kind of feed location a raw [`AmsMapping2Entry`](#amsmapping2entry)'s `ams_id`/`slot_id` pair names. |
| [`AmsLiteSlot`](#amsliteslot) | enum | How a model accepts an AMS Lite alongside its shared AMS pool. |
| [`AmsPoolComposition`](#amspoolcomposition) | enum | Per-model AMS unit pool structure, confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row (user-supplied official Bambu documentation). |
| [`MaterialSource`](#materialsource) | enum | Enumeration of possible physical feed locations for loaded spools. |
| [`build_ams_mapping`](#build-ams-mapping) | fn | Builds the flat `ams_mapping` integer array from raw project allocations. |
| [`build_ams_mapping2`](#build-ams-mapping2) | fn | Builds the structured `ams_mapping2` object array from raw project allocations. |
| [`classify_mapping2_entry`](#classify-mapping2-entry) | fn | Classifies a raw `ams_mapping2` entry by the kind of feed location it names. |
| [`flat_channel_id_for_entry`](#flat-channel-id-for-entry) | fn | Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to. |
| [`is_ams_pool_composition_valid`](#is-ams-pool-composition-valid) | fn | Validates a constructed `ams_mapping2` against the model's actual AMS pool structure. |
| [`is_external_spool_safety_valid`](#is-external-spool-safety-valid) | fn | Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle. |
| [`is_external_spool_safety_valid_flat`](#is-external-spool-safety-valid-flat) | fn | Flat-array equivalent of `is_external_spool_safety_valid`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`. |

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

  AMS unit index (0-3 for standard, 16 for an A2L-attached AMS Lite, 128-135 for AMS-HT,
  254/255 for external/unmapped).
  
  Id 16 is a real physical unit, not a sentinel — omitting it here is the exact footgun
  [`AmsEntryKind`](#amsentrykind) warns about, since a caller reading this field in isolation could add
  defensive rejection that breaks A2L support (issue #221). Classify with
  [`classify_mapping2_entry`](#classify-mapping2-entry) rather than re-deriving the ranges.

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

### `AmsEntryKind`

```rust
enum AmsEntryKind {
    Standard,
    Ht,
    AmsLite,
    External,
    Invalid,
}
```

What kind of feed location a raw [`AmsMapping2Entry`](#amsmapping2entry)'s `ams_id`/`slot_id` pair names.

Single place the "which `ams_id`s are real physical units" rule lives. `MaterialSource`'s
own methods can rely on the enum variant to tell them, but every function that instead
re-derives physical-ness from a hand-built `AmsMapping2Entry` has to reproduce the same
range checks — and each one independently missed an A2L-attached AMS Lite's physical id 16 when
it was added (issue #221). Route those through [`classify_mapping2_entry`](#classify-mapping2-entry) so a new one
cannot omit a unit type by hand.

#### Variants

- **`Standard`**

  Standard 4-slot AMS unit, `ams_id` 0-3 with an in-range `slot_id`.

- **`Ht`**

  Single-slot AMS-HT unit, `ams_id` 128-135.

- **`AmsLite`**

  The A2L's 4-slot AMS Lite, carried on the wire as physical unit id 16.

- **`External`**

  One of the two external-spool sentinel ids (254/255), including the
  `{255, 255}` unmapped sentinel.

- **`Invalid`**

  Neither a validly-ranged physical unit nor a recognized sentinel.

#### Trait Implementations

##### `impl Clone for AmsEntryKind`

- <span id="amsentrykind-clone"></span>`fn clone(&self) -> AmsEntryKind` — [`AmsEntryKind`](#amsentrykind)

##### `impl Copy for AmsEntryKind`

##### `impl Debug for AmsEntryKind`

- <span id="amsentrykind-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsEntryKind`

##### `impl PartialEq for AmsEntryKind`

- <span id="amsentrykind-partialeq-eq"></span>`fn eq(&self, other: &AmsEntryKind) -> bool` — [`AmsEntryKind`](#amsentrykind)

### `AmsLiteSlot`

```rust
enum AmsLiteSlot {
    None,
    Additive,
    Exclusive,
}
```

How a model accepts an AMS Lite alongside its shared AMS pool.

Confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row.

#### Variants

- **`None`**

  No AMS Lite attaches (X1C, X1E, P1P, P1S).

- **`Additive`**

  One AMS Lite attaches *in addition to* the full shared pool (A2L).
  
  It reports physical unit id 16 ([`AmsEntryKind::AmsLite`](#amsentrykind)), so it is counted on its own
  and never against the shared pool.

- **`Exclusive`**

  One AMS Lite attaches *instead of* the shared pool, never combined with it (A1, A1 Mini).
  
  On these models the AMS Lite shares ids `0..=3` with standard AMS units, so an
  `ams_mapping2` cannot say which unit is the Lite and [`is_ams_pool_composition_valid`](#is-ams-pool-composition-valid)
  cannot enforce the exclusivity; the unit's type is in telemetry
  ([`AmsUnitModel`](../../types/telemetry/index.md)).

#### Trait Implementations

##### `impl Clone for AmsLiteSlot`

- <span id="amsliteslot-clone"></span>`fn clone(&self) -> AmsLiteSlot` — [`AmsLiteSlot`](#amsliteslot)

##### `impl Copy for AmsLiteSlot`

##### `impl Debug for AmsLiteSlot`

- <span id="amsliteslot-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsLiteSlot`

##### `impl PartialEq for AmsLiteSlot`

- <span id="amsliteslot-partialeq-eq"></span>`fn eq(&self, other: &AmsLiteSlot) -> bool` — [`AmsLiteSlot`](#amsliteslot)

### `AmsPoolComposition`

```rust
enum AmsPoolComposition {
    Shared {
        max_units: u8,
        ams_lite: AmsLiteSlot,
    },
    Independent {
        max_standard: u8,
        max_ht: u8,
    },
}
```

Per-model AMS unit pool structure, confirmed against `MODEL_MATRIX.csv`'s
"AMS Unit Limits" row (user-supplied official Bambu documentation).

An A2L-attached AMS Lite reports physical unit id 16, which
[`normalize_ams_unit_id`](../parser/index.md#normalize-ams-unit-id) maps to 6 on ingest, and
[`MaterialSource::AmsLite`](#materialsource) addresses its slots with their own wire encodings.

#### Variants

- **`Shared`**

  Standard AMS and AMS-HT units draw from one combined pool of `max_units` total
  (X1C, X1E, P1P, P1S, A1, A1 Mini, A2L).

- **`Independent`**

  Standard AMS and AMS-HT units draw from independent pools, each with its own cap
  (H2C, H2D, H2D Pro, H2S, X2D, P2S).

#### Implementations

- <span id="amspoolcomposition-max-units"></span>`fn max_units(self) -> u8`

  Returns the most units of every kind the model can have attached at once — the number to size a per-unit UI to.

  An A2L's additive AMS Lite counts, so it answers 5. An A1's exclusive AMS Lite does not
  raise the total, since it replaces the pool rather than joining it.

#### Trait Implementations

##### `impl Clone for AmsPoolComposition`

- <span id="amspoolcomposition-clone"></span>`fn clone(&self) -> AmsPoolComposition` — [`AmsPoolComposition`](#amspoolcomposition)

##### `impl Copy for AmsPoolComposition`

##### `impl Debug for AmsPoolComposition`

- <span id="amspoolcomposition-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsPoolComposition`

##### `impl PartialEq for AmsPoolComposition`

- <span id="amspoolcomposition-partialeq-eq"></span>`fn eq(&self, other: &AmsPoolComposition) -> bool` — [`AmsPoolComposition`](#amspoolcomposition)

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
    AmsLite {
        slot_id: u8,
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

- **`AmsLite`**

  Spool loaded inside the A2L's 4-slot AMS Lite.
  
  Kept separate from [`MaterialSource::StandardAms`](#materialsource) because this unit's two wire
  encodings do not follow the standard unit's: the flat `ams_mapping` array carries the
  bare **local** slot rather than a global `ams_id * 4 + slot` channel, and `ams_mapping2`
  carries the **physical** unit id 16 rather than the normalized 6 the rest of the crate
  addresses it by. A `StandardAms { ams_id: 6, .. }` would get both wrong.

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

  **External Spool Flat-Mapping Restrictions [REF-AMS-MAP]:**
  The printer's motion controller rejects absolute external virtual spool IDs (such as
  254 or 255) if passed inside the flat `ams_mapping` array, throwing a `0700_8012`
  "Failed to get AMS mapping table" error. Virtual external spools and unused slots
  must strictly be mapped to the `-1` (unmapped) sentinel in the flat array.

  `StandardAms`/`AmsHt` fields are public `u8`s a caller can hand-build with an
  out-of-range `ams_id`/`slot_id` (unlike `parser.rs`'s inbound-side bounds-checking on
  wire data) — validated here the same way, falling back to the `-1` sentinel rather than
  producing a bogus flat channel value.

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
Ids above the physical ceiling of `AMS_MAX_PROJECT_FILAMENTS` (16 flat channels plus the 8
an AMS-HT configuration adds) are dropped with a warning rather than sizing the output array.

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

### `classify_mapping2_entry`

```rust
fn classify_mapping2_entry(entry: &AmsMapping2Entry) -> AmsEntryKind
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry), [`AmsEntryKind`](#amsentrykind)

Classifies a raw `ams_mapping2` entry by the kind of feed location it names.

`slot_id` is range-checked for the multi-slot unit types (standard and AMS Lite) but
ignored for AMS-HT, which is single-slot and encodes nothing in the field, and for the
external sentinels, whose `slot_id` is `0` or `255` depending on whether the entry means
"external spool" or "unmapped".

### `flat_channel_id_for_entry`

```rust
fn flat_channel_id_for_entry(entry: &AmsMapping2Entry) -> i32
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry)

Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to.

Inverse of `MaterialSource::flat_channel_id`, operating on the already-structured
`ams_id`/`slot_id` pair instead of a `MaterialSource` — keeps `ams_mapping` and
`ams_mapping2` from going out of sync when only the latter was supplied.

### `is_ams_pool_composition_valid`

```rust
fn is_ams_pool_composition_valid(mapping2: &[AmsMapping2Entry], composition: AmsPoolComposition) -> bool
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry), [`AmsPoolComposition`](#amspoolcomposition)

Validates a constructed `ams_mapping2` against the model's actual AMS pool structure.
Rejects configs no real hardware combination could serve — e.g. 4 standard +
8 AMS-HT units on a P2S, which only has independent pools of 4 and 4.

Counts *distinct* `ams_id`s used (not slot allocations) — a config referencing the same
unit across multiple slots isn't an extra unit. External-spool and unmapped sentinel
entries are ignored, since they don't occupy a physical AMS unit slot.

An A2L-attached AMS Lite (id 16) is valid only on a model with [`AmsLiteSlot::Additive`](#amsliteslot),
and never counts against the shared pool there. [`AmsLiteSlot::Exclusive`](#amsliteslot) is not enforced:
an A1's AMS Lite uses the standard ids, so nothing in the mapping identifies it.

### `is_external_spool_safety_valid`

```rust
fn is_external_spool_safety_valid(is_single_nozzle: bool, mapping2: &[AmsMapping2Entry]) -> bool
```

**Types:** [`AmsMapping2Entry`](#amsmapping2entry)

Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle.

**Mandatory use_ams Override on Single-Nozzle Systems [REF-AMS-USEAMS]:**
If a print job is dispatched exclusively from the external spool (meaning all active project
filaments map to `ExternalSpool` or are left `Unmapped`), single-nozzle printers require that the
`use_ams` command parameter be configured strictly to `false`. Failing to override this parameter
causes the printer's execution processor to reject the print task with error `07FF_8012`.

### `is_external_spool_safety_valid_flat`

```rust
fn is_external_spool_safety_valid_flat(is_single_nozzle: bool, ams_mapping: &[i32]) -> bool
```

Flat-array equivalent of `is_external_spool_safety_valid`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.

