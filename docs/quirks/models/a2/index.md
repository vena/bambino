*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [a2](index.md)*

---

# Module `a2`

# A2 Series (A2L Bed-Slinger) Quirks & Coordinates

The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`A2LQuirks`](#a2lquirks) | struct | Quirks for the A2L large-format open-frame bed-slinger. |
| [`A2L_BED_TEMP_MAX`](#a2l-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A2L_NOZZLE_TEMP_MAX`](#a2l-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`A2L_X_MAX`](#a2l-x-max) | const | A2L build volume X width (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm) (BUG-163). |
| [`A2L_Y_MAX`](#a2l-y-max) | const | A2L build volume Y depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm) (BUG-163). |
| [`A2L_Z_MAX`](#a2l-z-max) | const | A2L build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm). |

## Types

### `A2LQuirks`

```rust
struct A2LQuirks;
```

Quirks for the A2L large-format open-frame bed-slinger.

#### Trait Implementations

##### `impl ModelQuirks for A2LQuirks`

- <span id="a2lquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="a2lquirks-modelquirks-enforce-ftps-tls-1-2"></span>`fn enforce_ftps_tls_1_2(&self) -> bool`

- <span id="a2lquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="a2lquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="a2lquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="a2lquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="a2lquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="a2lquirks-modelquirks-has-active-chamber-heater"></span>`fn has_active_chamber_heater(&self) -> bool`

- <span id="a2lquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="a2lquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="a2lquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="a2lquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="a2lquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="a2lquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="a2lquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="a2lquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="a2lquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="a2lquirks-modelquirks-supports-prompt-sound"></span>`fn supports_prompt_sound(&self) -> bool`

- <span id="a2lquirks-modelquirks-supports-auxiliary-left-fan"></span>`fn supports_auxiliary_left_fan(&self) -> bool`


---

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

A2L build volume X width (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm) (BUG-163).

### `A2L_Y_MAX`
```rust
const A2L_Y_MAX: f32 = 320f32;
```

A2L build volume Y depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm) (BUG-163).

### `A2L_Z_MAX`
```rust
const A2L_Z_MAX: f32 = 325f32;
```

A2L build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).

