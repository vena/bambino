*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [control](index.md)*

---

# Module `control`

Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).

## Contents

- [Types](#types)
  - [`CalibrationPayload`](#calibrationpayload)
  - [`CalibrationRequest`](#calibrationrequest)
  - [`CleanPrintErrorPayload`](#cleanprinterrorpayload)
  - [`CleanPrintErrorRequest`](#cleanprinterrorrequest)
  - [`PrintSpeedPayload`](#printspeedpayload)
  - [`PrintSpeedRequest`](#printspeedrequest)
  - [`SkipObjectsPayload`](#skipobjectspayload)
  - [`SkipObjectsRequest`](#skipobjectsrequest)
  - [`StandardControlPayload`](#standardcontrolpayload)
  - [`StandardControlRequest`](#standardcontrolrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CalibrationPayload`](#calibrationpayload) | struct | Triggers automated physical resonance compensation sweeps and chassis alignments. |
| [`CalibrationRequest`](#calibrationrequest) | struct | Kicks off a calibration routine (vibration compensation, bed leveling, etc.). |
| [`CleanPrintErrorPayload`](#cleanprinterrorpayload) | struct | Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE]. |
| [`CleanPrintErrorRequest`](#cleanprinterrorrequest) | struct | Clears the printer's current error state so it can resume operation. |
| [`PrintSpeedPayload`](#printspeedpayload) | struct | Dynamically scales maximum movement velocity and acceleration limits. |
| [`PrintSpeedRequest`](#printspeedrequest) | struct | Changes the active print speed profile (silent, standard, sport, ludicrous). |
| [`SkipObjectsPayload`](#skipobjectspayload) | struct | Instructs the printer to bypass rendering specific objects within active multi-model jobs. |
| [`SkipObjectsRequest`](#skipobjectsrequest) | struct | Tells the printer to skip specific objects in a multi-object print. |
| [`StandardControlPayload`](#standardcontrolpayload) | struct | General control payload used for pause, resume, stop, and clean actions. |
| [`StandardControlRequest`](#standardcontrolrequest) | struct | Sends a print lifecycle command (pause, resume, stop) to the printer. |

## Types

### `CalibrationPayload`

```rust
struct CalibrationPayload {
    pub command: &'static str,
    pub option: u32,
    pub sequence_id: String,
}
```

Triggers automated physical resonance compensation sweeps and chassis alignments.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"calibration"`.

- **`option`**: `u32`

  Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for CalibrationPayload`

- <span id="calibrationpayload-clone"></span>`fn clone(&self) -> CalibrationPayload` — [`CalibrationPayload`](#calibrationpayload)

##### `impl Debug for CalibrationPayload`

- <span id="calibrationpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CalibrationPayload`

- <span id="calibrationpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CalibrationRequest`

```rust
struct CalibrationRequest {
    pub print: CalibrationPayload,
}
```

Kicks off a calibration routine (vibration compensation, bed leveling, etc.).

#### Fields

- **`print`**: `CalibrationPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="calibrationrequest-new"></span>`fn new(option_bitmask: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `calibration` request from a capability option bitmask.

#### Trait Implementations

##### `impl Clone for CalibrationRequest`

- <span id="calibrationrequest-clone"></span>`fn clone(&self) -> CalibrationRequest` — [`CalibrationRequest`](#calibrationrequest)

##### `impl Debug for CalibrationRequest`

- <span id="calibrationrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CalibrationRequest`

- <span id="calibrationrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CleanPrintErrorPayload`

```rust
struct CleanPrintErrorPayload {
    pub command: &'static str,
    pub sequence_id: String,
}
```

Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"clean_print_error"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-clone"></span>`fn clone(&self) -> CleanPrintErrorPayload` — [`CleanPrintErrorPayload`](#cleanprinterrorpayload)

##### `impl Debug for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CleanPrintErrorPayload`

- <span id="cleanprinterrorpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CleanPrintErrorRequest`

```rust
struct CleanPrintErrorRequest {
    pub print: CleanPrintErrorPayload,
}
```

Clears the printer's current error state so it can resume operation.

#### Fields

- **`print`**: `CleanPrintErrorPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="cleanprinterrorrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `clean_print_error` request.

#### Trait Implementations

##### `impl Clone for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-clone"></span>`fn clone(&self) -> CleanPrintErrorRequest` — [`CleanPrintErrorRequest`](#cleanprinterrorrequest)

##### `impl Debug for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintSpeedPayload`

```rust
struct PrintSpeedPayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: String,
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

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PrintSpeedPayload`

- <span id="printspeedpayload-clone"></span>`fn clone(&self) -> PrintSpeedPayload` — [`PrintSpeedPayload`](#printspeedpayload)

##### `impl Debug for PrintSpeedPayload`

- <span id="printspeedpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PrintSpeedPayload`

- <span id="printspeedpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintSpeedRequest`

```rust
struct PrintSpeedRequest {
    pub print: PrintSpeedPayload,
}
```

Changes the active print speed profile (silent, standard, sport, ludicrous).

#### Fields

- **`print`**: `PrintSpeedPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="printspeedrequest-new"></span>`fn new(speed_index_str: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `print_speed` request from a stringified speed index.

#### Trait Implementations

##### `impl Clone for PrintSpeedRequest`

- <span id="printspeedrequest-clone"></span>`fn clone(&self) -> PrintSpeedRequest` — [`PrintSpeedRequest`](#printspeedrequest)

##### `impl Debug for PrintSpeedRequest`

- <span id="printspeedrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PrintSpeedRequest`

- <span id="printspeedrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `SkipObjectsPayload`

```rust
struct SkipObjectsPayload {
    pub command: &'static str,
    pub obj_list: Vec<u32>,
    pub sequence_id: String,
}
```

Instructs the printer to bypass rendering specific objects within active multi-model jobs.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"skip_objects"`.

- **`obj_list`**: `Vec<u32>`

  List of object indices (as sliced) to skip rendering.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for SkipObjectsPayload`

- <span id="skipobjectspayload-clone"></span>`fn clone(&self) -> SkipObjectsPayload` — [`SkipObjectsPayload`](#skipobjectspayload)

##### `impl Debug for SkipObjectsPayload`

- <span id="skipobjectspayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SkipObjectsPayload`

- <span id="skipobjectspayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `SkipObjectsRequest`

```rust
struct SkipObjectsRequest {
    pub print: SkipObjectsPayload,
}
```

Tells the printer to skip specific objects in a multi-object print.

#### Fields

- **`print`**: `SkipObjectsPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="skipobjectsrequest-new"></span>`fn new(object_indices: Vec<u32>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `skip_objects` request from a list of object indices to skip.

#### Trait Implementations

##### `impl Clone for SkipObjectsRequest`

- <span id="skipobjectsrequest-clone"></span>`fn clone(&self) -> SkipObjectsRequest` — [`SkipObjectsRequest`](#skipobjectsrequest)

##### `impl Debug for SkipObjectsRequest`

- <span id="skipobjectsrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SkipObjectsRequest`

- <span id="skipobjectsrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardControlPayload`

```rust
struct StandardControlPayload {
    pub command: String,
    pub sequence_id: String,
}
```

General control payload used for pause, resume, stop, and clean actions.

#### Fields

- **`command`**: `String`

  Wire command name ("pause", "resume", "stop", etc.), a dynamic string rather than `&'static str`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for StandardControlPayload`

- <span id="standardcontrolpayload-clone"></span>`fn clone(&self) -> StandardControlPayload` — [`StandardControlPayload`](#standardcontrolpayload)

##### `impl Debug for StandardControlPayload`

- <span id="standardcontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardControlPayload`

- <span id="standardcontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StandardControlRequest`

```rust
struct StandardControlRequest {
    pub print: StandardControlPayload,
}
```

Sends a print lifecycle command (pause, resume, stop) to the printer.

#### Fields

- **`print`**: `StandardControlPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="standardcontrolrequest-new"></span>`fn new(command: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a control request for the given lifecycle command string ("pause", "resume", "stop").

#### Trait Implementations

##### `impl Clone for StandardControlRequest`

- <span id="standardcontrolrequest-clone"></span>`fn clone(&self) -> StandardControlRequest` — [`StandardControlRequest`](#standardcontrolrequest)

##### `impl Debug for StandardControlRequest`

- <span id="standardcontrolrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardControlRequest`

- <span id="standardcontrolrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

