*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [unknown](index.md)*

---

# Module `unknown`

# Unrecognized Model Fallback Quirks

Strategy used for [`PrinterModel::Unknown`](crate::types::PrinterModel::Unknown) — a printer
whose model string this crate does not recognize (a new SKU, a malformed SSDP `DevModel`
header, or a firmware that reports an unexpected token).

Physical limits here are the **floor of the entire supported family**, not any one model's
values: an unrecognized machine could be any of them, so every ceiling has to be one no
shipping model would exceed. This is why the fallback is not simply X1C's strategy — X1C's
bed ceiling is voltage-dependent and rises to 120 °C on a 110 V unit, 40 °C past the real
ceiling of the entry-level models an unrecognized printer might well be.

Connection-layer behavior (FTPS data-channel encryption, TLS 1.2 enforcement, camera
protocol) keeps the X1-series values, since those are interop choices rather than physical
safety ceilings and the X1 settings are the ones that reach the widest set of hosts.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`UnknownQuirks`](#unknownquirks) | struct | Conservative quirks for an unrecognized printer model — see the module docs. |
| [`UNKNOWN_AXIS_MAX`](#unknown-axis-max) | const | Travel ceiling (mm), applied to all three axes — the smallest build volume in the family (A1 Mini), per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`UNKNOWN_BED_TEMP_MAX`](#unknown-bed-temp-max) | const | Bed temperature ceiling (°C) — the lowest build-plate ceiling in the family (A1 Mini / A2L), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`UNKNOWN_NOZZLE_TEMP_MAX`](#unknown-nozzle-temp-max) | const | Nozzle temperature ceiling (°C) — the lowest hot-end ceiling in the family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |

## Types

### `UnknownQuirks`

```rust
struct UnknownQuirks;
```

Conservative quirks for an unrecognized printer model — see the module docs.

#### Trait Implementations

##### `impl ModelQuirks for UnknownQuirks`

- <span id="unknownquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="unknownquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="unknownquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

  Always false — `home_flag` bit assignments are only known for recognized models, so

  reading a door state out of an unrecognized machine's flags would be fabricating a

  sensor reading rather than reporting one.

- <span id="unknownquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="unknownquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="unknownquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="unknownquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

  Assumes the bug is present — treating a real `stg_cur` idle report as suspect costs a

  redundant state check, while missing the bug reports a running print as finished.

- <span id="unknownquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="unknownquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="unknownquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="unknownquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

  True so axis-constrained `G28` is rejected — a bed-slinger tolerates the homing variants

  a bed-on-Z machine crashes on, so assuming bed-on-Z is the direction that cannot break

  hardware.

- <span id="unknownquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="unknownquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="unknownquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="unknownquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="unknownquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="unknownquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

  `None` — an unrecognized machine gets no active chamber heater, so `M141` is refused

  rather than sent to a printer that may have no heater to receive it.


---

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

