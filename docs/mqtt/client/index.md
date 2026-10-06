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

## Contents

- [Types](#types)
  - [`EchoKey`](#echokey)
  - [`MqttClient`](#mqttclient)
  - [`MqttMessage`](#mqttmessage)
  - [`Liveness`](#liveness)
- [Functions](#functions)
  - [`echo_key`](#echo-key)
  - [`report_topic`](#report-topic)
  - [`request_topic`](#request-topic)
- [Constants](#constants)
  - [`MQTT_ZOMBIE_TIMEOUT_SECS`](#mqtt-zombie-timeout-secs)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`EchoKey`](#echokey) | struct | The `(command, sequence_id)` pair that ties a command to its echo [REF-MQTT-ACK]. |
| [`MqttClient`](#mqttclient) | struct | Lightweight MQTT client session running over an established `AsyncIo` stream. |
| [`MqttMessage`](#mqttmessage) | struct | Incoming MQTT message details parsed from the wire. |
| [`Liveness`](#liveness) | enum | Which liveness condition [`MqttClient::tick_zombie_check`](#mqttclient) found violated. |
| [`echo_key`](#echo-key) | fn | Reads the [`EchoKey`](#echokey) from the first top-level wrapper (`print`/`system`/`pushing`/`info`) that carries one. |
| [`report_topic`](#report-topic) | fn | The topic the printer publishes its reports and command echoes on. |
| [`request_topic`](#request-topic) | fn | The topic commands are published to. |
| [`MQTT_ZOMBIE_TIMEOUT_SECS`](#mqtt-zombie-timeout-secs) | const | Seconds a published command may go unanswered before [`MqttClient::tick_zombie_check`](#mqttclient) reports [`Liveness::WriteZombie`](#liveness). |

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

- <span id="echokey-clone"></span>`fn clone(&self) -> EchoKey` — [`EchoKey`](#echokey)

##### `impl Debug for EchoKey`

- <span id="echokey-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for EchoKey`

##### `impl PartialEq for EchoKey`

- <span id="echokey-partialeq-eq"></span>`fn eq(&self, other: &EchoKey) -> bool` — [`EchoKey`](#echokey)

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

- <span id="mqttclient-publish-command"></span>`async fn publish_command<T: TimerProvider>(&mut self, payload: &[u8], timer: &T) -> Result<u16, Error>` — [`Error`](../../error/index.md#error)

  Submits a serialized JSON command payload to the printer's request channel.

  **In-flight Bounds Verification:**
  If the unacknowledged queue size equals or exceeds `MQTT_IN_FLIGHT_LIMIT`, this function
  returns [`Error::Backpressure`](../../error/index.md#error) without sending, to protect memory space and prevent
  packet drift [REF-MQTT-CONN]. A saturated queue is not a timeout — retrying immediately
  will not clear it; drain it by servicing PUBACKs (`poll_wire`) or let
  `tick_zombie_check` age the entries out.

  Payloads larger than `MQTT_MAX_PAYLOAD_BYTES` are rejected with
  [`Error::ProtocolViolation`](../../error/index.md#error) rather than encoded, mirroring the read path's own cap.

  `timer` bounds the write (see `write_frame_with_timer`) and stamps the keepalive clock.
  Pass a real platform timer: a timer without a real clock
  ([`TimerProvider::has_real_clock`](../../io/index.md#timerprovider)) makes the write unbounded.

- <span id="mqttclient-poll-telemetry"></span>`async fn poll_telemetry<T: TimerProvider>(&mut self, timer: &T) -> Result<MqttMessage, Error>` — [`MqttMessage`](#mqttmessage), [`Error`](../../error/index.md#error)

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
  timer: with one that has no real clock ([`TimerProvider::has_real_clock`](../../io/index.md#timerprovider)) no keepalive
  is sent, so the broker drops the connection after about 45s of outbound silence, and a
  stalled read blocks forever.

- <span id="mqttclient-send-ping"></span>`async fn send_ping<T: TimerProvider>(&mut self, timer: &T) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

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

- <span id="mqttclient-tick-zombie-check"></span>`fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Liveness>` — [`Liveness`](#liveness)

  Advances the liveness clocks by `elapsed_secs` and reports the first violated condition.

  Two independent conditions, checked in this order:

  1. [`Liveness::WriteZombie`]: a published command has gone [`MQTT_ZOMBIE_TIMEOUT_SECS`](#mqtt-zombie-timeout-secs)
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

- <span id="mqttmessage-clone"></span>`fn clone(&self) -> MqttMessage` — [`MqttMessage`](#mqttmessage)

##### `impl Debug for MqttMessage`

- <span id="mqttmessage-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `Liveness`

```rust
enum Liveness {
    WriteZombie,
    Stale,
}
```

Which liveness condition [`MqttClient::tick_zombie_check`](#mqttclient) found violated.

#### Variants

- **`WriteZombie`**

  A published command has gone [`MQTT_ZOMBIE_TIMEOUT_SECS`](#mqtt-zombie-timeout-secs) with no answer [REF-MQTT-ZOMBIE].
  
  The broker may be discarding writes. Telemetry may still be arriving, so the read side can
  look healthy; it is still the signal to reconnect.

- **`Stale`**

  Nothing at all has arrived for `MQTT_STALE_CONNECTION_SECS` (60s) [REF-MQTT-CONN]: the link
  is dead. Reconnect.

#### Trait Implementations

##### `impl Clone for Liveness`

- <span id="liveness-clone"></span>`fn clone(&self) -> Liveness` — [`Liveness`](#liveness)

##### `impl Copy for Liveness`

##### `impl Debug for Liveness`

- <span id="liveness-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for Liveness`

- <span id="liveness-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for Liveness`

##### `impl Hash for Liveness`

- <span id="liveness-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for Liveness`

- <span id="liveness-partialeq-eq"></span>`fn eq(&self, other: &Liveness) -> bool` — [`Liveness`](#liveness)

##### `impl ToString for Liveness`

- <span id="liveness-tostring-to-string"></span>`fn to_string(&self) -> String`


---

## Functions

### `echo_key`

```rust
fn echo_key(payload: &[u8]) -> Option<EchoKey>
```

**Types:** [`EchoKey`](#echokey)

Reads the [`EchoKey`](#echokey) from the first top-level wrapper (`print`/`system`/`pushing`/`info`) that carries one.

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


---

## Constants

### `MQTT_ZOMBIE_TIMEOUT_SECS`
```rust
const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10u32;
```

Seconds a published command may go unanswered before [`MqttClient::tick_zombie_check`](#mqttclient) reports [`Liveness::WriteZombie`](#liveness).

