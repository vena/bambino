*[bambino](../../index.md) / [client](../index.md) / [types](index.md)*

---

# Module `types`

Client-facing helper types (telemetry events, print progress, calibration options).

The control enums `PrinterClient` takes (`FanTarget`, `PrintSpeed`, ...) live in
[`control`](../../mqtt/commands/control/index.md) and are re-exported from [`client`](../index.md).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`PrintProgress`](#printprogress) | struct | Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](../index.md#printerclient)). |
| [`TelemetryEvent`](#telemetryevent) | enum | Typed telemetry event from the printer's MQTT channel. |

## Types

### `PrintProgress`

```rust
struct PrintProgress {
    pub percent: Option<i32>,
    pub remaining_secs: Option<i32>,
    pub layer_num: Option<i32>,
    pub total_layers: Option<i32>,
}
```

Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](../index.md#printerclient)).

Bundled into one struct rather than four separate cached scalars (unlike `home_flag`/
`gcode_state`/`is_door_open`/`print_error`, which answer four independent questions) because
`mc_percent`, `mc_remaining_time`, `layer_num`, and `total_layers` are always consumed
together as one "how's the print going" question. Each field updates independently and
keeps its last-observed value across a telemetry message that omits it — a `None` field
means "never observed," not "printer reports zero/none."

#### Fields

- **`percent`**: `Option<i32>`

  Motion controller progress percentage (0-100).

- **`remaining_secs`**: `Option<i32>`

  Estimated remaining print duration, in seconds.
  
  Converted on ingest from `mc_remaining_time`, which the wire reports in **minutes**.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.

#### Trait Implementations

##### `impl Clone for PrintProgress`

- <span id="printprogress-clone"></span>`fn clone(&self) -> PrintProgress` — [`PrintProgress`](#printprogress)

##### `impl Copy for PrintProgress`

##### `impl Debug for PrintProgress`

- <span id="printprogress-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for PrintProgress`

- <span id="printprogress-default"></span>`fn default() -> PrintProgress` — [`PrintProgress`](#printprogress)

##### `impl Eq for PrintProgress`

##### `impl PartialEq for PrintProgress`

- <span id="printprogress-partialeq-eq"></span>`fn eq(&self, other: &PrintProgress) -> bool` — [`PrintProgress`](#printprogress)

### `TelemetryEvent`

```rust
enum TelemetryEvent {
    Report(Box<crate::types::TelemetryReport>, crate::mqtt::MqttMessage),
    Command(super::command::CommandResolution, Option<crate::mqtt::MqttMessage>),
    Unknown(crate::mqtt::MqttMessage),
}
```

Typed telemetry event from the printer's MQTT channel.

The library deserializes wire payloads into structured types so consumers don't
have to reimplement JSON parsing and model-quirk handling. Raw access is always
available via [`into_raw`](#telemetryevent).

#### Variants

- **`Report`**

  State telemetry update (print status, device hardware, or both).

- **`Command`**

  The terminal outcome of a command this client published.
  
  Carries the echo that decided it, or `None` for an outcome no message produced
  ([`CommandOutcome::TimedOut`](../command/index.md#commandoutcome), [`CommandOutcome::ConnectionLost`](../command/index.md#commandoutcome)).

- **`Unknown`**

  Payload that didn't match any known telemetry structure, including command echoes for
  `sequence_id`s this client did not send (other clients share the report topic).

#### Implementations

- <span id="telemetryevent-into-raw"></span>`fn into_raw(self) -> Option<MqttMessage>` — [`MqttMessage`](../../mqtt/client/index.md#mqttmessage)

  Consumes the event and returns the underlying raw MQTT message, if one produced it.

  `None` only for a [`Command`](#telemetryevent) outcome that no message produced.

- <span id="telemetryevent-raw"></span>`fn raw(&self) -> Option<&MqttMessage>` — [`MqttMessage`](../../mqtt/client/index.md#mqttmessage)

  Returns a reference to the underlying raw MQTT message, if one produced it.

  `None` only for a [`Command`](#telemetryevent) outcome that no message produced.

- <span id="telemetryevent-report"></span>`fn report(&self) -> Option<&TelemetryReport>` — [`TelemetryReport`](../../types/telemetry/index.md#telemetryreport)

  Returns the typed report if this is a `Report` variant.

- <span id="telemetryevent-command"></span>`fn command(&self) -> Option<&CommandResolution>` — [`CommandResolution`](../command/index.md#commandresolution)

  Returns the command resolution if this is a `Command` variant.

#### Trait Implementations

##### `impl Clone for TelemetryEvent`

- <span id="telemetryevent-clone"></span>`fn clone(&self) -> TelemetryEvent` — [`TelemetryEvent`](#telemetryevent)

##### `impl Debug for TelemetryEvent`

- <span id="telemetryevent-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

