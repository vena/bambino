*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [control](index.md)*

---

# Module `control`

Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).

## Contents

- [Types](#types)
  - [`CalibrationPayload`](#calibrationpayload)
  - [`CleanPrintErrorPayload`](#cleanprinterrorpayload)
  - [`HmsActionPayload`](#hmsactionpayload)
  - [`IdleIgnorePayload`](#idleignorepayload)
  - [`PrintSpeedPayload`](#printspeedpayload)
  - [`SkipObjectsPayload`](#skipobjectspayload)
  - [`StandardControlPayload`](#standardcontrolpayload)
  - [`UiopPayload`](#uioppayload)
  - [`IdleIgnoreScope`](#idleignorescope)
  - [`StandardCommand`](#standardcommand)
  - [`CalibrationRequest`](#calibrationrequest)
  - [`CleanPrintErrorRequest`](#cleanprinterrorrequest)
  - [`HmsActionRequest`](#hmsactionrequest)
  - [`IdleIgnoreRequest`](#idleignorerequest)
  - [`PrintSpeedRequest`](#printspeedrequest)
  - [`SkipObjectsRequest`](#skipobjectsrequest)
  - [`StandardControlRequest`](#standardcontrolrequest)
  - [`UiopRequest`](#uioprequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CalibrationPayload`](#calibrationpayload) | struct | Triggers automated physical resonance compensation sweeps and chassis alignments. |
| [`CleanPrintErrorPayload`](#cleanprinterrorpayload) | struct | Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE]. |
| [`HmsActionPayload`](#hmsactionpayload) | struct | Error-dialog action carrying the fault it answers [REF-MQTT-LIFECYCLE]. |
| [`IdleIgnorePayload`](#idleignorepayload) | struct | Dismisses a warning without resuming anything [REF-MQTT-LIFECYCLE]. |
| [`PrintSpeedPayload`](#printspeedpayload) | struct | Dynamically scales maximum movement velocity and acceleration limits. |
| [`SkipObjectsPayload`](#skipobjectspayload) | struct | Instructs the printer to bypass rendering specific objects within active multi-model jobs. |
| [`StandardControlPayload`](#standardcontrolpayload) | struct | General control payload used for pause, resume, stop and the other name-only commands. |
| [`UiopPayload`](#uioppayload) | struct | Closes the printer's on-screen `print_error` dialog [REF-MQTT-LIFECYCLE]. |
| [`IdleIgnoreScope`](#idleignorescope) | enum | How long an `idle_ignore` dismissal lasts. |
| [`StandardCommand`](#standardcommand) | enum | A print-lifecycle command that carries nothing but its name and `sequence_id` [REF-MQTT-LIFECYCLE]. |
| [`CalibrationRequest`](#calibrationrequest) | type | Kicks off a calibration routine (vibration compensation, bed leveling, etc.). |
| [`CleanPrintErrorRequest`](#cleanprinterrorrequest) | type | Clears the printer's current error state so it can resume operation. |
| [`HmsActionRequest`](#hmsactionrequest) | type | Answers a paused print's error dialog: ignore the fault and resume, or resume/stop naming it. |
| [`IdleIgnoreRequest`](#idleignorerequest) | type | Dismisses a non-pausing warning, once or permanently (BambuStudio `command_hms_idle_ignore`). |
| [`PrintSpeedRequest`](#printspeedrequest) | type | Changes the active print speed profile (silent, standard, sport, ludicrous). |
| [`SkipObjectsRequest`](#skipobjectsrequest) | type | Tells the printer to skip specific objects in a multi-object print. |
| [`StandardControlRequest`](#standardcontrolrequest) | type | Sends a name-only print lifecycle command (pause, resume, stop, ...) to the printer. |
| [`UiopRequest`](#uioprequest) | type | Closes the error dialog on the printer's screen (BambuStudio `command_clean_print_error_uiop`). |

## Types

### `CalibrationPayload`

```rust
struct CalibrationPayload {
    pub command: &'static str,
    pub option: u32,
    pub sequence_id: super::ClampedTaskId,
}
```

Triggers automated physical resonance compensation sweeps and chassis alignments.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"calibration"`.

- **`option`**: `u32`

  Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for CalibrationPayload`

- <span id="calibrationpayload-clone"></span>`fn clone(&self) -> CalibrationPayload` — [`CalibrationPayload`](#calibrationpayload)

##### `impl Debug for CalibrationPayload`

- <span id="calibrationpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CalibrationPayload`

- <span id="calibrationpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CleanPrintErrorPayload`

```rust
struct CleanPrintErrorPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"clean_print_error"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-clone"></span>`fn clone(&self) -> CleanPrintErrorPayload` — [`CleanPrintErrorPayload`](#cleanprinterrorpayload)

##### `impl Debug for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `HmsActionPayload`

```rust
struct HmsActionPayload {
    pub command: &'static str,
    pub err: String,
    pub param: &'static str,
    pub job_id: String,
    pub sequence_id: super::ClampedTaskId,
}
```

Error-dialog action carrying the fault it answers [REF-MQTT-LIFECYCLE].

One shape serves three commands, per BambuStudio's `command_hms_ignore`,
`command_hms_resume` and `command_hms_stop` (`DeviceManager.cpp`): `err`, `param: "reserve"`
and `job_id` alongside the command name. `err` is the code in *decimal* — BambuStudio passes
`std::to_string(m_error_code)` (`DeviceErrorDialog.cpp`); only `uiop` uses 8-digit hex.

#### Fields

- **`command`**: `&'static str`

  Wire command name: `"ignore"`, `"resume"` or `"stop"`.

- **`err`**: `String`

  The `print_error` code being answered, as a decimal string.

- **`param`**: `&'static str`

  Always `"reserve"`.

- **`job_id`**: `String`

  The current job's `job_id`, or empty when unknown (bambuddy sends `""` then).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for HmsActionPayload`

- <span id="hmsactionpayload-clone"></span>`fn clone(&self) -> HmsActionPayload` — [`HmsActionPayload`](#hmsactionpayload)

##### `impl Debug for HmsActionPayload`

- <span id="hmsactionpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for HmsActionPayload`

- <span id="hmsactionpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IdleIgnorePayload`

```rust
struct IdleIgnorePayload {
    pub command: &'static str,
    pub err: String,
    pub ignore_type: u8,
    pub sequence_id: super::ClampedTaskId,
}
```

Dismisses a warning without resuming anything [REF-MQTT-LIFECYCLE].

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"idle_ignore"`.

- **`err`**: `String`

  The `print_error` code being dismissed, as a decimal string.

- **`ignore_type`**: `u8`

  `0` dismisses this occurrence; `1` suppresses the same warning permanently.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for IdleIgnorePayload`

- <span id="idleignorepayload-clone"></span>`fn clone(&self) -> IdleIgnorePayload` — [`IdleIgnorePayload`](#idleignorepayload)

##### `impl Debug for IdleIgnorePayload`

- <span id="idleignorepayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for IdleIgnorePayload`

- <span id="idleignorepayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintSpeedPayload`

```rust
struct PrintSpeedPayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: super::ClampedTaskId,
}
```

Dynamically scales maximum movement velocity and acceleration limits.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_speed"`.

- **`param`**: `String`

  Target speed scaling index serialized as string:
  * `"1"`: Silent Mode (50% limits).
  * `"2"`: Standard Mode (100% nominal).
  * `"3"`: Sport Mode (124% limits).
  * `"4"`: Ludicrous Mode (166% limits).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PrintSpeedPayload`

- <span id="printspeedpayload-clone"></span>`fn clone(&self) -> PrintSpeedPayload` — [`PrintSpeedPayload`](#printspeedpayload)

##### `impl Debug for PrintSpeedPayload`

- <span id="printspeedpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PrintSpeedPayload`

- <span id="printspeedpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `SkipObjectsPayload`

```rust
struct SkipObjectsPayload {
    pub command: &'static str,
    pub obj_list: Vec<u32>,
    pub sequence_id: super::ClampedTaskId,
}
```

Instructs the printer to bypass rendering specific objects within active multi-model jobs.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"skip_objects"`.

- **`obj_list`**: `Vec<u32>`

  List of object indices (as sliced) to skip rendering.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for SkipObjectsPayload`

- <span id="skipobjectspayload-clone"></span>`fn clone(&self) -> SkipObjectsPayload` — [`SkipObjectsPayload`](#skipobjectspayload)

##### `impl Debug for SkipObjectsPayload`

- <span id="skipobjectspayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SkipObjectsPayload`

- <span id="skipobjectspayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardControlPayload`

```rust
struct StandardControlPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

General control payload used for pause, resume, stop and the other name-only commands.

#### Fields

- **`command`**: `&'static str`

  Wire command name — see [`StandardCommand`](#standardcommand).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for StandardControlPayload`

- <span id="standardcontrolpayload-clone"></span>`fn clone(&self) -> StandardControlPayload` — [`StandardControlPayload`](#standardcontrolpayload)

##### `impl Debug for StandardControlPayload`

- <span id="standardcontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardControlPayload`

- <span id="standardcontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `UiopPayload`

```rust
struct UiopPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub name: &'static str,
    pub action: &'static str,
    pub source: u8,
    pub ui_type: &'static str,
    pub err: String,
}
```

Closes the printer's on-screen `print_error` dialog [REF-MQTT-LIFECYCLE].

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"uiop"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`name`**: `&'static str`

  UI element family, always `"print_error"`.

- **`action`**: `&'static str`

  Always `"close"`.

- **`source`**: `u8`

  Sender: `0` the printer's own UI, `1` BambuStudio. bambino identifies as `1`.

- **`ui_type`**: `&'static str`

  Always `"dialog"`.

- **`err`**: `String`

  The `print_error` code whose dialog to close, as 8 uppercase hex digits.

#### Trait Implementations

##### `impl Clone for UiopPayload`

- <span id="uioppayload-clone"></span>`fn clone(&self) -> UiopPayload` — [`UiopPayload`](#uioppayload)

##### `impl Debug for UiopPayload`

- <span id="uioppayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for UiopPayload`

- <span id="uioppayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IdleIgnoreScope`

```rust
enum IdleIgnoreScope {
    Once,
    Permanent,
}
```

How long an `idle_ignore` dismissal lasts.

#### Variants

- **`Once`**

  Dismiss this occurrence only (`type: 0`).

- **`Permanent`**

  Never show this warning again (`type: 1`).

#### Trait Implementations

##### `impl Clone for IdleIgnoreScope`

- <span id="idleignorescope-clone"></span>`fn clone(&self) -> IdleIgnoreScope` — [`IdleIgnoreScope`](#idleignorescope)

##### `impl Copy for IdleIgnoreScope`

##### `impl Debug for IdleIgnoreScope`

- <span id="idleignorescope-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for IdleIgnoreScope`

##### `impl Hash for IdleIgnoreScope`

- <span id="idleignorescope-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for IdleIgnoreScope`

- <span id="idleignorescope-partialeq-eq"></span>`fn eq(&self, other: &IdleIgnoreScope) -> bool` — [`IdleIgnoreScope`](#idleignorescope)

### `StandardCommand`

```rust
enum StandardCommand {
    Pause,
    Resume,
    Stop,
    RefreshNozzle,
    CloseAirFilter,
    AutoStopAmsDry,
}
```

A print-lifecycle command that carries nothing but its name and `sequence_id` [REF-MQTT-LIFECYCLE].

#### Variants

- **`Pause`**

  Pause the running job (`pause`).

- **`Resume`**

  Resume a paused job (`resume`).

- **`Stop`**

  Stop the job (`stop`).

- **`RefreshNozzle`**

  Re-read the nozzle information (`refresh_nozzle`).

- **`CloseAirFilter`**

  Turn off air purification (`close_air_filt`).

- **`AutoStopAmsDry`**

  The error dialog's "stop drying" (`auto_stop_ams_dry`).

#### Implementations

- <span id="standardcommand-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The wire command name.

#### Trait Implementations

##### `impl Clone for StandardCommand`

- <span id="standardcommand-clone"></span>`fn clone(&self) -> StandardCommand` — [`StandardCommand`](#standardcommand)

##### `impl Copy for StandardCommand`

##### `impl Debug for StandardCommand`

- <span id="standardcommand-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for StandardCommand`

##### `impl Hash for StandardCommand`

- <span id="standardcommand-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for StandardCommand`

- <span id="standardcommand-partialeq-eq"></span>`fn eq(&self, other: &StandardCommand) -> bool` — [`StandardCommand`](#standardcommand)

### `CalibrationRequest`

```rust
type CalibrationRequest = super::Print<CalibrationPayload>;
```

Kicks off a calibration routine (vibration compensation, bed leveling, etc.).

### `CleanPrintErrorRequest`

```rust
type CleanPrintErrorRequest = super::Print<CleanPrintErrorPayload>;
```

Clears the printer's current error state so it can resume operation.

### `HmsActionRequest`

```rust
type HmsActionRequest = super::Print<HmsActionPayload>;
```

Answers a paused print's error dialog: ignore the fault and resume, or resume/stop naming it.

### `IdleIgnoreRequest`

```rust
type IdleIgnoreRequest = super::Print<IdleIgnorePayload>;
```

Dismisses a non-pausing warning, once or permanently (BambuStudio `command_hms_idle_ignore`).

### `PrintSpeedRequest`

```rust
type PrintSpeedRequest = super::Print<PrintSpeedPayload>;
```

Changes the active print speed profile (silent, standard, sport, ludicrous).

### `SkipObjectsRequest`

```rust
type SkipObjectsRequest = super::Print<SkipObjectsPayload>;
```

Tells the printer to skip specific objects in a multi-object print.

### `StandardControlRequest`

```rust
type StandardControlRequest = super::Print<StandardControlPayload>;
```

Sends a name-only print lifecycle command (pause, resume, stop, ...) to the printer.

### `UiopRequest`

```rust
type UiopRequest = super::System<UiopPayload>;
```

Closes the error dialog on the printer's screen (BambuStudio `command_clean_print_error_uiop`).

Separate from [`CleanPrintErrorRequest`](#cleanprinterrorrequest), which clears the error latch: BambuStudio sends
this once whenever its own copy of the dialog closes.

