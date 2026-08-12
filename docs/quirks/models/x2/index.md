*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [x2](index.md)*

---

# Module `x2`

# X2 Series (X2D CoreXY) Quirks

Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.

Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
Z-max uses the conservative aux/dual value (256mm).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`X2Quirks`](#x2quirks) | struct | Quirks for the X2D dual-carriage, dual-nozzle CoreXY platform. |
| [`X2D_BED_TEMP_MAX`](#x2d-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`X2D_CHAMBER_TEMP_MAX`](#x2d-chamber-temp-max) | const | Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row. |
| [`X2D_NOZZLE_TEMP_MAX`](#x2d-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`X2D_X_MAX`](#x2d-x-max) | const | Build volume X width (mm) — conservative aux/dual-nozzle value (235.5mm, smaller than the main-nozzle profile's 256mm); see module docs. |
| [`X2D_Y_MAX`](#x2d-y-max) | const | Build volume Y depth (mm) — 256mm across all nozzle profiles. |
| [`X2D_Z_MAX`](#x2d-z-max) | const | Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs. |

## Types

### `X2Quirks`

```rust
struct X2Quirks;
```

Quirks for the X2D dual-carriage, dual-nozzle CoreXY platform.

#### Trait Implementations

##### `impl ModelQuirks for X2Quirks`

- <span id="x2quirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="x2quirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

  X2D firmware `01.01.00.00` fails the implicit-FTPS handshake on port 990 with `[SSL: WRONG_VERSION_NUMBER]` against a TLS 1.3 `ClientHello`.

  **Root cause unconfirmed** — the independent `bambuddy` project (reporter `@vasmarfas`, upstream

  issue #1638) capped X2D to TLS 1.2 "by analogy" with the P2S session-ticket bug (see

  `P2Quirks::enforces_ftps_tls_1_2`), explicitly noting the X2D failure could be a distinct bug

  (different FTPS auth variant or port) rather than the same one. Treat this as

  confirmed-by-symptom, not confirmed-by-root-cause. See [REF-FTPS-CONN] in `reference/02_ftps.md`

  §2.1.

- <span id="x2quirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x2quirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x2quirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="x2quirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="x2quirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="x2quirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="x2quirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="x2quirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="x2quirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="x2quirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="x2quirks-modelquirks-supports-auxiliary-left2-fan"></span>`fn supports_auxiliary_left2_fan(&self) -> bool`

- <span id="x2quirks-modelquirks-reports-auxiliary-fan-percentage"></span>`fn reports_auxiliary_fan_percentage(&self) -> bool`

- <span id="x2quirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="x2quirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="x2quirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="x2quirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="x2quirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="x2quirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="x2quirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

- <span id="x2quirks-modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`


---

## Constants

### `X2D_BED_TEMP_MAX`
```rust
const X2D_BED_TEMP_MAX: u16 = 120u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `X2D_CHAMBER_TEMP_MAX`
```rust
const X2D_CHAMBER_TEMP_MAX: u16 = 65u16;
```

Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.

### `X2D_NOZZLE_TEMP_MAX`
```rust
const X2D_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `X2D_X_MAX`
```rust
const X2D_X_MAX: f32 = 235.5f32;
```

Build volume X width (mm) — conservative aux/dual-nozzle value (235.5mm, smaller than the
main-nozzle profile's 256mm); see module docs.

### `X2D_Y_MAX`
```rust
const X2D_Y_MAX: f32 = 256f32;
```

Build volume Y depth (mm) — 256mm across all nozzle profiles.

### `X2D_Z_MAX`
```rust
const X2D_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs.

