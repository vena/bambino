**bambino > client > types**

# Module: client::types

## Contents

**Structs**

- [`CalibrationOption`](#calibrationoption) - Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].
- [`PrintProgress`](#printprogress) - Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)).

**Enums**

- [`BuzzerMode`](#buzzermode) - Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE].
- [`FanTarget`](#fantarget) - Enumeration representing target onboard cooling fans [REF-CLIM-FANS].
- [`PrintSpeed`](#printspeed) - Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].
- [`PrintStatus`](#printstatus) - Decoded classification of the printer's high-level `gcode_state` telemetry field.
- [`TelemetryEvent`](#telemetryevent) - Typed telemetry event from the printer's MQTT channel.

---

## bambino::client::types::BuzzerMode

*Enum*

Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE].
Supported on models with a physical fire alarm buzzer (H2 series).

**Variants:**
- `Silent` - Silent/disarmed.
- `Alarm` - Alarm triggered.
- `Chirp` - Beeping attention chime.

**Traits:** Eq, Copy

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &BuzzerMode) -> bool`
- **Clone**
  - `fn clone(self: &Self) -> BuzzerMode`



## bambino::client::types::CalibrationOption

*Struct*

Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].

Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
(e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).

**Tuple Struct**: `(u32)`

**Methods:**


**Traits:** Eq, Copy

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &CalibrationOption) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **BitOr**
  - `fn bitor(self: Self, rhs: Self) -> Self`
- **Clone**
  - `fn clone(self: &Self) -> CalibrationOption`



## bambino::client::types::FanTarget

*Enum*

Enumeration representing target onboard cooling fans [REF-CLIM-FANS].

**Variants:**
- `PartCooling` - Primary part cooling fan (Port 1).
- `AuxiliaryLeft` - Primary left-side auxiliary fan (Port 2).
- `ChamberExhaust` - Chamber exhaust/filtration fan (Port 3).
- `AuxiliaryRight` - Secondary right-side auxiliary fan (Port 10, supported on X2D and P2S).

**Traits:** Eq, Copy

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> FanTarget`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &FanTarget) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`



## bambino::client::types::PrintProgress

*Struct*

Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)).

Bundled into one struct rather than four separate cached scalars (unlike `home_flag`/
`gcode_state`/`door_open`/`print_error`, which answer four independent questions) because
`mc_percent`, `mc_remaining_time`, `layer_num`, and `total_layers` are always consumed
together as one "how's the print going" question. Each field updates independently and
keeps its last-observed value across a telemetry message that omits it — a `None` field
means "never observed," not "printer reports zero/none."

**Fields:**
- `percent: Option<i32>` - Motion controller progress percentage (0-100).
- `remaining_secs: Option<i32>` - Estimated remaining duration of the active layer sequence, in seconds.
- `layer_num: Option<i32>` - Active layer progress tracker.
- `total_layers: Option<i32>` - Total layers within the sliced print pipeline.

**Traits:** Eq, Copy

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &PrintProgress) -> bool`
- **Default**
  - `fn default() -> PrintProgress`
- **Clone**
  - `fn clone(self: &Self) -> PrintProgress`



## bambino::client::types::PrintSpeed

*Enum*

Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].

**Variants:**
- `Silent` - 50% max acceleration and feedrate limits.
- `Standard` - 100% nominal feedrate limit.
- `Sport` - 124% nominal feedrate limit.
- `Ludicrous` - 166% nominal feedrate limit.

**Methods:**

- `fn from_level(level: u8) -> Option<Self>` - Classifies a raw `spd_lvl` telemetry value (`1`-`4`, matching the same wire values [`PrinterClient::set_print_speed()`](crate::client::PrinterClient::set_print_speed) sends).

**Traits:** Eq, Copy

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &PrintSpeed) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **Clone**
  - `fn clone(self: &Self) -> PrintSpeed`



## bambino::client::types::PrintStatus

*Enum*

Decoded classification of the printer's high-level `gcode_state` telemetry field.

`Unknown` covers both an unrecognized wire value and a missing field — callers
needing to tell those apart should inspect the raw `gcode_state` string directly.

**Variants:**
- `Idle` - No print job active or loaded (wire: `"IDLE"`).
- `Running` - Print job actively executing (wire: `"RUNNING"`).
- `Paused` - Print job paused, resumable (wire: `"PAUSE"`).
- `Finished` - Print job completed successfully (wire: `"FINISH"`).
- `Failed` - Print job aborted by an error condition (wire: `"FAILED"`).
- `Unknown` - Unrecognized wire value, or `gcode_state` field missing entirely — see the enum's doc comment.

**Methods:**

- `fn from_gcode_state(state: &str) -> Self` - Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

**Traits:** Eq, Copy

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &PrintStatus) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **Clone**
  - `fn clone(self: &Self) -> PrintStatus`



## bambino::client::types::TelemetryEvent

*Enum*

Typed telemetry event from the printer's MQTT channel.

The library deserializes wire payloads into structured types so consumers don't
have to reimplement JSON parsing and model-quirk handling. Raw access is always
available via [`into_raw`](TelemetryEvent::into_raw).

**Variants:**
- `Report(Box<crate::types::TelemetryReport>, crate::mqtt::MqttMessage)` - State telemetry update (print status, device hardware, or both).
- `Unknown(crate::mqtt::MqttMessage)` - Payload that didn't match any known telemetry structure.

**Methods:**

- `fn into_raw(self: Self) -> MqttMessage` - Consumes the event and returns the underlying raw MQTT message.
- `fn raw(self: &Self) -> &MqttMessage` - Returns a reference to the underlying raw MQTT message.
- `fn report(self: &Self) -> Option<&TelemetryReport>` - Returns the typed report if this is a `Report` variant.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> TelemetryEvent`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



