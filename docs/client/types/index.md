*[bambino](../../index.md) / [client](../index.md) / [types](index.md)*

---

# Module `types`

Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CalibrationOption`](#calibrationoption) | struct | Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE]. |
| [`PrintProgress`](#printprogress) | struct | Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)). |
| [`BuzzerMode`](#buzzermode) | enum | Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE]. |
| [`FanTarget`](#fantarget) | enum | Enumeration representing target onboard cooling fans [REF-CLIM-FANS]. |
| [`PrintSpeed`](#printspeed) | enum | Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE]. |
| [`PrintStatus`](#printstatus) | enum | Decoded classification of the printer's high-level `gcode_state` telemetry field. |
| [`TelemetryEvent`](#telemetryevent) | enum | Typed telemetry event from the printer's MQTT channel. |

## Types

### `CalibrationOption`

```rust
struct CalibrationOption(u32);
```

Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].

Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
(e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).

#### Implementations

- <span id="calibrationoption-const-bed-leveling"></span>`const BED_LEVELING: Self`

- <span id="calibrationoption-const-vibration-compensation"></span>`const VIBRATION_COMPENSATION: Self`

- <span id="calibrationoption-const-motor-noise-cancellation"></span>`const MOTOR_NOISE_CANCELLATION: Self`

- <span id="calibrationoption-const-nozzle-height"></span>`const NOZZLE_HEIGHT: Self`

- <span id="calibrationoption-const-heatbed-thermal"></span>`const HEATBED_THERMAL: Self`

#### Trait Implementations

##### `impl BitOr for CalibrationOption`

- <span id="calibrationoption-bitor-type-output"></span>`type Output = CalibrationOption`

- <span id="calibrationoption-bitor"></span>`fn bitor(self, rhs: Self) -> Self`

##### `impl Clone for CalibrationOption`

- <span id="calibrationoption-clone"></span>`fn clone(&self) -> CalibrationOption` — [`CalibrationOption`](#calibrationoption)

##### `impl Copy for CalibrationOption`

##### `impl Debug for CalibrationOption`

- <span id="calibrationoption-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CalibrationOption`

##### `impl Hash for CalibrationOption`

- <span id="calibrationoption-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CalibrationOption`

- <span id="calibrationoption-partialeq-eq"></span>`fn eq(&self, other: &CalibrationOption) -> bool` — [`CalibrationOption`](#calibrationoption)

### `PrintProgress`

```rust
struct PrintProgress {
    pub percent: Option<i32>,
    pub remaining_secs: Option<i32>,
    pub layer_num: Option<i32>,
    pub total_layers: Option<i32>,
}
```

Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)).

Bundled into one struct rather than four separate cached scalars (unlike `home_flag`/
`gcode_state`/`door_open`/`print_error`, which answer four independent questions) because
`mc_percent`, `mc_remaining_time`, `layer_num`, and `total_layers` are always consumed
together as one "how's the print going" question. Each field updates independently and
keeps its last-observed value across a telemetry message that omits it — a `None` field
means "never observed," not "printer reports zero/none."

#### Fields

- **`percent`**: `Option<i32>`

  Motion controller progress percentage (0-100).

- **`remaining_secs`**: `Option<i32>`

  Estimated remaining duration of the active layer sequence, in seconds.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.

#### Trait Implementations

##### `impl Clone for PrintProgress`

- <span id="printprogress-clone"></span>`fn clone(&self) -> PrintProgress` — [`PrintProgress`](#printprogress)

##### `impl Copy for PrintProgress`

##### `impl Debug for PrintProgress`

- <span id="printprogress-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for PrintProgress`

- <span id="printprogress-default"></span>`fn default() -> PrintProgress` — [`PrintProgress`](#printprogress)

##### `impl Eq for PrintProgress`

##### `impl PartialEq for PrintProgress`

- <span id="printprogress-partialeq-eq"></span>`fn eq(&self, other: &PrintProgress) -> bool` — [`PrintProgress`](#printprogress)

### `BuzzerMode`

```rust
enum BuzzerMode {
    Silent,
    Alarm,
    Chirp,
}
```

Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE].
Supported on models with a physical fire alarm buzzer (H2 series).

#### Variants

- **`Silent`**

  Silent/disarmed.

- **`Alarm`**

  Alarm triggered.

- **`Chirp`**

  Beeping attention chime.

#### Trait Implementations

##### `impl Clone for BuzzerMode`

- <span id="buzzermode-clone"></span>`fn clone(&self) -> BuzzerMode` — [`BuzzerMode`](#buzzermode)

##### `impl Copy for BuzzerMode`

##### `impl Debug for BuzzerMode`

- <span id="buzzermode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for BuzzerMode`

##### `impl PartialEq for BuzzerMode`

- <span id="buzzermode-partialeq-eq"></span>`fn eq(&self, other: &BuzzerMode) -> bool` — [`BuzzerMode`](#buzzermode)

### `FanTarget`

```rust
enum FanTarget {
    PartCooling,
    AuxiliaryLeft,
    ChamberExhaust,
    AuxiliaryRight,
}
```

Enumeration representing target onboard cooling fans [REF-CLIM-FANS].

#### Variants

- **`PartCooling`**

  Primary part cooling fan (Port 1).

- **`AuxiliaryLeft`**

  Primary left-side auxiliary fan (Port 2).

- **`ChamberExhaust`**

  Chamber exhaust/filtration fan (Port 3).

- **`AuxiliaryRight`**

  Secondary right-side auxiliary fan (Port 10, supported on X2D and P2S).

#### Trait Implementations

##### `impl Clone for FanTarget`

- <span id="fantarget-clone"></span>`fn clone(&self) -> FanTarget` — [`FanTarget`](#fantarget)

##### `impl Copy for FanTarget`

##### `impl Debug for FanTarget`

- <span id="fantarget-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FanTarget`

##### `impl Hash for FanTarget`

- <span id="fantarget-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FanTarget`

- <span id="fantarget-partialeq-eq"></span>`fn eq(&self, other: &FanTarget) -> bool` — [`FanTarget`](#fantarget)

### `PrintSpeed`

```rust
enum PrintSpeed {
    Silent,
    Standard,
    Sport,
    Ludicrous,
}
```

Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].

#### Variants

- **`Silent`**

  50% max acceleration and feedrate limits.

- **`Standard`**

  100% nominal feedrate limit.

- **`Sport`**

  124% nominal feedrate limit.

- **`Ludicrous`**

  166% nominal feedrate limit.

#### Implementations

- <span id="printspeed-from-level"></span>`fn from_level(level: u8) -> Option<Self>`

  Classifies a raw `spd_lvl` telemetry value (`1`-`4`, matching the same wire values [`PrinterClient::set_print_speed()`](crate::client::PrinterClient::set_print_speed) sends).

  Returns `None` for an out-of-range level.

#### Trait Implementations

##### `impl Clone for PrintSpeed`

- <span id="printspeed-clone"></span>`fn clone(&self) -> PrintSpeed` — [`PrintSpeed`](#printspeed)

##### `impl Copy for PrintSpeed`

##### `impl Debug for PrintSpeed`

- <span id="printspeed-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintSpeed`

##### `impl Hash for PrintSpeed`

- <span id="printspeed-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintSpeed`

- <span id="printspeed-partialeq-eq"></span>`fn eq(&self, other: &PrintSpeed) -> bool` — [`PrintSpeed`](#printspeed)

### `PrintStatus`

```rust
enum PrintStatus {
    Idle,
    Preparing,
    Running,
    Paused,
    Finished,
    Failed,
    Unknown,
}
```

Decoded classification of the printer's high-level `gcode_state` telemetry field.

`Unknown` covers both an unrecognized wire value and a missing field — callers
needing to tell those apart should inspect the raw `gcode_state` string directly.

#### Variants

- **`Idle`**

  No print job active or loaded (wire: `"IDLE"`).

- **`Preparing`**

  Print preparing to start — homing, bed leveling, or priming, physical
  motion in progress (wire: `"PREPARE"`).

- **`Running`**

  Print job actively executing (wire: `"RUNNING"`).

- **`Paused`**

  Print job paused, resumable (wire: `"PAUSE"`).

- **`Finished`**

  Print job completed successfully (wire: `"FINISH"`).

- **`Failed`**

  Print job aborted by an error condition (wire: `"FAILED"`).

- **`Unknown`**

  Unrecognized wire value, or `gcode_state` field missing entirely — see the enum's doc comment.

#### Implementations

- <span id="printstatus-from-gcode-state"></span>`fn from_gcode_state(state: &str) -> Self`

  Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

#### Trait Implementations

##### `impl Clone for PrintStatus`

- <span id="printstatus-clone"></span>`fn clone(&self) -> PrintStatus` — [`PrintStatus`](#printstatus)

##### `impl Copy for PrintStatus`

##### `impl Debug for PrintStatus`

- <span id="printstatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintStatus`

##### `impl Hash for PrintStatus`

- <span id="printstatus-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintStatus`

- <span id="printstatus-partialeq-eq"></span>`fn eq(&self, other: &PrintStatus) -> bool` — [`PrintStatus`](#printstatus)

### `TelemetryEvent`

```rust
enum TelemetryEvent {
    Report(Box<crate::types::TelemetryReport>, crate::mqtt::MqttMessage),
    Unknown(crate::mqtt::MqttMessage),
}
```

Typed telemetry event from the printer's MQTT channel.

The library deserializes wire payloads into structured types so consumers don't
have to reimplement JSON parsing and model-quirk handling. Raw access is always
available via [`into_raw`](TelemetryEvent::into_raw).

#### Variants

- **`Report`**

  State telemetry update (print status, device hardware, or both).

- **`Unknown`**

  Payload that didn't match any known telemetry structure.

#### Implementations

- <span id="telemetryevent-into-raw"></span>`fn into_raw(self) -> MqttMessage` — [`MqttMessage`](../../mqtt/client/index.md#mqttmessage)

  Consumes the event and returns the underlying raw MQTT message.

- <span id="telemetryevent-raw"></span>`fn raw(&self) -> &MqttMessage` — [`MqttMessage`](../../mqtt/client/index.md#mqttmessage)

  Returns a reference to the underlying raw MQTT message.

- <span id="telemetryevent-report"></span>`fn report(&self) -> Option<&TelemetryReport>` — [`TelemetryReport`](../../types/telemetry/index.md#telemetryreport)

  Returns the typed report if this is a `Report` variant.

#### Trait Implementations

##### `impl Clone for TelemetryEvent`

- <span id="telemetryevent-clone"></span>`fn clone(&self) -> TelemetryEvent` — [`TelemetryEvent`](#telemetryevent)

##### `impl Debug for TelemetryEvent`

- <span id="telemetryevent-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

