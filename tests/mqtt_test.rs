//! # MQTT Client Integration & Protocol Mock Tests
//!
//! Validates the state machine transitions, QoS 1 publish tracking, telemetry
//! polling, and timeout guards of the custom `BambuMqttClient`.
//!
//! Uses the shared `mock_mqtt` broker over in-memory duplex streams to ensure
//! deterministic verification of protocol packet framing and multiplexing.

mod common;

use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use bambino::error::BambuError;
use bambino::io::{TimerProvider, TokioIo};
use bambino::mqtt::BambuMqttClient;

use common::mock_mqtt::run_mock_mqtt_broker;

/// Dummy timer satisfying the platform-agnostic `TimerProvider` bound during testing.
struct DummyTimer;

impl TimerProvider for DummyTimer {
    async fn sleep(_duration: Duration) {
        // No-op for instantaneous mock tests
    }
}

#[tokio::test]
async fn test_mqtt_client_lifecycle_and_telemetry() {
    let (client_stream, server_stream) = tokio::io::duplex(8192);
    let (inject_tx, inject_rx) = mpsc::channel(10);
    let (ack_tx, ack_rx) = oneshot::channel();
    let serial = "01P000000000000";

    // 1. Spawn the background mock broker
    let broker_handle = tokio::spawn(run_mock_mqtt_broker(
        server_stream,
        serial.to_string(),
        inject_rx,
        ack_tx,
    ));

    // 2. Initialize Client (Executes CONNECT -> CONNACK and SUBSCRIBE -> SUBACK)
    let mut client =
        BambuMqttClient::connect::<DummyTimer>(TokioIo(client_stream), serial, "12345678")
            .await
            .expect("Failed to execute MQTT login and subscription handshake");

    // 3. Test QoS 1 Command Publishing and Tracking
    let _packet_id = client
        .publish_command(b"{\"command\":\"pushall\"}")
        .await
        .expect("QoS 1 command publish failed");

    // Ensure the packet is tracked in the unacknowledged queue
    assert_eq!(
        client.get_in_flight_count(),
        1,
        "Expected 1 in-flight packet"
    );

    // Block the test thread until the broker has directly observed, acknowledged,
    // and flushed the PUBACK to the client stream buffer. This replaces fragile "magic delays".
    ack_rx
        .await
        .expect("Failed to receive command acknowledgment signal from mock broker");

    // 4. Test Telemetry Reception & Implicit Acknowledgments
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

    // Verify the PUBACK was processed in the background during the poll loop
    assert_eq!(
        client.get_in_flight_count(),
        0,
        "In-flight queue did not clear after PUBACK reception"
    );

    // 5. Test PINGREQ / PINGRESP cycle
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

    // 6. Test Write-Channel Zombie Detection
    // Arm the zombie tracker by publishing a new command
    client
        .publish_command(b"{\"command\":\"zombie_test\"}")
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
        matches!(timeout_err, BambuError::Timeout),
        "Expected BambuError::Timeout, got {:?}",
        timeout_err
    );

    // Cleanup
    drop(client);
    drop(inject_tx);
    let _ = broker_handle.await; // Broker will exit its loop when streams drop
}
