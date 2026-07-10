*[bambino](../index.md) / [ams](index.md)*

---

# Module `ams`

# AMS Filament System

Helpers for working with Bambu Lab's Automatic Material System.

Handles the mapping between slicer material slots and physical AMS tray positions,
including multi-AMS index resolution, spool presence detection, and stale tray data
cleanup. Supports standard AMS units, AMS-HT dry chambers, and virtual external spools.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`mapping`](#mapping) | mod | # AMS Slicer Mapping & Filament Change Builders |
| [`parser`](#parser) | mod | # AMS Telemetry & Bitmask Parser |

## Modules

- [`mapping`](mapping/index.md#mapping) — # AMS Slicer Mapping & Filament Change Builders
- [`parser`](parser/index.md#parser) — # AMS Telemetry & Bitmask Parser


---

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

- <span id="amsmapping2entry-clone"></span>`fn clone(&self) -> AmsMapping2Entry` — [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

##### `impl Debug for AmsMapping2Entry`

- <span id="amsmapping2entry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for AmsMapping2Entry`

- <span id="amsmapping2entry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for AmsMapping2Entry`

##### `impl Eq for AmsMapping2Entry`

##### `impl PartialEq for AmsMapping2Entry`

- <span id="amsmapping2entry-partialeq-eq"></span>`fn eq(&self, other: &AmsMapping2Entry) -> bool` — [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

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

- <span id="materialsource-to-mapping2-entry"></span>`fn to_mapping2_entry(&self) -> AmsMapping2Entry` — [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

  Converts this source location into a structured `ams_mapping2` JSON entry.

#### Trait Implementations

##### `impl Clone for MaterialSource`

- <span id="materialsource-clone"></span>`fn clone(&self) -> MaterialSource` — [`MaterialSource`](mapping/index.md#materialsource)

##### `impl Copy for MaterialSource`

##### `impl Debug for MaterialSource`

- <span id="materialsource-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for MaterialSource`

##### `impl PartialEq for MaterialSource`

- <span id="materialsource-partialeq-eq"></span>`fn eq(&self, other: &MaterialSource) -> bool` — [`MaterialSource`](mapping/index.md#materialsource)


---

## Functions

### `build_ams_mapping`

```rust
fn build_ams_mapping(allocations: &[(usize, MaterialSource)]) -> Vec<i32>
```

**Types:** [`MaterialSource`](mapping/index.md#materialsource)

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

**Types:** [`MaterialSource`](mapping/index.md#materialsource), [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

Builds the structured `ams_mapping2` object array from raw project allocations.

Symmetrical to `build_ams_mapping`, this array provides detailed physical unit routing
parameters to ensure correct material transitions on multi-AMS and IDEX platforms.

### `validate_external_spool_safety`

```rust
fn validate_external_spool_safety(is_single_nozzle: bool, mapping2: &[AmsMapping2Entry]) -> bool
```

**Types:** [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

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

### `clean_stale_tray_data`

```rust
fn clean_stale_tray_data(tray: &mut crate::types::AmsTray)
```

**Types:** [`AmsTray`](../types/telemetry/ams/index.md#amstray)

Explicitly sanitizes and nullifies telemetry fields when a physical slot becomes empty.

**Incremental Telemetry Update Slot Cleansing Rules [REF-AMS-DECODE]:**
To save network bandwidth, the printer's incremental telemetry pushes omit configuration
parameters (like `tray_type` or `tray_color`) when a spool is extracted. Without active
cleanup on the client side, standard parsers would preserve the material properties of the
previously loaded spool indefinitely.

This routine inspects the tray's state — 9 (`AMS_TRAY_STATE_EMPTY`) meaning Empty/Absent and
10 (`AMS_TRAY_STATE_SPOOL_NOT_FED`) meaning a spool is physically present but not yet fed to
the extruder, both treated as absent-equivalent for stale-data cleansing (BUG-012, verified
against pybambu/Bambuddy — see `AMS_TRAY_STATE_SPOOL_NOT_FED`'s doc comment) — and clears all
stale config keys if either applies. It treats an empty `tray_type` string as an explicit
clearing signal too.

### `evaluate_spool_presence`

```rust
fn evaluate_spool_presence(tray_exist_bits: &str, ams_id: u8, tray_id: u8, power_on_flag: bool) -> Option<bool>
```

Evaluates if a physical spool is present in a specific standard AMS slot.

Standard AMS units contain up to 4 slots. The physical presence is tracked via
a hexadecimal bitmask string (`tray_exist_bits`).

**The Printer-Shutdown Telemetry Exception [REF-AMS-DECODE]:**
During printer shutdown sequences, the firmware often emits a final status packet
where `tray_exist_bits` is `0` and `power_on_flag` is `false`. To prevent downstream
observers from falsely reporting a cascade of physical "spool removed" events,
this evaluator returns `None` strictly when both conditions are met. If `power_on_flag`
is `false` but the parsed bitmask is non-zero, this represents a valid offline state
and is processed normally.

### `resolve_global_tray_id`

```rust
fn resolve_global_tray_id(ams_id: u8, tray_id: u8) -> Option<u8>
```

Computes the unique global channel identifier for a given expansion unit and local tray.

Returns `None` if `ams_id` falls outside all valid ranges (standard 0–3,
AMS-HT 128–135, external 254–255) or if `tray_id >= 4` on the standard path.

The physical mapping aligns as:
* **Standard AMS Slots**: Sized in blocks of 4 per expansion unit: `(ams_id * 4) + tray_id`.
* **AMS-HT Units**: Single-slot systems where the channel ID equals the bus `ams_id` directly.
* **Virtual Spools**: Channels mapped to the external spool holder (ID 254 or 255).

### `resolve_printing_global_id`

```rust
fn resolve_printing_global_id(tray_now: u8, active_extruder: Option<u8>, ams_extruder_map: &[u8]) -> Option<u8>
```

Resolves the currently printing tray's global ID, accounting for IDEX map translations.

**Multi-AMS Local Index Resolution [REF-AMS-DECODE]:**
Multi-extruder platforms (such as the H2D series) emit local slot indexes (0 to 3) inside
their `tray_now` telemetry parameter. To resolve this back to a global index, the client must
inspect the active extruder carriage and correlate it against the `ams_extruder_map` matrix.

