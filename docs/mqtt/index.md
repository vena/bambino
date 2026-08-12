*[bambino](../index.md) / [mqtt](index.md)*

---

# Module `mqtt`

# MQTT Client & Command Serialization

Low-level MQTT v3.1.1 implementation for talking to Bambu Lab printers.

[`MqttClient`](client/index.md#mqttclient) handles the connection handshake, QoS 1 publish/subscribe,
keep-alive pings, and zombie detection. The [`commands`](commands/index.md#commands) submodule contains all
the serializable request structs (G-code dispatch, print control, AMS operations,
LED/fan/buzzer commands, etc.) that get published to the printer's command topic.

Most users should use [`PrinterClient`](../client/index.md#printerclient) instead of this module
directly — it wraps `MqttClient` with higher-level methods and safety checks.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`client`](#client) | mod | # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session |
| [`commands`](#commands) | mod | # MQTT Command Payloads & Serialization Builders |

## Modules

- [`client`](client/index.md#client) — # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session
- [`commands`](commands/index.md#commands) — # MQTT Command Payloads & Serialization Builders


---

## Types

### `MqttClient<IO: AsyncIo>`

```rust
struct MqttClient<IO: AsyncIo> {
    // [REDACTED: Private Fields]
}
```

Lightweight MQTT client session running over an established `AsyncIo` stream.

#### Implementations

- <span id="mqttclient-connect"></span>`async fn connect(stream: IO, identity: &PrinterIdentity) -> Result<Self, Error>` — [`PrinterIdentity`](../identity/index.md#printeridentity), [`Error`](../error/index.md#error)

  Executes a secure local network connection handshake and subscription loop with the printer.

- <span id="mqttclient-serial"></span>`fn serial(&self) -> &str`

  Returns the serial number this client authenticated with (`connect()`'s `serial` argument).

- <span id="mqttclient-publish-command"></span>`async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Submits a serialized JSON command payload to the printer's request channel.

- <span id="mqttclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<MqttMessage, Error>` — [`MqttMessage`](client/index.md#mqttmessage), [`Error`](../error/index.md#error)

  Returns the next MQTT message, draining any buffered messages first.

- <span id="mqttclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.

- <span id="mqttclient-tick-zombie-check"></span>`fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Platform-agnostic timer tick update.

- <span id="mqttclient-in-flight-count"></span>`fn in_flight_count(&self) -> usize`

  Returns the number of current un-acknowledged QoS 1 packets.

#### Trait Implementations

### `MqttMessage`

```rust
struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}
```

Incoming MQTT message details parsed from the wire.

#### Fields

- **`topic`**: `String`

  Full MQTT topic string the message arrived on (e.g. "device/{serial}/report").

- **`payload`**: `Vec<u8>`

  Raw JSON payload bytes as received off the wire.

#### Trait Implementations

##### `impl Clone for MqttMessage`

- <span id="mqttmessage-clone"></span>`fn clone(&self) -> MqttMessage` — [`MqttMessage`](client/index.md#mqttmessage)

##### `impl Debug for MqttMessage`

- <span id="mqttmessage-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `AirductRequest`

```rust
struct AirductRequest {
    pub print: AirductPayload,
}
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

#### Fields

- **`print`**: `AirductPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="airductrequest-new"></span>`fn new(mode: AirductMode, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`AirductMode`](commands/hardware/index.md#airductmode), [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `set_airduct` request for the given damper mode.

#### Trait Implementations

##### `impl Clone for AirductRequest`

- <span id="airductrequest-clone"></span>`fn clone(&self) -> AirductRequest` — [`AirductRequest`](commands/hardware/index.md#airductrequest)

##### `impl Debug for AirductRequest`

- <span id="airductrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirductRequest`

- <span id="airductrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsChangeFilamentRequest`

```rust
struct AmsChangeFilamentRequest {
    pub print: AmsChangeFilamentPayload,
}
```

Loads or unloads filament from an AMS slot or external spool to the toolhead.

#### Fields

- **`print`**: `AmsChangeFilamentPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amschangefilamentrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, target: i32, curr_temp: i32, tar_temp: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds an `ams_change_filament` request to load or unload filament.

#### Trait Implementations

##### `impl Clone for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-clone"></span>`fn clone(&self) -> AmsChangeFilamentRequest` — [`AmsChangeFilamentRequest`](commands/ams/index.md#amschangefilamentrequest)

##### `impl Debug for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsChangeFilamentRequest`

- <span id="amschangefilamentrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsControlRequest`

```rust
struct AmsControlRequest {
    pub print: AmsControlPayload,
}
```

Sends a resume, pause, or reset command to the AMS feed mechanism.

#### Fields

- **`print`**: `AmsControlPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amscontrolrequest-new"></span>`fn new(operation: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds an `ams_control` request for the given operation ("resume", "pause", etc.).

#### Trait Implementations

##### `impl Clone for AmsControlRequest`

- <span id="amscontrolrequest-clone"></span>`fn clone(&self) -> AmsControlRequest` — [`AmsControlRequest`](commands/ams/index.md#amscontrolrequest)

##### `impl Debug for AmsControlRequest`

- <span id="amscontrolrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsControlRequest`

- <span id="amscontrolrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentDryingRequest`

```rust
struct AmsFilamentDryingRequest {
    pub print: AmsFilamentDryingPayload,
}
```

Starts or stops a filament drying cycle on an AMS unit with a built-in heater.

#### Fields

- **`print`**: `AmsFilamentDryingPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsfilamentdryingrequest-new"></span>`fn new(ams_id: i32, mode: i32, filament: &str, temp: u32, duration_hours: u32, humidity: u32, rotate_tray: bool, cooling_temp: i32, close_power_conflict: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds an `ams_filament_drying` request.

#### Trait Implementations

##### `impl Clone for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-clone"></span>`fn clone(&self) -> AmsFilamentDryingRequest` — [`AmsFilamentDryingRequest`](commands/ams/index.md#amsfilamentdryingrequest)

##### `impl Debug for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentDryingRequest`

- <span id="amsfilamentdryingrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsFilamentSettingRequest`

```rust
struct AmsFilamentSettingRequest {
    pub print: AmsFilamentSettingPayload,
}
```

Sets filament properties (type, color, temperature range) on an AMS tray or external spool.

#### Fields

- **`print`**: `AmsFilamentSettingPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsfilamentsettingrequest-new"></span>`fn new(ams_id: i32, tray_id: i32, preset_code: &str, material_type: &str, sub_brands: Option<&str>, color_hex: &str, temp_min: u32, temp_max: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Creates a request payload to update slot parameters.

#### Trait Implementations

##### `impl Clone for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-clone"></span>`fn clone(&self) -> AmsFilamentSettingRequest` — [`AmsFilamentSettingRequest`](commands/ams/index.md#amsfilamentsettingrequest)

##### `impl Debug for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsFilamentSettingRequest`

- <span id="amsfilamentsettingrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AmsGetRfidRequest`

```rust
struct AmsGetRfidRequest {
    pub print: AmsGetRfidPayload,
}
```

Requests an RFID tag scan on a specific AMS slot.

#### Fields

- **`print`**: `AmsGetRfidPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="amsgetrfidrequest-new"></span>`fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds an `ams_get_rfid` request.

#### Trait Implementations

##### `impl Clone for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-clone"></span>`fn clone(&self) -> AmsGetRfidRequest` — [`AmsGetRfidRequest`](commands/ams/index.md#amsgetrfidrequest)

##### `impl Debug for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AmsGetRfidRequest`

- <span id="amsgetrfidrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BuzzerRequest`

```rust
struct BuzzerRequest {
    pub print: BuzzerPayload,
}
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

#### Fields

- **`print`**: `BuzzerPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="buzzerrequest-new"></span>`fn new(mode_code: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `buzzer_ctrl` request for the given alarm mode.

#### Trait Implementations

##### `impl Clone for BuzzerRequest`

- <span id="buzzerrequest-clone"></span>`fn clone(&self) -> BuzzerRequest` — [`BuzzerRequest`](commands/hardware/index.md#buzzerrequest)

##### `impl Debug for BuzzerRequest`

- <span id="buzzerrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for BuzzerRequest`

- <span id="buzzerrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="calibrationrequest-new"></span>`fn new(option_bitmask: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `calibration` request from a capability option bitmask.

#### Trait Implementations

##### `impl Clone for CalibrationRequest`

- <span id="calibrationrequest-clone"></span>`fn clone(&self) -> CalibrationRequest` — [`CalibrationRequest`](commands/control/index.md#calibrationrequest)

##### `impl Debug for CalibrationRequest`

- <span id="calibrationrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CalibrationRequest`

- <span id="calibrationrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="cleanprinterrorrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `clean_print_error` request.

#### Trait Implementations

##### `impl Clone for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-clone"></span>`fn clone(&self) -> CleanPrintErrorRequest` — [`CleanPrintErrorRequest`](commands/control/index.md#cleanprinterrorrequest)

##### `impl Debug for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for CleanPrintErrorRequest`

- <span id="cleanprinterrorrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GCodeRequest`

```rust
struct GCodeRequest {
    pub print: GCodePayload,
}
```

Sends a raw G-code line to the printer for immediate execution.

#### Fields

- **`print`**: `GCodePayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="gcoderequest-new"></span>`fn new(gcode_line: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Creates a request envelope wrapping a raw G-code payload.

#### Trait Implementations

##### `impl Clone for GCodeRequest`

- <span id="gcoderequest-clone"></span>`fn clone(&self) -> GCodeRequest` — [`GCodeRequest`](commands/gcode/index.md#gcoderequest)

##### `impl Debug for GCodeRequest`

- <span id="gcoderequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GCodeRequest`

- <span id="gcoderequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GetVersionRequest`

```rust
struct GetVersionRequest {
    pub info: GetVersionPayload,
}
```

Queries the printer for its hardware and firmware version info.

#### Fields

- **`info`**: `GetVersionPayload`

  The `info` namespace envelope required by the wire protocol.

#### Implementations

- <span id="getversionrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `get_version` request.

#### Trait Implementations

##### `impl Clone for GetVersionRequest`

- <span id="getversionrequest-clone"></span>`fn clone(&self) -> GetVersionRequest` — [`GetVersionRequest`](commands/status/index.md#getversionrequest)

##### `impl Debug for GetVersionRequest`

- <span id="getversionrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GetVersionRequest`

- <span id="getversionrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `LedCtrlRequest`

```rust
struct LedCtrlRequest {
    pub system: LedCtrlPayload,
}
```

Turns chamber or toolhead LEDs on or off.

#### Fields

- **`system`**: `LedCtrlPayload`

  The `system` namespace envelope required by the wire protocol.

#### Implementations

- <span id="ledctrlrequest-new"></span>`fn new(led_node: &str, turn_on: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a simple on/off `ledctrl` request for the given fixture.

- <span id="ledctrlrequest-new-flashing"></span>`fn new_flashing(led_node: &str, on_time: u32, off_time: u32, loop_times: u32, interval_time: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a flashing-mode LED command with explicit on/off/loop/interval timing (`led_mode: "flashing"`), per [REF-MQTT-LIFECYCLE].

#### Trait Implementations

##### `impl Clone for LedCtrlRequest`

- <span id="ledctrlrequest-clone"></span>`fn clone(&self) -> LedCtrlRequest` — [`LedCtrlRequest`](commands/hardware/index.md#ledctrlrequest)

##### `impl Debug for LedCtrlRequest`

- <span id="ledctrlrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for LedCtrlRequest`

- <span id="ledctrlrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintJobConfig`

```rust
struct PrintJobConfig {
    pub job_filename: String,
    pub plate_gcode_path: String,
    pub subtask_name: String,
    pub raw_subtask_id: u64,
    pub bed_type: String,
    pub bed_leveling: CalibrationMode,
    pub run_flow_calibration: CalibrationMode,
    pub run_vibration_compensation: bool,
    pub timelapse: bool,
    pub layer_inspect: bool,
    pub nozzle_offset_cali: Option<CalibrationMode>,
    pub use_ams: bool,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Option<Vec<crate::ams::mapping::AmsMapping2Entry>>,
}
```

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
with named fields and sensible defaults for calibration flags.

#### Fields

- **`job_filename`**: `String`

  Filename of the `.3mf` file on SD card storage (e.g. "job.3mf").

- **`plate_gcode_path`**: `String`

  Sliced plate gcode path inside the `.3mf` (e.g. "Metadata/plate_1.gcode").

- **`subtask_name`**: `String`

  User-friendly label for the print queue task.

- **`raw_subtask_id`**: `u64`

  Unique 32-bit tracking identifier before clamping (see `ClampedTaskId`).

- **`bed_type`**: `String`

  Bed plate type (e.g. "textured", "smooth").

- **`bed_leveling`**: `CalibrationMode`

  Whether to run automatic bed leveling before the print.

- **`run_flow_calibration`**: `CalibrationMode`

  Whether to run dynamic flow calibration before the print.

- **`run_vibration_compensation`**: `bool`

  Whether to run vibration compensation calibration before the print.

- **`timelapse`**: `bool`

  Whether timelapse capture is enabled.

- **`layer_inspect`**: `bool`

  Whether to run first-layer inspection during the print.

- **`nozzle_offset_cali`**: `Option<CalibrationMode>`

  `None` defers to the quirks engine default in `PrinterClient::start_print()`.

- **`use_ams`**: `bool`

  Whether to route filament through the AMS rather than an external spool.

- **`ams_mapping`**: `Vec<i32>`

  Flat AMS slot mapping (one entry per plate object, -1 = no AMS slot).

- **`ams_mapping2`**: `Option<Vec<crate::ams::mapping::AmsMapping2Entry>>`

  Structured per-nozzle AMS mapping; takes precedence over `ams_mapping` when set.

#### Implementations

- <span id="printjobconfig-new"></span>`fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self`

  Builds a job config with calibration flags defaulted on and AMS disabled.

- <span id="printjobconfig-with-ams"></span>`fn with_ams(self, mapping: Vec<i32>) -> Self`

  Enables AMS and sets the flat slot-mapping array (`ams_mapping`).

- <span id="printjobconfig-with-ams-mapping2"></span>`fn with_ams_mapping2(self, mapping2: Vec<AmsMapping2Entry>) -> Self` — [`AmsMapping2Entry`](../ams/mapping/index.md#amsmapping2entry)

  Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`).

- <span id="printjobconfig-bed-leveling"></span>`fn bed_leveling(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Enables or disables automatic bed leveling for this job.

- <span id="printjobconfig-flow-calibration"></span>`fn flow_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Enables or disables flow calibration for this job.

- <span id="printjobconfig-vibration-compensation"></span>`fn vibration_compensation(self, enabled: bool) -> Self`

  Enables or disables vibration compensation calibration for this job.

- <span id="printjobconfig-timelapse"></span>`fn timelapse(self, enabled: bool) -> Self`

  Enables or disables timelapse capture for this job.

- <span id="printjobconfig-layer-inspect"></span>`fn layer_inspect(self, enabled: bool) -> Self`

  Enables or disables first-layer inspection for this job.

- <span id="printjobconfig-nozzle-offset-calibration"></span>`fn nozzle_offset_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Overrides the model's default nozzle-offset-calibration behavior for this job.

#### Trait Implementations

##### `impl Clone for PrintJobConfig`

- <span id="printjobconfig-clone"></span>`fn clone(&self) -> PrintJobConfig` — [`PrintJobConfig`](commands/print_job/index.md#printjobconfig)

##### `impl Debug for PrintJobConfig`

- <span id="printjobconfig-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

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

- <span id="printspeedrequest-new"></span>`fn new(speed_index_str: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `print_speed` request from a stringified speed index.

#### Trait Implementations

##### `impl Clone for PrintSpeedRequest`

- <span id="printspeedrequest-clone"></span>`fn clone(&self) -> PrintSpeedRequest` — [`PrintSpeedRequest`](commands/control/index.md#printspeedrequest)

##### `impl Debug for PrintSpeedRequest`

- <span id="printspeedrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PrintSpeedRequest`

- <span id="printspeedrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `ProjectFileRequest`

```rust
struct ProjectFileRequest {
    pub print: ProjectFilePayload,
}
```

Submits a `.3mf` print job from the SD card for execution.

#### Fields

- **`print`**: `ProjectFilePayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="projectfilerequest-from-config"></span>`fn from_config(config: &PrintJobConfig, sequence_id: impl Into<ClampedTaskId>, model: PrinterModel) -> Self` — [`PrintJobConfig`](commands/print_job/index.md#printjobconfig), [`ClampedTaskId`](commands/index.md#clampedtaskid), [`PrinterModel`](../models/index.md#printermodel)

  Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.

#### Trait Implementations

##### `impl Clone for ProjectFileRequest`

- <span id="projectfilerequest-clone"></span>`fn clone(&self) -> ProjectFileRequest` — [`ProjectFileRequest`](commands/print_job/index.md#projectfilerequest)

##### `impl Debug for ProjectFileRequest`

- <span id="projectfilerequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for ProjectFileRequest`

- <span id="projectfilerequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PromptSoundRequest`

```rust
struct PromptSoundRequest {
    pub print: PromptSoundPayload,
}
```

Enables or disables the printer's notification sounds.

#### Fields

- **`print`**: `PromptSoundPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="promptsoundrequest-new"></span>`fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `print_option` request enabling or disabling notification sounds.

#### Trait Implementations

##### `impl Clone for PromptSoundRequest`

- <span id="promptsoundrequest-clone"></span>`fn clone(&self) -> PromptSoundRequest` — [`PromptSoundRequest`](commands/hardware/index.md#promptsoundrequest)

##### `impl Debug for PromptSoundRequest`

- <span id="promptsoundrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PromptSoundRequest`

- <span id="promptsoundrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PushAllRequest`

```rust
struct PushAllRequest {
    pub pushing: PushAllPayload,
}
```

Requests a full state dump from the printer (all telemetry fields at once).

#### Fields

- **`pushing`**: `PushAllPayload`

  The `pushing` namespace envelope required by the wire protocol.

#### Implementations

- <span id="pushallrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `pushall` request.

#### Trait Implementations

##### `impl Clone for PushAllRequest`

- <span id="pushallrequest-clone"></span>`fn clone(&self) -> PushAllRequest` — [`PushAllRequest`](commands/status/index.md#pushallrequest)

##### `impl Debug for PushAllRequest`

- <span id="pushallrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PushAllRequest`

- <span id="pushallrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="skipobjectsrequest-new"></span>`fn new(object_indices: Vec<u32>, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a `skip_objects` request from a list of object indices to skip.

#### Trait Implementations

##### `impl Clone for SkipObjectsRequest`

- <span id="skipobjectsrequest-clone"></span>`fn clone(&self) -> SkipObjectsRequest` — [`SkipObjectsRequest`](commands/control/index.md#skipobjectsrequest)

##### `impl Debug for SkipObjectsRequest`

- <span id="skipobjectsrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SkipObjectsRequest`

- <span id="skipobjectsrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="standardcontrolrequest-new"></span>`fn new(command: &str, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

  Builds a control request for the given lifecycle command string ("pause", "resume", "stop").

#### Trait Implementations

##### `impl Clone for StandardControlRequest`

- <span id="standardcontrolrequest-clone"></span>`fn clone(&self) -> StandardControlRequest` — [`StandardControlRequest`](commands/control/index.md#standardcontrolrequest)

##### `impl Debug for StandardControlRequest`

- <span id="standardcontrolrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StandardControlRequest`

- <span id="standardcontrolrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirductMode`

```rust
enum AirductMode {
    Cooling,
    Heating,
    Laser,
}
```

Airduct damper operating mode [REF-MQTT-LIFECYCLE].

`Cooling` (0): closes internal recirculation dampers, routes hot air out through exhaust.
`Heating` (1): closes exhaust flaps, seals enclosure for heat retention.
`Laser` (2): configuration for laser engraving module operation.

#### Variants

- **`Cooling`**

  Closes internal recirculation dampers, routes hot air out through exhaust.

- **`Heating`**

  Seals enclosure, closes exhaust flaps for heat retention.

- **`Laser`**

  Laser engraving module configuration.

#### Trait Implementations

##### `impl Clone for AirductMode`

- <span id="airductmode-clone"></span>`fn clone(&self) -> AirductMode` — [`AirductMode`](commands/hardware/index.md#airductmode)

##### `impl Copy for AirductMode`

##### `impl Debug for AirductMode`

- <span id="airductmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirductMode`

##### `impl PartialEq for AirductMode`

- <span id="airductmode-partialeq-eq"></span>`fn eq(&self, other: &AirductMode) -> bool` — [`AirductMode`](commands/hardware/index.md#airductmode)

### `AmsMappingTable`

```rust
enum AmsMappingTable {
    Inactive(String),
    Active(Vec<i32>),
}
```

Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

**The Polymorphic Mapping Rule:**
* When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
* When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).

Utilizing an untagged enum ensures standard JSON compliance across all execution profiles.

#### Variants

- **`Inactive`**

  External-spool mode: serializes to an empty string.

- **`Active`**

  AMS active mode: serializes to an integer slot-mapping array.

#### Trait Implementations

##### `impl Clone for AmsMappingTable`

- <span id="amsmappingtable-clone"></span>`fn clone(&self) -> AmsMappingTable` — [`AmsMappingTable`](commands/print_job/index.md#amsmappingtable)

##### `impl Debug for AmsMappingTable`

- <span id="amsmappingtable-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsMappingTable`

##### `impl PartialEq for AmsMappingTable`

- <span id="amsmappingtable-partialeq-eq"></span>`fn eq(&self, other: &AmsMappingTable) -> bool` — [`AmsMappingTable`](commands/print_job/index.md#amsmappingtable)

##### `impl Serialize for AmsMappingTable`

- <span id="amsmappingtable-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `CalibrationMode`

```rust
enum CalibrationMode {
    Off,
    On,
    Auto,
}
```

Tri-state calibration setting: force every print, skip entirely, or let the firmware decide
based on whether the relevant calibration ran recently [REF-MQTT-LIFECYCLE].

Mirrors BambuStudio's own `getValueInt()` encoding for these fields (confirmed in
`bambu_networking.hpp`'s `auto_bed_leveling` member and `SelectMachine.cpp`'s
`ops_auto`-driven checkboxes): `Off` = 0, `On` = 1, `Auto` = 2 (skip if not needed recently).

#### Variants

- **`Off`**

  Never run this calibration.

- **`On`**

  Always run this calibration.

- **`Auto`**

  Let the firmware run it only if it wasn't done recently.

#### Trait Implementations

##### `impl Clone for CalibrationMode`

- <span id="calibrationmode-clone"></span>`fn clone(&self) -> CalibrationMode` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

##### `impl Copy for CalibrationMode`

##### `impl Debug for CalibrationMode`

- <span id="calibrationmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for CalibrationMode`

- <span id="calibrationmode-default"></span>`fn default() -> CalibrationMode` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

##### `impl Eq for CalibrationMode`

##### `impl PartialEq for CalibrationMode`

- <span id="calibrationmode-partialeq-eq"></span>`fn eq(&self, other: &CalibrationMode) -> bool` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)


---

## Functions

### `clamp_task_id`

```rust
fn clamp_task_id(raw_id: u64) -> u32
```

Wraps a 64-bit transaction or tracking identifier (typically standard UNIX epoch milliseconds) into the strict boundary limits of a 32-bit signed integer (`2147483647`) via modulo, not saturation.

**Why this is critical [REF-MQTT-ENV]:**
The printer's onboard G-code parsing routine clamps subtask identifiers to standard 32-bit
signed integer limits. If a connecting client uses an un-clamped millisecond epoch (13-digit integer),
the memory allocation registers on the motion board will overflow. This causes the printer to lock
indefinitely in an `IDLE` state and reject all subsequent print dispatches.

The modulo semantics are deliberate (`client/mod.rs`'s `next_sequence_id()` wants
continuation across the wraparound, not a reset to a fixed ceiling) — `clamp_task_id(TASK_ID_MAX)
== 0`, asserted by `test_clamp_task_id_wraps_near_max` below.

