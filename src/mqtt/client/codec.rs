//! MQTT v3.1.1 packet encoding helpers.
//!
//! Pure, stateless functions over primitive args — no dependency on
//! `BambuMqttClient` or `AsyncIo`.

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

// MQTT v3.1.1 packet type codes (upper 4 bits of fixed header byte)
pub(crate) const PACKET_TYPE_CONNACK: u8 = 2;
pub(crate) const PACKET_TYPE_PUBLISH: u8 = 3;
pub(crate) const PACKET_TYPE_PUBACK: u8 = 4;
pub(crate) const PACKET_TYPE_SUBACK: u8 = 9;
pub(crate) const PACKET_TYPE_PINGRESP: u8 = 13;

// MQTT fixed header bytes for outgoing packet types
pub(crate) const HEADER_CONNECT: u8 = 0x10;
pub(crate) const HEADER_SUBSCRIBE: u8 = 0x82;
pub(crate) const HEADER_PUBLISH_QOS1: u8 = 0x32;
pub(crate) const HEADER_PUBACK: u8 = 0x40;
pub(crate) const HEADER_PINGREQ: u8 = 0xC0;

pub(crate) const MQTT_KEEP_ALIVE_SECS: u16 = 30;

/// Encodes an input length parameter into a variable-length MQTT remaining length block (1 to 4 bytes).
pub(crate) fn encode_remaining_length(mut len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4);
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 128;
        }
        bytes.push(byte);
        if len == 0 {
            break;
        }
    }
    bytes
}

/// Encodes a standard MQTT CONNECT packet using Clean Session = True, Username, and Password flags.
pub(crate) fn encode_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(16 + client_id.len() + username.len() + password.len());

    // Protocol Name length prefix and string
    payload.extend_from_slice(&[0x00, 0x04]);
    payload.extend_from_slice(b"MQTT");

    // Protocol Level: 4 (v3.1.1)
    payload.push(0x04);

    // Connect Flags: Clean Session (0x02) | Username (0x80) | Password (0x40) -> 0xC2
    payload.push(0xC2);

    payload.extend_from_slice(&MQTT_KEEP_ALIVE_SECS.to_be_bytes());

    // Client ID
    payload.extend_from_slice(&(client_id.len() as u16).to_be_bytes());
    payload.extend_from_slice(client_id.as_bytes());

    // Username
    payload.extend_from_slice(&(username.len() as u16).to_be_bytes());
    payload.extend_from_slice(username.as_bytes());

    // Password
    payload.extend_from_slice(&(password.len() as u16).to_be_bytes());
    payload.extend_from_slice(password.as_bytes());

    let mut packet = vec![HEADER_CONNECT];
    packet.extend_from_slice(&encode_remaining_length(payload.len()));
    packet.extend(payload);
    packet
}

/// Encodes an MQTT SUBSCRIBE packet with QoS 1 flags.
pub(crate) fn encode_subscribe(packet_id: u16, topic: &str, qos: u8) -> Vec<u8> {
    let mut payload = Vec::with_capacity(5 + topic.len());

    // Packet ID
    payload.extend_from_slice(&packet_id.to_be_bytes());

    // Topic string length prefix and bytes
    payload.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    payload.extend_from_slice(topic.as_bytes());

    // Requested QoS byte
    payload.push(qos);

    let mut packet = vec![HEADER_SUBSCRIBE];
    packet.extend_from_slice(&encode_remaining_length(payload.len()));
    packet.extend(payload);
    packet
}

/// Encodes an MQTT PUBLISH packet with QoS 1 flags.
pub(crate) fn encode_publish_qos1(packet_id: u16, topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut var_header = Vec::with_capacity(4 + topic.len());

    // Topic string length prefix and bytes
    var_header.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    var_header.extend_from_slice(topic.as_bytes());

    // Packet Identifier for QoS 1
    var_header.extend_from_slice(&packet_id.to_be_bytes());

    let remaining_length = var_header.len() + payload.len();
    let mut packet = vec![HEADER_PUBLISH_QOS1];
    packet.extend_from_slice(&encode_remaining_length(remaining_length));
    packet.extend(var_header);
    packet.extend_from_slice(payload);
    packet
}

/// Encodes an MQTT PUBACK confirmation packet.
pub(crate) fn encode_puback(packet_id: u16) -> Vec<u8> {
    let mut packet = vec![HEADER_PUBACK, 0x02];
    packet.extend_from_slice(&packet_id.to_be_bytes());
    packet
}

/// Encodes an MQTT PINGREQ frame.
pub(crate) fn encode_pingreq() -> Vec<u8> {
    vec![HEADER_PINGREQ, 0x00]
}
