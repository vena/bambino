*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [a1](index.md)*

---

# Module `a1`

# A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates

Kinematics, safety boundaries, and mechanical constraints of the A1 bed-slinger family
[REF-MOTO-GCODE].

- A1: 256×256×256mm build volume
- A1 Mini: 180×180×180mm build volume

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`A1_BED_TEMP_MAX`](#a1-bed-temp-max) | const | A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A1_MINI_BED_TEMP_MAX`](#a1-mini-bed-temp-max) | const | A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A1_MINI_Z_MAX`](#a1-mini-z-max) | const | A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`A1_NOZZLE_TEMP_MAX`](#a1-nozzle-temp-max) | const | Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`A1_Z_MAX`](#a1-z-max) | const | A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |

## Constants

### `A1_BED_TEMP_MAX`
```rust
const A1_BED_TEMP_MAX: u16 = 100u16;
```

A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `A1_MINI_BED_TEMP_MAX`
```rust
const A1_MINI_BED_TEMP_MAX: u16 = 80u16;
```

A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `A1_MINI_Z_MAX`
```rust
const A1_MINI_Z_MAX: f32 = 180f32;
```

A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.

### `A1_NOZZLE_TEMP_MAX`
```rust
const A1_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `A1_Z_MAX`
```rust
const A1_Z_MAX: f32 = 256f32;
```

A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.

