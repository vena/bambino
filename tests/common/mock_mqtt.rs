//! # Mock MQTT Broker & Shared Test Helpers
//!
//! Provides a deterministic, state-machine driven MQTT v3.1.1 broker designed to test
//! the `BambuMqttClient` over in-memory `tokio::io::duplex` streams.
//!
//! **Behavioral Design:**
//! 1. Awaits and acknowledges the standard `CONNECT` and `SUBSCRIBE` handshakes.
//! 2. Enters a `tokio::select!` multiplexing loop to simultaneously handle incoming
//!    client commands (echoing `PUBACK` confirmations) and outgoing telemetry injections.
//! 3. Telemetry injection is controlled by the test suite via a `mpsc::Receiver`. When
//!    the test suite pushes a JSON payload into the channel, the mock broker wraps it
//!    in a QoS 1 `PUBLISH` frame and transmits it to the client.

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{mpsc, oneshot};

// MQTT v3.1.1 packet type codes (upper 4 bits of fixed header byte)
pub const PACKET_TYPE_CONNECT: u8 = 1;
pub const PACKET_TYPE_PUBLISH: u8 = 3;
pub const PACKET_TYPE_PUBACK: u8 = 4;
pub const PACKET_TYPE_SUBSCRIBE: u8 = 8;
pub const PACKET_TYPE_PINGREQ: u8 = 12;
pub const PACKET_TYPE_DISCONNECT: u8 = 14;

// MQTT fixed header bytes for outgoing/expected packet types
pub const HEADER_CONNECT: u8 = 0x10;
pub const HEADER_CONNACK: u8 = 0x20;
pub const HEADER_PUBLISH_QOS1: u8 = 0x32;
pub const HEADER_PUBACK: u8 = 0x40;
pub const HEADER_SUBSCRIBE: u8 = 0x82;
pub const HEADER_SUBACK: u8 = 0x90;
pub const HEADER_PINGRESP: u8 = 0xD0;

/// Encodes an integer into the standard MQTT variable-length remaining length format.
fn encode_remaining_length(mut len: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
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

/// Reads a single, complete MQTT frame from an asynchronous stream.
pub async fn read_packet(stream: &mut DuplexStream) -> Result<(u8, Vec<u8>), std::io::Error> {
    let mut header = [0u8; 1];
    stream.read_exact(&mut header).await?;

    let mut rem_len: usize = 0;
    let mut multiplier: usize = 1;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        let b = byte[0];
        rem_len += ((b & 127) as usize) * multiplier;
        if (b & 128) == 0 {
            break;
        }
        multiplier *= 128;
    }

    let mut payload = vec![0u8; rem_len];
    if rem_len > 0 {
        stream.read_exact(&mut payload).await?;
    }

    Ok((header[0], payload))
}

/// Simulates the standard MQTTS CONNECT + SUBSCRIBE handshake sequence.
///
/// Reads and validates the CONNECT packet, replies with CONNACK (accepted),
/// then reads and validates the SUBSCRIBE packet, replies with SUBACK (QoS 1 granted).
pub async fn handle_mqtt_handshake(stream: &mut DuplexStream) {
    // 1. Validate CONNECT packet
    let (header, _payload) = read_packet(stream)
        .await
        .expect("Failed to read CONNECT packet");
    assert_eq!(
        header, HEADER_CONNECT,
        "Expected CONNECT header (0x{:02X})",
        HEADER_CONNECT
    );

    // Reply with CONNACK (accepted)
    stream
        .write_all(&[HEADER_CONNACK, 0x02, 0x00, 0x00])
        .await
        .expect("Failed to write CONNACK response");
    stream.flush().await.expect("Failed to flush CONNACK");

    // 2. Validate SUBSCRIBE packet
    let (header, payload) = read_packet(stream)
        .await
        .expect("Failed to read SUBSCRIBE packet");
    assert_eq!(
        header, HEADER_SUBSCRIBE,
        "Expected SUBSCRIBE header (0x{:02X})",
        HEADER_SUBSCRIBE
    );

    // Reply with SUBACK, echoing the packet ID and granting QoS 1
    stream
        .write_all(&[HEADER_SUBACK, 0x03, payload[0], payload[1], 0x01])
        .await
        .expect("Failed to write SUBACK response");
    stream.flush().await.expect("Failed to flush SUBACK");
}

/// Intercepts and parses the JSON body of the next MQTT PUBLISH packet sent by the client.
pub async fn read_publish_payload(stream: &mut DuplexStream) -> serde_json::Value {
    let (header, packet) = read_packet(stream)
        .await
        .expect("Failed to read PUBLISH packet");
    assert_eq!(
        header, HEADER_PUBLISH_QOS1,
        "Expected PUBLISH QoS 1 header (0x{:02X})",
        HEADER_PUBLISH_QOS1
    );

    let topic_len = u16::from_be_bytes([packet[0], packet[1]]) as usize;
    let payload_start = 2 + topic_len + 2; // +2 topic len prefix, +2 packet ID

    serde_json::from_slice(&packet[payload_start..]).expect("Failed to parse PUBLISH JSON payload")
}

/// Executes the mock MQTT v3.1.1 broker task on the provided bidirectional stream.
///
/// * `stream`: The broker-side end of the duplex TCP control stream.
/// * `serial`: The mocked printer serial number (used for telemetry topic formatting).
/// * `inject_rx`: Channel receiver used by the test suite to push mock telemetry payloads.
/// * `ack_tx`: A oneshot channel used to signal the test suite the exact moment a `PUBACK`
///   is flushed to the client socket, preventing race conditions.
pub async fn run_mock_mqtt_broker(
    mut stream: DuplexStream,
    serial: String,
    mut inject_rx: mpsc::Receiver<Vec<u8>>,
    ack_tx: oneshot::Sender<()>,
) {
    handle_mqtt_handshake(&mut stream).await;

    // Enter Multiplexing Event Loop
    let mut server_packet_id: u16 = 1000;
    let topic = format!("device/{}/report", serial);
    let mut ack_sender = Some(ack_tx);

    loop {
        tokio::select! {
            // A: Listen for incoming client packets
            result = read_packet(&mut stream) => {
                match result {
                    Ok((header, payload)) => {
                        let packet_type = header >> 4;
                        match packet_type {
                            PACKET_TYPE_PUBLISH => {
                                let qos = (header & 0x06) >> 1;
                                if qos == 1 {
                                    let topic_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
                                    let id_msb = payload[2 + topic_len];
                                    let id_lsb = payload[3 + topic_len];

                                    stream
                                        .write_all(&[HEADER_PUBACK, 0x02, id_msb, id_lsb])
                                        .await
                                        .expect("Failed to write PUBACK");
                                    stream.flush().await.expect("Failed to flush PUBACK");

                                    if let Some(tx) = ack_sender.take() {
                                        let _ = tx.send(());
                                    }
                                }
                            }
                            PACKET_TYPE_PINGREQ => {
                                stream
                                    .write_all(&[HEADER_PINGRESP, 0x00])
                                    .await
                                    .expect("Failed to write PINGRESP");
                            }
                            PACKET_TYPE_DISCONNECT => {
                                break;
                            }
                            PACKET_TYPE_PUBACK => {
                                // Client acknowledged our telemetry — safely ignore in mock
                            }
                            _ => panic!("Unexpected MQTT packet type received: {}", packet_type),
                        }
                    }
                    Err(_) => break,
                }
            }

            // B: Listen for test-suite payload injections
            Some(injection_payload) = inject_rx.recv() => {
                let mut var_header = Vec::new();

                var_header.extend_from_slice(&(topic.len() as u16).to_be_bytes());
                var_header.extend_from_slice(topic.as_bytes());

                var_header.extend_from_slice(&server_packet_id.to_be_bytes());
                server_packet_id = server_packet_id.wrapping_add(1);

                let remaining_length = var_header.len() + injection_payload.len();
                let mut packet = vec![HEADER_PUBLISH_QOS1];
                packet.extend_from_slice(&encode_remaining_length(remaining_length));
                packet.extend(var_header);
                packet.extend_from_slice(&injection_payload);

                stream.write_all(&packet).await.expect("Failed to write injected PUBLISH");
                stream.flush().await.expect("Failed to flush injected PUBLISH");
            }
        }
    }
}
