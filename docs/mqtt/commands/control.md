**bambino > mqtt > commands > control**

# Module: mqtt::commands::control

## Contents

**Structs**

- [`CalibrationPayload`](#calibrationpayload) - Triggers automated physical resonance compensation sweeps and chassis alignments.
- [`CalibrationRequest`](#calibrationrequest) - Kicks off a calibration routine (vibration compensation, bed leveling, etc.).
- [`CleanPrintErrorPayload`](#cleanprinterrorpayload) - Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
- [`CleanPrintErrorRequest`](#cleanprinterrorrequest) - Clears the printer's current error state so it can resume operation.
- [`PrintSpeedPayload`](#printspeedpayload) - Dynamically scales maximum movement velocity and acceleration limits.
- [`PrintSpeedRequest`](#printspeedrequest) - Changes the active print speed profile (silent, standard, sport, ludicrous).
- [`SkipObjectsPayload`](#skipobjectspayload) - Instructs the printer to bypass rendering specific objects within active multi-model jobs.
- [`SkipObjectsRequest`](#skipobjectsrequest) - Tells the printer to skip specific objects in a multi-object print.
- [`StandardControlPayload`](#standardcontrolpayload) - General control payload used for pause, resume, stop, and clean actions.
- [`StandardControlRequest`](#standardcontrolrequest) - Sends a print lifecycle command (pause, resume, stop) to the printer.

---

## bambino::mqtt::commands::control::CalibrationPayload

*Struct*

Triggers automated physical resonance compensation sweeps and chassis alignments.

**Fields:**
- `command: &'static str` - Wire command name, always `"calibration"`.
- `option: u32` - Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> CalibrationPayload`



## bambino::mqtt::commands::control::CalibrationRequest

*Struct*

Kicks off a calibration routine (vibration compensation, bed leveling, etc.).

**Fields:**
- `print: CalibrationPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(option_bitmask: u32, sequence_id: u64) -> Self` - Builds a `calibration` request from a capability option bitmask.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> CalibrationRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::CleanPrintErrorPayload

*Struct*

Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

**Fields:**
- `command: &'static str` - Wire command name, always `"clean_print_error"`.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> CleanPrintErrorPayload`



## bambino::mqtt::commands::control::CleanPrintErrorRequest

*Struct*

Clears the printer's current error state so it can resume operation.

**Fields:**
- `print: CleanPrintErrorPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(sequence_id: u64) -> Self` - Builds a `clean_print_error` request.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> CleanPrintErrorRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::PrintSpeedPayload

*Struct*

Dynamically scales maximum movement velocity and acceleration limits.

**Fields:**
- `command: &'static str` - Wire command name, always `"print_speed"`.
- `param: String` - Target speed scaling index serialized as string:
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> PrintSpeedPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::control::PrintSpeedRequest

*Struct*

Changes the active print speed profile (silent, standard, sport, ludicrous).

**Fields:**
- `print: PrintSpeedPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(speed_index_str: &str, sequence_id: u64) -> Self` - Builds a `print_speed` request from a stringified speed index.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> PrintSpeedRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::SkipObjectsPayload

*Struct*

Instructs the printer to bypass rendering specific objects within active multi-model jobs.

**Fields:**
- `command: &'static str` - Wire command name, always `"skip_objects"`.
- `obj_list: Vec<u32>` - List of object indices (as sliced) to skip rendering.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> SkipObjectsPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::SkipObjectsRequest

*Struct*

Tells the printer to skip specific objects in a multi-object print.

**Fields:**
- `print: SkipObjectsPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(object_indices: Vec<u32>, sequence_id: u64) -> Self` - Builds a `skip_objects` request from a list of object indices to skip.

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> SkipObjectsRequest`



## bambino::mqtt::commands::control::StandardControlPayload

*Struct*

General control payload used for pause, resume, stop, and clean actions.

**Fields:**
- `command: String` - Wire command name ("pause", "resume", "stop", etc.), a dynamic string rather than `&'static str`.
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> StandardControlPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::StandardControlRequest

*Struct*

Sends a print lifecycle command (pause, resume, stop) to the printer.

**Fields:**
- `print: StandardControlPayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(command: &str, sequence_id: u64) -> Self` - Builds a control request for the given lifecycle command string ("pause", "resume", "stop").

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> StandardControlRequest`



