*[bambino](../../index.md) / [types](../index.md) / [control](index.md)*

---

# Module `control`

Closed sets of control values shared by the command builders, the telemetry decoders and `PrinterClient`.

These live below both `mqtt::commands` and `client` so a request constructor can take the
typed value instead of the raw wire string or integer, without the command layer depending on
the client. `crate::client` re-exports every one of them.

## Contents

- [Types](#types)
  - [`CalibrationOption`](#calibrationoption)
  - [`AirPurificationMode`](#airpurificationmode)
  - [`BuzzerMode`](#buzzermode)
  - [`DoorOpenCheck`](#dooropencheck)
  - [`FanTarget`](#fantarget)
  - [`IdleHeatingProtection`](#idleheatingprotection)
  - [`LedNode`](#lednode)
  - [`LightMode`](#lightmode)
  - [`NozzleBlobDetectMode`](#nozzleblobdetectmode)
  - [`PrintSpeed`](#printspeed)
  - [`PrintStatus`](#printstatus)
  - [`XcamHaltSensitivity`](#xcamhaltsensitivity)
  - [`XcamModule`](#xcammodule)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CalibrationOption`](#calibrationoption) | struct | Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE]. |
| [`AirPurificationMode`](#airpurificationmode) | enum | Where the chamber air is purified at the end of a print, `print_option`'s `air_purification` and `print.cfg` bits 36-37 [REF-MQTT-TELEMETRY]. |
| [`BuzzerMode`](#buzzermode) | enum | Buzzer alarm/attention chime mode [REF-MQTT-LIFECYCLE]; supported on models with a physical fire alarm buzzer (H2 series). |
| [`DoorOpenCheck`](#dooropencheck) | enum | What the printer does when its door opens mid-print, `set_door_stat`'s `config` and `print.cfg` bits 20-21 [REF-MQTT-TELEMETRY]. |
| [`FanTarget`](#fantarget) | enum | Target onboard cooling fans [REF-CLIM-FANS]. |
| [`IdleHeatingProtection`](#idleheatingprotection) | enum | Idle heating protection as reported in `print.cfg` bits 32-33 [REF-MQTT-TELEMETRY]. |
| [`LedNode`](#lednode) | enum | A printer LED fixture addressed by `ledctrl` and reported in `lights_report`. |
| [`LightMode`](#lightmode) | enum | An LED fixture's mode, as sent in `ledctrl` and reported in `lights_report`. |
| [`NozzleBlobDetectMode`](#nozzleblobdetectmode) | enum | Smart nozzle blob detection mode, `print_option`'s `nozzle_blob_detect_v2` and `print.cfg` bits 43-44 [REF-MQTT-TELEMETRY]. |
| [`PrintSpeed`](#printspeed) | enum | Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE]. |
| [`PrintStatus`](#printstatus) | enum | Decoded classification of the printer's high-level `gcode_state` telemetry field. |
| [`XcamHaltSensitivity`](#xcamhaltsensitivity) | enum | How eagerly a camera detector halts the print, as `xcam_control_set`'s `halt_print_sensitivity`. |
| [`XcamModule`](#xcammodule) | enum | A camera detector `xcam_control_set` addresses by `module_name` (BambuStudio `DevPrintOptions.cpp`). |

## Types

### `CalibrationOption`

```rust
struct CalibrationOption();
```

Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].

Combine flags with `|` (or collect an iterator of them) to trigger several routines at once,
e.g. `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`. Only the
named constants can be built, so a value never carries bits no routine owns.

#### Implementations

- <span id="calibrationoption-const-bed-leveling"></span>`const BED_LEVELING: Self`

- <span id="calibrationoption-const-vibration-compensation"></span>`const VIBRATION_COMPENSATION: Self`

- <span id="calibrationoption-const-motor-noise-cancellation"></span>`const MOTOR_NOISE_CANCELLATION: Self`

- <span id="calibrationoption-const-nozzle-height"></span>`const NOZZLE_HEIGHT: Self`

- <span id="calibrationoption-const-heatbed-thermal"></span>`const HEATBED_THERMAL: Self`

- <span id="calibrationoption-empty"></span>`const fn empty() -> Self`

  No routines.

- <span id="calibrationoption-bits"></span>`const fn bits(self) -> u32`

  The wire `option` bitmask.

- <span id="calibrationoption-contains"></span>`const fn contains(self, other: Self) -> bool`

  Whether every routine in `other` is also in `self`.

- <span id="calibrationoption-is-empty"></span>`const fn is_empty(self) -> bool`

  Whether no routine is selected.

#### Trait Implementations

##### `impl BitOr for CalibrationOption`

- <span id="calibrationoption-bitor-type-output"></span>`type Output = CalibrationOption`

- <span id="calibrationoption-bitor"></span>`fn bitor(self, rhs: Self) -> Self`

##### `impl BitOrAssign for CalibrationOption`

- <span id="calibrationoption-bitorassign-bitor-assign"></span>`fn bitor_assign(&mut self, rhs: Self)`

##### `impl Clone for CalibrationOption`

- <span id="calibrationoption-clone"></span>`fn clone(&self) -> CalibrationOption` — [`CalibrationOption`](#calibrationoption)

##### `impl Copy for CalibrationOption`

##### `impl Debug for CalibrationOption`

- <span id="calibrationoption-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for CalibrationOption`

- <span id="calibrationoption-default"></span>`fn default() -> CalibrationOption` — [`CalibrationOption`](#calibrationoption)

##### `impl Eq for CalibrationOption`

##### `impl FromIterator<CalibrationOption> for CalibrationOption`

- <span id="calibrationoption-fromiterator-from-iter"></span>`fn from_iter<I: IntoIterator<Item = CalibrationOption>>(iter: I) -> Self`

##### `impl Hash for CalibrationOption`

- <span id="calibrationoption-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CalibrationOption`

- <span id="calibrationoption-partialeq-eq"></span>`fn eq(&self, other: &CalibrationOption) -> bool` — [`CalibrationOption`](#calibrationoption)

### `AirPurificationMode`

```rust
enum AirPurificationMode {
    Disabled,
    Inside,
    Outside,
}
```

Where the chamber air is purified at the end of a print, `print_option`'s `air_purification` and `print.cfg` bits 36-37 [REF-MQTT-TELEMETRY].

#### Variants

- **`Disabled`**

  No purification at print end.

- **`Inside`**

  Recirculate through the internal filter.

- **`Outside`**

  Exhaust to the outside.

#### Implementations

- <span id="airpurificationmode-code"></span>`const fn code(self) -> u8`

  The wire code: `0` disabled, `1` inside, `2` outside.

- <span id="airpurificationmode-from-code"></span>`const fn from_code(code: u32) -> Option<Self>`

  Decodes a wire code; `None` for any value outside `0..=2`.

#### Trait Implementations

##### `impl Clone for AirPurificationMode`

- <span id="airpurificationmode-clone"></span>`fn clone(&self) -> AirPurificationMode` — [`AirPurificationMode`](#airpurificationmode)

##### `impl Copy for AirPurificationMode`

##### `impl Debug for AirPurificationMode`

- <span id="airpurificationmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirPurificationMode`

##### `impl Hash for AirPurificationMode`

- <span id="airpurificationmode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AirPurificationMode`

- <span id="airpurificationmode-partialeq-eq"></span>`fn eq(&self, other: &AirPurificationMode) -> bool` — [`AirPurificationMode`](#airpurificationmode)

### `BuzzerMode`

```rust
enum BuzzerMode {
    Silent,
    Alarm,
    Chirp,
}
```

Buzzer alarm/attention chime mode [REF-MQTT-LIFECYCLE]; supported on models with a physical fire alarm buzzer (H2 series).

#### Variants

- **`Silent`**

  Silent/disarmed.

- **`Alarm`**

  Alarm triggered.

- **`Chirp`**

  Beeping attention chime.

#### Implementations

- <span id="buzzermode-code"></span>`const fn code(self) -> i32`

  The `buzzer_ctrl` `mode` code: `0` silent, `1` alarm, `2` chirp.

#### Trait Implementations

##### `impl Clone for BuzzerMode`

- <span id="buzzermode-clone"></span>`fn clone(&self) -> BuzzerMode` — [`BuzzerMode`](#buzzermode)

##### `impl Copy for BuzzerMode`

##### `impl Debug for BuzzerMode`

- <span id="buzzermode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for BuzzerMode`

##### `impl Hash for BuzzerMode`

- <span id="buzzermode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for BuzzerMode`

- <span id="buzzermode-partialeq-eq"></span>`fn eq(&self, other: &BuzzerMode) -> bool` — [`BuzzerMode`](#buzzermode)

### `DoorOpenCheck`

```rust
enum DoorOpenCheck {
    Disabled,
    Warn,
    PausePrint,
}
```

What the printer does when its door opens mid-print, `set_door_stat`'s `config` and `print.cfg` bits 20-21 [REF-MQTT-TELEMETRY].

#### Variants

- **`Disabled`**

  Nothing.

- **`Warn`**

  Show a notification.

- **`PausePrint`**

  Pause the print.

#### Implementations

- <span id="dooropencheck-code"></span>`const fn code(self) -> u8`

  The wire code: `0` disabled, `1` warn, `2` pause print.

- <span id="dooropencheck-from-code"></span>`const fn from_code(code: u32) -> Option<Self>`

  Decodes a wire code; `None` for any value outside `0..=2`.

#### Trait Implementations

##### `impl Clone for DoorOpenCheck`

- <span id="dooropencheck-clone"></span>`fn clone(&self) -> DoorOpenCheck` — [`DoorOpenCheck`](#dooropencheck)

##### `impl Copy for DoorOpenCheck`

##### `impl Debug for DoorOpenCheck`

- <span id="dooropencheck-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DoorOpenCheck`

##### `impl Hash for DoorOpenCheck`

- <span id="dooropencheck-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for DoorOpenCheck`

- <span id="dooropencheck-partialeq-eq"></span>`fn eq(&self, other: &DoorOpenCheck) -> bool` — [`DoorOpenCheck`](#dooropencheck)

### `FanTarget`

```rust
enum FanTarget {
    PartCooling,
    AuxiliaryLeft,
    ChamberExhaust,
    AuxiliaryLeft2,
}
```

Target onboard cooling fans [REF-CLIM-FANS].

#### Variants

- **`PartCooling`**

  Primary part cooling fan (Port 1).

- **`AuxiliaryLeft`**

  Primary left-side auxiliary fan (Port 2).

- **`ChamberExhaust`**

  Chamber exhaust/filtration fan (Port 3).

- **`AuxiliaryLeft2`**

  Secondary left-side auxiliary fan (Port 10, supported on X2D and P2S) [REF-CLIM-FANS].
  
  Despite the wire port number (M106 `P10`) and read-side airduct id (160) suggesting a
  "right" fan, BambuStudio's `DevFan.h` names decoded id 10 `FAN_REMOTE_COOLING_1_IDX` —
  a second left-side auxiliary fan, distinct from [`AuxiliaryLeft`](#fantarget)'s
  primary port-2 fan (`FAN_REMOTE_COOLING_0_IDX`, mirrored into `big_fan1_speed`).
  Confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary
  part cooling fan" throughout (issue #60).

#### Implementations

- <span id="fantarget-const-all"></span>`const ALL: &'static [FanTarget]`

- <span id="fantarget-write-port"></span>`const fn write_port(self) -> u16`

  The M106 `P` port that drives this fan.

- <span id="fantarget-airduct-part-id"></span>`const fn airduct_part_id(self) -> Option<u32>`

  The `device.airduct.parts[].id` this fan reports under, for the one fan read from there.

  A different address space from [`write_port`](#fantarget): the three other fans
  report through `print.*_fan_speed` strings instead and return `None`.

- <span id="fantarget-is-supported-by"></span>`fn is_supported_by(self, quirks: &crate::quirks::ModelQuirks) -> bool` — [`ModelQuirks`](../../quirks/index.md#modelquirks)

  Whether `quirks` says this model has the fan.

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

### `IdleHeatingProtection`

```rust
enum IdleHeatingProtection {
    Off,
    On,
    Unavailable,
}
```

Idle heating protection as reported in `print.cfg` bits 32-33 [REF-MQTT-TELEMETRY].

Three states, though the setter takes a bool. BambuStudio's Safety Options dialog greys the
toggle out on `2` with "Unavailable while heating maintenance function is on."
(`SafetyOptionsDialog.cpp`, `updateIdelHeatingProtect`); that meaning comes from UI text only.

#### Variants

- **`Off`**

  Off.

- **`On`**

  On.

- **`Unavailable`**

  Can't be changed while the heating maintenance function runs.

#### Implementations

- <span id="idleheatingprotection-from-code"></span>`const fn from_code(code: u32) -> Option<Self>`

  Decodes the two-bit field; `None` for the unassigned code `3`.

#### Trait Implementations

##### `impl Clone for IdleHeatingProtection`

- <span id="idleheatingprotection-clone"></span>`fn clone(&self) -> IdleHeatingProtection` — [`IdleHeatingProtection`](#idleheatingprotection)

##### `impl Copy for IdleHeatingProtection`

##### `impl Debug for IdleHeatingProtection`

- <span id="idleheatingprotection-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for IdleHeatingProtection`

##### `impl Hash for IdleHeatingProtection`

- <span id="idleheatingprotection-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for IdleHeatingProtection`

- <span id="idleheatingprotection-partialeq-eq"></span>`fn eq(&self, other: &IdleHeatingProtection) -> bool` — [`IdleHeatingProtection`](#idleheatingprotection)

### `LedNode`

```rust
enum LedNode {
    Chamber,
    Chamber2,
    Work,
}
```

A printer LED fixture addressed by `ledctrl` and reported in `lights_report`.

#### Variants

- **`Chamber`**

  The chamber light (`chamber_light`).

- **`Chamber2`**

  The second chamber light on models with two (`chamber_light2`).

- **`Work`**

  The work light (`work_light`).

#### Implementations

- <span id="lednode-const-all"></span>`const ALL: &'static [LedNode]`

- <span id="lednode-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

#### Trait Implementations

##### `impl Clone for LedNode`

- <span id="lednode-clone"></span>`fn clone(&self) -> LedNode` — [`LedNode`](#lednode)

##### `impl Copy for LedNode`

##### `impl Debug for LedNode`

- <span id="lednode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for LedNode`

- <span id="lednode-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for LedNode`

##### `impl FromStr for LedNode`

- <span id="lednode-fromstr-type-err"></span>`type Err = Error`

- <span id="lednode-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

##### `impl Hash for LedNode`

- <span id="lednode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for LedNode`

- <span id="lednode-partialeq-eq"></span>`fn eq(&self, other: &LedNode) -> bool` — [`LedNode`](#lednode)

##### `impl ToString for LedNode`

- <span id="lednode-tostring-to-string"></span>`fn to_string(&self) -> String`

### `LightMode`

```rust
enum LightMode {
    On,
    Off,
    Flashing,
}
```

An LED fixture's mode, as sent in `ledctrl` and reported in `lights_report`.

#### Variants

- **`On`**

  Lit.

- **`Off`**

  Dark.

- **`Flashing`**

  Cycling on a flash timing.

#### Implementations

- <span id="lightmode-const-all"></span>`const ALL: &'static [LightMode]`

- <span id="lightmode-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

#### Trait Implementations

##### `impl Clone for LightMode`

- <span id="lightmode-clone"></span>`fn clone(&self) -> LightMode` — [`LightMode`](#lightmode)

##### `impl Copy for LightMode`

##### `impl Debug for LightMode`

- <span id="lightmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for LightMode`

- <span id="lightmode-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for LightMode`

##### `impl FromStr for LightMode`

- <span id="lightmode-fromstr-type-err"></span>`type Err = Error`

- <span id="lightmode-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

##### `impl Hash for LightMode`

- <span id="lightmode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for LightMode`

- <span id="lightmode-partialeq-eq"></span>`fn eq(&self, other: &LightMode) -> bool` — [`LightMode`](#lightmode)

##### `impl ToString for LightMode`

- <span id="lightmode-tostring-to-string"></span>`fn to_string(&self) -> String`

### `NozzleBlobDetectMode`

```rust
enum NozzleBlobDetectMode {
    Off,
    On,
    Auto,
}
```

Smart nozzle blob detection mode, `print_option`'s `nozzle_blob_detect_v2` and `print.cfg` bits 43-44 [REF-MQTT-TELEMETRY].

#### Variants

- **`Off`**

  Detection off.

- **`On`**

  Detection on.

- **`Auto`**

  The printer decides per print.

#### Implementations

- <span id="nozzleblobdetectmode-code"></span>`const fn code(self) -> u8`

  The wire code: `0` off, `1` on, `2` auto.

- <span id="nozzleblobdetectmode-from-code"></span>`const fn from_code(code: u32) -> Option<Self>`

  Decodes a wire code; `None` for any value outside `0..=2`.

#### Trait Implementations

##### `impl Clone for NozzleBlobDetectMode`

- <span id="nozzleblobdetectmode-clone"></span>`fn clone(&self) -> NozzleBlobDetectMode` — [`NozzleBlobDetectMode`](#nozzleblobdetectmode)

##### `impl Copy for NozzleBlobDetectMode`

##### `impl Debug for NozzleBlobDetectMode`

- <span id="nozzleblobdetectmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for NozzleBlobDetectMode`

##### `impl Hash for NozzleBlobDetectMode`

- <span id="nozzleblobdetectmode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for NozzleBlobDetectMode`

- <span id="nozzleblobdetectmode-partialeq-eq"></span>`fn eq(&self, other: &NozzleBlobDetectMode) -> bool` — [`NozzleBlobDetectMode`](#nozzleblobdetectmode)

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

- <span id="printspeed-const-all"></span>`const ALL: &'static [PrintSpeed]`

- <span id="printspeed-from-level"></span>`fn from_level(level: u8) -> Option<Self>`

  Classifies a raw `spd_lvl` telemetry value (`1`-`4`, the same values `print_speed` sends); `None` for an out-of-range level.

- <span id="printspeed-level"></span>`const fn level(self) -> u8`

  The wire level, `1`-`4` — the inverse of [`from_level`](#printspeed).

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
    Slicing,
    Running,
    Paused,
    Finished,
    Failed,
    Unknown,
}
```

Decoded classification of the printer's high-level `gcode_state` telemetry field.

`Unknown` covers an unrecognized wire value; callers needing to tell that apart from a known
state should inspect the raw `gcode_state` string directly.

#### Variants

- **`Idle`**

  No print job active or loaded (wire: `"IDLE"`).

- **`Preparing`**

  Print preparing to start — homing, bed leveling, or priming, physical
  motion in progress (wire: `"PREPARE"`).

- **`Slicing`**

  Printer is slicing a job on-device, before any physical motion (wire: `"SLICING"`).
  
  Distinct from [`Preparing`](#printstatus): nothing is moving yet. It is still a
  busy state — a job is in flight — so treat it like the other active states when
  deciding whether the printer can accept new work.

- **`Running`**

  Print job actively executing (wire: `"RUNNING"`).

- **`Paused`**

  Print job paused, resumable (wire: `"PAUSE"`).

- **`Finished`**

  Print job completed successfully (wire: `"FINISH"`).

- **`Failed`**

  Print job aborted by an error condition (wire: `"FAILED"`).

- **`Unknown`**

  Unrecognized wire value — see the enum's doc comment.

#### Implementations

- <span id="printstatus-from-gcode-state"></span>`fn from_gcode_state(state: &str) -> Self`

  Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"SLICING"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

- <span id="printstatus-as-str"></span>`const fn as_str(self) -> Option<&'static str>`

  The `gcode_state` wire value for this status; `None` for [`Unknown`](#printstatus).

- <span id="printstatus-is-busy"></span>`fn is_busy(self) -> bool`

  True while a job is in flight — preparing, slicing, running or paused — so the printer
  shouldn't be given new work or motion that could collide with a part.

  `Unknown` is not busy, so a caller gating on safety must treat a missing status
  (`PrinterClient::print_status() == None`) or `Unknown` as "can't confirm idle" itself.

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

### `XcamHaltSensitivity`

```rust
enum XcamHaltSensitivity {
    NeverHalt,
    Low,
    Medium,
    High,
}
```

How eagerly a camera detector halts the print, as `xcam_control_set`'s `halt_print_sensitivity`.

`NeverHalt` only notifies. It is the AI-monitoring level BambuStudio offers alongside the three
`XcamSensitivity` levels the per-detector telemetry reports.

#### Variants

- **`NeverHalt`**

  Notify only.

- **`Low`**

  Least eager to halt.

- **`Medium`**

  Medium.

- **`High`**

  Most eager to halt.

#### Implementations

- <span id="xcamhaltsensitivity-const-all"></span>`const ALL: &'static [XcamHaltSensitivity]`

- <span id="xcamhaltsensitivity-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

#### Trait Implementations

##### `impl Clone for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-clone"></span>`fn clone(&self) -> XcamHaltSensitivity` — [`XcamHaltSensitivity`](#xcamhaltsensitivity)

##### `impl Copy for XcamHaltSensitivity`

##### `impl Debug for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for XcamHaltSensitivity`

##### `impl FromStr for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-fromstr-type-err"></span>`type Err = Error`

- <span id="xcamhaltsensitivity-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

##### `impl Hash for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-partialeq-eq"></span>`fn eq(&self, other: &XcamHaltSensitivity) -> bool` — [`XcamHaltSensitivity`](#xcamhaltsensitivity)

##### `impl ToString for XcamHaltSensitivity`

- <span id="xcamhaltsensitivity-tostring-to-string"></span>`fn to_string(&self) -> String`

### `XcamModule`

```rust
enum XcamModule {
    PrintingMonitor,
    SpaghettiDetector,
    PileupDetector,
    ClumpDetector,
    AirprintDetector,
    FirstLayerInspector,
    BuildplateMarkerDetector,
    PlateOffsetSwitch,
    FodCheck,
    ModelMovementCheck,
}
```

A camera detector `xcam_control_set` addresses by `module_name` (BambuStudio `DevPrintOptions.cpp`).

#### Variants

- **`PrintingMonitor`**

  AI monitoring, the older global switch.

- **`SpaghettiDetector`**

  Spaghetti detection.

- **`PileupDetector`**

  Purge chute pile-up detection.

- **`ClumpDetector`**

  Nozzle clumping detection.

- **`AirprintDetector`**

  Camera air-printing detection (not `print_option`'s non-visual one).

- **`FirstLayerInspector`**

  First-layer inspection.

- **`BuildplateMarkerDetector`**

  Build plate marker (plate type) detection.

- **`PlateOffsetSwitch`**

  Build plate alignment detection.

- **`FodCheck`**

  Foreign object detection.

- **`ModelMovementCheck`**

  Displacement detection.

#### Implementations

- <span id="xcammodule-const-all"></span>`const ALL: &'static [XcamModule]`

- <span id="xcammodule-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

- <span id="xcammodule-takes-sensitivity"></span>`const fn takes_sensitivity(self) -> bool`

  Whether BambuStudio sends a `halt_print_sensitivity` with this module.

#### Trait Implementations

##### `impl Clone for XcamModule`

- <span id="xcammodule-clone"></span>`fn clone(&self) -> XcamModule` — [`XcamModule`](#xcammodule)

##### `impl Copy for XcamModule`

##### `impl Debug for XcamModule`

- <span id="xcammodule-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for XcamModule`

- <span id="xcammodule-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for XcamModule`

##### `impl FromStr for XcamModule`

- <span id="xcammodule-fromstr-type-err"></span>`type Err = Error`

- <span id="xcammodule-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

##### `impl Hash for XcamModule`

- <span id="xcammodule-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for XcamModule`

- <span id="xcammodule-partialeq-eq"></span>`fn eq(&self, other: &XcamModule) -> bool` — [`XcamModule`](#xcammodule)

##### `impl ToString for XcamModule`

- <span id="xcammodule-tostring-to-string"></span>`fn to_string(&self) -> String`

