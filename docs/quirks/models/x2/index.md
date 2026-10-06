*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [x2](index.md)*

---

# Module `x2`

# X2 Series (X2D CoreXY) Quirks

Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.

Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
Z-max uses the conservative aux/dual value (256mm).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`X2D_BED_TEMP_MAX`](#x2d-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`X2D_CHAMBER_TEMP_MAX`](#x2d-chamber-temp-max) | const | Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row. |
| [`X2D_MIN_REMOTE_DRY_FIRMWARE`](#x2d-min-remote-dry-firmware) | const | Firmware release that introduced remote AMS drying, and drying while printing, on the X2D. |
| [`X2D_NOZZLE_TEMP_MAX`](#x2d-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X2D_X_MAX`](#x2d-x-max) | const | Build volume X width (mm) — conservative aux/dual-nozzle value (235.5mm, smaller than the main-nozzle profile's 256mm); see module docs. |
| [`X2D_Y_MAX`](#x2d-y-max) | const | Build volume Y depth (mm) — 256mm across all nozzle profiles. |
| [`X2D_Z_MAX`](#x2d-z-max) | const | Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs. |

## Constants

### `X2D_BED_TEMP_MAX`
```rust
const X2D_BED_TEMP_MAX: u16 = 120u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `X2D_CHAMBER_TEMP_MAX`
```rust
const X2D_CHAMBER_TEMP_MAX: u16 = 65u16;
```

Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.

### `X2D_MIN_REMOTE_DRY_FIRMWARE`
```rust
const X2D_MIN_REMOTE_DRY_FIRMWARE: &str;
```

Firmware release that introduced remote AMS drying, and drying while printing, on the X2D.

X2D `01.01.00.00` (2026-04-14, <https://wiki.bambulab.com/en/x2d/manual/x2d-firmware-release-history>):
"Added support for remote activation of filament drying" and "Added support for 'Print While
Drying' feature" (the latter needs the separately sold AMS external power supply). The *Filament
drying guide for AMS 2 Pro and AMS HT* gives the same minimum in both lists, and bambuddy's
`_DRY_WHILE_PRINTING_MIN_FIRMWARE` agrees; its `_DRYING_MIN_FIRMWARE` omits the X2D.

This is the earliest published X2D release, so an unread version is inferred supported rather
than assumed.

### `X2D_NOZZLE_TEMP_MAX`
```rust
const X2D_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `X2D_X_MAX`
```rust
const X2D_X_MAX: f32 = 235.5f32;
```

Build volume X width (mm) — conservative aux/dual-nozzle value (235.5mm, smaller than the
main-nozzle profile's 256mm); see module docs.

### `X2D_Y_MAX`
```rust
const X2D_Y_MAX: f32 = 256f32;
```

Build volume Y depth (mm) — 256mm across all nozzle profiles.

### `X2D_Z_MAX`
```rust
const X2D_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs.

