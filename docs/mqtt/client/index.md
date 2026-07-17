*[bambino](../../index.md) / [mqtt](../index.md) / [client](index.md)*

---

# Module `client`

# Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session

Implements a dedicated async MQTT client designed to execute over our abstract
`AsyncIo` trait bounds. This custom client facilitates secure MQTTS connection
negotiations, subscription registrations, QoS 1 publish queues, keep-alive frames,
and write-channel zombie detection [REF-MQTT-CONN] [REF-MQTT-ZOMBIE].

Designed for absolute execution safety across standard hosts, ESP-IDF microcontrollers,
and bare-metal Embassy targets.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`MqttClient`](#mqttclient) | struct | Lightweight MQTT client session running over an established `AsyncIo` stream. |
| [`MqttMessage`](#mqttmessage) | struct | Incoming MQTT message details parsed from the wire. |

## Types

### `MqttClient<IO: AsyncIo>`

```rust
struct MqttClient<IO: AsyncIo> {
    // [REDACTED: Private Fields]
}
```

Lightweight MQTT client session running over an established `AsyncIo` stream.

#### Implementations

- <span id="mqttclient-connect"></span>`async fn connect(stream: IO, serial: &str, access_code: &str) -> Result<Self, Error>` — [`Error`](../../error/index.md#error)

  Executes a secure local network connection handshake and subscription loop with the printer.

- <span id="mqttclient-publish-command"></span>`async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, Error>` — [`Error`](../../error/index.md#error)

  Submits a serialized JSON command payload to the printer's request channel.

- <span id="mqttclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<MqttMessage, Error>` — [`MqttMessage`](#mqttmessage), [`Error`](../../error/index.md#error)

  Returns the next MQTT message, draining any buffered messages first.

- <span id="mqttclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.

- <span id="mqttclient-tick-zombie-check"></span>`fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

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

- <span id="mqttmessage-clone"></span>`fn clone(&self) -> MqttMessage` — [`MqttMessage`](#mqttmessage)

##### `impl Debug for MqttMessage`

- <span id="mqttmessage-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

