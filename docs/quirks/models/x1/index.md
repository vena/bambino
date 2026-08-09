*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [x1](index.md)*

---

# Module `x1`

# X1 Series (X1C, X1E CoreXY) Quirks

Implements hardware safety guidelines and thermal parameters for the premium CoreXY platforms.
X1C and X1E share all behavior except active chamber heater support (X1E only).

## Contents

- [Types](#types)
  - [`X1CQuirks`](#x1cquirks)
  - [`X1EQuirks`](#x1equirks)
- [Constants](#constants)
  - [`X1C_BED_TEMP_MAX_110V`](#x1c-bed-temp-max-110v)
  - [`X1C_BED_TEMP_MAX_220V`](#x1c-bed-temp-max-220v)
  - [`X1C_NOZZLE_TEMP_MAX`](#x1c-nozzle-temp-max)
  - [`X1E_BED_TEMP_MAX`](#x1e-bed-temp-max)
  - [`X1E_CHAMBER_TEMP_MAX`](#x1e-chamber-temp-max)
  - [`X1E_NOZZLE_TEMP_MAX`](#x1e-nozzle-temp-max)
  - [`X1_Z_MAX`](#x1-z-max)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`X1CQuirks`](#x1cquirks) | struct | Quirks for the X1C — no active chamber heater, voltage-dependent bed ceiling (see `x1c_bed_temp_max`). |
| [`X1EQuirks`](#x1equirks) | struct | Quirks for the X1E — active chamber heater, higher nozzle ceiling than X1C. |
| [`X1C_BED_TEMP_MAX_110V`](#x1c-bed-temp-max-110v) | const | Bed temperature ceiling on a 110V-region unit. |
| [`X1C_BED_TEMP_MAX_220V`](#x1c-bed-temp-max-220v) | const | Bed temperature ceiling on a 220V-region unit — confirmed, per the official spec sheet, non-obviously *lower* than the 110V ceiling. |
| [`X1C_NOZZLE_TEMP_MAX`](#x1c-nozzle-temp-max) | const | X1C nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X1E_BED_TEMP_MAX`](#x1e-bed-temp-max) | const | X1E bed temperature ceiling (°C) — flat, not voltage-dependent (see `x1e_bed_temp_max`), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`X1E_CHAMBER_TEMP_MAX`](#x1e-chamber-temp-max) | const | X1E chamber temperature ceiling (°C) — X1E has an active chamber heater, X1C does not, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row. |
| [`X1E_NOZZLE_TEMP_MAX`](#x1e-nozzle-temp-max) | const | X1E nozzle temperature ceiling (°C) — higher than X1C's, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X1_Z_MAX`](#x1-z-max) | const | Build volume Z depth (mm) shared by X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row. |

## Types

### `X1CQuirks`

```rust
struct X1CQuirks;
```

Quirks for the X1C — no active chamber heater, voltage-dependent bed ceiling (see `x1c_bed_temp_max`).

#### Trait Implementations

##### `impl ModelQuirks for X1CQuirks`

- <span id="x1cquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="x1cquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="x1cquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x1cquirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x1cquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="x1cquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="x1cquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="x1cquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="x1cquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="x1cquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="x1cquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="x1cquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="x1cquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="x1cquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="x1cquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="x1cquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="x1cquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16`

- <span id="x1cquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

### `X1EQuirks`

```rust
struct X1EQuirks;
```

Quirks for the X1E — active chamber heater, higher nozzle ceiling than X1C.

#### Trait Implementations

##### `impl ModelQuirks for X1EQuirks`

- <span id="x1equirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="x1equirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="x1equirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x1equirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x1equirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="x1equirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="x1equirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="x1equirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="x1equirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="x1equirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="x1equirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="x1equirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="x1equirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="x1equirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="x1equirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="x1equirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="x1equirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16`

- <span id="x1equirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`


---

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

X1E bed temperature ceiling (°C) — flat, not voltage-dependent (see `x1e_bed_temp_max`), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

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

Build volume Z depth (mm) shared by X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row.

