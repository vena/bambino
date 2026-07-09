**bambino > mqtt > client**

# Module: mqtt::client

## Contents

**Structs**

- [`BambuMqttClient`](#bambumqttclient) - Lightweight MQTT client session running over an established `AsyncIo` stream.
- [`MqttMessage`](#mqttmessage) - Incoming MQTT message details parsed from the wire.

---

## bambino::mqtt::client::BambuMqttClient

*Struct*

Lightweight MQTT client session running over an established `AsyncIo` stream.

**Generic Parameters:**
- IO

**Methods:**

- `fn connect(stream: IO, serial: &str, access_code: &str) -> Result<Self, BambuError>` - Executes a secure local network connection handshake and subscription loop with the printer.
- `fn publish_command(self: & mut Self, payload: &[u8]) -> Result<u16, BambuError>` - Submits a serialized JSON command payload to the printer's request channel.
- `fn poll_telemetry(self: & mut Self) -> Result<MqttMessage, BambuError>` - Returns the next MQTT message, draining any buffered messages first.
- `fn send_ping(self: & mut Self) -> Result<(), BambuError>` - Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.
- `fn tick_zombie_check(self: & mut Self, elapsed_secs: u32) -> Result<(), BambuError>` - Platform-agnostic timer tick update.
- `fn get_in_flight_count(self: &Self) -> usize` - Returns a slice containing current un-acknowledged QoS 1 packet identifiers.



## bambino::mqtt::client::MqttMessage

*Struct*

Incoming MQTT message details parsed from the wire.

**Fields:**
- `topic: String` - Full MQTT topic string the message arrived on (e.g. "device/{serial}/report").
- `payload: Vec<u8>` - Raw JSON payload bytes as received off the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> MqttMessage`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



