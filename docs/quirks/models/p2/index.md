*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [p2](index.md)*

---

# Module `p2`

# P2 Series (P2S CoreXY) Quirks

Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`P2Quirks`](#p2quirks) | struct | Quirks for the P2S CoreXY platform. |
| [`P2S_BED_TEMP_MAX`](#p2s-bed-temp-max) | const | Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`P2S_NOZZLE_TEMP_MAX`](#p2s-nozzle-temp-max) | const | Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`P2S_Z_MAX`](#p2s-z-max) | const | Build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |

## Types

### `P2Quirks`

```rust
struct P2Quirks;
```

Quirks for the P2S CoreXY platform.

#### Trait Implementations

##### `impl ModelQuirks for P2Quirks`

- <span id="p2quirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="p2quirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

  P2S firmware `01.02.00.00`'s embedded vsFTPd can't process TLS 1.3's asynchronous session-ticket model on the FTPS data channel — transfers truncate mid-stream with `426 "Failure reading network stream"`.
  This is a firmware bug, not a real TLS-1.3 incompatibility: independently confirmed by the
  `bambuddy` project (reporter `@iitazz`, upstream issue #1401), which hit the identical symptom
  only after its own client started defaulting to TLS 1.3. See [REF-FTPS-CONN] in
  `reference/02_ftps.md` §2.1.

  The cap narrows the race, it doesn't close it: `bambuddy`'s own
  follow-up (issue #1417) found P2S can still return a transient `426` on
  the final post-upload response even under TLS 1.2 — the data-channel
  close still occasionally races the `226` confirmation, just later and
  less often than the pre-cap mid-stream truncation. What actually closes
  it is verifying the transfer via `SIZE` regardless of which reply code
  came back, which `FtpsClient::upload_file` already does
  unconditionally (see its doc comment in `src/ftps/client.rs`) — this
  quirk alone would not have been a complete fix.

- <span id="p2quirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="p2quirks-modelquirks-has-door-sensor-field"></span>`fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="p2quirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="p2quirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="p2quirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="p2quirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="p2quirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="p2quirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="p2quirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="p2quirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="p2quirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="p2quirks-modelquirks-requires-wallclock-rtsp-timestamps"></span>`fn requires_wallclock_rtsp_timestamps(&self) -> bool`

- <span id="p2quirks-modelquirks-supports-auxiliary-left2-fan"></span>`fn supports_auxiliary_left2_fan(&self) -> bool`

- <span id="p2quirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="p2quirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="p2quirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="p2quirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="p2quirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="p2quirks-modelquirks-supports-vibration-compensation"></span>`fn supports_vibration_compensation(&self) -> bool`

  P2S does not run vibration compensation the way the X1/P1 series does, so the flag is
  forced off. Rests on bambuddy `be18ebb3` alone and is unverified on hardware here — see
  [`crate::quirks::ModelQuirks::supports_vibration_compensation`](../../index.md#modelquirks) and issue #133.

- <span id="p2quirks-modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`


---

## Constants

### `P2S_BED_TEMP_MAX`
```rust
const P2S_BED_TEMP_MAX: u16 = 110u16;
```

Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

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

