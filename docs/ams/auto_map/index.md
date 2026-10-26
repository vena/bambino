*[bambino](../../index.md) / [ams](../index.md) / [auto_map](index.md)*

---

# Module `auto_map`

# Deriving an AMS mapping from a plate's filaments and the loaded trays

A port of BambuStudio's automatic AMS mapping: `DevMappingUtil::ams_filament_mapping`
(`DeviceCore/DevMapping.cpp`) as `SelectMachineDialog::do_ams_mapping` (`SelectMachine.cpp`)
calls it with `use_ams = true`. It is what a consumer that doesn't ask the user for a
mapping needs; see `reference/05_materials_ams.md`, "Choosing a Mapping When the User Is Not
Asked".

[`derive_ams_mapping`](#derive-ams-mapping) takes the plate's filaments (from
[`read_plate_filaments`](../../ftps/project/index.md), or built by the caller)
and the cached AMS telemetry, and does no I/O. Its [`AmsMappingResult`](#amsmappingresult) holds the
`ams_mapping2` array [`PrintJobConfig::with_ams_mapping2`](../../mqtt/index.md)
takes and, per filament, which tray it matched, so a consumer can say "filament 3 has no
loaded match" instead of printing wrong.

# The match

For each filament and each loaded tray BambuStudio computes a distance, and picks the
lowest. A tray of a different material is never picked (`type` is a hard condition). For the
same material the distance is the colour distance (0 to 1000, CIELAB ΔE76 combined with the
alpha difference; see [`color_distance`](#color-distance)) plus 0 when the preset (`setting_id`) matches, 10000
when only the filament class (`filament_id`, the file's `tray_info_idx` against the tray's)
matches, and 20000 otherwise. So preset beats class beats colour.

**A file's filament has no `setting_id`**, only the `tray_info_idx` that BambuStudio calls
`filament_id`, so for a file the preset branch never fires and a match is class, then colour.
The code keeps the branch because the internal filament record carries a `setting_id`; the
public entry point leaves it empty.

The selection is greedy: one round per filament, each picking the lowest-distance pair among
the filaments not yet mapped and the trays not yet used. A used tray can be picked again
only at distance under 0.0001, or, when no unused tray matched any remaining filament, at
any distance (BambuStudio's "retry to mapping to used tray"), so two filaments of one
material share a single loaded tray rather than going unmapped. A tie prefers an exact
`filament_id` match, then the tray with less filament left among exact matches
(`_prefer_tray_by_remain`).

# Differences from BambuStudio

- **Index versus key.** BambuStudio's loop reads `tray_filaments[picked_tar_idx]` where
  `tray_filaments` is a `std::map` keyed by the tray's flat index and `picked_tar_idx` is a
  column position. Whenever the two differ (any unit but the first, any gap), `operator[]`
  inserts or reads the wrong tray in the tie-break. This port compares the tray the column
  names.
- **Retry order.** BambuStudio tries the used-tray retry for a filament as soon as that
  filament finds nothing unused, before later filaments are looked at. This port finishes the
  unused-tray pass over every remaining filament first, which is what its comments describe.
- **Empty ids never tie-break.** BambuStudio's tie-break compares `filament_id` strings with
  `==`, so two empty ids count as an exact match. Here an exact match needs a non-empty id,
  as the class offset already does.
- **A loaded tray** is one [`AmsTray::is_loaded`](../../types/telemetry/ams/index.md#amstray) accepts and that names a material, since the
  cache holds stale fields for an emptied tray (BambuStudio blanks the type itself).
- **Single-nozzle printers ignore a unit's extruder binding.** BambuStudio filters units by
  binding on every printer; the binding only means something with two extruders.
- **No filament switch signal.** For a unit reporting `0xE` ("not fixed"), BambuStudio uses
  both extruders only when a Filament Track Switch is installed and the unit's
  `bind_switch_in` is a valid inlet, and none otherwise (`ParseAmsInfo`, `DevFilaSystem.cpp`).
  bambino doesn't model the switch's own telemetry (bambuddy detects it from the presence of
  `device.fila_switch`), so it checks the inlet alone: both extruders with one, none without.

# Printers with two extruders

Filaments are split by the file's `filament_maps` (`1` left/deputy, `2` right/main) and each
side is matched on its own against the AMS units bound to that extruder, plus that side's
external spool only when no unit is bound to it. A filament with no usable `filament_maps`
value is left unmatched. A unit reachable from both extruders (a Filament Track Switch)
is a candidate on both sides, and, as in BambuStudio, the two sides don't see each other's
picks. On a single-nozzle printer the external spool is never a candidate.

# Using the result

A filament with no match is [`AmsMapping2Entry::UNMAPPED`](../mapping/index.md#amsmapping2entry) in the array. Sending a job that
uses such a filament fails with an AMS mapping error on the printer; whether to send
anyway, ask the user, or stop is the caller's decision, which is why
[`AmsMappingResult::unmatched`](#amsmappingresult) exists.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AmsMappingResult`](#amsmappingresult) | struct | The mapping [`derive_ams_mapping`](#derive-ams-mapping) found and what each filament matched. |
| [`FilamentMatch`](#filamentmatch) | struct | Where one filament landed. |
| [`color_distance`](#color-distance) | fn | BambuStudio's `GUI::calc_color_distance`: CIELAB ΔE76 combined with the alpha difference, clamped to `0..=1000`. |
| [`derive_ams_mapping`](#derive-ams-mapping) | fn | Derives an `ams_mapping2` for `filaments` from the AMS telemetry in `ams` and the external spools in `external_spools`. |

## Types

### `AmsMappingResult`

```rust
struct AmsMappingResult {
    pub mapping2: Vec<super::mapping::AmsMapping2Entry>,
    pub matches: Vec<FilamentMatch>,
}
```

The mapping [`derive_ams_mapping`](#derive-ams-mapping) found and what each filament matched.

#### Fields

- **`mapping2`**: `Vec<super::mapping::AmsMapping2Entry>`

  The `ams_mapping2` array: one entry per project filament index up to the highest the
  plate uses, [`AmsMapping2Entry::UNMAPPED`](../mapping/index.md#amsmapping2entry) for an index the plate doesn't use and for a
  filament nothing matched. Pass it to
  [`PrintJobConfig::with_ams_mapping2`](../../mqtt/index.md).

- **`matches`**: `Vec<FilamentMatch>`

  One record per filament given, in the order given.

#### Implementations

- <span id="amsmappingresult-unmatched"></span>`fn unmatched(&self) -> impl Iterator<Item = &FilamentMatch>` — [`FilamentMatch`](#filamentmatch)

  The filaments nothing loaded matched.

  A job started with one of these in `mapping2` fails with an AMS mapping error. Report it,
  ask the user for a mapping, or stop: the crate does not choose.

- <span id="amsmappingresult-is-complete"></span>`fn is_complete(&self) -> bool`

  Whether every filament matched a tray.

#### Trait Implementations

##### `impl Clone for AmsMappingResult`

- <span id="amsmappingresult-clone"></span>`fn clone(&self) -> AmsMappingResult` — [`AmsMappingResult`](#amsmappingresult)

##### `impl Debug for AmsMappingResult`

- <span id="amsmappingresult-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl PartialEq for AmsMappingResult`

- <span id="amsmappingresult-partialeq-eq"></span>`fn eq(&self, other: &AmsMappingResult) -> bool` — [`AmsMappingResult`](#amsmappingresult)

### `FilamentMatch`

```rust
struct FilamentMatch {
    pub filament_id: u32,
    pub source: Option<super::mapping::MaterialSource>,
    pub distance: Option<f32>,
}
```

Where one filament landed.

#### Fields

- **`filament_id`**: `u32`

  The project filament index, 1-based, as [`PlateFilament::id`](../../ftps/project/slice_info/index.md#platefilament).

- **`source`**: `Option<super::mapping::MaterialSource>`

  The tray it matched, or `None` when nothing loaded matched.

- **`distance`**: `Option<f32>`

  BambuStudio's distance for the pair, `None` when unmatched.
  
  Under 1000 is a preset match and the colour distance; 10000 to 11000 a class match plus
  colour; 20000 and above a same-material tray of another class. Read it to decide whether
  a match is close enough to print.

#### Trait Implementations

##### `impl Clone for FilamentMatch`

- <span id="filamentmatch-clone"></span>`fn clone(&self) -> FilamentMatch` — [`FilamentMatch`](#filamentmatch)

##### `impl Copy for FilamentMatch`

##### `impl Debug for FilamentMatch`

- <span id="filamentmatch-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl PartialEq for FilamentMatch`

- <span id="filamentmatch-partialeq-eq"></span>`fn eq(&self, other: &FilamentMatch) -> bool` — [`FilamentMatch`](#filamentmatch)


---

## Functions

### `color_distance`

```rust
fn color_distance(a: [u8; 4], b: [u8; 4]) -> f32
```

BambuStudio's `GUI::calc_color_distance`: CIELAB ΔE76 combined with the alpha difference, clamped to `0..=1000`.

Each colour is `[r, g, b, a]`. The distance is
`sqrt(ΔE76(Lab1, Lab2)^2 + ((a1 - a2) * 100)^2)` with alpha in `0.0..=1.0`, and `Lab` comes
from `RGB2Lab` (`RGB2XYZ` with the D65 matrix of `ColorSpaceConvert.cpp`, then `XYZ2Lab`
against its `95.047 / 100 / 108.883` white).

### `derive_ams_mapping`

```rust
fn derive_ams_mapping(filaments: &crate::ftps::project::PlateFilaments, ams: Option<&crate::types::telemetry::AmsStatusReport>, external_spools: &[crate::types::telemetry::VirtualTray], dual_nozzle: bool) -> AmsMappingResult
```

**Types:** [`PlateFilaments`](../../ftps/project/index.md#platefilaments), [`AmsStatusReport`](../../types/telemetry/ams/index.md#amsstatusreport), [`VirtualTray`](../../types/telemetry/ams/index.md#virtualtray), [`AmsMappingResult`](#amsmappingresult)

Derives an `ams_mapping2` for `filaments` from the AMS telemetry in `ams` and the external spools in `external_spools`.

`ams` is [`PrinterClient::ams`](../../client/index.md#printerclient) and `external_spools` is
[`vir_slot`](../../client/index.md#printerclient) (empty on a single-nozzle printer, which
never maps to its external spool). `dual_nozzle` is whether the printer has more than one
nozzle, `quirks().physical_nozzle_count() > 1`. `None` for `ams`, or no loaded tray, leaves
every filament unmatched; it is not an error.

Prefer [`PrinterClient::derive_ams_mapping`](../../client/index.md#printerclient),
which supplies all three. See the module doc for the match and where it differs from
BambuStudio. A filament whose `id` is 0 or above 32 (BambuStudio's project limit) is left
unmatched and out of the array.

