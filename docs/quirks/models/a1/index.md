*[bambino](../../../index.md) / [quirks](../../index.md) / [models](../index.md) / [a1](index.md)*

---

# Module `a1`

# A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates

Handles the kinematics, safety boundaries, and mechanical constraints of the
A1 bed-slinger family [REF-MOTO-GCODE].

- A1: 256×256×256mm build volume
- A1 Mini: 180×180×180mm build volume

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`A1MiniQuirks`](#a1miniquirks) | struct | Quirks for the A1 Mini bed-slinger (same family, smaller build volume/bed ceiling). |
| [`A1Quirks`](#a1quirks) | struct | Quirks for the full-size A1 bed-slinger. |
| [`A1_BED_TEMP_MAX`](#a1-bed-temp-max) | const | A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A1_MINI_BED_TEMP_MAX`](#a1-mini-bed-temp-max) | const | A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. |
| [`A1_MINI_Z_MAX`](#a1-mini-z-max) | const | A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |
| [`A1_NOZZLE_TEMP_MAX`](#a1-nozzle-temp-max) | const | Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row. |
| [`A1_Z_MAX`](#a1-z-max) | const | A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row. |

## Types

### `A1MiniQuirks`

```rust
struct A1MiniQuirks;
```

Quirks for the A1 Mini bed-slinger (same family, smaller build volume/bed ceiling).

#### Trait Implementations

##### `impl ModelQuirks for A1MiniQuirks`

- <span id="a1miniquirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="a1miniquirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="a1miniquirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="a1miniquirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="a1miniquirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="a1miniquirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="a1miniquirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="a1miniquirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="a1miniquirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="a1miniquirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="a1miniquirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="a1miniquirks-modelquirks-ams-remote-drying-support"></span>`fn ams_remote_drying_support(&self, _ctx: &crate::quirks::QuirkContext<'_>) -> crate::quirks::Support` — [`QuirkContext`](../../context/index.md#quirkcontext), [`Support`](../../index.md#support)

  Never: no known firmware path on the A1 series exposes a remote-dry command.

  Bambu Lab's *Filament drying guide for AMS 2 Pro and AMS HT* lists A1/A1 mini as
  "not supported yet", and bambuddy lists both in `_DRYING_UNSUPPORTED_MODELS`
  (`printer_manager.py:223`).

  **Not a hardware limit.** The A1 series does take AMS 2 Pro and AMS-HT units —
  see `ams_pool_composition()` a few lines below, which
  returns a shared pool of 4 like the X1C/P1/A2L (with one AMS Lite as the
  alternative), plus
  `reference/05_materials_ams.md` and `MODEL_MATRIX.csv`. The gate is about the
  command channel, not the attachable hardware; this puts A1 in the same bucket as
  P1P/P1S rather than a "never possible" one.

  Unconditional rather than `fun2`-first only because the A1 family sends no
  `fun2` at all, so there is no reported bit to defer to.

- <span id="a1miniquirks-modelquirks-ams-drying-while-printing-support"></span>`fn ams_drying_while_printing_support(&self, _ctx: &crate::quirks::QuirkContext<'_>) -> crate::quirks::Support` — [`QuirkContext`](../../context/index.md#quirkcontext), [`Support`](../../index.md#support)

  Never supports drying while printing.

  The drying guide names A1/A1 mini as "not supported yet" for simultaneous drying
  and printing.

- <span id="a1miniquirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="a1miniquirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="a1miniquirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="a1miniquirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="a1miniquirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="a1miniquirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="a1miniquirks-modelquirks-supports-prompt-sound"></span>`fn supports_prompt_sound(&self) -> bool`

- <span id="a1miniquirks-modelquirks-supports-auxiliary-left-fan"></span>`fn supports_auxiliary_left_fan(&self) -> bool`

### `A1Quirks`

```rust
struct A1Quirks;
```

Quirks for the full-size A1 bed-slinger.

#### Trait Implementations

##### `impl ModelQuirks for A1Quirks`

- <span id="a1quirks-modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

- <span id="a1quirks-modelquirks-enforces-ftps-tls-1-2"></span>`fn enforces_ftps_tls_1_2(&self) -> bool`

- <span id="a1quirks-modelquirks-is-door-open"></span>`fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool` — [`PrinterTelemetry`](../../../types/telemetry/report/index.md#printertelemetry)

- <span id="a1quirks-modelquirks-has-door-sensor"></span>`fn has_door_sensor(&self) -> bool`

- <span id="a1quirks-modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../../../camera/index.md#cameraprotocol)

- <span id="a1quirks-modelquirks-ignores-chamber-temperature"></span>`fn ignores_chamber_temperature(&self) -> bool`

- <span id="a1quirks-modelquirks-has-stg-cur-idle-bug"></span>`fn has_stg_cur_idle_bug(&self) -> bool`

- <span id="a1quirks-modelquirks-active-chamber-heater-max-temp-c"></span>`fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

- <span id="a1quirks-modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

- <span id="a1quirks-modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../../../ams/mapping/index.md#amspoolcomposition)

- <span id="a1quirks-modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

- <span id="a1quirks-modelquirks-ams-remote-drying-support"></span>`fn ams_remote_drying_support(&self, _ctx: &crate::quirks::QuirkContext<'_>) -> crate::quirks::Support` — [`QuirkContext`](../../context/index.md#quirkcontext), [`Support`](../../index.md#support)

  Never: no known firmware path on the A1 series exposes a remote-dry command.

  Bambu Lab's *Filament drying guide for AMS 2 Pro and AMS HT* lists A1/A1 mini as
  "not supported yet", and bambuddy lists both in `_DRYING_UNSUPPORTED_MODELS`
  (`printer_manager.py:223`).

  **Not a hardware limit.** The A1 series does take AMS 2 Pro and AMS-HT units —
  see `ams_pool_composition()` a few lines below, which
  returns a shared pool of 4 like the X1C/P1/A2L (with one AMS Lite as the
  alternative), plus
  `reference/05_materials_ams.md` and `MODEL_MATRIX.csv`. The gate is about the
  command channel, not the attachable hardware; this puts A1 in the same bucket as
  P1P/P1S rather than a "never possible" one.

  Unconditional rather than `fun2`-first only because the A1 family sends no
  `fun2` at all, so there is no reported bit to defer to.

- <span id="a1quirks-modelquirks-ams-drying-while-printing-support"></span>`fn ams_drying_while_printing_support(&self, _ctx: &crate::quirks::QuirkContext<'_>) -> crate::quirks::Support` — [`QuirkContext`](../../context/index.md#quirkcontext), [`Support`](../../index.md#support)

  Never supports drying while printing.

  The drying guide names A1/A1 mini as "not supported yet" for simultaneous drying
  and printing.

- <span id="a1quirks-modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

- <span id="a1quirks-modelquirks-z-max"></span>`fn z_max(&self) -> f32`

- <span id="a1quirks-modelquirks-x-max"></span>`fn x_max(&self) -> f32`

- <span id="a1quirks-modelquirks-y-max"></span>`fn y_max(&self) -> f32`

- <span id="a1quirks-modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

- <span id="a1quirks-modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16`

- <span id="a1quirks-modelquirks-supports-prompt-sound"></span>`fn supports_prompt_sound(&self) -> bool`

- <span id="a1quirks-modelquirks-supports-auxiliary-left-fan"></span>`fn supports_auxiliary_left_fan(&self) -> bool`


---

## Constants

### `A1_BED_TEMP_MAX`
```rust
const A1_BED_TEMP_MAX: u16 = 100u16;
```

A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `A1_MINI_BED_TEMP_MAX`
```rust
const A1_MINI_BED_TEMP_MAX: u16 = 80u16;
```

A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.

### `A1_MINI_Z_MAX`
```rust
const A1_MINI_Z_MAX: f32 = 180f32;
```

A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.

### `A1_NOZZLE_TEMP_MAX`
```rust
const A1_NOZZLE_TEMP_MAX: u16 = 300u16;
```

Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.

### `A1_Z_MAX`
```rust
const A1_Z_MAX: f32 = 256f32;
```

A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.

