*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [x1](index.md)*

---

# Module `x1`

# X1 Series (X1, X1C, X1E CoreXY) Quirks

Hardware safety limits and thermal parameters for the premium CoreXY platforms. X1C and X1E
share everything except active chamber heater support (X1E only), ceilings and drying rule.
The plain X1 is the X1C minus a stock auxiliary part-cooling fan — see `X1`.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`X1C_BED_TEMP_MAX_110V`](#x1c-bed-temp-max-110v) | const | Bed temperature ceiling on a 110V-region unit. |
| [`X1C_BED_TEMP_MAX_220V`](#x1c-bed-temp-max-220v) | const | Bed temperature ceiling on a 220V-region unit — confirmed, per the official spec sheet, non-obviously *lower* than the 110V ceiling. |
| [`X1C_NOZZLE_TEMP_MAX`](#x1c-nozzle-temp-max) | const | X1C nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X1E_BED_TEMP_MAX`](#x1e-bed-temp-max) | const | X1E bed temperature ceiling (°C) — flat, not voltage-dependent like X1C's, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`X1E_CHAMBER_TEMP_MAX`](#x1e-chamber-temp-max) | const | X1E chamber temperature ceiling (°C) — X1E has an active chamber heater, X1C does not, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row. |
| [`X1E_NOZZLE_TEMP_MAX`](#x1e-nozzle-temp-max) | const | X1E nozzle temperature ceiling (°C) — higher than X1C's, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X1_Z_MAX`](#x1-z-max) | const | Build volume Z depth (mm) shared by X1, X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row. |

## Constants

### `X1C_BED_TEMP_MAX_110V`
```rust
const X1C_BED_TEMP_MAX_110V: u16 = 120u16;
```

Bed temperature ceiling on a 110V-region unit.

### `X1C_BED_TEMP_MAX_220V`
```rust
const X1C_BED_TEMP_MAX_220V: u16 = 110u16;
```

Bed temperature ceiling on a 220V-region unit — confirmed, per the official spec sheet, non-obviously *lower* than the 110V ceiling.
Also the conservative default when the mains region is unknown (no `home_flag` telemetry
received yet).

### `X1C_NOZZLE_TEMP_MAX`
```rust
const X1C_NOZZLE_TEMP_MAX: u16 = 300u16;
```

X1C nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `X1E_BED_TEMP_MAX`
```rust
const X1E_BED_TEMP_MAX: u16 = 110u16;
```

X1E bed temperature ceiling (°C) — flat, not voltage-dependent like X1C's, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `X1E_CHAMBER_TEMP_MAX`
```rust
const X1E_CHAMBER_TEMP_MAX: u16 = 60u16;
```

X1E chamber temperature ceiling (°C) — X1E has an active chamber heater, X1C does not, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.

### `X1E_NOZZLE_TEMP_MAX`
```rust
const X1E_NOZZLE_TEMP_MAX: u16 = 320u16;
```

X1E nozzle temperature ceiling (°C) — higher than X1C's, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `X1_Z_MAX`
```rust
const X1_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm) shared by X1, X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row.

