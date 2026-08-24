*[bambino](../index.md) / [quirks](index.md)*

---

# Module `quirks`

# Model-Specific Quirks

Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
scattering `match model { ... }` blocks everywhere, the [`ModelQuirks`](#modelquirks) trait captures
all model-specific behavior in one place. Call [`PrinterModel::quirks()`](../models/index.md#printermodel) to get the
strategy implementation for any model.

Per-model strategy structs live in the [`models`](../models/index.md) submodule. This module also provides
shared helpers like [`fan_step_to_percentage()`](#fan-step-to-percentage) and [`FanSpeedDebouncer`](#fanspeeddebouncer) for dealing
with the low-resolution PWM fan telemetry common across most models.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`models`](models/index.md) | mod | # Model-Specific Kinematic and Operational Configuration Submodules |
| [`FanSpeedDebouncer`](#fanspeeddebouncer) | struct | Filters out transient quantization oscillation artifacts emitted by physical fan controllers. |
| [`ModelQuirks`](#modelquirks) | trait | Polymorphic interface tracking model-specific hardware variations and transport exceptions. |
| [`decode_fan_percentage`](#decode-fan-percentage) | fn | Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage via [`fan_step_to_percentage()`](#fan-step-to-percentage). |
| [`fan_step_to_percentage`](#fan-step-to-percentage) | fn | Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS]. |

## Modules

- [`models`](models/index.md) — # Model-Specific Kinematic and Operational Configuration Submodules


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

  Allows large transitions (greater than 1 step or ~7% diff) to commit immediately
  to maintain user responsiveness, while locking single-step toggles until they persist
  for at least 3 consecutive frames.

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

  This is a firmware bug workaround, not a real protocol ceiling — see the
  doc comments on `P2Quirks`/`X2Quirks` (the only two implementers returning
  `true`) for per-model evidence and confidence level.

- `fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool`

  Evaluates whether the physical front enclosure door is open based on model-specific sensor routing [REF-NET-DOOR].

  If the target model lacks an electronic door sensor switch, returns `false`.

- `fn has_door_sensor(&self) -> bool`

  Returns true if the physical machine chassis is equipped with an electronic front enclosure door open sensor switch.

- `fn camera_protocol(&self) -> CameraProtocol`

  Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].

- `fn ignores_chamber_temperature(&self) -> bool`

  Returns true if the model is an open-frame or entry-level machine lacking a physical chamber temperature sensor [REF-THER-DECODE].

- `fn has_stg_cur_idle_bug(&self) -> bool`

  Returns true if the model series exhibits the idle state-machine bug where `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].

- `fn physical_nozzle_count(&self) -> u8`

  Returns the number of physical extruder carriages present on the machine carriage bus.

  * `1` for standard single-nozzle configurations.
  * `2` for independent dual-extruder (IDEX) platforms.
  * `7` for automatic tool changer storage racks (1 dedicated + 6 interchangeable).

- `fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition`

  Returns this model's physical AMS unit pool structure — whether standard AMS
  and AMS-HT units share one combined pool or draw from independent pools, and each
  pool's unit-count ceiling. Confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row.
  See `crate::ams::AmsPoolComposition`'s doc comment for the AMS-lite modeling
  limitation.

- `fn supports_nozzle_offset_calibration(&self) -> bool`

  Returns true if the model supports electronic alignment and nozzle offset calibration sweeps.

- `fn is_bed_on_z(&self) -> bool`

  Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].

- `fn z_max(&self) -> f32`

  Returns the maximum safe Z-axis travel distance in millimeters for this model.

- `fn x_max(&self) -> f32`

  Returns the maximum safe X-axis travel distance in millimeters for this model.

- `fn y_max(&self) -> f32`

  Returns the maximum safe Y-axis travel distance in millimeters for this model.

- `fn nozzle_temp_max(&self) -> u16`

  Returns the maximum safe nozzle/hotend temperature in °C for this model.

- `fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16`

  Returns the maximum safe heated bed temperature in °C for this model.

  `mains_220v` is `Some(true)`/`Some(false)` when the printer's mains voltage region is
  known (from `PrinterTelemetry::is_220v_power()`, derived from `home_flag` bit 3), or
  `None` before any `home_flag` telemetry has been received. Every model except X1C ignores
  this parameter and returns a flat constant — see `X1CQuirks::bed_temp_max` for the one
  model where the ceiling is genuinely voltage-dependent per the official spec sheet
  ("Max Build Plate Temperature: 110°C @220V, 120°C @110V").

- `fn active_chamber_heater_max_temp_c(&self) -> Option<u16>`

  Returns this model's active PTC chamber heater ceiling in °C (M141), or `None` if it
  has no active chamber heater [REF-MOTO-GCODE].

  Supported on: X1E, X2D, H2S, H2D, H2D Pro, H2C. Combining "has an active heater" and
  "its max temp" into one `Option`-returning method (rather than two separate methods,
  one of which used to default to `0`) makes the two facts impossible to state
  inconsistently — no trait default means every implementor must supply both together,
  so a future model can't set "has heater" true while silently inheriting a stale/absent
  max temp.

#### Provided Methods 

- `fn has_door_sensor_field(&self, _telemetry: &PrinterTelemetry) -> bool`

  Returns true if `telemetry` carries the specific wire field this model's
  [`is_door_open()`](#modelquirks) actually reads (`home_flag` for X1 series,
  `stat` for H2/P2/X2 series) [REF-NET-DOOR].

  Used to gate telemetry-cache updates (`PrinterClient::update_state_cache`) so an
  incremental message that omits this field doesn't overwrite a previously-observed
  door state with `is_door_open()`'s absent-field default of `false`.
  Defaults to `false`, correct for every model without a door sensor.

- `fn is_unsafe_homing_command(&self, gcode: &str) -> bool`

  Evaluates if a given G-code command carries unsafe axis-constrained homing directions [REF-MOTO-GCODE].

  Default: bed-on-Z models reject G28 with axis constraints (Z, X, or Y) to prevent
  nozzle-to-plate collisions. Bed-slingers allow all homing variants.

  Scans every line of `gcode` independently — multi-statement `\n`-joined payloads are a
  documented, supported wire shape (see `GCodeRequest`) — and recognizes `G28` as a
  case-insensitive prefix match on a line rather than requiring it to be the entire leading
  whitespace-split token, so glued forms like `G28X` (no space before the axis letter) are
  caught too, alongside the already-handled space-separated form (`G28 X`).

- `fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String`

  Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].

  Evaluates travel limits specific to Bed-Slinger or CoreXY build envelopes. Returns an empty
  string if commanded relative distances exceed mechanical bounds.

- `fn relative_xy_move_gcode(&self, axis: char, distance: f32, feedrate: u32) -> String`

  Generates a bounded relative X/Y-axis movement G-code command — the same
  single-command distance-cap pattern `relative_z_move_gcode` uses for Z (see its doc
  comment for why this isn't true position-aware crash prevention). Returns an empty
  string if `distance` is zero, non-finite, exceeds the axis's `x_max()`/`y_max()` bound,
  or `axis` is neither `'X'` nor `'Y'`.

- `fn requires_wallclock_rtsp_timestamps(&self) -> bool`

  Returns true if the model's RTSP camera stream requires wallclock timestamps instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].

- `fn supports_auxiliary_left2_fan(&self) -> bool`

  Returns true if the model has a second left-side auxiliary fan (port 10, wire-labeled
  "right" but confirmed a left-side fan — see `FanTarget::AuxiliaryLeft2`'s doc comment,
  issue #60) [REF-CLIM-FANS].

- `fn supports_auxiliary_left_fan(&self) -> bool`

  Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].

  Universal default: only A1, A1 Mini, A2L (open-frame bed-slingers lacking this fan)
  and P1P (`MODEL_MATRIX.csv` lists it `Optional`, not guaranteed present) override
  this to `false`.

- `fn has_chamber_exhaust_fan(&self) -> bool`

  Returns true if the model has a chamber exhaust/filtration fan (port 3) [REF-CLIM-FANS].

  Supported on: H2S, H2D, H2D Pro, H2C, X2D.

- `fn supports_airduct_mode(&self) -> bool`

  Returns true if the model has controllable airduct dampers for climate mode switching (cooling vs heating recirculation) [REF-CLIM-FANS].

  Supported on: H2S, H2D, H2D Pro, H2C, P2S, X2D.

- `fn supports_prompt_sound(&self) -> bool`

  Returns true if the model has onboard speakers for prompt sound notifications.

  Supported on: A1, A1 Mini, A2L (confirmed by Bambu Studio profiles).

- `fn supports_buzzer(&self) -> bool`

  Returns true if the model has a physical fire alarm buzzer module.

  Supported on: H2S, H2D, H2D Pro, H2C (confirmed by pybambu).

- `fn supports_ams_remote_drying(&self) -> bool`

  Returns true if `ams_filament_drying` sent over MQTT is actually honored by the host
  printer's firmware, rather than acked `result: success` and silently discarded.

  Default `true` (AMS 2 Pro / AMS-HT drying is remote-controllable on every other host).
  `false` on P1P/P1S: confirmed by Bambu's own P1 manual ("P1S connected AMS drying
  functions may only be controlled from the P1S screen"), by bambuddy (`fix(drying)`,
  #2533 — reporter saw `dry_status` stay `0` after three acked commands), and by direct
  hardware testing against this crate's `start_drying()` on a P1S.

- `fn supports_vibration_compensation(&self) -> bool`

  Returns true if the model runs vibration-compensation (resonance) calibration as part of a print job.

  Default `true` (the X1/P1 series and everything modelled after them). `false` on P2S,
  where `vibration_cali` must be forced off in the `project_file` payload regardless of
  what the caller asked for.

  **This one rests on upstream authority alone, unlike its neighbours.** BambuStudio has
  no per-model vibration capability flag to consult — its printer profiles carry 30+
  `support_*` keys and none concerns vibration, and its own calibration checkbox is
  ungated by model. The sole source is bambuddy `be18ebb3` ("Fix P2S printer support —
  disable vibration_cali and fix FTP SSL"), a single community commit whose *other* half
  is the P2S FTPS TLS-1.3 quirk this crate independently confirmed and implements in
  `models::p2::P2Quirks`. That makes the contributor demonstrably right about the same
  machine, which is corroboration of the source, not proof of this claim.

  No P2S has been available to verify it here. See issue #133 — if one ever is, confirm
  before treating this as settled.

- `fn uses_nozzle_rack(&self) -> bool`

  Returns true if the model mounts its hotends from a swappable tool-changer rack.

  Only the H2C. A rack model addresses nozzles by *physical ID* rather than by extruder
  index, and the two namespaces overlap in a way that makes an untranslated value silently
  wrong rather than obviously wrong — see
  [`crate::mqtt::resolve_rack_nozzle_mapping`](../mqtt/index.md) for the translation and why it matters.

#### Implementors

- [`A1MiniQuirks`](models/a1/index.md#a1miniquirks)
- [`A1Quirks`](models/a1/index.md#a1quirks)
- [`A2LQuirks`](models/a2/index.md#a2lquirks)
- [`H2CQuirks`](models/h2/index.md#h2cquirks)
- [`H2DProQuirks`](models/h2/index.md#h2dproquirks)
- [`H2DQuirks`](models/h2/index.md#h2dquirks)
- [`H2SQuirks`](models/h2/index.md#h2squirks)
- [`P1PQuirks`](models/p1/index.md#p1pquirks)
- [`P1SQuirks`](models/p1/index.md#p1squirks)
- [`P2Quirks`](models/p2/index.md#p2quirks)
- [`UnknownQuirks`](models/unknown/index.md#unknownquirks)
- [`X1CQuirks`](models/x1/index.md#x1cquirks)
- [`X1EQuirks`](models/x1/index.md#x1equirks)
- [`X2Quirks`](models/x2/index.md#x2quirks)


---

## Functions

### `decode_fan_percentage`

```rust
fn decode_fan_percentage(raw: Option<&str>) -> Option<u8>
```

Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage via [`fan_step_to_percentage()`](#fan-step-to-percentage).
Returns `None` if `raw` is absent or not a valid `u8`.

### `fan_step_to_percentage`

```rust
fn fan_step_to_percentage(step: u8) -> u8
```

Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].

Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.

