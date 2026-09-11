*[bambino](../../index.md) / [types](../index.md) / [drying](index.md)*

---

# Module `drying`

# Filament Drying Presets

Static per-material drying parameters, as the printer's own drying screen uses them.

[`start_drying`](../../client/index.md#printerclient) takes a bare `filament: &str` and a
`temp`, leaving a consumer to source both. This module is the vendor's own answer: one entry
per naked material type, each carrying the temperature, duration and cooling temperature
BambuStudio fills in when that material is picked.

**This is not the filament-*preset* list.** `tray_info_idx`/`setting_id` name brands and
variants (`"Bambu PLA Basic"`); these are the base material types behind them, one per
`fdm_filament_<type>.json` profile in BambuStudio's own resource tree.

All values transcribed from `resources/profiles/BBL/filament/fdm_filament_*.json` at
BambuStudio, read directly rather than from a secondary account of them. bambuddy
corroborates the idle column independently (`DEFAULT_DRYING_PRESETS`,
`print_scheduler.py:777-786`) and selects the per-unit column the same way (line 3807,
`temp_key = module_type if module_type in ("n3f", "n3s") else "n3f"`).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`DryingMaterial`](#dryingmaterial) | enum | A base filament material with vendor-published drying parameters. |
| [`DEFAULT_COMMAND_COOLING_TEMP`](#default-command-cooling-temp) | const | Fallback `cooling_temp` BambuStudio sends when a tray's filament has no drying preset (`AMSDryControl.cpp:813`, `int cooling_temp = 50;`). |

## Types

### `DryingMaterial`

```rust
enum DryingMaterial {
    Pla,
    Petg,
    Pctg,
    Abs,
    Asa,
    Hips,
    Pc,
    Pa,
    Pva,
    Bvoh,
    Tpu,
    Pp,
    Pe,
    Pha,
    Eva,
    Ppa,
    Pps,
}
```

A base filament material with vendor-published drying parameters.

Each material's profile stores temperature and time as a 4-element array indexed
`[N3F idle, N3S idle, N3F printing, N3S printing]` — the mapping is explicit in
BambuStudio's `DevUtilBackend.cpp:87-91`, which reads exactly those four positions into
`..._on_idle[N3F]`, `..._on_idle[N3S]`, `..._on_print[N3F]`, `..._on_print[N3S]`. Everything
here is indexed the same way, via [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel) plus a `printing` flag.

**A convenience layer, not a replacement for the `&str` parameter.** The wire `dry_filament`
field is free-form — BambuStudio sends the tray's own `filament_type` string — so
[`start_drying`](../../client/index.md#printerclient) keeps taking an arbitrary `&str` and
this enum stays open at the edges via [`from_filament_type`](#dryingmaterial)
returning `None`.

#### Variants

- **`Pla`**

  Polylactic acid.

- **`Petg`**

  Polyethylene terephthalate glycol. Profile `fdm_filament_pet.json`.

- **`Pctg`**

  Copolyester (PCTG).

- **`Abs`**

  Acrylonitrile butadiene styrene.

- **`Asa`**

  Acrylonitrile styrene acrylate.

- **`Hips`**

  High-impact polystyrene.

- **`Pc`**

  Polycarbonate.

- **`Pa`**

  Polyamide (nylon).

- **`Pva`**

  Polyvinyl alcohol (support material).

- **`Bvoh`**

  Butenediol vinyl alcohol copolymer (support material).

- **`Tpu`**

  Thermoplastic polyurethane.

- **`Pp`**

  Polypropylene.

- **`Pe`**

  Polyethylene.

- **`Pha`**

  Polyhydroxyalkanoate.

- **`Eva`**

  Ethylene-vinyl acetate.

- **`Ppa`**

  Polyphthalamide. Profile `fdm_filament_ppa.json`; sold as PPA-CF.

- **`Pps`**

  Polyphenylene sulfide.

#### Implementations

- <span id="dryingmaterial-all"></span>`fn all() -> &'static [DryingMaterial]` — [`DryingMaterial`](#dryingmaterial)

  Every material in this table.

- <span id="dryingmaterial-from-filament-type"></span>`fn from_filament_type(filament_type: &str) -> Option<Self>`

  Matches a wire `filament_type` string to a material, case-insensitively.

  Accepts the bare material name and the common composite suffixes that share a base
  profile — `"PA-CF"`, `"PAHT-CF"` and `"PA6-GF"` all resolve to [`Pa`](#dryingmaterial), because
  BambuStudio's own composite presets inherit their drying parameters from the base
  `fdm_filament_pa.json`. `None` for anything unrecognized, which is the case the free-form
  `&str` parameter on `start_drying` exists to serve.

- <span id="dryingmaterial-wire-name"></span>`fn wire_name(self) -> &'static str`

  The canonical material name, as it appears in a wire `filament_type` field.

- <span id="dryingmaterial-default-temp"></span>`fn default_temp(self, unit: AmsUnitModel, printing: bool) -> Option<u32>` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Vendor default drying temperature in °C for this material on this unit.

  `printing` selects the lower while-printing column, which exists because the AMS sits in
  the print's thermal envelope. `None` for a unit with no drying chamber.

  **This is a starting value, not a bound.** It always falls inside
  [`AmsUnitModel::dry_temp_range`](../telemetry/ams/index.md#amsunitmodel), but the range is the hardware limit and this is the
  vendor's recommendation within it. BambuStudio additionally floors the printing-column
  value at use: `min(printing_temp, softening_temp, heat_distortion_temp)`
  (`AMSDryControl.cpp:1723-1725`) — see [`softening_temp`](#dryingmaterial) and
  [`heat_distortion_temp`](#dryingmaterial) to reproduce that clamp.

- <span id="dryingmaterial-default-duration-hours"></span>`fn default_duration_hours(self, unit: AmsUnitModel, printing: bool) -> Option<u32>` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Vendor default drying duration in whole hours for this material on this unit.

  `None` for a unit with no drying chamber.

- <span id="dryingmaterial-softening-temp"></span>`fn softening_temp(self) -> u32`

  Temperature (°C) at which this material begins to soften
  (`filament_dev_drying_softening_temperature`).

  Also the value to pass as `start_drying`'s `cooling_temp` — see
  [`command_cooling_temp`](#dryingmaterial).

- <span id="dryingmaterial-command-cooling-temp"></span>`fn command_cooling_temp(self) -> i32`

  What to send as `start_drying`'s `cooling_temp` for this material.

  **The wire `cooling_temp` carries the *softening* temperature, not the profile's
  `filament_dev_drying_cooling_temperature`.** That second field exists and BambuStudio
  parses it (`DevUtilBackend.cpp:109-110`), but never sends it — the drying command is
  built from `filament_dev_drying_softening_temperature` (`AMSDryControl.cpp:816`). Reading
  the similarly-named field instead is the obvious mistake here, so this accessor exists to
  make the right one the easy one.

  Equal to [`softening_temp`](#dryingmaterial); see
  [`DEFAULT_COMMAND_COOLING_TEMP`](#default-command-cooling-temp) for what BambuStudio sends when a tray's filament
  resolves to no preset at all.

- <span id="dryingmaterial-heat-distortion-temp"></span>`fn heat_distortion_temp(self) -> Option<u32>`

  Heat-distortion temperature (°C)
  (`filament_dev_ams_drying_heat_distortion_temperature`), where the profile publishes one.

  `None` for [`Pe`](#dryingmaterial) and [`Pha`](#dryingmaterial), whose profiles omit the key. One of
  the three inputs to BambuStudio's while-printing clamp
  (`min(printing_temp, softening_temp, heat_distortion_temp)`, `AMSDryControl.cpp:1723-1725`).

- <span id="dryingmaterial-fully-dryable-by"></span>`fn fully_dryable_by(self, unit: AmsUnitModel) -> bool` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Returns true if this unit can dry this material *completely*.

  A `false` here does not mean "don't dry it" — BambuStudio still permits the cycle and
  shows "This filament may not be completely dried" (`AMSDryControl.cpp:1203`). It means
  the cycle will not fully remove the moisture.

  Read from `filament_dev_ams_drying_ams_limitations`, whose values do **not** use the
  `DevAmsType` numbering the rest of this module does: in that field `"0"` is the AMS 2 Pro
  and `"1"` the AMS-HT (`s_ams_type_map`, `DevUtilBackend.cpp:58-61`), where `DevAmsType`
  makes them `3` and `4`. `["-1"]` means neither unit qualifies.

  **A profile that omits the key entirely behaves exactly like `["-1"]`.** BambuStudio
  builds an empty set when the key is absent and then warns on set non-membership
  (`AMSDryControl.cpp:1184` and `1203`), so the seven materials with no key published —
  ABS, ASA, HIPS, PC, PA, PVA, TPU — get the same warning as PPA and PPS, which name
  `["-1"]` explicitly. This method reports `false` for all nine rather than treating an
  absent key as permission.

#### Trait Implementations

##### `impl Clone for DryingMaterial`

- <span id="dryingmaterial-clone"></span>`fn clone(&self) -> DryingMaterial` — [`DryingMaterial`](#dryingmaterial)

##### `impl Copy for DryingMaterial`

##### `impl Debug for DryingMaterial`

- <span id="dryingmaterial-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DryingMaterial`

##### `impl PartialEq for DryingMaterial`

- <span id="dryingmaterial-partialeq-eq"></span>`fn eq(&self, other: &DryingMaterial) -> bool` — [`DryingMaterial`](#dryingmaterial)


---

## Constants

### `DEFAULT_COMMAND_COOLING_TEMP`
```rust
const DEFAULT_COMMAND_COOLING_TEMP: i32 = 50i32;
```

Fallback `cooling_temp` BambuStudio sends when a tray's filament has no drying preset
(`AMSDryControl.cpp:813`, `int cooling_temp = 50;`).

