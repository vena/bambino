*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [unknown](index.md)*

---

# Module `unknown`

# Unrecognized Model Fallback Quirks

Row used for [`PrinterModel::Unknown`](../../../models/index.md#printermodel) — a printer
whose model string this crate does not recognize (a new SKU, a malformed SSDP `DevModel`
header, or a firmware that reports an unexpected token).

Physical limits here are the **floor of the entire supported family**, not any one model's
values: an unrecognized machine could be any of them, so every ceiling has to be one no
shipping model would exceed. This is why the fallback is not simply X1C's row — X1C's
bed ceiling is voltage-dependent and rises to 120 °C on a 110 V unit, 40 °C past the real
ceiling of the entry-level models an unrecognized printer might well be.

Connection-layer behavior (FTPS data-channel encryption, TLS 1.2 enforcement, camera
protocol) keeps the X1-series values, since those are interop choices rather than physical
safety ceilings and the X1 settings are the ones that reach the widest set of hosts.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`UNKNOWN_AXIS_MAX`](#unknown-axis-max) | const | Travel ceiling (mm), applied to all three axes — the smallest build volume in the family (A1 Mini), per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`UNKNOWN_BED_TEMP_MAX`](#unknown-bed-temp-max) | const | Bed temperature ceiling (°C) — the lowest build-plate ceiling in the family (A1 Mini / A2L), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`UNKNOWN_NOZZLE_TEMP_MAX`](#unknown-nozzle-temp-max) | const | Nozzle temperature ceiling (°C) — the lowest hot-end ceiling in the family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |

## Constants

### `UNKNOWN_AXIS_MAX`
```rust
const UNKNOWN_AXIS_MAX: f32 = 180f32;
```

Travel ceiling (mm), applied to all three axes — the smallest build volume in the family
(A1 Mini), per `MODEL_MATRIX.csv`'s Build Volume row.

### `UNKNOWN_BED_TEMP_MAX`
```rust
const UNKNOWN_BED_TEMP_MAX: u16 = 80u16;
```

Bed temperature ceiling (°C) — the lowest build-plate ceiling in the family (A1 Mini / A2L),
per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. Flat, never voltage-dependent: the
mains region of an unrecognized machine says nothing about which model it is.

### `UNKNOWN_NOZZLE_TEMP_MAX`
```rust
const UNKNOWN_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C) — the lowest hot-end ceiling in the family, per
`MODEL_MATRIX.csv`'s Max Hot End Temperature row.

