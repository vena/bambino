*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [diagnostics](index.md)*

---

# Module `diagnostics`

Diagnostic telemetry types (HMS alerts, light reports).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CtcInfo`](#ctcinfo) | struct | Controller information segment detailing current temperature coordinates. |
| [`CtcTelemetry`](#ctctelemetry) | struct | Chamber Temperature Controller (CTC) telemetry sub-object. |
| [`HmsEntry`](#hmsentry) | struct | Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS]. |
| [`IpcamTelemetry`](#ipcamtelemetry) | struct | Camera and recording state telemetry, nested as `print.ipcam` on the wire. |

## Types

### `CtcInfo`

```rust
struct CtcInfo {
    pub temp: Option<u32>,
    pub target: Option<u32>,
}
```

Controller information segment detailing current temperature coordinates.

#### Fields

- **`temp`**: `Option<u32>`

  Composite-packed integer temperature value [REF-THER-DECODE].
  Use `PrinterTelemetry::unpack_temperature()` on this value cast to `f64`.

- **`target`**: `Option<u32>`

  Explicit CTC target temperature (authoritative on new-gen models).

#### Trait Implementations

##### `impl Clone for CtcInfo`

- <span id="ctcinfo-clone"></span>`fn clone(&self) -> CtcInfo` — [`CtcInfo`](#ctcinfo)

##### `impl Debug for CtcInfo`

- <span id="ctcinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for CtcInfo`

- <span id="ctcinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for CtcInfo`

##### `impl Serialize for CtcInfo`

- <span id="ctcinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CtcTelemetry`

```rust
struct CtcTelemetry {
    pub info: Option<CtcInfo>,
    pub state: Option<u32>,
}
```

Chamber Temperature Controller (CTC) telemetry sub-object.

#### Fields

- **`info`**: `Option<CtcInfo>`

  Controller info containing thermal actuals and targets.

- **`state`**: `Option<u32>`

  CTC controller state (0 = idle, 2 = heating).

#### Trait Implementations

##### `impl Clone for CtcTelemetry`

- <span id="ctctelemetry-clone"></span>`fn clone(&self) -> CtcTelemetry` — [`CtcTelemetry`](#ctctelemetry)

##### `impl Debug for CtcTelemetry`

- <span id="ctctelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for CtcTelemetry`

- <span id="ctctelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for CtcTelemetry`

##### `impl Serialize for CtcTelemetry`

- <span id="ctctelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `HmsEntry`

```rust
struct HmsEntry {
    pub attr: u32,
    pub code: u32,
    pub ts_boot: Option<u64>,
    pub ts_unix: Option<String>,
}
```

Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].

Each entry represents an active hardware fault or status indication. Use
`diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.

#### Fields

- **`attr`**: `u32`

  Packed attribute word encoding module ID, severity, and subsystem address.

- **`code`**: `u32`

  Packed code word encoding fault category and error index.

- **`ts_boot`**: `Option<u64>`

  Seconds since boot when the alert was raised (confirmed present on X2 only; unverified on H2/P2).

- **`ts_unix`**: `Option<String>`

  UTC timestamp string when the alert was raised (e.g. `"20260426002648"`).

#### Trait Implementations

##### `impl Clone for HmsEntry`

- <span id="hmsentry-clone"></span>`fn clone(&self) -> HmsEntry` — [`HmsEntry`](#hmsentry)

##### `impl Debug for HmsEntry`

- <span id="hmsentry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for HmsEntry`

- <span id="hmsentry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for HmsEntry`

##### `impl Serialize for HmsEntry`

- <span id="hmsentry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `IpcamTelemetry`

```rust
struct IpcamTelemetry {
    pub ipcam_dev: Option<String>,
    pub ipcam_record: Option<String>,
    pub timelapse: Option<String>,
    pub mode_bits: Option<u32>,
    pub resolution: Option<String>,
    pub tutk_server: Option<String>,
    pub rtsp_url: Option<String>,
}
```

Camera and recording state telemetry, nested as `print.ipcam` on the wire.

#### Fields

- **`ipcam_dev`**: `Option<String>`

  Internal identifier or state of the hardware camera module.

- **`ipcam_record`**: `Option<String>`

  Camera live feed recording status (`"enable"` or `"disable"`).

- **`timelapse`**: `Option<String>`

  Frame-by-layer timelapse recording status (`"enable"` or `"disable"`).

- **`mode_bits`**: `Option<u32>`

  Camera mode bitmask.

- **`resolution`**: `Option<String>`

  Camera resolution setting.

- **`tutk_server`**: `Option<String>`

  TUTK server status (`"enable"` or `"disable"`).

- **`rtsp_url`**: `Option<String>`

  RTSP streaming URL (e.g. `"rtsps://192.168.1.64/streaming/live/1"`).

#### Trait Implementations

##### `impl Clone for IpcamTelemetry`

- <span id="ipcamtelemetry-clone"></span>`fn clone(&self) -> IpcamTelemetry` — [`IpcamTelemetry`](#ipcamtelemetry)

##### `impl Debug for IpcamTelemetry`

- <span id="ipcamtelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for IpcamTelemetry`

- <span id="ipcamtelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for IpcamTelemetry`

##### `impl Serialize for IpcamTelemetry`

- <span id="ipcamtelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

