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
- `command: &'static str`
- `option: u32` - Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].
- `sequence_id: String`

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
- `print: CalibrationPayload`

**Methods:**

- `fn new(option_bitmask: u32, sequence_id: u64) -> Self`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> CalibrationRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::control::CleanPrintErrorPayload

*Struct*

Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

**Fields:**
- `command: &'static str`
- `sequence_id: String`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> CleanPrintErrorPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::CleanPrintErrorRequest

*Struct*

Clears the printer's current error state so it can resume operation.

**Fields:**
- `print: CleanPrintErrorPayload`

**Methods:**

- `fn new(sequence_id: u64) -> Self`

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> CleanPrintErrorRequest`



## bambino::mqtt::commands::control::PrintSpeedPayload

*Struct*

Dynamically scales maximum movement velocity and acceleration limits.

**Fields:**
- `command: &'static str`
- `param: String` - Target speed scaling index serialized as string:
- `sequence_id: String`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> PrintSpeedPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::control::PrintSpeedRequest

*Struct*

Changes the active print speed profile (silent, standard, sport, ludicrous).

**Fields:**
- `print: PrintSpeedPayload`

**Methods:**

- `fn new(speed_index_str: &str, sequence_id: u64) -> Self`

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
- `command: &'static str`
- `obj_list: Vec<u32>`
- `sequence_id: String`

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
- `print: SkipObjectsPayload`

**Methods:**

- `fn new(object_indices: Vec<u32>, sequence_id: u64) -> Self`

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
- `command: String`
- `sequence_id: String`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> StandardControlPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::control::StandardControlRequest

*Struct*

Sends a print lifecycle command (pause, resume, stop) to the printer.

**Fields:**
- `print: StandardControlPayload`

**Methods:**

- `fn new(command: &str, sequence_id: u64) -> Self`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> StandardControlRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



