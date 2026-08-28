//! # MQTT Client Integration & Protocol Mock Tests
//!
//! Validates the state machine transitions, QoS 1 publish tracking, telemetry
//! polling, and timeout guards of the custom `MqttClient`.
//!
//! Uses the shared `mock_mqtt` broker over in-memory duplex streams to ensure
//! deterministic verification of protocol packet framing and multiplexing.

mod common;

use tokio::sync::{mpsc, oneshot};

use bambino::error::Error;
use bambino::identity::PrinterIdentity;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use bambino::mqtt::MqttClient;

use common::mock_mqtt::run_mock_mqtt_broker;

#[tokio::test]
async fn test_mqtt_client_lifecycle_and_telemetry() {
    let (client_stream, server_stream) = tokio::io::duplex(8192);
    let (inject_tx, inject_rx) = mpsc::channel(10);
    let (ack_tx, ack_rx) = oneshot::channel();
    let serial = "01P000000000000";

    let broker_handle = tokio::spawn(run_mock_mqtt_broker(
        server_stream,
        serial.to_string(),
        inject_rx,
        ack_tx,
    ));

    let mut client = MqttClient::connect(
        TokioIo(client_stream),
        &PrinterIdentity {
            ip: String::new(),
            serial: serial.to_string(),
            access_code: "12345678".to_string(),
            model: PrinterModel::P1S,
        },
    )
    .await
    .expect("Failed to execute MQTT login and subscription handshake");

    let _packet_id = client
        .publish_command(b"{\"pushing\":{\"command\":\"pushall\",\"sequence_id\":\"1\"}}")
        .await
        .expect("QoS 1 command publish failed");

    assert_eq!(client.in_flight_count(), 1, "Expected 1 in-flight packet");

    // Block the test thread until the broker has directly observed, acknowledged,
    // and flushed the PUBACK to the client stream buffer. This replaces fragile "magic delays".
    ack_rx
        .await
        .expect("Failed to receive command acknowledgment signal from mock broker");

    // Inject a telemetry payload into the broker so the client can pull it.
    // As the client pulls this, it will also read the queued `PUBACK` from the broker,
    // thereby clearing the in-flight tracker.
    inject_tx
        .send(b"{\"mock\":\"telemetry_payload_1\"}".to_vec())
        .await
        .expect("Failed to inject telemetry payload 1 into mock broker");

    let msg = client
        .poll_telemetry()
        .await
        .expect("Telemetry poll returned error instead of injected message");

    assert_eq!(msg.topic, format!("device/{}/report", serial));
    assert_eq!(msg.payload, b"{\"mock\":\"telemetry_payload_1\"}");

    assert_eq!(
        client.in_flight_count(),
        0,
        "In-flight queue did not clear after PUBACK reception"
    );

    client
        .send_ping()
        .await
        .expect("PINGREQ keep-alive dispatch failed");

    // Inject another message to unblock the poll loop and process the PINGRESP
    inject_tx
        .send(b"{\"mock\":\"telemetry_payload_2\"}".to_vec())
        .await
        .expect("Failed to inject telemetry payload 2 into mock broker");
    let _ = client
        .poll_telemetry()
        .await
        .expect("Telemetry poll failed after PINGREQ cycle");

    // Arm the zombie tracker by publishing a new command. "zombie_test" isn't a real command
    // name and isn't in ACK_CORRELATED_COMMANDS, so this intentionally exercises only the
    // uncorrelated fallback-clear path (sequence_id-matching is covered elsewhere, in mod.rs's
    // own async_tests) — the wrapper shape below matches the real Payload+Request pattern.
    client
        .publish_command(b"{\"print\":{\"command\":\"zombie_test\",\"sequence_id\":\"2\"}}")
        .await
        .expect("Zombie test command publish failed");

    // Tick forward 5 seconds. Timeout boundary is 10 seconds, so this should pass.
    assert!(
        client.tick_zombie_check(5).is_ok(),
        "Client falsely triggered zombie timeout under 10 seconds"
    );

    // Tick forward another 6 seconds (Total = 11s). This must trigger a timeout error.
    let timeout_err = client.tick_zombie_check(6).unwrap_err();
    assert!(
        matches!(timeout_err, Error::Timeout),
        "Expected Error::Timeout, got {:?}",
        timeout_err
    );

    // Cleanup
    drop(client);
    drop(inject_tx);
    let _ = broker_handle.await; // Broker will exit its loop when streams drop
}
