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
| [`mapping`](mapping/index.md) | mod | # AMS Slicer Mapping & Filament Change Builders |
| [`parser`](parser/index.md) | mod | # AMS Telemetry & Bitmask Parser |

## Modules

- [`mapping`](mapping/index.md) — # AMS Slicer Mapping & Filament Change Builders
- [`parser`](parser/index.md) — # AMS Telemetry & Bitmask Parser


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

  AMS unit index (0-3 for standard, 128-135 for AMS-HT, 254/255 for external/unmapped).

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

What kind of feed location a raw [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)'s `ams_id`/`slot_id` pair names.

Single place the "which `ams_id`s are real physical units" rule lives. `MaterialSource`'s
own methods can rely on the enum variant to tell them, but every function that instead
re-derives physical-ness from a hand-built `AmsMapping2Entry` has to reproduce the same
range checks — and each one independently missed the A2L AMS Lite's physical id 16 when
it was added (issue #221). Route those through [`classify_mapping2_entry`](mapping/index.md#classify-mapping2-entry) so a new one
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

- <span id="amsentrykind-clone"></span>`fn clone(&self) -> AmsEntryKind` — [`AmsEntryKind`](mapping/index.md#amsentrykind)

##### `impl Copy for AmsEntryKind`

##### `impl Debug for AmsEntryKind`

- <span id="amsentrykind-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsEntryKind`

##### `impl PartialEq for AmsEntryKind`

- <span id="amsentrykind-partialeq-eq"></span>`fn eq(&self, other: &AmsEntryKind) -> bool` — [`AmsEntryKind`](mapping/index.md#amsentrykind)

### `AmsPoolComposition`

```rust
enum AmsPoolComposition {
    Shared {
        max_units: u8,
    },
    Independent {
        max_standard: u8,
        max_ht: u8,
    },
}
```

Per-model AMS unit pool structure, confirmed against `MODEL_MATRIX.csv`'s
"AMS Unit Limits" row (user-supplied official Bambu documentation).

**Known limitation**: this enum still cannot express A1/A1 Mini's "shared pool OR 1 AMS
Lite, not combinable" exclusivity, or A2L's "shared pool + 1 AMS Lite simultaneously"
additive capacity. Both are conservatively modeled as `Shared { max_units: 4 }`, the same
as the plain shared-pool models — this may under-count A2L's true capacity by one unit, but
never accepts a config that's actually invalid.

This is a *capacity-counting* gap only. The addressing gap it used to describe — "AMS Lite
units are not independently addressable ... they use the same `ams_id` space as standard AMS
units" — is fixed: the A2L AMS Lite reports physical unit id 16, which
[`normalize_ams_unit_id`](parser/index.md#normalize-ams-unit-id) maps to 6 on ingest, and
[`MaterialSource::AmsLite`](mapping/index.md#materialsource) addresses its slots with their own wire encodings.

#### Variants

- **`Shared`**

  Standard AMS and AMS-HT units draw from one combined pool of `max_units` total
  (X1C, X1E, P1P, P1S, A1, A1 Mini, A2L).

- **`Independent`**

  Standard AMS and AMS-HT units draw from independent pools, each with its own cap
  (H2C, H2D, H2D Pro, H2S, X2D, P2S).

#### Trait Implementations

##### `impl Clone for AmsPoolComposition`

- <span id="amspoolcomposition-clone"></span>`fn clone(&self) -> AmsPoolComposition` — [`AmsPoolComposition`](mapping/index.md#amspoolcomposition)

##### `impl Copy for AmsPoolComposition`

##### `impl Debug for AmsPoolComposition`

- <span id="amspoolcomposition-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsPoolComposition`

##### `impl PartialEq for AmsPoolComposition`

- <span id="amspoolcomposition-partialeq-eq"></span>`fn eq(&self, other: &AmsPoolComposition) -> bool` — [`AmsPoolComposition`](mapping/index.md#amspoolcomposition)

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
  
  Kept separate from [`MaterialSource::StandardAms`](mapping/index.md#materialsource) because this unit's two wire
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

**Types:** [`MaterialSource`](mapping/index.md#materialsource), [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

Builds the structured `ams_mapping2` object array from raw project allocations.

Symmetrical to `build_ams_mapping`, this array provides detailed physical unit routing
parameters to ensure correct material transitions on multi-AMS and IDEX platforms.

### `classify_mapping2_entry`

```rust
fn classify_mapping2_entry(entry: &AmsMapping2Entry) -> AmsEntryKind
```

**Types:** [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry), [`AmsEntryKind`](mapping/index.md#amsentrykind)

Classifies a raw `ams_mapping2` entry by the kind of feed location it names.

`slot_id` is range-checked for the multi-slot unit types (standard and AMS Lite) but
ignored for AMS-HT, which is single-slot and encodes nothing in the field, and for the
external sentinels, whose `slot_id` is `0` or `255` depending on whether the entry means
"external spool" or "unmapped".

### `is_ams_pool_composition_valid`

```rust
fn is_ams_pool_composition_valid(mapping2: &[AmsMapping2Entry], composition: AmsPoolComposition) -> bool
```

**Types:** [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry), [`AmsPoolComposition`](mapping/index.md#amspoolcomposition)

Validates a constructed `ams_mapping2` against the model's actual AMS pool structure.
Rejects configs no real hardware combination could serve — e.g. 4 standard +
8 AMS-HT units on a P2S, which only has independent pools of 4 and 4.

Counts *distinct* `ams_id`s used (not slot allocations) — a config referencing the same
unit across multiple slots isn't an extra unit. External-spool and unmapped sentinel
entries are ignored, since they don't occupy a physical AMS unit slot.

### `is_external_spool_safety_valid`

```rust
fn is_external_spool_safety_valid(is_single_nozzle: bool, mapping2: &[AmsMapping2Entry]) -> bool
```

**Types:** [`AmsMapping2Entry`](mapping/index.md#amsmapping2entry)

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

### `clean_stale_tray_data`

```rust
fn clean_stale_tray_data(tray: &mut crate::types::AmsTray, ams_id: u8)
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
the extruder, both treated as absent-equivalent for stale-data cleansing (verified
against pybambu/Bambuddy — see `AMS_TRAY_STATE_SPOOL_NOT_FED`'s doc comment) — and clears all
stale config keys if either applies. Treats an explicit empty `tray_type` string as a
clearing signal too — but an *absent* `tray_type` (the common incremental-update case,
e.g. a `state: 11` update that simply doesn't repeat `tray_type`) is not, by itself, a
clearing signal. Confirmed against `reference/05_materials_ams.md`'s
Bambuddy cross-check (`on_ams_change`'s `loaded = cur_state == 11 or (cur_state not in
(9, 10) and cur_type.strip())`): state 11 is unconditionally treated as loaded regardless
of whether `tray_type` was repeated in that update, so clearing on absence alone would
wipe a currently-printing tray's material data.

`ams_id` gates the state heuristic: on AMS-HT units (`ams_id` 128-135), a partial
power-on frame reports its *loaded* tray as state 9 — the opposite of its meaning on a
standard 4-slot AMS — and its state field is firmware-variant, so neither state 9 nor
state 10 is treated as a clearing signal for HT units. Independently corroborated by
Bambuddy's incremental-merge handler, which skips the same heuristic for `ams_id >= 128`
after a live H2D Pro wiped an HT spool on every power-on (their issue #2594); the
exception is recorded in `reference/05_materials_ams.md`, which also explains why
`AMS_TRAY_STATE_POWER_OFF` (0) is deliberately *not* gated the same way.

A tray whose update carries **no `state` field at all** is treated as absent only when it
also carries no filament metadata. Some firmware reports a fully populated tray without a
`state` key, and an omitted field is not a report of emptiness — see the inline note on the
`has_filament_metadata` check below.

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

**AMS-HT units (IDs 128-135) do participate in `tray_exist_bits`, at a fixed offset**
— BambuStudio's `DevAms::GetTrayId` (`DevFilaSystem.cpp:833`, `GetTrayId`'s N3S
branch) computes the bit index as `16 + (ams_id - 128) + slot_id`, confirmed independently
in OrcaSlicer with an equivalent formula. This reopens and reverses the earlier "AMS-HT
doesn't participate" conclusion, which was based on an incomplete read of BambuStudio's
source.

### `normalize_ams_unit_id`

```rust
fn normalize_ams_unit_id(ams_id: u8) -> u8
```

Normalizes an AMS unit id reported on the wire into the id this crate addresses it by.

Only the A2L AMS Lite's physical id 16 is remapped (to 6); every other id passes through
untouched, and no other Bambu unit reports id 16, so the remap is self-scoping. Applied on
the inbound telemetry boundary so that `tray_exist_bits`, `resolve_global_tray_id` and the
mapping builders all agree on one id; the physical 16 is restored only on the outbound wire
by `crate::ams::MaterialSource::to_mapping2_entry`.

The firmware is internally inconsistent about this unit, which is why one constant cannot
cover it: `tray_exist_bits` uses bit base 24 (id 6's position, not id 16's), `tray_now`
reports a local slot 0-3, and only `ams_mapping2` and the per-unit commands carry 16.

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

AMS-HT is single-slot, so its only valid `tray_id` is `0`; a non-zero one is rejected
rather than silently ignored. Without that check this function and its sibling
[`evaluate_spool_presence`](parser/index.md#evaluate-spool-presence) disagreed on the same `(ams_id, tray_id)` pair — an
AMS-HT id paired with a bad `tray_id` (from a mis-decoded `tray_now`, say) got a
silently-accepted `Some(ams_id)` here but `None` there, masking the caller bug the
sibling catches. [`normalize_ams_unit_id`](parser/index.md#normalize-ams-unit-id)'s doc comment requires the two to agree.

### `resolve_printing_global_id`

```rust
fn resolve_printing_global_id(tray_now: u8, active_extruder: Option<u8>, ams_extruder_map: &[u8]) -> Option<u8>
```

Resolves the currently printing tray's global ID via `tray_now` + an `ams_extruder_map`
inversion, accounting for IDEX map translations.

**Multi-AMS Local Index Resolution [REF-AMS-DECODE]:**
Multi-extruder platforms (such as the H2D series) emit local slot indexes (0 to 3) inside
their `tray_now` telemetry parameter. To resolve this back to a global index, the client must
inspect the active extruder carriage and correlate it against the `ams_extruder_map` matrix.

This is the *fallback* path — prefer
[`crate::client::PrinterClient::printing_tray_global_id`](../client/index.md#printerclient), which decodes
`ExtruderInfo::current_ams_slot()` (`snow`) directly and needs no `ams_extruder_map` at all.
This function remains unwired in the crate's own client code: `ams_extruder_map`'s
construction from real wire data is itself an unresolved, unconfirmed design question
(no field in this crate's telemetry types currently sources it), and the map can be
genuinely ambiguous (N AMS units per extruder) in ways a flat `&[u8]` array can't express —
a caller with its own confirmed `ams_extruder_map` source may still use this directly.

