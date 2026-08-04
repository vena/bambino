*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [h2](index.md)*

---

# Module `h2`

# H2 Series (H2S, H2D, H2D Pro, H2C) Quirks

Manages the properties and kinematic characteristics of the single-nozzle,
IDEX, and tool-changer platforms [REF-MOTO-GCODE].

Z-axis limits vary by model — per `MODEL_MATRIX.csv`'s Build Volume row, Z max does
not vary by active nozzle for these three models:
- H2S: 340mm (single nozzle only)
- H2D/H2D Pro: 325mm
- H2C: 325mm

H2C has 6 Vortek tool-changer hotends + 1 fixed hotend = 7 nozzles.
O1C and O1C2 are hardware revisions with identical quirks.

## Contents

- [Types](#types)
  - [`H2CQuirks`](#h2cquirks)
  - [`H2DProQuirks`](#h2dproquirks)
  - [`H2DQuirks`](#h2dquirks)
  - [`H2SQuirks`](#h2squirks)
- [Constants](#constants)
  - [`H2S_X_MAX`](#h2s-x-max)
  - [`H2S_Y_MAX`](#h2s-y-max)
  - [`H2S_Z_MAX`](#h2s-z-max)
  - [`H2_BED_TEMP_MAX`](#h2-bed-temp-max)
  - [`H2_CHAMBER_TEMP_MAX`](#h2-chamber-temp-max)
  - [`H2_DUAL_X_MAX`](#h2-dual-x-max)
  - [`H2_DUAL_Y_MAX`](#h2-dual-y-max)
  - [`H2_DUAL_Z_MAX`](#h2-dual-z-max)
  - [`H2_NOZZLE_TEMP_MAX`](#h2-nozzle-temp-max)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`H2CQuirks`](#h2cquirks) | struct | Quirks for the H2C — Vortek tool-changer platform (6 tool-changer nozzles + 1 fixed nozzle). |
| [`H2DProQuirks`](#h2dproquirks) | struct | Quirks for the H2D Pro — same kinematics as H2D. |
| [`H2DQuirks`](#h2dquirks) | struct | Quirks for the H2D — dual-nozzle (IDEX) CoreXY. |
| [`H2SQuirks`](#h2squirks) | struct | Quirks for the H2S — single-nozzle CoreXY, tallest Z of the H2 family. |
| [`H2S_X_MAX`](#h2s-x-max) | const | H2S build volume X/Y (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row (340×320×340mm). |
| [`H2S_Y_MAX`](#h2s-y-max) | const | See `H2S_X_MAX`'s doc comment. |
| [`H2S_Z_MAX`](#h2s-z-max) | const | H2S build volume Z depth (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`H2_BED_TEMP_MAX`](#h2-bed-temp-max) | const | Bed temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`H2_CHAMBER_TEMP_MAX`](#h2-chamber-temp-max) | const | Chamber temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row. |
| [`H2_DUAL_X_MAX`](#h2-dual-x-max) | const | X/Y (mm) shared by H2D, H2D Pro, and H2C — conservative dual-nozzle value (the smaller of each model's single/dual-nozzle profiles), same approach as `H2_DUAL_Z_MAX`. |
| [`H2_DUAL_Y_MAX`](#h2-dual-y-max) | const | See `H2_DUAL_X_MAX`'s doc comment. |
| [`H2_DUAL_Z_MAX`](#h2-dual-z-max) | const | Z depth (mm) shared by H2D, H2D Pro, and H2C — does not vary by active nozzle, per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`H2_NOZZLE_TEMP_MAX`](#h2-nozzle-temp-max) | const | Nozzle temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |

## Types

### `H2CQuirks`

```rust
struct H2CQuirks;
```

Quirks for the H2C — Vortek tool-changer platform (6 tool-changer nozzles + 1 fixed nozzle).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for H2CQuirks`

##### `impl<E> AsTaggedImplicit<'a, E> for H2CQuirks`

##### `impl ModelQuirks for H2CQuirks`

- <span id="h2cquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="h2cquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="h2cquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2cquirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2cquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="h2cquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="h2cquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="h2cquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="h2cquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="h2cquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="h2cquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="h2cquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="h2cquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="h2cquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="h2cquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="h2cquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="h2cquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="h2cquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="h2cquirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

- <span id="h2cquirks-modelquirks-supports-buzzer"></span>`fn supports_buzzer(&self) -> bool`

- <span id="h2cquirks-modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`

### `H2DProQuirks`

```rust
struct H2DProQuirks;
```

Quirks for the H2D Pro — same kinematics as H2D.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for H2DProQuirks`

##### `impl<E> AsTaggedImplicit<'a, E> for H2DProQuirks`

##### `impl ModelQuirks for H2DProQuirks`

- <span id="h2dproquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="h2dproquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="h2dproquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2dproquirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2dproquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="h2dproquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="h2dproquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="h2dproquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="h2dproquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="h2dproquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="h2dproquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="h2dproquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="h2dproquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="h2dproquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="h2dproquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="h2dproquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="h2dproquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="h2dproquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="h2dproquirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

- <span id="h2dproquirks-modelquirks-supports-buzzer"></span>`fn supports_buzzer(&self) -> bool`

- <span id="h2dproquirks-modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`

### `H2DQuirks`

```rust
struct H2DQuirks;
```

Quirks for the H2D — dual-nozzle (IDEX) CoreXY.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for H2DQuirks`

##### `impl<E> AsTaggedImplicit<'a, E> for H2DQuirks`

##### `impl ModelQuirks for H2DQuirks`

- <span id="h2dquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="h2dquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="h2dquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2dquirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2dquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="h2dquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="h2dquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="h2dquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="h2dquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="h2dquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="h2dquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="h2dquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="h2dquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="h2dquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="h2dquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="h2dquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="h2dquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="h2dquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="h2dquirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

- <span id="h2dquirks-modelquirks-supports-buzzer"></span>`fn supports_buzzer(&self) -> bool`

- <span id="h2dquirks-modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`

### `H2SQuirks`

```rust
struct H2SQuirks;
```

Quirks for the H2S — single-nozzle CoreXY, tallest Z of the H2 family.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for H2SQuirks`

##### `impl<E> AsTaggedImplicit<'a, E> for H2SQuirks`

##### `impl ModelQuirks for H2SQuirks`

- <span id="h2squirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="h2squirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="h2squirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2squirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="h2squirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="h2squirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="h2squirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="h2squirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="h2squirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="h2squirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="h2squirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="h2squirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="h2squirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="h2squirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="h2squirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="h2squirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="h2squirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="h2squirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="h2squirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

- <span id="h2squirks-modelquirks-supports-buzzer"></span>`fn supports_buzzer(&self) -> bool`

- <span id="h2squirks-modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`


---

## Constants

### `H2S_X_MAX`
```rust
const H2S_X_MAX: f32 = 340f32;
```

H2S build volume X/Y (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row (340×320×340mm).

### `H2S_Y_MAX`
```rust
const H2S_Y_MAX: f32 = 320f32;
```

See `H2S_X_MAX`'s doc comment.

### `H2S_Z_MAX`
```rust
const H2S_Z_MAX: f32 = 340f32;
```

H2S build volume Z depth (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row.

### `H2_BED_TEMP_MAX`
```rust
const H2_BED_TEMP_MAX: u16 = 120u16;
```

Bed temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `H2_CHAMBER_TEMP_MAX`
```rust
const H2_CHAMBER_TEMP_MAX: u16 = 65u16;
```

Chamber temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.

### `H2_DUAL_X_MAX`
```rust
const H2_DUAL_X_MAX: f32 = 300f32;
```

X/Y (mm) shared by H2D, H2D Pro, and H2C — conservative dual-nozzle value (the smaller of
each model's single/dual-nozzle profiles), same approach as `H2_DUAL_Z_MAX`.

### `H2_DUAL_Y_MAX`
```rust
const H2_DUAL_Y_MAX: f32 = 320f32;
```

See `H2_DUAL_X_MAX`'s doc comment.

### `H2_DUAL_Z_MAX`
```rust
const H2_DUAL_Z_MAX: f32 = 325f32;
```

Z depth (mm) shared by H2D, H2D Pro, and H2C — does not vary by active nozzle, per `MODEL_MATRIX.csv`'s Build Volume row.

### `H2_NOZZLE_TEMP_MAX`
```rust
const H2_NOZZLE_TEMP_MAX: u16 = 350u16;
```

Nozzle temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

