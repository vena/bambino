**bambino > types > telemetry > diagnostics**

# Module: types::telemetry::diagnostics

## Contents

**Structs**

- [`CtcInfo`](#ctcinfo) - Controller information segment detailing current temperature coordinates.
- [`CtcTelemetry`](#ctctelemetry) - Chamber Temperature Controller (CTC) telemetry sub-object.
- [`HmsEntry`](#hmsentry) - Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].
- [`IpcamTelemetry`](#ipcamtelemetry) - Camera and recording state telemetry, nested as `print.ipcam` on the wire.

---

## bambino::types::telemetry::diagnostics::CtcInfo

*Struct*

Controller information segment detailing current temperature coordinates.

**Fields:**
- `temp: Option<u32>` - Composite-packed integer temperature value [REF-THER-DECODE].
- `target: Option<u32>` - Explicit CTC target temperature (authoritative on new-gen models).

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> CtcInfo`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## bambino::types::telemetry::diagnostics::CtcTelemetry

*Struct*

Chamber Temperature Controller (CTC) telemetry sub-object.

**Fields:**
- `info: Option<CtcInfo>` - Controller info containing thermal actuals and targets.
- `state: Option<u32>` - CTC controller state (0 = idle, 2 = heating).

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> CtcTelemetry`



## bambino::types::telemetry::diagnostics::HmsEntry

*Struct*

Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].

Each entry represents an active hardware fault or status indication. Use
`diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.

**Fields:**
- `attr: u32` - Packed attribute word encoding module ID, severity, and subsystem address.
- `code: u32` - Packed code word encoding fault category and error index.
- `ts_boot: Option<u64>` - Seconds since boot when the alert was raised (present on X2/H2/P2 models).
- `ts_unix: Option<String>` - UTC timestamp string when the alert was raised (e.g. `"20260426002648"`).

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> HmsEntry`



## bambino::types::telemetry::diagnostics::IpcamTelemetry

*Struct*

Camera and recording state telemetry, nested as `print.ipcam` on the wire.

**Fields:**
- `ipcam_dev: Option<String>` - Internal identifier or state of the hardware camera module.
- `ipcam_record: Option<String>` - Camera live feed recording status (`"enable"` or `"disable"`).
- `timelapse: Option<String>` - Frame-by-layer timelapse recording status (`"enable"` or `"disable"`).
- `mode_bits: Option<u32>` - Camera mode bitmask.
- `resolution: Option<String>` - Camera resolution setting.
- `tutk_server: Option<String>` - TUTK server status (`"enable"` or `"disable"`).
- `rtsp_url: Option<String>` - RTSP streaming URL (e.g. `"rtsps://192.168.1.64/streaming/live/1"`).

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> IpcamTelemetry`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



