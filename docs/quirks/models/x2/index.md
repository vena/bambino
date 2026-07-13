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

- <span id="x2quirks-modelquirks-enforce-ftps-tls-1-2"></span>`fn enforce_ftps_tls_1_2(&self) -> bool`

  X2D firmware `01.01.00.00` fails the implicit-FTPS handshake on port 990 with `[SSL: WRONG_VERSION_NUMBER]` against a TLS 1.3 `ClientHello`.

  **Root cause unconfirmed** — the independent `bambuddy` project (reporter `@vasmarfas`, upstream

  issue #1638) capped X2D to TLS 1.2 "by analogy" with the P2S session-ticket bug (see

  `P2Quirks::enforce_ftps_tls_1_2`), explicitly noting the X2D failure could be a distinct bug

  (different FTPS auth variant or port) rather than the same one. Treat this as

  confirmed-by-symptom, not confirmed-by-root-cause. See [REF-FTPS-CONN] in `reference/02_ftps.md`

  §2.1.

- <span id="x2quirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x2quirks-modelquirks-door-sensor-field-present"></span>`fn door_sensor_field_present(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="x2quirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="x2quirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="x2quirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="x2quirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="x2quirks-modelquirks-has-active-chamber-heater"></span>`fn has_active_chamber_heater(&self) -> bool`

- <span id="x2quirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="x2quirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="x2quirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="x2quirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="x2quirks-modelquirks-supports-auxiliary-right-fan"></span>`fn supports_auxiliary_right_fan(&self) -> bool`

- <span id="x2quirks-modelquirks-auxiliary-fan-uses-percentage"></span>`fn auxiliary_fan_uses_percentage(&self) -> bool`

- <span id="x2quirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="x2quirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="x2quirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="x2quirks-modelquirks-chamber-temp-max"></span>`fn chamber_temp_max(&self) -> u16`

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

### `X2D_Z_MAX`
```rust
const X2D_Z_MAX: f32 = 256f32;
```

Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs.

