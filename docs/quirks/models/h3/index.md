*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [h3](index.md)*

---

# Module `h3`

# H3 Series (H3C) Quirks

The H3C is a CoreXY tool changer with a nine-dock hotend rack and no fixed hotend, so every
hotend is a rack position (ids `16..=24`, [`ModelQuirks::rack_nozzle_ids`](../../index.md#modelquirks)). Its facts come
from BambuStudio's `resources/printers/N10.json` and its `Bambu Lab H3C 0.4 nozzle.json`
machine profile, plus ha-bambulab's `MOCK-H3C.json` capture; see `MODEL_MATRIX.csv`.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`H3C_BED_TEMP_MAX`](#h3c-bed-temp-max) | const | H3C bed temperature ceiling (°C), `bed_temp_range` in BambuStudio's `N10.json`. |
| [`H3C_CHAMBER_TEMP_MAX`](#h3c-chamber-temp-max) | const | H3C chamber heater ceiling (°C), `support_chamber_temp_edit_range` in BambuStudio's `N10.json`. |
| [`H3C_NOZZLE_TEMP_MAX`](#h3c-nozzle-temp-max) | const | H3C nozzle temperature ceiling (°C), `nozzle_temp_range` in BambuStudio's `N10.json`. |
| [`H3C_RACK_DOCKS`](#h3c-rack-docks) | const | Rack docks on the H3C, `default_nozzle_count` in BambuStudio's `N10.json`. |
| [`H3C_X_MAX`](#h3c-x-max) | const | H3C build volume X (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`. |
| [`H3C_Y_MAX`](#h3c-y-max) | const | H3C build volume Y (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`. |
| [`H3C_Z_MAX`](#h3c-z-max) | const | H3C build volume Z (mm), `printable_height` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`. |

## Constants

### `H3C_BED_TEMP_MAX`
```rust
const H3C_BED_TEMP_MAX: u16 = 120u16;
```

H3C bed temperature ceiling (°C), `bed_temp_range` in BambuStudio's `N10.json`.

### `H3C_CHAMBER_TEMP_MAX`
```rust
const H3C_CHAMBER_TEMP_MAX: u16 = 65u16;
```

H3C chamber heater ceiling (°C), `support_chamber_temp_edit_range` in BambuStudio's `N10.json`.

### `H3C_NOZZLE_TEMP_MAX`
```rust
const H3C_NOZZLE_TEMP_MAX: u16 = 350u16;
```

H3C nozzle temperature ceiling (°C), `nozzle_temp_range` in BambuStudio's `N10.json`.

### `H3C_RACK_DOCKS`
```rust
const H3C_RACK_DOCKS: u8 = 9u8;
```

Rack docks on the H3C, `default_nozzle_count` in BambuStudio's `N10.json`.

### `H3C_X_MAX`
```rust
const H3C_X_MAX: f32 = 330f32;
```

H3C build volume X (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.

### `H3C_Y_MAX`
```rust
const H3C_Y_MAX: f32 = 320f32;
```

H3C build volume Y (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.

### `H3C_Z_MAX`
```rust
const H3C_Z_MAX: f32 = 325f32;
```

H3C build volume Z (mm), `printable_height` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.

