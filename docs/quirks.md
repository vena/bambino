**bambino > quirks**

# Module: quirks

## Contents

**Modules**

- [`models`](#models) - # Model-Specific Kinematic and Operational Configuration Submodules

**Structs**

- [`FanSpeedDebouncer`](#fanspeeddebouncer) - Filters out transient quantization oscillation artifacts emitted by physical fan controllers.

**Functions**

- [`decode_fan_percentage`](#decode_fan_percentage) - Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/
- [`fan_step_to_percentage`](#fan_step_to_percentage) - Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].

**Traits**

- [`ModelQuirks`](#modelquirks) - Polymorphic interface tracking model-specific hardware variations and transport exceptions.

---

## bambino::quirks::FanSpeedDebouncer

*Struct*

Filters out transient quantization oscillation artifacts emitted by physical fan controllers.

**Why this is required [REF-CLIM-FANS]:**
Due to the low-resolution 0–15 PWM mapping on physical boards, minor fan throttle drift
can cause telemetry reports to bounce rapidly between adjacent steps (e.g. step 7 and step 8),
triggering interface flickering. This state tracker dampens steps by requiring persistent,
consecutive readings before committing a one-step change.

**Methods:**

- `fn new() -> Self` - Instantiates a new debouncer initialized to 0% speed.
- `fn debounce(self: & mut Self, incoming_percentage: u8) -> u8` - Processes an raw incoming fan speed percentage, filtering minor step oscillations.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> FanSpeedDebouncer`
- **Default**
  - `fn default() -> Self`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::quirks::ModelQuirks

*Trait*

Polymorphic interface tracking model-specific hardware variations and transport exceptions.

**Methods:**

- `uses_plaintext_ftps_data_channel`: Returns true if this model series requires plaintext transmissions on the
- `enforce_ftps_tls_1_2`: Returns true if this model series must restrict its TLS version strictly
- `is_door_open`: Evaluates whether the physical front enclosure door is open based on
- `has_door_sensor`: Returns true if the physical machine chassis is equipped with an electronic
- `camera_protocol`: Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].
- `ignores_chamber_temperature`: Returns true if the model is an open-frame or entry-level machine lacking
- `has_stg_cur_idle_bug`: Returns true if the model series exhibits the idle state-machine bug where
- `has_active_chamber_heater`: Returns true if the model possesses an active PTC chamber heater (M141) [REF-MOTO-GCODE].
- `physical_nozzle_count`: Returns the number of physical extruder carriages present on the machine carriage bus.
- `supports_nozzle_offset_calibration`: Returns true if the model supports electronic alignment and nozzle offset calibration sweeps.
- `is_bed_on_z`: Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
- `is_unsafe_homing_command`: Evaluates if a given G-code command carries unsafe axis-constrained homing directions [REF-MOTO-GCODE].
- `z_max`: Returns the maximum safe Z-axis travel distance in millimeters for this model.
- `relative_z_move_gcode`: Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].
- `requires_wallclock_rtsp_timestamps`: Returns true if the model's RTSP camera stream requires wallclock timestamps
- `supports_auxiliary_right_fan`: Returns true if the model has a secondary right-side auxiliary fan (port 10) [REF-CLIM-FANS].
- `supports_auxiliary_left_fan`: Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].
- `has_chamber_exhaust_fan`: Returns true if the model has a chamber exhaust/filtration fan (port 3) [REF-CLIM-FANS].
- `auxiliary_fan_uses_percentage`: Returns true if the model's auxiliary fan telemetry reports speed as a direct
- `supports_airduct_mode`: Returns true if the model has controllable airduct dampers for climate
- `supports_prompt_sound`: Returns true if the model has onboard speakers for prompt sound notifications.
- `supports_buzzer`: Returns true if the model has a physical fire alarm buzzer module.
- `nozzle_temp_max`: Returns the maximum safe nozzle/hotend temperature in °C for this model.
- `bed_temp_max`: Returns the maximum safe heated bed temperature in °C for this model.
- `chamber_temp_max`: Returns the maximum active chamber heater temperature in °C for this model.



## bambino::quirks::decode_fan_percentage

*Function*

Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/
`big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage.

`uses_percentage` should come from [`ModelQuirks::auxiliary_fan_uses_percentage()`] — most
models report a 0-15 step value needing [`fan_step_to_percentage()`], but some report an
already-clamped percentage directly. Returns `None` if `raw` is absent or not a valid `u8`.

```rust
fn decode_fan_percentage(raw: Option<&str>, uses_percentage: bool) -> Option<u8>
```



## bambino::quirks::fan_step_to_percentage

*Function*

Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].

Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.

```rust
fn fan_step_to_percentage(step: u8) -> u8
```



## Module: models

# Model-Specific Kinematic and Operational Configuration Submodules

Isolates physical constraints (such as safe homing rules, bed coordinate limits,
and relative axis orientation guidelines) into individual, model-specific modules,
one per `BambuModel` variant.



