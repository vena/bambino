*[bambino](../index.md) / [mqtt](index.md)*

---

# Module `mqtt`

# MQTT Client & Command Serialization

Low-level MQTT v3.1.1 implementation for talking to Bambu Lab printers.

[`MqttClient`](client/index.md#mqttclient) handles the connection handshake, QoS 1 publish/subscribe,
keep-alive pings, and zombie detection. Keep-alive pings and read/write deadlines depend on
the [`TimerProvider`](../io/index.md#timerprovider) passed to `poll_telemetry`,
`publish_command` and `send_ping`, so pass a real platform timer. Zombie detection still
needs the caller to call `tick_zombie_check` periodically. The [`commands`](commands/index.md) submodule contains all
the serializable request structs (G-code dispatch, print control, AMS operations,
LED/fan/buzzer commands, etc.) that get published to the printer's command topic.

Most users should use [`PrinterClient`](../client/index.md#printerclient) instead of this module
directly — it wraps `MqttClient` with higher-level methods and safety checks.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`client`](client/index.md) | mod | # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session |
| [`commands`](commands/index.md) | mod | # MQTT Command Payloads & Serialization Builders |
| [`MQTTS_PORT`](#mqtts-port) | const | MQTT-over-TLS port every Bambu printer's local broker listens on. |

## Modules

- [`client`](client/index.md) — # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session
- [`commands`](commands/index.md) — # MQTT Command Payloads & Serialization Builders


---

## Types

### `EchoKey`

```rust
struct EchoKey {
    pub command: String,
    pub sequence_id: u32,
}
```

The `(command, sequence_id)` pair that ties a command to its echo [REF-MQTT-ACK].

Both halves are required: background `push_status` telemetry carries its own independent
`sequence_id` counter under the same shape, so a number match alone can be a different
message.

#### Fields

- **`command`**: `String`

  The wire command name.

- **`sequence_id`**: `u32`

  The `sequence_id`, decoded from whichever form (decimal string or number) it was sent in.

#### Trait Implementations

##### `impl Clone for EchoKey`

- <span id="echokey-clone"></span>`fn clone(&self) -> EchoKey` — [`EchoKey`](client/index.md#echokey)

##### `impl Debug for EchoKey`

- <span id="echokey-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for EchoKey`

##### `impl PartialEq for EchoKey`

- <span id="echokey-partialeq-eq"></span>`fn eq(&self, other: &EchoKey) -> bool` — [`EchoKey`](client/index.md#echokey)

### `MqttClient<IO: AsyncIo>`

```rust
struct MqttClient<IO: AsyncIo> {
    // [REDACTED: Private Fields]
}
```

Lightweight MQTT client session running over an established `AsyncIo` stream.

#### Implementations

- <span id="mqttclient-connect"></span>`async fn connect(stream: IO, serial: &str, access_code: &str) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

  Executes a secure local network connection handshake and subscription loop with the printer.

  **Authentication Note:** If the printer's physical broker rejects credentials due to
  an invalid access code, this function returns `Error::AccessDenied`.

  **Unbounded by design — callers must supply their own deadline.** The CONNECT/CONNACK
  and SUBSCRIBE/SUBACK writes and reads inside this function have no internal timeout
  (`DummyTimer` is used throughout, so a stalled peer hangs this call forever). This is
  safe for `PrinterClient::ensure_mqtt()`, the sole production call site, because it
  wraps the *entire* dial+connect sequence in `race_against_connect_timeout`. A caller
  invoking `MqttClient::connect()` directly (bypassing `PrinterClient`) gets no such
  bound and must wrap this call in its own timeout (e.g. `tokio::time::timeout`) against
  a peer that stalls before CONNACK/SUBACK.

- <span id="mqttclient-serial"></span>`fn serial(&self) -> &str`

  Returns the serial number this client authenticated with.

- <span id="mqttclient-publish-command"></span>`async fn publish_command<T: TimerProvider>(&mut self, payload: &[u8], timer: &T) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Submits a serialized JSON command payload to the printer's request channel.

  **In-flight Bounds Verification:**
  If the unacknowledged queue size equals or exceeds `MQTT_IN_FLIGHT_LIMIT`, this function
  returns [`Error::Backpressure`](../error/index.md#error) without sending, to protect memory space and prevent
  packet drift [REF-MQTT-CONN]. A saturated queue is not a timeout — retrying immediately
  will not clear it; drain it by servicing PUBACKs (`poll_wire`) or let
  `tick_zombie_check` age the entries out.

  Payloads larger than `MQTT_MAX_PAYLOAD_BYTES` are rejected with
  [`Error::ProtocolViolation`](../error/index.md#error) rather than encoded, mirroring the read path's own cap.

  `timer` bounds the write (see `write_frame_with_timer`) and stamps the keepalive clock.
  Pass a real platform timer: a timer without a real clock
  ([`TimerProvider::has_real_clock`](../io/index.md#timerprovider)) makes the write unbounded.

- <span id="mqttclient-poll-telemetry"></span>`async fn poll_telemetry<T: TimerProvider>(&mut self, timer: &T) -> Result<MqttMessage, Error>` — [`MqttMessage`](client/index.md#mqttmessage), [`Error`](../error/index.md#error)

  Returns the next MQTT message, draining any buffered messages first.

  Messages are buffered when request-response methods (e.g. `get_version()`) read
  non-matching messages off the wire while waiting for a specific response. This
  method drains those buffered messages in FIFO order before reading new packets
  from the wire.

  Handles MQTT protocol frames transparently: sends `PUBACK` for incoming QoS 1
  publishes, clears matching packet IDs from the in-flight tracker on `PUBACK`,
  and acknowledges `PINGRESP` — only application-level `PUBLISH` payloads are
  returned.

  `timer` drives the keepalive PINGREQ the CONNECT keepalive obliges this client to send,
  and the 30s per-read deadline. Pass a real platform
  timer: with one that has no real clock ([`TimerProvider::has_real_clock`](../io/index.md#timerprovider)) no keepalive
  is sent, so the broker drops the connection after about 45s of outbound silence, and a
  stalled read blocks forever.

- <span id="mqttclient-send-ping"></span>`async fn send_ping<T: TimerProvider>(&mut self, timer: &T) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.

  `timer` bounds the write (see `write_frame_with_timer`). `poll_telemetry` already pings
  when one is due, so calling this is only needed when not polling.

- <span id="mqttclient-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  Returns true once either side of the stream is permanently desynced.

  The write side poisons when a write fails: it may have put a partial frame on the wire and,
  unlike a read, has no resumable progress state, so every later `publish_command`,
  `send_ping` and automatic PUBACK returns `ConnectionAborted`. The read side poisons on a
  malformed length prefix or an oversized frame, after which every read returns
  `InvalidInput`. Either way the client can never recover; the correct response is to drop
  the connection and reconnect (`PrinterClient::disconnect_mqtt()`), not to retry.

- <span id="mqttclient-tick-zombie-check"></span>`fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Liveness>` — [`Liveness`](client/index.md#liveness)

  Advances the liveness clocks by `elapsed_secs` and reports the first violated condition.

  Two independent conditions, checked in this order:

  1. [`Liveness::WriteZombie`]: a published command has gone [`MQTT_ZOMBIE_TIMEOUT_SECS`](client/index.md#mqtt-zombie-timeout-secs)
     with no answer [REF-MQTT-ZOMBIE].
  2. [`Liveness::Stale`]: no packets of any kind for `MQTT_STALE_CONNECTION_SECS` (60s),
     a silently dropped connection [REF-MQTT-CONN].

  Both mean the connection should be dropped and re-established.

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

### `ClampedTaskId`

```rust
struct ClampedTaskId();
```

A task or sequence id already reduced into the range firmware accepts (below `i32::MAX`).

Every request constructor takes `impl Into<ClampedTaskId>`; the only way to make one is the
clamping `From<u64>`, so an out-of-range id can't reach the wire. Serializes as a decimal
string, the form the printer expects.

#### Implementations

- <span id="clampedtaskid-get"></span>`const fn get(self) -> u32`

  The clamped value.

#### Trait Implementations

##### `impl Clone for ClampedTaskId`

- <span id="clampedtaskid-clone"></span>`fn clone(&self) -> ClampedTaskId` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

##### `impl Copy for ClampedTaskId`

##### `impl Debug for ClampedTaskId`

- <span id="clampedtaskid-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for ClampedTaskId`

- <span id="clampedtaskid-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for ClampedTaskId`

##### `impl Hash for ClampedTaskId`

- <span id="clampedtaskid-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for ClampedTaskId`

- <span id="clampedtaskid-partialeq-eq"></span>`fn eq(&self, other: &ClampedTaskId) -> bool` — [`ClampedTaskId`](commands/index.md#clampedtaskid)

##### `impl Serialize for ClampedTaskId`

- <span id="clampedtaskid-serialize"></span>`fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<<S as >::Ok, <S as >::Error>`

##### `impl ToString for ClampedTaskId`

- <span id="clampedtaskid-tostring-to-string"></span>`fn to_string(&self) -> String`

### `DryingParams`

```rust
struct DryingParams {
    pub filament: String,
    pub temp: u32,
    pub duration_hours: u32,
    pub humidity: u32,
    pub rotate_tray: bool,
    pub cooling_temp: u32,
    pub close_power_conflict: bool,
}
```

Everything a drying-cycle start carries besides the unit and the mode.

`Default` is the all-zero/empty set BambuStudio sends to stop a cycle; a start needs at
least `temp` and `duration_hours`.

#### Fields

- **`filament`**: `String`

  Filament material type being dried (e.g. "PA-CF").

- **`temp`**: `u32`

  Drying temperature (°C).

- **`duration_hours`**: `u32`

  Drying duration in whole hours.

- **`humidity`**: `u32`

  Target humidity (0 = firmware default / no target).

- **`rotate_tray`**: `bool`

  Whether to periodically rotate the tray during drying.

- **`cooling_temp`**: `u32`

  Cooling temperature applied after the cycle; BambuStudio sends the filament's
  softening temperature here.

- **`close_power_conflict`**: `bool`

  Whether to override the AMS unit's power-conflict interlock.

#### Trait Implementations

##### `impl Clone for DryingParams`

- <span id="dryingparams-clone"></span>`fn clone(&self) -> DryingParams` — [`DryingParams`](commands/ams/index.md#dryingparams)

##### `impl Debug for DryingParams`

- <span id="dryingparams-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for DryingParams`

- <span id="dryingparams-default"></span>`fn default() -> DryingParams` — [`DryingParams`](commands/ams/index.md#dryingparams)

##### `impl Eq for DryingParams`

##### `impl PartialEq for DryingParams`

- <span id="dryingparams-partialeq-eq"></span>`fn eq(&self, other: &DryingParams) -> bool` — [`DryingParams`](commands/ams/index.md#dryingparams)

### `NozzleRack`

```rust
struct NozzleRack {
    pub slot_extruders: Vec<i32>,
    pub rack_nozzle_id: i32,
}
```

Tool-changer rack routing for a print job: both inputs [`resolve_rack_nozzle_mapping`](commands/print_job/index.md#resolve-rack-nozzle-mapping) needs.

#### Fields

- **`slot_extruders`**: `Vec<i32>`

  Extruder index per filament slot, negative for unprinted slots.

- **`rack_nozzle_id`**: `i32`

  Physical nozzle ID of the rack position the printer currently reports as live, in
  `RACK_NOZZLE_ID_MIN..=RACK_NOZZLE_ID_MAX` (16..=21). The caller must supply this because
  the mounted hotend can change between slicing and dispatch, and bambino does not model
  rack telemetry.

#### Trait Implementations

##### `impl Clone for NozzleRack`

- <span id="nozzlerack-clone"></span>`fn clone(&self) -> NozzleRack` — [`NozzleRack`](commands/print_job/index.md#nozzlerack)

##### `impl Debug for NozzleRack`

- <span id="nozzlerack-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for NozzleRack`

##### `impl PartialEq for NozzleRack`

- <span id="nozzlerack-partialeq-eq"></span>`fn eq(&self, other: &NozzleRack) -> bool` — [`NozzleRack`](commands/print_job/index.md#nozzlerack)

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
    pub run_vibration_compensation: CalibrationMode,
    pub timelapse: bool,
    pub layer_inspect: bool,
    pub nozzle_offset_cali: Option<CalibrationMode>,
    pub ams: Option<AmsSource>,
    pub nozzle_rack: Option<NozzleRack>,
}
```

Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].

Replaces the positional parameter list on `start_print()` with named fields and sensible
defaults for calibration flags.

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

- **`run_vibration_compensation`**: `CalibrationMode`

  Whether to run vibration compensation calibration before the print. Defaults to `Off`
  on every model, matching BambuStudio, which always sends `false` (see
  `reference/03_mqtt_telemetry.md`, "Default (`vibration_cali`)"). No tri-state companion
  field exists on the wire for this one, so `Auto` serializes identically to `Off` via
  `as_wire_bool()`.

- **`timelapse`**: `bool`

  Whether timelapse capture is enabled.

- **`layer_inspect`**: `bool`

  Whether to run first-layer inspection during the print.

- **`nozzle_offset_cali`**: `Option<CalibrationMode>`

  `None` defers to the quirks engine default in `PrinterClient::start_print()`.

- **`ams`**: `Option<AmsSource>`

  AMS routing; `None` prints from the external spool (`use_ams: false`).

- **`nozzle_rack`**: `Option<NozzleRack>`

  Tool-changer rack routing, set via [`PrintJobConfig::with_nozzle_rack`](commands/print_job/index.md#printjobconfig).
  
  Only consulted on a model whose quirks report `has_nozzle_rack`.

#### Implementations

- <span id="printjobconfig-new"></span>`fn new(job_filename: &str, plate_gcode_path: &str, subtask_name: &str, raw_subtask_id: u64, bed_type: &str) -> Self`

  Builds a job config with these defaults: bed leveling and flow calibration on,
  **timelapse recording and first-layer inspection on**, vibration compensation off, AMS
  disabled, and the model's own nozzle-offset-calibration default.

- <span id="printjobconfig-with-ams"></span>`fn with_ams(self, mapping: Vec<i32>) -> Self`

  Enables AMS and sets the flat slot-mapping array (`ams_mapping`).

  Values outside the documented flat channel space (`0..=15` standard AMS, `128..=135`
  AMS-HT, or `-1` unmapped) are folded to `-1` with a `log::warn!` — firmware rejects
  out-of-range values (254/255 in particular) with a visible error (`0700_8012`/
  `07FF_8012`; `reference/05_materials_ams.md`, "External Spool Flat-Mapping Restrictions").
  The `with_ams_mapping2`-derived path
  already sanitizes via `flat_channel_id_for_entry`; this mirrors it for the raw path
  (issue #56).

  This is a convenience, not the enforcement point: `ams` is a public field, so
  `ProjectFileRequest::from_config` re-runs the same sanitization at serialization time
  (issue #120). Bypassing this builder cannot produce an out-of-range flat channel on the
  wire.

  Replaces any mapping set by [`with_ams_mapping2`](commands/print_job/index.md#printjobconfig).

- <span id="printjobconfig-with-ams-mapping2"></span>`fn with_ams_mapping2(self, mapping2: Vec<AmsMapping2Entry>) -> Self` — [`AmsMapping2Entry`](../ams/mapping/index.md#amsmapping2entry)

  Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`); the flat array is
  derived from them.

  Replaces any mapping set by [`with_ams`](commands/print_job/index.md#printjobconfig).

- <span id="printjobconfig-bed-leveling"></span>`fn bed_leveling(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Enables or disables automatic bed leveling for this job.

- <span id="printjobconfig-flow-calibration"></span>`fn flow_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Enables or disables flow calibration for this job.

- <span id="printjobconfig-vibration-compensation"></span>`fn vibration_compensation(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Enables or disables vibration compensation calibration for this job. No tri-state
  companion field exists on the wire for this one, so `CalibrationMode::Auto` serializes
  identically to `Off`.

- <span id="printjobconfig-timelapse"></span>`fn timelapse(self, enabled: bool) -> Self`

  Enables or disables timelapse capture for this job.

- <span id="printjobconfig-layer-inspect"></span>`fn layer_inspect(self, enabled: bool) -> Self`

  Enables or disables first-layer inspection for this job.

- <span id="printjobconfig-with-nozzle-rack"></span>`fn with_nozzle_rack(self, slot_extruders: Vec<i32>, rack_nozzle_id: i32) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

  Supplies the tool-changer rack routing for this job (H2C only).

  `slot_extruders` is one extruder index per filament slot (negative = slot not printed);
  `rack_nozzle_id` is the physical ID of the live rack position. See
  [`resolve_rack_nozzle_mapping`](commands/print_job/index.md#resolve-rack-nozzle-mapping) for how they combine and when the resulting
  `nozzle_mapping` is deliberately omitted. Ignored entirely on non-rack models.

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) when `rack_nozzle_id` is outside the rack's physical IDs
  (`RACK_NOZZLE_ID_MIN..=RACK_NOZZLE_ID_MAX`), rather than silently sending no mapping.

- <span id="printjobconfig-nozzle-offset-calibration"></span>`fn nozzle_offset_calibration(self, mode: impl Into<CalibrationMode>) -> Self` — [`CalibrationMode`](commands/print_job/index.md#calibrationmode)

  Overrides the model's default nozzle-offset-calibration behavior for this job.

#### Trait Implementations

##### `impl Clone for PrintJobConfig`

- <span id="printjobconfig-clone"></span>`fn clone(&self) -> PrintJobConfig` — [`PrintJobConfig`](commands/print_job/index.md#printjobconfig)

##### `impl Debug for PrintJobConfig`

- <span id="printjobconfig-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `Liveness`

```rust
enum Liveness {
    WriteZombie,
    Stale,
}
```

Which liveness condition [`MqttClient::tick_zombie_check`](client/index.md#mqttclient) found violated.

#### Variants

- **`WriteZombie`**

  A published command has gone [`MQTT_ZOMBIE_TIMEOUT_SECS`](client/index.md#mqtt-zombie-timeout-secs) with no answer [REF-MQTT-ZOMBIE].
  
  The broker may be discarding writes. Telemetry may still be arriving, so the read side can
  look healthy; it is still the signal to reconnect.

- **`Stale`**

  Nothing at all has arrived for `MQTT_STALE_CONNECTION_SECS` (60s) [REF-MQTT-CONN]: the link
  is dead. Reconnect.

#### Trait Implementations

##### `impl Clone for Liveness`

- <span id="liveness-clone"></span>`fn clone(&self) -> Liveness` — [`Liveness`](client/index.md#liveness)

##### `impl Copy for Liveness`

##### `impl Debug for Liveness`

- <span id="liveness-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for Liveness`

- <span id="liveness-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for Liveness`

##### `impl Hash for Liveness`

- <span id="liveness-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for Liveness`

- <span id="liveness-partialeq-eq"></span>`fn eq(&self, other: &Liveness) -> bool` — [`Liveness`](client/index.md#liveness)

##### `impl ToString for Liveness`

- <span id="liveness-tostring-to-string"></span>`fn to_string(&self) -> String`

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
    Inactive,
    Active(Vec<i32>),
}
```

Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].

**The Polymorphic Mapping Rule:**
* When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
* When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).

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

- <span id="amsmappingtable-serialize"></span>`fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<<S as >::Ok, <S as >::Error>`

### `AmsSource`

```rust
enum AmsSource {
    Flat(Vec<i32>),
    Structured(Vec<crate::ams::mapping::AmsMapping2Entry>),
}
```

Where a print job's AMS routing comes from: one mapping form or the other, never both.

#### Variants

- **`Flat`**

  A flat `ams_mapping` channel array (one entry per project filament, `-1` = unmapped).
  No `ams_mapping2` is sent.

- **`Structured`**

  Structured `ams_mapping2` entries. The flat array is derived from them, so the two
  wire arrays always agree index for index [REF-AMS-MAP].

#### Trait Implementations

##### `impl Clone for AmsSource`

- <span id="amssource-clone"></span>`fn clone(&self) -> AmsSource` — [`AmsSource`](commands/print_job/index.md#amssource)

##### `impl Debug for AmsSource`

- <span id="amssource-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AmsSource`

##### `impl PartialEq for AmsSource`

- <span id="amssource-partialeq-eq"></span>`fn eq(&self, other: &AmsSource) -> bool` — [`AmsSource`](commands/print_job/index.md#amssource)

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

### `AirductRequest`

```rust
type AirductRequest = super::Print<AirductPayload>;
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

### `AmsChangeFilamentRequest`

```rust
type AmsChangeFilamentRequest = super::Print<AmsChangeFilamentPayload>;
```

Loads or unloads filament from an AMS slot or external spool to the toolhead.

### `AmsControlRequest`

```rust
type AmsControlRequest = super::Print<AmsControlPayload>;
```

Sends a resume, pause, or reset command to the AMS feed mechanism.

### `AmsFilamentDryingRequest`

```rust
type AmsFilamentDryingRequest = super::Print<AmsFilamentDryingPayload>;
```

Starts or stops a filament drying cycle on an AMS unit with a built-in heater.

### `AmsFilamentSettingRequest`

```rust
type AmsFilamentSettingRequest = super::Print<AmsFilamentSettingPayload>;
```

Sets filament properties (type, color, temperature range) on an AMS tray or external spool.

### `AmsGetRfidRequest`

```rust
type AmsGetRfidRequest = super::Print<AmsGetRfidPayload>;
```

Requests an RFID tag scan on a specific AMS slot.

### `BuzzerRequest`

```rust
type BuzzerRequest = super::Print<BuzzerPayload>;
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

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

### `GCodeRequest`

```rust
type GCodeRequest = super::Print<GCodePayload>;
```

Sends a raw G-code line to the printer for immediate execution.

### `GetAccessCodeRequest`

```rust
type GetAccessCodeRequest = super::System<GetAccessCodePayload>;
```

Queries the printer for its own current LAN access code.

Distinct from the access code the caller supplies to authenticate: this re-reads the value
from the printer over an already-authenticated session, which is how a client notices that a
rotated code has invalidated its cached credential.

The reply is `system`-wrapped and echoes the request's `sequence_id`, alongside
`access_code`, `result`, and `reason` — confirmed on a P1S via `bambino-cli ack-probe`
(issue #140); see `reference/03_mqtt_telemetry.md` for the observed shape.

Treat the returned code as a credential: it must never be logged or written to disk.

### `GetVersionRequest`

```rust
type GetVersionRequest = super::Info<GetVersionPayload>;
```

Queries the printer for its hardware and firmware version info.

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

### `LedCtrlRequest`

```rust
type LedCtrlRequest = super::System<LedCtrlPayload>;
```

Turns chamber or toolhead LEDs on or off.

### `PrintSpeedRequest`

```rust
type PrintSpeedRequest = super::Print<PrintSpeedPayload>;
```

Changes the active print speed profile (silent, standard, sport, ludicrous).

### `ProjectFileRequest`

```rust
type ProjectFileRequest = super::Print<ProjectFilePayload>;
```

Submits a `.3mf` print job from the SD card for execution.

### `PromptSoundRequest`

```rust
type PromptSoundRequest = super::Print<PromptSoundPayload>;
```

Enables or disables the printer's notification sounds.

### `PushAllRequest`

```rust
type PushAllRequest = super::Pushing<PushAllPayload>;
```

Requests a full state dump from the printer (all telemetry fields at once).

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

Separate from [`CleanPrintErrorRequest`](commands/control/index.md#cleanprinterrorrequest), which clears the error latch: BambuStudio sends
this once whenever its own copy of the dialog closes.


---

## Functions

### `echo_key`

```rust
fn echo_key(payload: &[u8]) -> Option<EchoKey>
```

**Types:** [`EchoKey`](client/index.md#echokey)

Reads the [`EchoKey`](client/index.md#echokey) from the first top-level wrapper (`print`/`system`/`pushing`/`info`) that carries one.

The one reader for both directions: an outgoing payload when a command is published, and an
incoming echo when deciding whether it answers the command that armed the write-zombie
timer. Accepting both `sequence_id` forms on both sides keeps a numerically-echoed id from
resolving a command handle while leaving its write-zombie armed.

### `report_topic`

```rust
fn report_topic(serial: &str) -> String
```

The topic the printer publishes its reports and command echoes on.

### `request_topic`

```rust
fn request_topic(serial: &str) -> String
```

The topic commands are published to.

### `resolve_rack_nozzle_mapping`

```rust
fn resolve_rack_nozzle_mapping(slot_extruders: &[i32], rack_nozzle_id: i32) -> Option<Vec<i32>>
```

Translates a per-slot extruder mapping into an H2C `nozzle_mapping` of physical nozzle IDs.

`slot_extruders` holds one extruder index per filament slot, with any negative value meaning
"this slot is not printed". `rack_nozzle_id` is the physical ID of the rack position the
printer currently reports as live, which only the caller can know — the mounted hotend can
change between slicing and dispatch.

Returns a 32-slot vector of physical IDs (the fixed wire length), or `None` when it cannot
be resolved with confidence. **`None` means "omit the field entirely" and is the deliberate
failure mode, not an error path.** Omitting it returns the firmware to its own nozzle pick,
which is merely suboptimal; a *wrong* physical ID makes the printer level with one nozzle and
print with another millimetres off the bed. Upstream reached that failure twice, so this
declines rather than guesses when any of the following holds:

- the slot list is empty, or longer than the wire format carries;
- `rack_nozzle_id` is not a real rack position;
- no slot actually needs the rack — BambuStudio omits `nozzle_mapping` for a fixed-hotend-only
  plate, so this matches rather than naming a nozzle it need not name;
- a slot names a carriage an H2C does not have, meaning the file was mapped for another
  machine and forwarding the value raw would name a physical nozzle by a foreign index.

# The two namespaces

Extruder indices and physical nozzle IDs overlap numerically and mean different things. On an
H2C the *fixed* hotend is extruder index `1` and physical ID `1`; the *rack* is extruder index
`0` and physical IDs `16..=21`. Passing an index where an ID belongs is the entire bug class
this function exists to prevent.

**Unverified on hardware here** — no H2C is available. Every value above is taken from
bambuddy's hardware-measured constants; see `reference/03_mqtt_telemetry.md` for the
measurements and the two corrections upstream made to them.


---

## Constants

### `MQTT_ZOMBIE_TIMEOUT_SECS`
```rust
const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10u32;
```

Seconds a published command may go unanswered before [`MqttClient::tick_zombie_check`](client/index.md#mqttclient) reports [`Liveness::WriteZombie`](client/index.md#liveness).

### `MQTTS_PORT`
```rust
const MQTTS_PORT: u16 = 8_883u16;
```

MQTT-over-TLS port every Bambu printer's local broker listens on.

