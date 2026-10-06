*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [a2](index.md)*

---

# Module `a2`

# A2 Series (A2L Bed-Slinger) Quirks & Coordinates

The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`A2L_BED_TEMP_MAX`](#a2l-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A2L_NOZZLE_TEMP_MAX`](#a2l-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`A2L_X_MAX`](#a2l-x-max) | const | A2L build volume X width (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm). |
| [`A2L_Y_MAX`](#a2l-y-max) | const | A2L build volume Y depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm). |
| [`A2L_Z_MAX`](#a2l-z-max) | const | A2L build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm). |

## Constants

### `A2L_BED_TEMP_MAX`
```rust
const A2L_BED_TEMP_MAX: u16 = 80u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `A2L_NOZZLE_TEMP_MAX`
```rust
const A2L_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `A2L_X_MAX`
```rust
const A2L_X_MAX: f32 = 330f32;
```

A2L build volume X width (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).

### `A2L_Y_MAX`
```rust
const A2L_Y_MAX: f32 = 320f32;
```

A2L build volume Y depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).

### `A2L_Z_MAX`
```rust
const A2L_Z_MAX: f32 = 325f32;
```

A2L build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).

