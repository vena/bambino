*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [p1](index.md)*

---

# Module `p1`

# P1 Series (P1P & P1S CoreXY) Quirks

Constraints and kinematic properties of the early and enclosed low-power RTOS machines.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`P1_BED_TEMP_MAX`](#p1-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`P1_NOZZLE_TEMP_MAX`](#p1-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`P1_Z_MAX`](#p1-z-max) | const | Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row. |

## Constants

### `P1_BED_TEMP_MAX`
```rust
const P1_BED_TEMP_MAX: u16 = 100u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `P1_NOZZLE_TEMP_MAX`
```rust
const P1_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `P1_Z_MAX`
```rust
const P1_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row.

