//! # Mock MQTT Broker
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

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

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
async fn read_packet(
    stream: &mut tokio::io::DuplexStream,
) -> Result<(u8, Vec<u8>), std::io::Error> {
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

/// Executes the mock MQTT v3.1.1 broker task on the provided bidirectional stream.
///
/// * `stream`: The broker-side end of the duplex TCP control stream.
/// * `serial`: The mocked printer serial number (used for telemetry topic formatting).
/// * `inject_rx`: Channel receiver used by the test suite to push mock telemetry payloads.
/// * `ack_tx`: A oneshot channel used to signal the test suite the exact moment a `PUBACK`
///   is flushed to the client socket, preventing race conditions.
pub async fn run_mock_mqtt_broker(
    mut stream: tokio::io::DuplexStream,
    serial: String,
    mut inject_rx: mpsc::Receiver<Vec<u8>>,
    ack_tx: oneshot::Sender<()>,
) {
    // 1. Handle CONNECT Handshake
    let (header, _payload) = read_packet(&mut stream)
        .await
        .expect("Failed to read CONNECT");
    assert_eq!(header >> 4, 1, "Expected CONNECT packet type (1)");

    // Reply with CONNACK (0x20) indicating connection accepted (0x00)
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();

    // 2. Handle SUBSCRIBE Handshake
    let (header, payload) = read_packet(&mut stream)
        .await
        .expect("Failed to read SUBSCRIBE");
    assert_eq!(header >> 4, 8, "Expected SUBSCRIBE packet type (8)");

    // Extract packet ID from the first 2 bytes of the SUBSCRIBE payload
    let packet_id_msb = payload[0];
    let packet_id_lsb = payload[1];

    // Reply with SUBACK (0x90), echoing packet ID, granting QoS 1 (0x01)
    stream
        .write_all(&[0x90, 0x03, packet_id_msb, packet_id_lsb, 0x01])
        .await
        .unwrap();

    // 3. Enter Multiplexing Event Loop
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
                            3 => {
                                // PUBLISH received from client. Echo a PUBACK to confirm.
                                let qos = (header & 0x06) >> 1;
                                if qos == 1 {
                                    // Extract variable topic length to find packet ID position
                                    let topic_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
                                    let id_msb = payload[2 + topic_len];
                                    let id_lsb = payload[3 + topic_len];

                                    // Send PUBACK (0x40)
                                    stream.write_all(&[0x40, 0x02, id_msb, id_lsb]).await.unwrap();
                                    stream.flush().await.unwrap();

                                    // Signal the test thread that the PUBACK has been successfully flushed
                                    if let Some(tx) = ack_sender.take() {
                                        let _ = tx.send(());
                                    }
                                }
                            }
                            12 => {
                                // PINGREQ (0xC0) received. Echo PINGRESP (0xD0).
                                stream.write_all(&[0xD0, 0x00]).await.unwrap();
                            }
                            14 => {
                                // DISCONNECT received. Terminate broker loop.
                                break;
                            }
                            4 => {
                                // PUBACK received (Client acknowledged our telemetry).
                                // Safely ignore in mock.
                            }
                            _ => panic!("Unexpected MQTT packet type received: {}", packet_type),
                        }
                    }
                    Err(_) => break, // Stream closed abruptly
                }
            }

            // B: Listen for test-suite payload injections
            Some(injection_payload) = inject_rx.recv() => {
                let mut var_header = Vec::new();

                // Encode Topic
                var_header.extend_from_slice(&(topic.len() as u16).to_be_bytes());
                var_header.extend_from_slice(topic.as_bytes());

                // Encode Packet ID
                var_header.extend_from_slice(&server_packet_id.to_be_bytes());
                server_packet_id = server_packet_id.wrapping_add(1);

                // Build QoS 1 PUBLISH Frame (0x32)
                let remaining_length = var_header.len() + injection_payload.len();
                let mut packet = vec![0x32];
                packet.extend_from_slice(&encode_remaining_length(remaining_length));
                packet.extend(var_header);
                packet.extend_from_slice(&injection_payload);

                stream.write_all(&packet).await.unwrap();
                stream.flush().await.unwrap();
            }
        }
    }
}
