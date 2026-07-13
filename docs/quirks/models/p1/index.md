*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [p1](index.md)*

---

# Module `p1`

# P1 Series (P1P & P1S CoreXY) Quirks

Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`P1Quirks`](#p1quirks) | struct | Quirks shared by the P1P and P1S CoreXY platforms. |
| [`P1_BED_TEMP_MAX`](#p1-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`P1_NOZZLE_TEMP_MAX`](#p1-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`P1_Z_MAX`](#p1-z-max) | const | Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row. |

## Types

### `P1Quirks`

```rust
struct P1Quirks;
```

Quirks shared by the P1P and P1S CoreXY platforms.

#### Trait Implementations

##### `impl ModelQuirks for P1Quirks`

- <span id="p1quirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="p1quirks-modelquirks-enforce-ftps-tls-1-2"></span>`fn enforce_ftps_tls_1_2(&self) -> bool`

- <span id="p1quirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="p1quirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="p1quirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="p1quirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="p1quirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="p1quirks-modelquirks-has-active-chamber-heater"></span>`fn has_active_chamber_heater(&self) -> bool`

- <span id="p1quirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="p1quirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="p1quirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="p1quirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="p1quirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="p1quirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="p1quirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`


---

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

