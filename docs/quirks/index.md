*[bambino](../index.md) / [quirks](index.md)*

---

# Module `quirks`

# Model-Specific Quirks

Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
scattering `match model { ... }` blocks everywhere, [`ModelQuirks`](#modelquirks) captures all
model-specific behavior in one place. Call [`PrinterModel::quirks()`](../models/index.md#printermodel) to get any model's row.

The per-model rows live in the [`models`](../models/index.md) submodule, as data: one `const` per model. This
module also provides shared helpers like [`fan_step_to_percentage()`](#fan-step-to-percentage) and
[`FanSpeedDebouncer`](#fanspeeddebouncer) for dealing with the low-resolution PWM fan telemetry common across most
models.

## Contents

- [Modules](#modules)
  - [`context`](context/index.md)
  - [`models`](models/index.md)
- [Types](#types)
  - [`BuildVolume`](#buildvolume)
  - [`FanSpeedDebouncer`](#fanspeeddebouncer)
  - [`ModelQuirks`](#modelquirks)
  - [`DoorSensor`](#doorsensor)
  - [`NozzleLayout`](#nozzlelayout)
  - [`Support`](#support)
- [Functions](#functions)
  - [`decode_fan_percentage`](#decode-fan-percentage)
  - [`fan_step_to_percentage`](#fan-step-to-percentage)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`context`](context/index.md) | mod | # Quirk Context |
| [`models`](models/index.md) | mod | # Model-Specific Kinematic and Operational Configuration Submodules |
| [`BuildVolume`](#buildvolume) | struct | A model's maximum safe travel per axis, in millimeters. |
| [`FanSpeedDebouncer`](#fanspeeddebouncer) | struct | Filters out transient quantization oscillation artifacts emitted by physical fan controllers. |
| [`ModelQuirks`](#modelquirks) | struct | One printer model's hardware variations and transport exceptions, as data. |
| [`DoorSensor`](#doorsensor) | enum | Where a model reports its front door state, if it has a door sensor at all [REF-NET-DOOR]. |
| [`NozzleLayout`](#nozzlelayout) | enum | How a model's hotends are arranged. |
| [`Support`](#support) | enum | How a capability answer was reached — the printer's own report, an inference, or a default. |
| [`decode_fan_percentage`](#decode-fan-percentage) | fn | Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage via [`fan_step_to_percentage()`](#fan-step-to-percentage). |
| [`fan_step_to_percentage`](#fan-step-to-percentage) | fn | Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS]. |

## Modules

- [`context`](context/index.md) — # Quirk Context
- [`models`](models/index.md) — # Model-Specific Kinematic and Operational Configuration Submodules


---

## Types

### `QuirkContext<'a>`

```rust
struct QuirkContext<'a> {
    pub fun2: Option<&'a str>,
    pub firmware: Option<&'a str>,
}
```

Wire-derived inputs a quirk may consult, all optional.

See the [module docs](self) for why these are passed in rather than composed at the call
site.

#### Fields

- **`fun2`**: `Option<&'a str>`

  The `fun2` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely, so a quirk that prefers a `fun2` bit is inert
  on those models and must still carry a sound model default. Single-source: only
  BambuStudio reads this field.

- **`firmware`**: `Option<&'a str>`

  The printer's OTA firmware version (`module[name="ota"].sw_ver`, see [`OTA_MODULE_NAME`](../types/version/index.md#ota-module-name), e.g. `"01.09.00.00"`),
  if a `get_version` response has been seen.
  
  Several capabilities ship in a specific firmware release rather than being inherent to
  the model — remote AMS drying is version-gated on H2D, H2D Pro, H2S, H2C, P2S and X2D for
  exactly this reason.

#### Implementations

- <span id="quirkcontext-empty"></span>`fn empty() -> Self`

  An empty context — every input absent, so every quirk falls back to its model default.

  Use when no telemetry has been seen, or to ask what a model claims about itself before
  any report has arrived.

- <span id="quirkcontext-with-fun2"></span>`fn with_fun2(self, fun2: Option<&'a str>) -> Self`

  Sets the `fun2` capability bitfield.

- <span id="quirkcontext-with-firmware"></span>`fn with_firmware(self, firmware: Option<&'a str>) -> Self`

  Sets the OTA firmware version.

#### Trait Implementations

##### `impl Clone for QuirkContext<'a>`

- <span id="quirkcontext-clone"></span>`fn clone(&self) -> QuirkContext<'a>` — [`QuirkContext`](context/index.md#quirkcontext)

##### `impl Copy for QuirkContext<'a>`

##### `impl Debug for QuirkContext<'a>`

- <span id="quirkcontext-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for QuirkContext<'a>`

- <span id="quirkcontext-default"></span>`fn default() -> QuirkContext<'a>` — [`QuirkContext`](context/index.md#quirkcontext)

### `BuildVolume`

```rust
struct BuildVolume {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
```

A model's maximum safe travel per axis, in millimeters.

#### Fields

- **`x`**: `f32`

  X-axis travel ceiling.

- **`y`**: `f32`

  Y-axis travel ceiling.

- **`z`**: `f32`

  Z-axis travel ceiling.

#### Trait Implementations

##### `impl Clone for BuildVolume`

- <span id="buildvolume-clone"></span>`fn clone(&self) -> BuildVolume` — [`BuildVolume`](#buildvolume)

##### `impl Copy for BuildVolume`

##### `impl Debug for BuildVolume`

- <span id="buildvolume-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl PartialEq for BuildVolume`

- <span id="buildvolume-partialeq-eq"></span>`fn eq(&self, other: &BuildVolume) -> bool` — [`BuildVolume`](#buildvolume)

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

### `ModelQuirks`

```rust
struct ModelQuirks {
    // [REDACTED: Private Fields]
}
```

One printer model's hardware variations and transport exceptions, as data.

Get one from [`PrinterModel::quirks()`](../models/index.md#printermodel). Every model is one `const` row in
[`models`](../models/index.md), built from `ModelQuirks::new` (which demands the safety limits,
camera protocol, AMS layout and drying rule) plus overrides of the defaulted fields. Fields
are private: a row is a claim about real hardware, so only this crate writes one.

Method names follow one convention: `has_*` for physical hardware present on the machine,
`supports_*` for a firmware feature, and `uses_*`/`requires_*` for protocol behavior the client
must adapt to.

#### Implementations

- <span id="modelquirks-uses-plaintext-ftps-data-channel"></span>`fn uses_plaintext_ftps_data_channel(&self) -> bool`

  Returns true if this model requires a plaintext FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].

- <span id="modelquirks-requires-ftps-tls-1-2"></span>`fn requires_ftps_tls_1_2(&self) -> bool`

  Returns true if this model's FTPS server must be reached over TLS 1.2 or lower [REF-FTPS-CONN].

  This is a firmware bug workaround, not a real protocol ceiling. Both caps (P2S and X2D) are
  **confirmed by symptom with no confirmed mechanism** — each reporter saw the failure
  clear when the cap was applied, but neither root cause has been traced, and the X2D's
  original explanation has since been measured wrong. A cap costs nothing on a printer that
  never offers TLS 1.3, so both are kept. See the `P2S` and `X2D` rows in
  `src/quirks/models/` for per-model evidence.

- <span id="modelquirks-ftps-tls-versions"></span>`fn ftps_tls_versions(&self) -> crate::io::TlsVersions` — [`TlsVersions`](../io/index.md#tlsversions)

  Returns the TLS versions an FTPS connector for this model may offer — see [`Self::requires_ftps_tls_1_2`](#modelquirks).

- <span id="modelquirks-door-sensor"></span>`fn door_sensor(&self) -> DoorSensor` — [`DoorSensor`](#doorsensor)

  Returns where this model reports its door state, or [`DoorSensor::None`](#doorsensor) without a door sensor.

  Read the state itself with
  `PrinterTelemetry::door_state`.

- <span id="modelquirks-camera-protocol"></span>`fn camera_protocol(&self) -> CameraProtocol` — [`CameraProtocol`](../camera/index.md#cameraprotocol)

  Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].

- <span id="modelquirks-has-chamber-temperature-sensor"></span>`fn has_chamber_temperature_sensor(&self) -> bool`

  Returns true if the model has a physical chamber temperature sensor [REF-THER-DECODE].

  False on open-frame and entry-level machines (A1, A1 Mini, A2L, P1P, P1S), whose reported
  chamber value is not a measurement.

- <span id="modelquirks-nozzle-layout"></span>`fn nozzle_layout(&self) -> NozzleLayout` — [`NozzleLayout`](#nozzlelayout)

  Returns how this model's hotends are arranged.

- <span id="modelquirks-physical-nozzle-count"></span>`fn physical_nozzle_count(&self) -> u8`

  Returns the number of physical hotends: `1` single, `2` IDEX, `7` on the H2C's rack (1 fixed + 6 interchangeable).

- <span id="modelquirks-has-nozzle-rack"></span>`fn has_nozzle_rack(&self) -> bool`

  Returns true if the model mounts its hotends from a swappable tool-changer rack (H2C) — see [`NozzleLayout::Rack`](#nozzlelayout).

- <span id="modelquirks-ams-pool-composition"></span>`fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition` — [`AmsPoolComposition`](../ams/mapping/index.md#amspoolcomposition)

  Returns this model's physical AMS unit pool structure.

  Whether standard AMS and AMS-HT units share one combined pool or draw from independent
  pools, each pool's unit-count ceiling, and whether an AMS Lite attaches alongside or
  instead of the shared pool. Confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row.
  Size a per-unit UI from
  `AmsPoolComposition::max_units`.

- <span id="modelquirks-supports-nozzle-offset-calibration"></span>`fn supports_nozzle_offset_calibration(&self) -> bool`

  Returns true if the model runs nozzle offset calibration — every model with more than one hotend.

- <span id="modelquirks-supports-heatbed-thermal-calibration"></span>`fn supports_heatbed_thermal_calibration(&self) -> bool`

  Returns true if the model runs heatbed leveling and thermal profile calibration (`calibration` option bit 5).

  False on every model so far. Observed inert on a P1S: the firmware accepts the bit,
  acknowledges the command `"result": "success"`, and queues no stage for it
  [REF-MQTT-LIFECYCLE]. Since the wire reports success either way, a model is assumed not to
  support this until a capture shows a stage queued for it: guessing wrong toward
  "unsupported" costs a rejected command rather than a silently skipped calibration.

- <span id="modelquirks-supported-calibration-mask"></span>`fn supported_calibration_mask(&self) -> u32`

  Returns the mask of `calibration` option bits this model actually executes [REF-MQTT-LIFECYCLE].

  Bits 1–3 (bed leveling, vibration compensation, motor noise) are supported everywhere
  observed. Bit 4 follows [`Self::supports_nozzle_offset_calibration`](#modelquirks) and bit 5 follows
  [`Self::supports_heatbed_thermal_calibration`](#modelquirks). Bits 0 and 6 are internal/undocumented
  and never included.

- <span id="modelquirks-is-bed-on-z"></span>`fn is_bed_on_z(&self) -> bool`

  Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].

- <span id="modelquirks-validate-gcode"></span>`fn validate_gcode(&self, gcode: &str, mains_220v: Option<bool>) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Checks raw G-code against this model's limits: what `PrinterClient::send_gcode` enforces.

  Rejects, with [`Error::ModelMismatch`]:

  - axis-constrained `G28` (Z, X or Y) on a bed-on-Z model, which risks a nozzle-to-plate
    collision; bed-slingers allow every homing variant [REF-MOTO-GCODE];
  - an `M104`/`M109` `S`/`R`/`B` above [`Self::nozzle_temp_max`](#modelquirks);
  - an `M140`/`M190` `S`/`R` above [`Self::bed_temp_max`](#modelquirks) for `mains_220v`;
  - any `M141`/`M191` on a model without an active chamber heater, and an `S`/`R` above
    [`Self::chamber_heater_temp_max`](#modelquirks) on one with a heater.

  A temperature argument that isn't a plain decimal (`S3e2`, `S0x1F`) is rejected with
  [`Error::InvalidArgument`](../error/index.md#error) rather than interpreted. Unlike the typed setters, nothing is
  clamped: the G-code is either sent as written or refused.

  Every statement is checked independently, splitting on `\n` and a bare `\r` —
  multi-statement payloads are a documented, supported wire shape (see `GCodeRequest`).
  Comments and a leading `M117` message are skipped; `G28X`, `G 28 Z` and `G028 Z` are all
  recognized as `G28`, since the firmware's parser is undocumented and the scan resolves
  every ambiguity toward rejecting.

  Relative moves are **not** bounded — the printer reports no absolute position, so there is
  nothing to bound them against; `move_relative` caps a single move's distance instead.

- <span id="modelquirks-build-volume"></span>`fn build_volume(&self) -> BuildVolume` — [`BuildVolume`](#buildvolume)

  Returns this model's maximum safe travel per axis.

- <span id="modelquirks-relative-z-move-gcode"></span>`fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String`

  Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].

  Returns an empty string if `distance` exceeds the model's Z travel.

- <span id="modelquirks-relative-xy-move-gcode"></span>`fn relative_xy_move_gcode(&self, axis: char, distance: f32, feedrate: u32) -> String`

  Generates a bounded relative X/Y-axis movement G-code command.

  The same single-command distance cap `relative_z_move_gcode` applies to Z (see
  `format_z_move_gcode` for why this isn't true position-aware crash prevention). Returns an
  empty string if `distance` is zero, non-finite, exceeds the axis's travel, or `axis` is
  neither `'X'` nor `'Y'`.

- <span id="modelquirks-requires-wallclock-rtsp-timestamps"></span>`fn requires_wallclock_rtsp_timestamps(&self) -> bool`

  Returns true if the model's RTSP camera stream requires wallclock timestamps instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].

- <span id="modelquirks-has-auxiliary-left2-fan"></span>`fn has_auxiliary_left2_fan(&self) -> bool`

  Returns true if the model has a second left-side auxiliary fan (port 10) [REF-CLIM-FANS].

  Wire-labeled "right" but confirmed a left-side fan — see `FanTarget::AuxiliaryLeft2`'s doc
  comment, issue #60.

- <span id="modelquirks-has-auxiliary-left-fan"></span>`fn has_auxiliary_left_fan(&self) -> bool`

  Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].

  Every model except A1, A1 Mini, A2L (open-frame bed-slingers lacking this fan), P1P
  (`MODEL_MATRIX.csv` lists it `Optional`, not guaranteed present) and the plain X1
  (BambuStudio's X1 profile sets `auxiliary_fan` to `0`).

- <span id="modelquirks-has-chamber-exhaust-fan"></span>`fn has_chamber_exhaust_fan(&self) -> bool`

  Returns true if the model has a chamber exhaust/filtration fan (port 3): H2S, H2D, H2D Pro, H2C, X2D [REF-CLIM-FANS].

- <span id="modelquirks-supports-airduct-mode"></span>`fn supports_airduct_mode(&self) -> bool`

  Returns true if the model switches climate modes with airduct dampers: H2S, H2D, H2D Pro, H2C, P2S, X2D [REF-CLIM-FANS].

- <span id="modelquirks-supports-prompt-sound"></span>`fn supports_prompt_sound(&self) -> bool`

  Returns true if the model plays prompt sound notifications: A1, A1 Mini, A2L (per Bambu Studio profiles).

- <span id="modelquirks-has-buzzer"></span>`fn has_buzzer(&self) -> bool`

  Returns true if the model has a fire alarm buzzer module: H2S, H2D, H2D Pro, H2C (per pybambu).

- <span id="modelquirks-ams-remote-drying-support"></span>`fn ams_remote_drying_support(&self, ctx: &QuirkContext<'_>) -> Support` — [`QuirkContext`](context/index.md#quirkcontext), [`Support`](#support)

  Returns whether `ams_filament_drying` sent over MQTT is honored by this printer, with its provenance.

  "Honored" rather than "accepted": an unsupported printer acks `result: success` and
  silently discards the command.

  Resolved in two stages. First, `ctx.fun2` bit 5 — the printer's own answer
  (`DeviceManager.cpp:4469`) — wins where it is present, in both directions, since a
  per-model rule is a claim about every unit of that model while `fun2` is the machine in
  front of you speaking. Second, when `fun2` is absent, the model's rule decides.

  **The second stage is the one that usually runs.** Only BambuStudio reads `fun2` at all,
  and the P1 and A1 families send no `fun2` (`reference/03_mqtt_telemetry.md`), so on a
  large share of real hardware the reported bit never appears.

  Model rules, sourced from Bambu Lab's per-model firmware release histories and its
  *Filament drying guide for AMS 2 Pro and AMS HT* wiki page (thresholds and rejected values
  tabulated in `reference/05_materials_ams.md` §5.4). BambuStudio has no model rule of its
  own — its `is_support_remote_dry` is a bare `false` initializer that only `fun2` ever sets:

  * **A1 / A1 Mini — never.** Not a hardware limit: these models take AMS 2 Pro and AMS-HT
    units from the shared pool. No known firmware path exposes a remote-dry command; the
    drying guide lists them as "not supported yet", and bambuddy lists them in
    `_DRYING_UNSUPPORTED_MODELS`. The gate is about the command channel, not the attachable
    hardware.
  * **P1P / P1S — never.** The AMS can dry, but only from the printer's own screen: P1
    `01.08.00.00` (2025-04-29) says drying starts "from the printer's screen", no later P1
    release adds remote drying, and the drying guide names both as unsupported. Bambu's P1
    manual agrees ("P1S connected AMS drying functions may only be controlled from the P1S
    screen"), bambuddy lists them in `_DRYING_SCREEN_ONLY_MODELS` citing its #2533, and this
    crate's own drying command was tested against a P1S directly.
  * **X1, X1C — never.** The drying guide names the X1C alongside P1 and A1 as "not supported
    yet"; X1 `01.09.00.00` (2025-04-29) carries the same screen-only sentence as P1
    `01.08.00.00`, and no X1/X1C release through `01.12.00.00` mentions remote drying.
  * **H2D, H2D Pro, H2S, H2C, P2S, X2D — firmware-gated.** The capability shipped in a
    specific release; see each model's constant for the version and its release history.
  * **A2L — always.** Its earliest published release, `01.01.00.00`, already has it.
  * **Everything else (X1E, unrecognized models) — assumed allowed.** No vendor source states
    either way; the printer answers `result: "fail"` if it can't.

  Takes a [`QuirkContext`](context/index.md#quirkcontext) so there is one answer to this question and not two that can
  disagree — the failure #240 fixed. Prefer
  [`PrinterClient::capabilities`](../client/index.md#printerclient), which builds
  the context from cached telemetry for you.

- <span id="modelquirks-ams-drying-while-printing-support"></span>`fn ams_drying_while_printing_support(&self, ctx: &QuirkContext<'_>) -> Support` — [`QuirkContext`](context/index.md#quirkcontext), [`Support`](#support)

  Returns whether an AMS drying cycle can run while a print is in progress, with its provenance.

  A separate, strictly narrower capability than
  [`ams_remote_drying_support`](#modelquirks). During a print the firmware
  lowers the drying temperature below the printed filament's softening point; this crate does
  not reimplement that clamp. On an unsupported printer the firmware refuses mid-print with
  `dry_sf_reason` `0`.

  Sourced from the drying guide's "Introduction to Simultaneous Drying and Printing Function"
  list plus each model's release history ("Added support for printing while filament is
  drying" / "Print While Drying"). **Defaults to deny**, unlike idle remote drying. No `fun2`
  bit reports it, but a reported *clear* remote-dry bit refuses it.

- <span id="modelquirks-nozzle-temp-max"></span>`fn nozzle_temp_max(&self) -> u16`

  Returns the maximum safe nozzle/hotend temperature in °C for this model.

- <span id="modelquirks-bed-temp-max"></span>`fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16`

  Returns the maximum safe heated bed temperature in °C for this model.

  `mains_220v` is `Some(true)`/`Some(false)` when the printer's mains voltage region is
  known (from `PrinterTelemetry::is_220v_power()`, derived from `home_flag` bit 3), or
  `None` before any `home_flag` telemetry has been received. Only the X1 and X1C have a
  voltage-dependent ceiling, per the official spec sheet ("Max Build Plate Temperature:
  110°C @220V, 120°C @110V"); with the region unknown they return the lower one. Every
  other model ignores the parameter.

- <span id="modelquirks-chamber-heater-temp-max"></span>`fn chamber_heater_temp_max(&self) -> Option<u16>`

  Returns this model's active chamber heater ceiling in °C (M141), or `None` if it has no active chamber heater [REF-MOTO-GCODE].

  Supported on: X1E, X2D, H2S, H2D, H2D Pro, H2C. One `Option` rather than a "has heater"
  flag plus a ceiling, so the two facts can't be stated inconsistently.

#### Trait Implementations

##### `impl Clone for ModelQuirks`

- <span id="modelquirks-clone"></span>`fn clone(&self) -> ModelQuirks` — [`ModelQuirks`](#modelquirks)

##### `impl Copy for ModelQuirks`

##### `impl Debug for ModelQuirks`

- <span id="modelquirks-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `DoorSensor`

```rust
enum DoorSensor {
    None,
    HomeFlag,
    Stat,
}
```

Where a model reports its front door state, if it has a door sensor at all [REF-NET-DOOR].

#### Variants

- **`None`**

  No electronic door sensor, so no door state is ever reported.

- **`HomeFlag`**

  `home_flag` bit 23 (X1 series) — see
  `PrinterTelemetry::is_door_open_from_home_flag`.

- **`Stat`**

  `stat` (H2, P2S and X2D series) — see
  `PrinterTelemetry::is_door_open_from_stat`.

#### Trait Implementations

##### `impl Clone for DoorSensor`

- <span id="doorsensor-clone"></span>`fn clone(&self) -> DoorSensor` — [`DoorSensor`](#doorsensor)

##### `impl Copy for DoorSensor`

##### `impl Debug for DoorSensor`

- <span id="doorsensor-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DoorSensor`

##### `impl PartialEq for DoorSensor`

- <span id="doorsensor-partialeq-eq"></span>`fn eq(&self, other: &DoorSensor) -> bool` — [`DoorSensor`](#doorsensor)

### `NozzleLayout`

```rust
enum NozzleLayout {
    Single,
    Dual,
    Rack {
        nozzles: u8,
    },
}
```

How a model's hotends are arranged.

#### Variants

- **`Single`**

  One hotend.

- **`Dual`**

  Two hotends on independent carriages (IDEX).

- **`Rack`**

  Hotends mounted from a swappable tool-changer rack (H2C: 1 fixed + 6 rack hotends).
  
  A rack model addresses nozzles by *physical ID* rather than by extruder index, and the two
  namespaces overlap in a way that makes an untranslated value silently wrong rather than
  obviously wrong — see [`resolve_rack_nozzle_mapping`](../mqtt/commands/print_job/index.md#resolve-rack-nozzle-mapping).

#### Implementations

- <span id="nozzlelayout-nozzle-count"></span>`fn nozzle_count(self) -> u8`

  Returns the number of physical hotends, rack slots included.

#### Trait Implementations

##### `impl Clone for NozzleLayout`

- <span id="nozzlelayout-clone"></span>`fn clone(&self) -> NozzleLayout` — [`NozzleLayout`](#nozzlelayout)

##### `impl Copy for NozzleLayout`

##### `impl Debug for NozzleLayout`

- <span id="nozzlelayout-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for NozzleLayout`

##### `impl PartialEq for NozzleLayout`

- <span id="nozzlelayout-partialeq-eq"></span>`fn eq(&self, other: &NozzleLayout) -> bool` — [`NozzleLayout`](#nozzlelayout)

### `Support`

```rust
enum Support {
    Reported(bool),
    Inferred(bool),
    Assumed(bool),
}
```

How a capability answer was reached — the printer's own report, an inference, or a default.

A plain `bool` collapses "this X1C reported the bit clear" and "no telemetry has arrived, so
yes was assumed" into the same value, though they warrant opposite handling: the first is
settled, the second means "ask again once connected". [`is_supported`](#support)
collapses back to that `bool` for callers that don't need the distinction.

Only capabilities that resolve a reported value against model rules against a default carry
this type. Static model facts (build volume, camera protocol, …) have no provenance question.

#### Variants

- **`Reported`**

  The printer said so itself, e.g. through a `fun2` capability bit.

- **`Inferred`**

  Derived from what is known about this printer without it saying so: its firmware version
  against a documented threshold, or a documented rule for its model.

- **`Assumed`**

  Nothing about this printer settles it, so this is the capability's default.

#### Implementations

- <span id="support-is-supported"></span>`fn is_supported(self) -> bool`

  The answer with its provenance discarded.

#### Trait Implementations

##### `impl Clone for Support`

- <span id="support-clone"></span>`fn clone(&self) -> Support` — [`Support`](#support)

##### `impl Copy for Support`

##### `impl Debug for Support`

- <span id="support-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for Support`

##### `impl PartialEq for Support`

- <span id="support-partialeq-eq"></span>`fn eq(&self, other: &Support) -> bool` — [`Support`](#support)


---

## Functions

### `firmware_at_least`

```rust
fn firmware_at_least(have: &str, want: &str) -> bool
```

Compares two Bambu firmware version strings, returning true if `have` is at least `want`.

Versions are dotted numeric quads (`"01.09.00.00"`). Compared component-wise as integers
rather than lexicographically: upstream's zero-padded strings happen to sort correctly as
text, but a single unpadded component (`"1.9.0.0"`) would silently compare wrong, and nothing
guarantees the padding.

Missing trailing components read as `0`, so `"01.09"` and `"01.09.00.00"` are equal. A
component that isn't a number makes the whole comparison `false` — an unparseable version
cannot be shown to meet a minimum, and claiming a capability on a string we failed to read is
the wrong direction to fail.

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

