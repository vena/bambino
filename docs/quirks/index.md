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

## Contents

- [Modules](#modules)
  - [`context`](context/index.md)
  - [`models`](models/index.md)
- [Types](#types)
  - [`FanSpeedDebouncer`](#fanspeeddebouncer)
  - [`Support`](#support)
- [Traits](#traits)
  - [`ModelQuirks`](#modelquirks)
- [Functions](#functions)
  - [`decode_fan_percentage`](#decode-fan-percentage)
  - [`fan_step_to_percentage`](#fan-step-to-percentage)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`context`](context/index.md) | mod | # Quirk Context |
| [`models`](models/index.md) | mod | # Model-Specific Kinematic and Operational Configuration Submodules |
| [`FanSpeedDebouncer`](#fanspeeddebouncer) | struct | Filters out transient quantization oscillation artifacts emitted by physical fan controllers. |
| [`Support`](#support) | enum | How a capability answer was reached — the printer's own report, an inference, or a default. |
| [`ModelQuirks`](#modelquirks) | trait | Polymorphic interface tracking model-specific hardware variations and transport exceptions. |
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
    pub fun: Option<&'a str>,
    pub fun2: Option<&'a str>,
    pub firmware: Option<&'a str>,
    pub telemetry: Option<&'a crate::types::PrinterTelemetry>,
}
```

Wire-derived inputs a quirk may consult, all optional.

See the [module docs](self) for why these are passed in rather than composed at the call
site.

#### Fields

- **`fun`**: `Option<&'a str>`

  The `fun` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely. Bit 29 is Developer LAN Mode; BambuStudio
  reads a dozen more (`DeviceManager.cpp:4433-4455`) that this crate does not yet.

- **`fun2`**: `Option<&'a str>`

  The `fun2` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely, so a quirk that prefers a `fun2` bit is inert
  on those models and must still carry a sound model default. Single-source: only
  BambuStudio reads this field.

- **`firmware`**: `Option<&'a str>`

  The printer's OTA firmware version (`module[name="ota"].sw_ver`, e.g. `"01.09.00.00"`),
  if a `get_version` response has been seen.
  
  Several capabilities ship in a specific firmware release rather than being inherent to
  the model — remote AMS drying is version-gated on H2D, H2D Pro, H2S, H2C, P2S and X2D for
  exactly this reason.

- **`telemetry`**: `Option<&'a crate::types::PrinterTelemetry>`

  The most recent `print` telemetry object, for quirks that read a live state field.
  
  Distinct from the capability fields above: this is machine *state*
  ([`is_door_open`](#modelquirks) reads a door bit that flips as
  someone opens the door), not a capability claim.

#### Implementations

- <span id="quirkcontext-empty"></span>`fn empty() -> Self`

  An empty context — every input absent, so every quirk falls back to its model default.

  Use when no telemetry has been seen, or to ask what a model claims about itself before
  any report has arrived.

- <span id="quirkcontext-with-fun"></span>`fn with_fun(self, fun: Option<&'a str>) -> Self`

  Sets the `fun` capability bitfield.

- <span id="quirkcontext-with-fun2"></span>`fn with_fun2(self, fun2: Option<&'a str>) -> Self`

  Sets the `fun2` capability bitfield.

- <span id="quirkcontext-with-firmware"></span>`fn with_firmware(self, firmware: Option<&'a str>) -> Self`

  Sets the OTA firmware version.

- <span id="quirkcontext-with-telemetry"></span>`fn with_telemetry(self, telemetry: Option<&'a PrinterTelemetry>) -> Self` — [`PrinterTelemetry`](../types/telemetry/report/index.md#printertelemetry)

  Sets the live `print` telemetry object.

#### Trait Implementations

##### `impl Clone for QuirkContext<'a>`

- <span id="quirkcontext-clone"></span>`fn clone(&self) -> QuirkContext<'a>` — [`QuirkContext`](context/index.md#quirkcontext)

##### `impl Copy for QuirkContext<'a>`

##### `impl Debug for QuirkContext<'a>`

- <span id="quirkcontext-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for QuirkContext<'a>`

- <span id="quirkcontext-default"></span>`fn default() -> QuirkContext<'a>` — [`QuirkContext`](context/index.md#quirkcontext)

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
this type. Static model facts (`z_max`, camera protocol, …) have no provenance question.

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

  Returns true if this model series must restrict its TLS version strictly to TLS 1.2 [REF-FTPS-CONN].

  This is a firmware bug workaround, not a real protocol ceiling. Both caps are
  **confirmed by symptom with no confirmed mechanism** — each reporter saw the failure
  clear when the cap was applied, but neither root cause has been traced, and the X2D's
  original explanation has since been measured wrong. A cap costs nothing on a printer that
  never offers TLS 1.3, so both are kept. See the doc comments on `P2Quirks`/`X2Quirks`
  (the only two implementers returning `true`) for per-model evidence.

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
  pool's unit-count ceiling, and whether an AMS Lite attaches alongside or instead of the
  shared pool. Confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row. Size a
  per-unit UI from `AmsPoolComposition::max_units`.

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

- `fn supports_heatbed_thermal_calibration(&self) -> bool`

  Returns true if the model runs heatbed leveling and thermal profile calibration (`calibration` option bit 5).

  Default `false`. Observed inert on a P1S: the firmware accepts the bit, acknowledges the
  command `"result": "success"`, and queues no stage for it [REF-MQTT-LIFECYCLE]. Since the
  wire reports success either way, a model is assumed not to support this until a capture
  shows a stage queued for it — the same fail-safe direction as
  [`Self::has_stg_cur_idle_bug`](#modelquirks), where guessing wrong toward "unsupported" costs a
  rejected command rather than a silently skipped calibration.

- `fn supported_calibration_mask(&self) -> u32`

  Returns the mask of `calibration` option bits this model actually executes [REF-MQTT-LIFECYCLE].

  Bits 1–3 (bed leveling, vibration compensation, motor noise) are supported everywhere
  observed. Bit 4 follows [`Self::supports_nozzle_offset_calibration`](#modelquirks) and bit 5 follows
  [`Self::supports_heatbed_thermal_calibration`](#modelquirks). Bits 0 and 6 are internal/undocumented
  and never included.

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

- `fn supports_ams_remote_drying(&self, ctx: &QuirkContext<'_>) -> bool`

  Returns true if `ams_filament_drying` sent over MQTT is actually honored by the host
  printer's firmware, rather than acked `result: success` and silently discarded.

  Resolved in two stages. First, `ctx.fun2` bit 5 — the printer's own answer
  (`DeviceManager.cpp:4469`) — wins where it is present, since a per-model rule is a claim
  about every unit of that model while `fun2` is the machine in front of you speaking.
  Second, when `fun2` is absent, the model's own rule decides.

  **A1 and A1 Mini are the exception: they ignore the bit entirely.** Not because the
  hardware cannot be attached — both draw from the shared AMS pool alongside AMS 2 Pro and
  AMS-HT units (`reference/05_materials_ams.md`, `MODEL_MATRIX.csv`, and
  `A1Quirks::ams_pool_composition()` all agree) — but because no known firmware path on
  these models exposes a remote-dry command at all, and bambuddy lists them in
  `_DRYING_UNSUPPORTED_MODELS`. They are hard-coded rather than bit-driven because the
  families send neither `fun` nor `fun2`, so there is no bit to defer to in the first
  place. Every other model defers to a reported bit in both directions.

  Do not loosen this branch on the strength of "but the A1 takes an AMS-HT" — it does; the
  gate is about the command channel, not the attachable hardware.

  **The second stage is the one that usually runs.** Only BambuStudio reads `fun2` at all,
  and the P1 and A1 families send neither `fun` nor `fun2`
  (`reference/03_mqtt_telemetry.md`), so on a large share of real hardware the reported bit
  never appears. Treat the model rules as the primary mechanism, not a fallback.

  Model rules, sourced from Bambu Lab's per-model firmware release histories and its *Filament
  drying guide for AMS 2 Pro and AMS HT* wiki page (thresholds and rejected values tabulated
  in `reference/05_materials_ams.md` §5.4). BambuStudio has no model rule of its own — its
  `is_support_remote_dry` is a bare `false` initializer that only `fun2` ever sets:

  * **A1 / A1 Mini — never.** Not a hardware limit: these models do take AMS 2 Pro and
    AMS-HT units from the shared pool. The drying guide lists them as "not supported yet",
    and bambuddy lists them in `_DRYING_UNSUPPORTED_MODELS`.
  * **P1P / P1S — never.** The AMS can dry, but only from the printer's own screen: P1
    `01.08.00.00` (2025-04-29) says drying starts "from the printer's screen", no later P1
    release adds remote drying, and the drying guide names both as unsupported. Bambu's P1
    manual agrees ("P1S connected AMS drying functions may only be controlled from the P1S
    screen"), bambuddy lists them in `_DRYING_SCREEN_ONLY_MODELS` citing its #2533, and this
    crate's own drying command was tested against a P1S directly.
  * **X1C — never.** The drying guide names it alongside P1 and A1 as "not supported yet";
    X1 `01.09.00.00` (2025-04-29) carries the same screen-only sentence as P1 `01.08.00.00`,
    and no X1/X1C release through `01.12.00.00` mentions remote drying. Bambu Lab has stated
    the related dry-while-printing feature needs hardware the X1 Carbon lacks.
  * **H2D, H2D Pro, H2S, H2C, P2S, X2D — firmware-gated.** The capability shipped in a
    specific release; see each model's constant for the version and its release history.
  * **A2L — always.** Its earliest published release, `01.01.00.00`, already has it.
  * **Everything else (X1E, future models) — assumed allowed.** No vendor source states
    either way; the printer answers `result: "fail"` if it can't. Matches bambuddy's "all
    other models ... are allowed".

  Takes a [`QuirkContext`](context/index.md#quirkcontext) rather than letting callers compose the answer, so there is one
  answer to this question and not two that can disagree — the failure #240 fixed. Prefer
  [`PrinterClient::capabilities`](../client/index.md#printerclient), which builds the
  context from cached telemetry for you.

  Implementors override [`ams_remote_drying_support`](#modelquirks), not
  this method, so the two cannot disagree.

- `fn ams_remote_drying_support(&self, ctx: &QuirkContext<'_>) -> Support`

  Remote-drying support with its provenance attached.

  The same answer as [`supports_ams_remote_drying`](#modelquirks), plus
  whether the printer reported it, it was inferred from firmware or a model rule, or it is the
  default because nothing was known.

- `fn supports_ams_drying_while_printing(&self, ctx: &QuirkContext<'_>) -> bool`

  Returns true if an AMS drying cycle can run while a print is in progress.

  A separate, strictly narrower capability than
  [`supports_ams_remote_drying`](#modelquirks). During a print the
  firmware lowers the drying temperature below the printed filament's softening point; this
  crate does not reimplement that clamp. On an unsupported printer the firmware refuses
  mid-print with `dry_sf_reason` `0`.

  Sourced from the drying guide's "Introduction to Simultaneous Drying and Printing
  Function" list plus each model's release history ("Added support for printing while
  filament is drying" / "Print While Drying"). **Defaults to deny**, unlike idle remote
  drying — see `dry_while_printing_from_firmware` for why the asymmetry is deliberate.

  Implementors override
  [`ams_drying_while_printing_support`](#modelquirks).

- `fn ams_drying_while_printing_support(&self, ctx: &QuirkContext<'_>) -> Support`

  Drying-while-printing support with its provenance attached.

  The same answer as
  [`supports_ams_drying_while_printing`](#modelquirks).

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

