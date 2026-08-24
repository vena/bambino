*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [p1](index.md)*

---

# Module `p1`

# P1 Series (P1P & P1S CoreXY) Quirks

Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`P1PQuirks`](#p1pquirks) | struct | Quirks for the P1P CoreXY platform. |
| [`P1SQuirks`](#p1squirks) | struct | Quirks for the P1S CoreXY platform (same family, enclosed, guaranteed aux fan). |
| [`P1_BED_TEMP_MAX`](#p1-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`P1_NOZZLE_TEMP_MAX`](#p1-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`P1_Z_MAX`](#p1-z-max) | const | Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row. |

## Types

### `P1PQuirks`

```rust
struct P1PQuirks;
```

Quirks for the P1P CoreXY platform.

#### Trait Implementations

##### `impl ModelQuirks for P1PQuirks`

- <span id="p1pquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="p1pquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="p1pquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="p1pquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="p1pquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="p1pquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="p1pquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="p1pquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="p1pquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="p1pquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="p1pquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="p1pquirks-modelquirks-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

- <span id="p1pquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="p1pquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="p1pquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="p1pquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="p1pquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="p1pquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="p1pquirks-modelquirks-supports-auxiliary-left-fan"></span>`fn supports_auxiliary_left_fan(&self) -> bool`

  `MODEL_MATRIX.csv`'s Aux Part Cooling Fan row lists P1P as `Optional`
  (not guaranteed present) vs. P1S's `Yes` — the shared `P1Quirks` struct this
  split from couldn't distinguish the two and unconditionally reported `true`,
  which would over-report support on a P1P without the physical fan installed.

### `P1SQuirks`

```rust
struct P1SQuirks;
```

Quirks for the P1S CoreXY platform (same family, enclosed, guaranteed aux fan).

#### Trait Implementations

##### `impl ModelQuirks for P1SQuirks`

- <span id="p1squirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="p1squirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="p1squirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="p1squirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="p1squirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="p1squirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="p1squirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="p1squirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="p1squirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="p1squirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="p1squirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="p1squirks-modelquirks-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

- <span id="p1squirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="p1squirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="p1squirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="p1squirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="p1squirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="p1squirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="p1squirks-modelquirks-supports-auxiliary-left-fan"></span>`fn supports_auxiliary_left_fan(&self) -> bool`

  `MODEL_MATRIX.csv`'s Aux Part Cooling Fan row lists P1P as `Optional`
  (not guaranteed present) vs. P1S's `Yes` — the shared `P1Quirks` struct this
  split from couldn't distinguish the two and unconditionally reported `true`,
  which would over-report support on a P1P without the physical fan installed.


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

