*[bambino](../index.md) / [quirks](index.md)*

---

# Module `quirks`

# Model-Specific Quirks

Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
scattering `match model { ... }` blocks everywhere, the [`ModelQuirks`](#modelquirks) trait captures
all model-specific behavior in one place. Call [`PrinterModel::quirks()`] to get the
strategy implementation for any model.

Per-model strategy structs live in the [`models`](../models/index.md#models) submodule. This module also provides
shared helpers like [`fan_step_to_percentage()`] and [`FanSpeedDebouncer`](#fanspeeddebouncer) for dealing
with the low-resolution PWM fan telemetry common across most models.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`models`](#models) | mod | # Model-Specific Kinematic and Operational Configuration Submodules |
| [`FanSpeedDebouncer`](#fanspeeddebouncer) | struct | Filters out transient quantization oscillation artifacts emitted by physical fan controllers. |
| [`ModelQuirks`](#modelquirks) | trait | Polymorphic interface tracking model-specific hardware variations and transport exceptions. |
| [`decode_fan_percentage`](#decode-fan-percentage) | fn | Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage. |
| [`fan_step_to_percentage`](#fan-step-to-percentage) | fn | Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS]. |

## Modules

- [`models`](models/index.md#models) — # Model-Specific Kinematic and Operational Configuration Submodules


---

## Types

### `FanSpeedDebouncer`

```rust
struct FanSpeedDebouncer {
    // [REDACTED: Private Fields]
}
```

Filters out transient quantization oscillation artifacts emitted by physical fan controllers.

**Why this is required [REF-CLIM-FANS]:**
Due to the low-resolution 0–15 PWM mapping on physical boards, minor fan throttle drift
can cause telemetry reports to bounce rapidly between adjacent steps (e.g. step 7 and step 8),
triggering interface flickering. This state tracker dampens steps by requiring persistent,
consecutive readings before committing a one-step change.

#### Implementations

- <span id="fanspeeddebouncer-new"></span>`fn new() -> Self`

  Instantiates a new debouncer initialized to 0% speed.

- <span id="fanspeeddebouncer-debounce"></span>`fn debounce(&mut self, incoming_percentage: u8) -> u8`

  Processes an raw incoming fan speed percentage, filtering minor step oscillations.

#### Trait Implementations

##### `impl Clone for FanSpeedDebouncer`

- <span id="fanspeeddebouncer-clone"></span>`fn clone(&self) -> FanSpeedDebouncer` — [`FanSpeedDebouncer`](#fanspeeddebouncer)

##### `impl Debug for FanSpeedDebouncer`

- <span id="fanspeeddebouncer-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for FanSpeedDebouncer`

- <span id="fanspeeddebouncer-default"></span>`fn default() -> Self`


---

## Traits

### `ModelQuirks`

```rust
trait ModelQuirks { ... }
```


Polymorphic interface tracking model-specific hardware variations and transport exceptions.

#### Required Methods

- `fn uses_plaintext_ftps_data_channel(&self) -> bool`

  Returns true if this model series requires plaintext transmissions on the FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].

- `fn enforces_ftps_tls_1_2(&self) -> bool`

  Returns true if this model series must restrict its TLS version strictly to TLS 1.2 to prevent session resumption failure [REF-FTPS-CONN].

- `fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool`

  Evaluates whether the physical front enclosure door is open based on model-specific sensor routing [REF-NET-DOOR].

- `fn has_door_sensor(&self) -> bool`

  Returns true if the physical machine chassis is equipped with an electronic front enclosure door open sensor switch.

- `fn camera_protocol(&self) -> CameraProtocol`

  Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].

- `fn ignores_chamber_temperature(&self) -> bool`

  Returns true if the model is an open-frame or entry-level machine lacking a physical chamber temperature sensor [REF-THER-DECODE].

- `fn has_stg_cur_idle_bug(&self) -> bool`

  Returns true if the model series exhibits the idle state-machine bug where `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].

- `fn has_active_chamber_heater(&self) -> bool`

  Returns true if the model possesses an active PTC chamber heater (M141) [REF-MOTO-GCODE].

- `fn physical_nozzle_count(&self) -> u8`

  Returns the number of physical extruder carriages present on the machine carriage bus.

- `fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition`

  Returns this model's physical AMS unit pool structure (BUG-122) — whether standard AMS

- `fn supports_nozzle_offset_calibration(&self) -> bool`

  Returns true if the model supports electronic alignment and nozzle offset calibration sweeps.

- `fn is_bed_on_z(&self) -> bool`

  Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].

- `fn nozzle_temp_max(&self) -> u16`

  Returns the maximum safe nozzle/hotend temperature in °C for this model.

- `fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16`

  Returns the maximum safe heated bed temperature in °C for this model.

#### Provided Methods 

- `fn has_door_sensor_field(&self, _telemetry: &PrinterTelemetry) -> bool`

  Returns true if `telemetry` carries the specific wire field this model's

- `fn is_unsafe_homing_command(&self, gcode: &str) -> bool`

  Evaluates if a given G-code command carries unsafe axis-constrained homing directions [REF-MOTO-GCODE].

- `fn z_max(&self) -> f32`

  Returns the maximum safe Z-axis travel distance in millimeters for this model.

- `fn x_max(&self) -> f32`

  Returns the maximum safe X-axis travel distance in millimeters for this model (BUG-163).

- `fn y_max(&self) -> f32`

  Returns the maximum safe Y-axis travel distance in millimeters for this model (BUG-163).

- `fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String`

  Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].

- `fn relative_xy_move_gcode(&self, axis: char, distance: f32, feedrate: u32) -> String`

  Generates a bounded relative X/Y-axis movement G-code command (BUG-163) — the same

- `fn requires_wallclock_rtsp_timestamps(&self) -> bool`

  Returns true if the model's RTSP camera stream requires wallclock timestamps instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].

- `fn supports_auxiliary_right_fan(&self) -> bool`

  Returns true if the model has a secondary right-side auxiliary fan (port 10) [REF-CLIM-FANS].

- `fn supports_auxiliary_left_fan(&self) -> bool`

  Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].

- `fn has_chamber_exhaust_fan(&self) -> bool`

  Returns true if the model has a chamber exhaust/filtration fan (port 3) [REF-CLIM-FANS].

- `fn reports_auxiliary_fan_percentage(&self) -> bool`

  Returns true if the model's auxiliary fan telemetry reports speed as a direct percentage (0-100) instead of discrete PWM steps (0-15) [REF-CLIM-FANS].

- `fn supports_airduct_mode(&self) -> bool`

  Returns true if the model has controllable airduct dampers for climate mode switching (cooling vs heating recirculation) [REF-CLIM-FANS].

- `fn supports_prompt_sound(&self) -> bool`

  Returns true if the model has onboard speakers for prompt sound notifications.

- `fn supports_buzzer(&self) -> bool`

  Returns true if the model has a physical fire alarm buzzer module.

- `fn supports_ams_remote_drying(&self) -> bool`

  Returns true if `ams_filament_drying` sent over MQTT is actually honored by the host

- `fn chamber_temp_max(&self) -> u16`

  Returns the maximum active chamber heater temperature in °C for this model.

#### Implementors

- [`A1MiniQuirks`](models/a1/index.md#a1miniquirks)
- [`A1Quirks`](models/a1/index.md#a1quirks)
- [`A2LQuirks`](models/a2/index.md#a2lquirks)
- [`H2CQuirks`](models/h2/index.md#h2cquirks)
- [`H2DProQuirks`](models/h2/index.md#h2dproquirks)
- [`H2DQuirks`](models/h2/index.md#h2dquirks)
- [`H2SQuirks`](models/h2/index.md#h2squirks)
- [`P1Quirks`](models/p1/index.md#p1quirks)
- [`P2Quirks`](models/p2/index.md#p2quirks)
- [`X1CQuirks`](models/x1/index.md#x1cquirks)
- [`X1EQuirks`](models/x1/index.md#x1equirks)
- [`X2Quirks`](models/x2/index.md#x2quirks)


---

## Functions

### `decode_fan_percentage`

```rust
fn decode_fan_percentage(raw: Option<&str>, uses_percentage: bool) -> Option<u8>
```

Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage.

`uses_percentage` should come from [`ModelQuirks::reports_auxiliary_fan_percentage()`] — most
models report a 0-15 step value needing [`fan_step_to_percentage()`], but some report an
already-clamped percentage directly. Returns `None` if `raw` is absent or not a valid `u8`.

### `fan_step_to_percentage`

```rust
fn fan_step_to_percentage(step: u8) -> u8
```

Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].

Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.

