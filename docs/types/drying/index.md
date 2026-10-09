*[bambino](../../index.md) / [types](../index.md) / [drying](index.md)*

---

# Module `drying`

# Filament Drying Presets

Static per-material drying parameters, as the printer's own drying screen uses them.

A drying cycle takes a free-form `filament` string and a `temp`, leaving a consumer to
source both. This module is the vendor's own answer: one entry
per naked material type, each carrying the temperature, duration and cooling temperature
BambuStudio fills in when that material is picked.

**This is not the filament-*preset* list.** `tray_info_idx`/`setting_id` name brands and
variants (`"Bambu PLA Basic"`); these are the base material types behind them, one per
`fdm_filament_<type>.json` profile in BambuStudio's own resource tree.

All values transcribed from `resources/profiles/BBL/filament/fdm_filament_*.json` at
BambuStudio, read directly rather than from a secondary account of them. bambuddy
corroborates the idle column independently (`DEFAULT_DRYING_PRESETS`,
`print_scheduler.py`) and selects the per-unit column the same way
(`temp_key = module_type if module_type in ("n3f", "n3s") else "n3f"`).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`DryingMaterial`](#dryingmaterial) | enum | A base filament material with vendor-published drying parameters. |
| [`DEFAULT_COMMAND_COOLING_TEMP`](#default-command-cooling-temp) | const | Fallback `cooling_temp` BambuStudio sends when a tray's filament has no drying preset (`AMSDryControl.cpp`, `int cooling_temp = 50;`). |

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
BambuStudio's `DevUtilBackend.cpp`, which reads exactly those four positions into
`..._on_idle[N3F]`, `..._on_idle[N3S]`, `..._on_print[N3F]`, `..._on_print[N3S]`. Everything
here is indexed the same way, via [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel) plus a `printing` flag.

**A convenience layer, not a replacement for the `&str` parameter.** The wire `dry_filament`
field is free-form — BambuStudio sends the tray's own `filament_type` string — so
`DryingCycle::filament` keeps taking an arbitrary
`&str` and this enum stays open at the edges via
[`from_filament_type`](#dryingmaterial) returning `None`.

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

  The base material is the leading run of ASCII letters, so composite and variant
  spellings resolve to the profile they inherit from: `"PA-CF"`, `"PAHT-CF"` and `"PA6-GF"`
  to [`Pa`](#dryingmaterial) (BambuStudio's composite presets inherit the base
  `fdm_filament_pa.json`), `"PLA+"` and `"PLA Silk"` to [`Pla`](#dryingmaterial), `"PETG HF"` to
  [`Petg`](#dryingmaterial). `None` for anything unrecognized, which is the case the free-form
  `&str` parameter on `DryingCycle::filament` exists to serve.

- <span id="dryingmaterial-wire-name"></span>`fn wire_name(self) -> &'static str`

  The canonical material name, as it appears in a wire `filament_type` field.

- <span id="dryingmaterial-default-temp"></span>`fn default_temp(self, unit: AmsUnitModel, printing: bool) -> Option<u32>` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Vendor default drying temperature in °C for this material on this unit.

  `printing` selects the lower while-printing column, which exists because the AMS sits in
  the print's thermal envelope. `None` for a unit with no drying chamber.

  **This is a starting value, not a bound.** It always falls inside
  [`AmsUnitModel::dry_temp_range`](../telemetry/ams/index.md#amsunitmodel), but the range is the hardware limit and this is the
  vendor's recommendation within it.

  **Capped at [`heat_distortion_temp`](#dryingmaterial) in both columns.**
  BambuStudio disables Start whenever the temperature exceeds the heat-distortion
  temperature and a tray is loaded, idle or printing (`AMSDryControl.cpp`), yet
  three raw profile values break that rule (TPU idle on both units, PVA idle on the AMS-HT).
  The value returned is the one BambuStudio would let a loaded tray start with. It also
  floors the printing column at the softening temperature
  (`min(printing_temp, softening_temp, heat_distortion_temp)`, `AMSDryControl.cpp`);
  no published printing value exceeds [`softening_temp`](#dryingmaterial), so that
  half is a no-op here.

- <span id="dryingmaterial-default-duration-hours"></span>`fn default_duration_hours(self, unit: AmsUnitModel, printing: bool) -> Option<u32>` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Vendor default drying duration in whole hours for this material on this unit.

  `None` for a unit with no drying chamber.

- <span id="dryingmaterial-softening-temp"></span>`fn softening_temp(self) -> u32`

  Temperature (°C) at which this material begins to soften
  (`filament_dev_drying_softening_temperature`).

  **Also what a drying cycle sends as its wire `cooling_temp`** — not the profile's
  similarly named `filament_dev_drying_cooling_temperature`. BambuStudio parses that second
  field (`DevUtilBackend.cpp`) but never sends it: the drying command is built from
  the softening temperature (`AMSDryControl.cpp`). See [`DEFAULT_COMMAND_COOLING_TEMP`](#default-command-cooling-temp)
  for what BambuStudio sends when a tray's filament resolves to no preset at all.

- <span id="dryingmaterial-heat-distortion-temp"></span>`fn heat_distortion_temp(self) -> u32`

  Heat-distortion temperature (°C) (`filament_dev_ams_drying_heat_distortion_temperature`).

  [`Pe`](#dryingmaterial) and [`Pha`](#dryingmaterial) publish no value of their own and inherit 45 °C
  from `fdm_filament_common.json:108-110`; BambuStudio reads the merged parent+child config
  (`PresetBundle.cpp`). BambuStudio refuses to start a cycle above this on a
  loaded tray (`AMSDryControl.cpp`), and it is one of the three inputs to the
  while-printing clamp (`min(printing_temp, softening_temp, heat_distortion_temp)`,
  `AMSDryControl.cpp`).

- <span id="dryingmaterial-fully-dryable-by"></span>`fn fully_dryable_by(self, unit: AmsUnitModel) -> bool` — [`AmsUnitModel`](../telemetry/ams/index.md#amsunitmodel)

  Returns true if this unit can dry this material *completely*.

  A `false` here does not mean "don't dry it" — BambuStudio still permits the cycle and
  shows "This filament may not be completely dried" (`AMSDryControl.cpp`). It means
  the cycle will not fully remove the moisture.

  Read from `filament_dev_ams_drying_ams_limitations`, whose values do **not** use the
  `DevAmsType` numbering the rest of this module does: in that field `"0"` is the AMS 2 Pro
  and `"1"` the AMS-HT (`s_ams_type_map`, `DevUtilBackend.cpp`), where `DevAmsType`
  makes them `3` and `4`. `["-1"]` means neither unit qualifies.

  **A profile that omits the key inherits `["1"]` (AMS-HT only)** from
  `fdm_filament_common.json:105-107`, which every material preset inherits; BambuStudio
  reads the merged parent+child config (`PresetBundle.cpp`,
  `DevUtilBackend.cpp`). So the seven materials with no key of their own — ABS, ASA,
  HIPS, PC, PA, PVA, TPU — are fully dryable by the AMS-HT and not the AMS 2 Pro. PPA and
  PPS name `["-1"]` explicitly.

#### Trait Implementations

##### `impl Clone for DryingMaterial`

- <span id="dryingmaterial-clone"></span>`fn clone(&self) -> DryingMaterial` — [`DryingMaterial`](#dryingmaterial)

##### `impl Copy for DryingMaterial`

##### `impl Debug for DryingMaterial`

- <span id="dryingmaterial-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for DryingMaterial`

- <span id="dryingmaterial-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for DryingMaterial`

##### `impl PartialEq for DryingMaterial`

- <span id="dryingmaterial-partialeq-eq"></span>`fn eq(&self, other: &DryingMaterial) -> bool` — [`DryingMaterial`](#dryingmaterial)

##### `impl ToString for DryingMaterial`

- <span id="dryingmaterial-tostring-to-string"></span>`fn to_string(&self) -> String`


---

## Constants

### `DEFAULT_COMMAND_COOLING_TEMP`
```rust
const DEFAULT_COMMAND_COOLING_TEMP: u32 = 50u32;
```

Fallback `cooling_temp` BambuStudio sends when a tray's filament has no drying preset
(`AMSDryControl.cpp`, `int cooling_temp = 50;`).

