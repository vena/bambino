*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [p2](index.md)*

---

# Module `p2`

# P2 Series (P2S CoreXY) Quirks

Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`P2S_BED_TEMP_MAX`](#p2s-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`P2S_MIN_REMOTE_DRY_FIRMWARE`](#p2s-min-remote-dry-firmware) | const | Firmware release that introduced remote AMS drying, and drying while printing, on the P2S. |
| [`P2S_NOZZLE_TEMP_MAX`](#p2s-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`P2S_Z_MAX`](#p2s-z-max) | const | Build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |

## Constants

### `P2S_BED_TEMP_MAX`
```rust
const P2S_BED_TEMP_MAX: u16 = 110u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `P2S_MIN_REMOTE_DRY_FIRMWARE`
```rust
const P2S_MIN_REMOTE_DRY_FIRMWARE: &str;
```

Firmware release that introduced remote AMS drying, and drying while printing, on the P2S.

P2S `01.02.00.00` (2026-04-09, <https://wiki.bambulab.com/en/p2s/manual/p2s-firmware-release-history>):
"Added support for remote activation of filament drying" and "Added support for 'Print While
Drying' feature". The *Filament drying guide for AMS 2 Pro and AMS HT* gives the same minimum in
both lists; bambuddy's two drying tables agree (under `P2S` and its model code `N7`).

### `P2S_NOZZLE_TEMP_MAX`
```rust
const P2S_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `P2S_Z_MAX`
```rust
const P2S_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.

