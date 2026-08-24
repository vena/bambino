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

- <span id="mqttclient-connect"></span>`async fn connect(stream: IO, identity: &PrinterIdentity) -> Result<Self, Error>` — [`PrinterIdentity`](../../identity/index.md#printeridentity), [`Error`](../../error/index.md#error)

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

  Returns the serial number this client authenticated with (`connect()`'s `serial` argument).

- <span id="mqttclient-publish-command"></span>`async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, Error>` — [`Error`](../../error/index.md#error)

  Submits a serialized JSON command payload to the printer's request channel.

  **In-flight Bounds Verification:**
  If the unacknowledged queue size equals or exceeds `MQTT_IN_FLIGHT_LIMIT`, this function
  returns [`Error::Backpressure`](../../error/index.md#error) without sending, to protect memory space and prevent
  packet drift [REF-MQTT-CONN]. A saturated queue is not a timeout — retrying immediately
  will not clear it; drain it by servicing PUBACKs (`poll_wire`) or let
  `tick_zombie_check` age the entries out.

  Payloads larger than `MQTT_MAX_PAYLOAD_BYTES` are rejected with
  [`Error::ProtocolViolation`](../../error/index.md#error) rather than encoded, mirroring the read path's own cap.

  `DummyTimer` (`has_real_clock() == false`) makes the underlying write unbounded here.
  `PrinterClient` callers get the new stalled-write protection via
  `publish_command_with_timer()` instead, since they have a real `Timer` available.

- <span id="mqttclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<MqttMessage, Error>` — [`MqttMessage`](#mqttmessage), [`Error`](../../error/index.md#error)

  Returns the next MQTT message, draining any buffered messages first.

  Messages are buffered when request-response methods (e.g. `get_version()`) read
  non-matching messages off the wire while waiting for a specific response. This
  method drains those buffered messages in FIFO order before reading new packets
  from the wire.

  Handles MQTT protocol frames transparently: sends `PUBACK` for incoming QoS 1
  publishes, clears matching packet IDs from the in-flight tracker on `PUBACK`,
  and acknowledges `PINGRESP` — only application-level `PUBLISH` payloads are
  returned.

- <span id="mqttclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.

  `DummyTimer` makes the underlying write unbounded here, mirroring `publish_command()`.
  `PrinterClient` callers get stalled-write protection via `send_ping_with_timer()`
  instead.

- <span id="mqttclient-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  Returns true once a write has failed and left the stream possibly desynced.

  A poisoned client is permanently unusable: every later `publish_command`, `send_ping`,
  and automatic PUBACK returns `ConnectionAborted` forever, because a failed write may have
  put a partial frame on the wire and, unlike a read, has no resumable progress state.
  Without this accessor a retry loop could not tell that error apart from a transient one
  and would spin against a client that can never recover; the correct response is to drop
  the connection and reconnect (`PrinterClient::disconnect_mqtt()`).

- <span id="mqttclient-tick-zombie-check"></span>`fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Platform-agnostic timer tick update.

  Evaluates two independent liveness conditions:
  1. **Write zombie**: A published command has gone unanswered for 10+ seconds
     [REF-MQTT-ZOMBIE].
  2. **Connection staleness**: No packets of any kind received for 60+ seconds,
     indicating a silently dropped connection — independent of (1) [REF-MQTT-CONN].

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

