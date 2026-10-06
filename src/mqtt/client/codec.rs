//! MQTT v3.1.1 packet encoding helpers.
//!
//! Pure, stateless functions over primitive args — no dependency on
//! `MqttClient` or `AsyncIo`.

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

/// Largest value MQTT v3.1.1's 4-byte remaining-length varint can encode (§2.2.3).
pub(crate) const MQTT_MAX_REMAINING_LENGTH: usize = 268_435_455;

/// Encodes an input length parameter into a variable-length MQTT remaining length block (1 to 4 bytes).
///
/// `len` must not exceed [`MQTT_MAX_REMAINING_LENGTH`]: the loop below is otherwise happy to
/// emit a 5th continuation byte, producing a frame the decoder (`frame.rs`, which correctly
/// rejects varints longer than 4 bytes) and any conforming broker will reject — desyncing the
/// connection rather than failing the call. `publish_command` enforces the caller-facing bound;
/// this assertion catches an internal caller that forgot to.
pub(crate) fn encode_remaining_length(mut len: usize) -> Vec<u8> {
    debug_assert!(
        len <= MQTT_MAX_REMAINING_LENGTH,
        "remaining length {len} exceeds the 4-byte varint maximum"
    );
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

/// Bits 1-2 of a PUBLISH fixed header: the QoS level.
const PUBLISH_QOS_MASK: u8 = 0b0000_0110;

/// The QoS level a PUBLISH fixed header carries.
pub(crate) fn qos_of(header: u8) -> u8 {
    (header & PUBLISH_QOS_MASK) >> 1
}

/// Appends an MQTT UTF-8 string: a big-endian `u16` length prefix, then the bytes.
///
/// The prefix is a `u16` wire field; every value written here derives from short serials or
/// fixed strings, so the length check is a debug assertion rather than an error path.
fn put_str(buf: &mut Vec<u8>, s: &str) {
    debug_assert!(s.len() <= u16::MAX as usize, "MQTT string exceeds u16::MAX");
    buf.extend_from_slice(&(s.len() as u16).to_be_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// Prepends `header` and the remaining-length varint to `body`.
fn frame(header: u8, body: &[u8]) -> Vec<u8> {
    let mut packet = vec![header];
    packet.extend_from_slice(&encode_remaining_length(body.len()));
    packet.extend_from_slice(body);
    packet
}

/// Encodes a standard MQTT CONNECT packet using Clean Session = True, Username, and Password flags.
pub(crate) fn encode_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(16 + client_id.len() + username.len() + password.len());
    put_str(&mut payload, "MQTT");
    // Protocol Level: 4 (v3.1.1)
    payload.push(0x04);
    // Connect Flags: Clean Session (0x02) | Username (0x80) | Password (0x40) -> 0xC2
    payload.push(0xC2);
    payload.extend_from_slice(&MQTT_KEEP_ALIVE_SECS.to_be_bytes());
    put_str(&mut payload, client_id);
    put_str(&mut payload, username);
    put_str(&mut payload, password);
    frame(HEADER_CONNECT, &payload)
}

/// Encodes an MQTT SUBSCRIBE packet for one topic, requesting QoS 1.
pub(crate) fn encode_subscribe(packet_id: u16, topic: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(5 + topic.len());
    payload.extend_from_slice(&packet_id.to_be_bytes());
    put_str(&mut payload, topic);
    // Requested QoS
    payload.push(1);
    frame(HEADER_SUBSCRIBE, &payload)
}

/// Encodes an MQTT PUBLISH packet with QoS 1 flags.
pub(crate) fn encode_publish_qos1(packet_id: u16, topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + topic.len() + payload.len());
    put_str(&mut body, topic);
    body.extend_from_slice(&packet_id.to_be_bytes());
    body.extend_from_slice(payload);
    frame(HEADER_PUBLISH_QOS1, &body)
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
