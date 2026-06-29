//! # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session
//!
//! Implements a dedicated async MQTT client designed to execute over our abstract
//! `AsyncIo` trait bounds. This custom client facilitates secure MQTTS connection
//! negotiations, subscription registrations, QoS 1 publish queues, keep-alive frames,
//! and write-channel zombie detection [REF-MQTT-CONN] [REF-MQTT-ZOMBIE].
//!
//! Designed for absolute execution safety across standard hosts, ESP-IDF microcontrollers,
//! and bare-metal Embassy targets.

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeSet;
#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::collections::BTreeSet;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError};

/// Monotonic counter for generating unique MQTT client IDs across connections.
/// Each `connect()` call increments this to avoid stale QoS 1 queue conflicts
/// when the broker hasn't fully torn down a prior session's TCP socket.
static CONNECTION_COUNTER: AtomicU32 = AtomicU32::new(0);

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

pub(crate) const MQTT_MAX_PAYLOAD_BYTES: usize = 1_048_576; // 1 MiB
pub(crate) const MQTT_IN_FLIGHT_LIMIT: usize = 200;
pub(crate) const MQTT_KEEP_ALIVE_SECS: u16 = 30;
pub(crate) const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10;
pub(crate) const MQTT_STALE_CONNECTION_SECS: u32 = 60;

// ============================================================================
// MQTT Packet Serialization Helpers
// ============================================================================

/// Encodes an input length parameter into a variable-length MQTT remaining length block (1 to 4 bytes).
fn encode_remaining_length(mut len: usize) -> Vec<u8> {
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
fn encode_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
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
fn encode_subscribe(packet_id: u16, topic: &str, qos: u8) -> Vec<u8> {
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
fn encode_publish_qos1(packet_id: u16, topic: &str, payload: &[u8]) -> Vec<u8> {
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
fn encode_puback(packet_id: u16) -> Vec<u8> {
    let mut packet = vec![HEADER_PUBACK, 0x02];
    packet.extend_from_slice(&packet_id.to_be_bytes());
    packet
}

/// Encodes an MQTT PINGREQ frame.
fn encode_pingreq() -> Vec<u8> {
    vec![HEADER_PINGREQ, 0x00]
}

/// Reads exactly one standard MQTT frame asynchronously from our abstract socket.
async fn read_exact_packet<IO: AsyncIo>(
    stream: &mut IO,
    payload_buf: &mut Vec<u8>,
) -> Result<(u8, usize), SocketError> {
    // Read the fixed header packet type byte
    let mut header = [0u8; 1];
    stream.read_exact(&mut header).await.map_err(|e| {
        log::trace!("MQTT header read failed: {:?}", e);
        SocketError::ConnectionReset
    })?;

    // Read variable-length remaining length
    let mut rem_len: usize = 0;
    let mut multiplier: usize = 1;
    loop {
        let mut single_byte = [0u8; 1];
        stream.read_exact(&mut single_byte).await.map_err(|e| {
            log::trace!("MQTT remaining-length read failed: {:?}", e);
            SocketError::ConnectionReset
        })?;
        let b = single_byte[0];
        rem_len += ((b & 127) as usize) * multiplier;
        if (b & 128) == 0 {
            break;
        }
        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(SocketError::InvalidInput); // Protocol violation
        }
    }

    if rem_len > MQTT_MAX_PAYLOAD_BYTES {
        log::warn!("MQTT payload length {} exceeds maximum", rem_len);
        return Err(SocketError::InvalidInput);
    }

    // Resize our buffer and read exactly the remaining length bytes
    payload_buf.resize(rem_len, 0);
    if rem_len > 0 {
        stream.read_exact(payload_buf).await.map_err(|e| {
            log::trace!("MQTT payload read failed: {:?}", e);
            SocketError::ConnectionReset
        })?;
    }

    Ok((header[0], rem_len))
}

// ============================================================================
// MQTT Client Session Management
// ============================================================================

/// Incoming MQTT message details parsed from the wire.
#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Lightweight MQTT client session running over an established `AsyncIo` stream.
pub struct BambuMqttClient<IO: AsyncIo> {
    stream: IO,
    request_topic: String,
    next_packet_id: u16,
    /// Outgoing QoS 1 packet tracking registry. Handles up to 200 concurrent unacknowledged entries.
    in_flight: BTreeSet<u16>,
    /// Messages buffered by request-response round-trips (e.g. `poll_until`),
    /// drained first by `poll_telemetry()` before reading from the wire.
    pending_messages: VecDeque<MqttMessage>,
    /// Accumulated elapsed seconds since the last command publish while waiting for a response update.
    write_pending_secs: Option<u32>,
    /// Incremental scale of unacknowledged ping requests.
    ping_outstanding: bool,
    /// Accumulated elapsed seconds since the last received message of any kind.
    /// Used to detect silent connection loss independent of publish activity.
    secs_since_last_message: u32,
}

impl<IO: AsyncIo> BambuMqttClient<IO> {
    /// Executes a secure local network connection handshake and subscription loop with the printer.
    ///
    /// **Authentication Note:** If the printer's physical broker rejects credentials due to
    /// an invalid access code, this function returns `BambuError::AccessDenied`.
    pub async fn connect(
        mut stream: IO,
        serial: &str,
        access_code: &str,
    ) -> Result<Self, BambuError> {
        let conn_id = CONNECTION_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let client_id = format!("bambino_{}_{}", serial, conn_id);
        let connect_pkt = encode_connect(&client_id, "bblp", access_code);

        log::debug!(
            "Transmitting CONNECT payload (client_id: '{}', user: 'bblp')",
            client_id
        );

        stream
            .write_all(&connect_pkt)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

        // Read CONNACK packet
        let mut payload_buf = Vec::new();

        log::debug!("Awaiting broker CONNACK response packet");

        let (header, rem_len) = read_exact_packet(&mut stream, &mut payload_buf).await?;

        let packet_type = header >> 4;

        log::debug!(
            "Received raw packet header type: {}, remaining size: {} bytes",
            packet_type,
            rem_len
        );

        if packet_type != PACKET_TYPE_CONNACK {
            return Err(BambuError::ProtocolViolation(
                "Expected CONNACK frame".into(),
            ));
        }
        if payload_buf.len() < 2 {
            return Err(BambuError::ProtocolViolation(
                "Short CONNACK payload".into(),
            ));
        }
        let connack_code = payload_buf[1];

        log::debug!("Connection accepted response byte: {}", connack_code);

        if connack_code != 0 {
            log::warn!(
                "Broker rejected connection with CONNACK return code: {}",
                connack_code
            );
            return Err(BambuError::AccessDenied);
        }

        // Subscribe to report topic
        let report_topic = format!("device/{}/report", serial);

        log::debug!(
            "Sending SUBSCRIBE frame targeting topic: '{}' (granted QoS 1)",
            report_topic
        );

        let subscribe_pkt = encode_subscribe(1, &report_topic, 1);

        stream
            .write_all(&subscribe_pkt)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

        // Read SUBACK packet
        log::debug!("Awaiting broker SUBACK verification packet");

        let (sub_header, _sub_rem_len) = read_exact_packet(&mut stream, &mut payload_buf).await?;
        let sub_type = sub_header >> 4;

        log::debug!("Received raw packet header type: {}", sub_type);

        if sub_type != PACKET_TYPE_SUBACK {
            return Err(BambuError::ProtocolViolation(
                "Expected SUBACK frame".into(),
            ));
        }
        if payload_buf.len() < 3 {
            return Err(BambuError::ProtocolViolation("Short SUBACK payload".into()));
        }
        let return_code = payload_buf[2];

        log::debug!("SUBACK response status granted: 0x{:02X}", return_code);

        if return_code == 0x80 {
            return Err(BambuError::ProtocolViolation(
                "Subscription rejected by physical broker".into(),
            ));
        }

        Ok(Self {
            stream,
            request_topic: format!("device/{}/request", serial),
            next_packet_id: 2, // 1 is consumed by SUBSCRIBE handshake
            in_flight: BTreeSet::new(),
            pending_messages: VecDeque::new(),
            write_pending_secs: None,
            ping_outstanding: false,
            secs_since_last_message: 0,
        })
    }

    /// Submits a serialized JSON command payload to the printer's request channel.
    ///
    /// **In-flight Bounds Verification:**
    /// If the unacknowledged queue size equals or exceeds 200, this function returns a
    /// network timeout error to protect memory space and prevent packet drift [REF-MQTT-CONN].
    pub async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, BambuError> {
        if self.in_flight.len() >= MQTT_IN_FLIGHT_LIMIT {
            log::warn!(
                "In-flight command backlog saturated ({} items)",
                self.in_flight.len()
            );
            return Err(BambuError::NetworkError(SocketError::TimedOut));
        }

        let packet_id = self.next_packet_id;
        self.next_packet_id = self.next_packet_id.wrapping_add(1);
        if self.next_packet_id == 0 {
            self.next_packet_id = 1; // 0 is reserved in MQTT specifications
        }

        log::debug!(
            "Publishing QoS 1 command (packet_id: {}) to topic: '{}' (payload length: {} bytes)",
            packet_id,
            self.request_topic,
            payload.len()
        );

        let packet = encode_publish_qos1(packet_id, &self.request_topic, payload);

        self.stream
            .write_all(&packet)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

        self.in_flight.insert(packet_id);

        // Arm/reset write-channel zombie detection tracking [REF-MQTT-ZOMBIE]
        self.write_pending_secs = Some(0);

        Ok(packet_id)
    }

    /// Returns the next MQTT message, draining any buffered messages first.
    ///
    /// Messages are buffered when request-response methods (e.g. `get_version()`) read
    /// non-matching messages off the wire while waiting for a specific response. This
    /// method drains those buffered messages in FIFO order before reading new packets
    /// from the wire.
    ///
    /// Handles MQTT protocol frames transparently: sends `PUBACK` for incoming QoS 1
    /// publishes, clears matching packet IDs from the in-flight tracker on `PUBACK`,
    /// and acknowledges `PINGRESP` — only application-level `PUBLISH` payloads are
    /// returned.
    pub async fn poll_telemetry(&mut self) -> Result<MqttMessage, BambuError> {
        if let Some(buffered) = self.pending_messages.pop_front() {
            return Ok(buffered);
        }
        self.poll_wire().await
    }

    /// Reads the next message directly from the wire, bypassing the pending buffer.
    ///
    /// Used by `PrinterClient::poll_until()` which manages its own buffer stashing
    /// and must not re-read messages it just pushed.
    pub(crate) async fn poll_wire(&mut self) -> Result<MqttMessage, BambuError> {
        let mut payload_buf = Vec::new();
        loop {
            let (header, rem_len) = read_exact_packet(&mut self.stream, &mut payload_buf).await?;

            self.secs_since_last_message = 0;

            let packet_type = header >> 4;

            log::trace!(
                "Parsed wire packet type: {}, size: {} bytes",
                packet_type,
                rem_len
            );

            match packet_type {
                PACKET_TYPE_PUBLISH => {
                    let qos = (header & 0x06) >> 1;

                    if payload_buf.len() < 2 {
                        return Err(BambuError::ProtocolViolation(
                            "Short publish payload".into(),
                        ));
                    }
                    let topic_len = u16::from_be_bytes([payload_buf[0], payload_buf[1]]) as usize;
                    if payload_buf.len() < 2 + topic_len {
                        return Err(BambuError::ProtocolViolation(
                            "Invalid topic length bounds".into(),
                        ));
                    }

                    let topic = core::str::from_utf8(&payload_buf[2..2 + topic_len])
                        .map_err(|_| BambuError::ProtocolViolation("Non-UTF8 topic name".into()))?
                        .to_string();

                    let mut payload_start = 2 + topic_len;
                    let mut packet_id = None;
                    if qos >= 1 {
                        if payload_buf.len() < payload_start + 2 {
                            return Err(BambuError::ProtocolViolation(
                                "Missing packet ID in QoS 1+".into(),
                            ));
                        }
                        let id = u16::from_be_bytes([
                            payload_buf[payload_start],
                            payload_buf[payload_start + 1],
                        ]);
                        packet_id = Some(id);
                        payload_start += 2;
                    }

                    let payload = payload_buf[payload_start..].to_vec();

                    log::debug!(
                        "Received PUBLISH frame on topic: '{}', QoS: {}, packet_id: {:?}, payload size: {} bytes",
                        topic,
                        qos,
                        packet_id,
                        payload.len()
                    );

                    // QoS 1 requires PUBACK; QoS 2 would require PUBREC (unsupported)
                    if qos == 1 {
                        let id = packet_id.expect("QoS 1 always has packet_id");
                        log::trace!("Sending automatic PUBACK for packet_id: {}", id);

                        let ack = encode_puback(id);
                        self.stream.write_all(&ack).await.map_err(|_| {
                            BambuError::NetworkError(SocketError::ConnectionAborted)
                        })?;
                        self.stream.flush().await.map_err(|_| {
                            BambuError::NetworkError(SocketError::ConnectionAborted)
                        })?;
                    }

                    // Reset write channel zombie tracking since a telemetry update was received
                    self.write_pending_secs = None;

                    return Ok(MqttMessage { topic, payload });
                }
                PACKET_TYPE_PUBACK => {
                    if payload_buf.len() < 2 {
                        return Err(BambuError::ProtocolViolation(
                            "Invalid PUBACK length".into(),
                        ));
                    }
                    let ack_id = u16::from_be_bytes([payload_buf[0], payload_buf[1]]);

                    log::trace!(
                        "Received PUBACK from broker for outbound packet_id: {}",
                        ack_id
                    );

                    self.in_flight.remove(&ack_id);
                }
                PACKET_TYPE_PINGRESP => {
                    log::trace!("Received keep-alive PINGRESP from broker");
                    self.ping_outstanding = false;
                }
                _ => {
                    log::debug!("Ignoring un-handled control frame code: {}", packet_type);
                }
            }
        }
    }

    /// Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.
    pub async fn send_ping(&mut self) -> Result<(), BambuError> {
        log::trace!("Transmitting PINGREQ keep-alive packet");

        let ping = encode_pingreq();
        self.stream
            .write_all(&ping)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.ping_outstanding = true;
        Ok(())
    }

    /// Platform-agnostic timer tick update.
    ///
    /// Evaluates two independent liveness conditions:
    /// 1. **Write zombie**: A published command has gone unanswered for 10+ seconds.
    /// 2. **Connection staleness**: No packets of any kind received for 60+ seconds,
    ///    indicating a silently dropped connection [REF-MQTT-ZOMBIE].
    pub fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), BambuError> {
        if let Some(ref mut secs) = self.write_pending_secs {
            *secs += elapsed_secs;
            if *secs >= MQTT_ZOMBIE_TIMEOUT_SECS {
                log::warn!(
                    "Zombie state detected: command issued but zero telemetry updates received for >= {}s",
                    MQTT_ZOMBIE_TIMEOUT_SECS
                );
                return Err(BambuError::Timeout);
            }
        }

        self.secs_since_last_message += elapsed_secs;
        if self.secs_since_last_message >= MQTT_STALE_CONNECTION_SECS {
            log::warn!(
                "Connection stale: no packets received for >= {}s",
                MQTT_STALE_CONNECTION_SECS
            );
            return Err(BambuError::Timeout);
        }

        Ok(())
    }

    /// Returns a slice containing current un-acknowledged QoS 1 packet identifiers.
    pub fn get_in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Stashes a message back into the pending buffer for later retrieval.
    ///
    /// Used by `PrinterClient::poll_until()` to buffer non-matching messages
    /// during request-response round-trips.
    pub(crate) fn push_pending(&mut self, msg: MqttMessage) {
        self.pending_messages.push_back(msg);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_packet_id_skips_zero_on_wraparound() {
        let mut next_packet_id: u16 = u16::MAX;

        let issued_id = next_packet_id;
        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(issued_id, u16::MAX);
        assert_eq!(next_packet_id, 1, "Packet ID must skip 0 after wraparound");
    }

    #[test]
    fn test_packet_id_normal_increment() {
        let mut next_packet_id: u16 = 100;

        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(next_packet_id, 101);
    }

    #[test]
    fn test_packet_id_one_before_max() {
        let mut next_packet_id: u16 = u16::MAX - 1;

        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(
            next_packet_id,
            u16::MAX,
            "ID before MAX should increment normally"
        );
    }

    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::super::*;
        use crate::io::TokioIo;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        #[tokio::test]
        async fn test_connack_rejection_returns_access_denied() {
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                // Read and discard the CONNECT packet
                let mut discard = vec![0u8; 256];
                let _ = server_stream.read(&mut discard).await;

                // Reply with CONNACK: return code 5 (not authorized)
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x05])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
            });

            let result =
                BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::BambuError::AccessDenied),
                "Expected AccessDenied, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_suback_rejection_returns_protocol_violation() {
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                let mut discard = vec![0u8; 256];
                // Read CONNECT
                let _ = server_stream.read(&mut discard).await;
                // Reply CONNACK accepted
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x00])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();

                // Read SUBSCRIBE
                let _ = server_stream.read(&mut discard).await;
                // Reply SUBACK with return code 0x80 (rejected)
                server_stream
                    .write_all(&[0x90, 0x03, 0x00, 0x01, 0x80])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
            });

            let result =
                BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::BambuError::ProtocolViolation(_)),
                "Expected ProtocolViolation for SUBACK rejection, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_read_exact_packet_oom_guard() {
            // Craft a packet with remaining length exceeding MQTT_MAX_PAYLOAD_BYTES (1 MiB)
            let oversized_len: usize = MQTT_MAX_PAYLOAD_BYTES + 1;
            let mut data = vec![0x30u8]; // PUBLISH header
            data.extend_from_slice(&encode_remaining_length(oversized_len));

            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut buf = Vec::new();
            let result = read_exact_packet(&mut stream, &mut buf).await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for oversized payload, got {:?}",
                result
            );
        }

        #[tokio::test]
        async fn test_read_exact_packet_malformed_remaining_length() {
            // 5 continuation bytes → multiplier exceeds 128^3, protocol violation
            let data = vec![0x30, 0x80, 0x80, 0x80, 0x80, 0x01];
            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut buf = Vec::new();
            let result = read_exact_packet(&mut stream, &mut buf).await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for malformed remaining length, got {:?}",
                result
            );
        }
    }
}
