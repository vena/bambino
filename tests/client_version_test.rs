//! # Client Coordinator — get_version Round-Trip Tests
//!
//! Split from `client_test.rs` Phase 18 section (see issue #35).

mod common;


use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use common::client::{connect_test_client, SERIAL};
use common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

// ============================================================================
// Command-response round-trip tests (Phase 18)
// ============================================================================

const VERSION_RESPONSE: &str = r#"{"info":{"command":"get_version","sequence_id":"10001","module":[{"product_name":"Bambu Lab P1S","name":"ota","hw_ver":"OTA","sw_ver":"01.09.00.00","sn":"01P000000000001","visible":true},{"name":"esp32","sw_ver":"01.02.03.04","sn":"01P000000000002"}]}}"#;

// Correct command but a sequence ID that doesn't belong to us — simulates a stray
// response from another MQTT client querying the same printer concurrently.
// Module content deliberately differs from `VERSION_RESPONSE` so a test that
// wrongly accepts this decoy fails on content, not just on a missed assertion.
const VERSION_RESPONSE_DECOY_SEQ: &str = r#"{"info":{"command":"get_version","sequence_id":"99999","module":[{"name":"decoy","sw_ver":"00.00.00.00","sn":"DECOY0000000000"}]}}"#;

#[tokio::test]
async fn test_get_version_round_trip() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read the get_version request
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["info"]["command"], "get_version");

        // Send a telemetry message first (should get buffered by poll_until)
        let telemetry = br#"{"print":{"gcode_state":"IDLE","mc_percent":0}}"#;
        send_publish_payload(&mut server_stream, &topic, 1000, telemetry).await;
        read_puback(&mut server_stream).await;

        // Then send the version response
        send_publish_payload(
            &mut server_stream,
            &topic,
            1001,
            VERSION_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let info = client.get_version().await.expect("get_version failed");

    assert_eq!(info.command, "get_version");
    assert_eq!(info.module.len(), 2);
    assert_eq!(info.module[0].name, "ota");
    assert_eq!(info.module[0].sw_ver, "01.09.00.00");
    assert_eq!(info.module[1].product_name, "");
    assert!(info.module[1].visible);

    // The buffered telemetry message should be recoverable
    let event = client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should drain buffer");
    let report = event.report().expect("should be a Report variant");
    assert_eq!(
        report.print.as_ref().unwrap().gcode_state,
        Some("IDLE".into())
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_get_version_ignores_mismatched_sequence_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read the get_version request
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["info"]["command"], "get_version");

        // A decoy response with the right command but a sequence ID that doesn't
        // belong to us (e.g. a second MQTT client querying the same printer).
        send_publish_payload(
            &mut server_stream,
            &topic,
            1000,
            VERSION_RESPONSE_DECOY_SEQ.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;

        // Then the real response, correctly sequenced.
        send_publish_payload(
            &mut server_stream,
            &topic,
            1001,
            VERSION_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let info = client
        .get_version()
        .await
        .expect("get_version should skip the decoy and find the real response");

    // Must be the real response's modules, not the decoy's.
    assert_eq!(info.module.len(), 2);
    assert_eq!(info.module[0].name, "ota");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_get_version_times_out_when_only_decoy_sequence_id_seen() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let _json = read_publish_payload(&mut server_stream).await;

        // The correctly-sequenced response never arrives — only decoys with someone
        // else's sequence ID — so get_version must exhaust the message-count safety
        // valve and time out rather than ever accepting a mismatched response.
        for i in 0..200u16 {
            send_publish_payload(
                &mut server_stream,
                &topic,
                5000 + i,
                VERSION_RESPONSE_DECOY_SEQ.as_bytes(),
            )
            .await;
            read_puback(&mut server_stream).await;
        }
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let result = client.get_version().await;
    assert!(
        matches!(result, Err(Error::Timeout)),
        "expected timeout when only a mismatched-sequence decoy is ever sent, got {:?}",
        result
    );

    broker_task.await.expect("Broker task panicked");
}

// Correct command, matching sequence ID, but a malformed shape (`name` is a number, not a
// string) that fails to deserialize as VersionInfo — simulates a firmware response arriving
// but failing to parse (issue #52).
const VERSION_RESPONSE_MALFORMED: &str =
    r#"{"info":{"command":"get_version","sequence_id":"10001","module":[{"name":123}]}}"#;

#[tokio::test]
async fn test_get_version_surfaces_serialization_error_on_malformed_matching_response() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let _json = read_publish_payload(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            1001,
            VERSION_RESPONSE_MALFORMED.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let result = client.get_version().await;
    assert!(
        matches!(result, Err(Error::Serialization)),
        "a matching-command response that fails to parse must surface Error::Serialization, \
         not Error::Timeout — got {:?}",
        result
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_poll_until_buffers_unmatched_messages() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read the get_version request
        let _json = read_publish_payload(&mut server_stream).await;

        // Send 3 telemetry messages before the version response
        for i in 0..3u16 {
            let telemetry = format!(
                r#"{{"print":{{"gcode_state":"RUNNING","mc_percent":{}}}}}"#,
                i * 10
            );
            send_publish_payload(&mut server_stream, &topic, 1000 + i, telemetry.as_bytes()).await;
            read_puback(&mut server_stream).await;
        }

        // Finally send the matching response
        send_publish_payload(
            &mut server_stream,
            &topic,
            1003,
            VERSION_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let info = client
        .get_version()
        .await
        .expect("get_version should find response after unmatched messages");
    assert_eq!(info.module.len(), 2);

    // All 3 buffered messages should be drainable in order
    for i in 0..3 {
        let event = client.poll_telemetry().await.expect("should drain buffer");
        let report = event.report().expect("should be Report");
        assert_eq!(report.print.as_ref().unwrap().mc_percent, Some(i * 10));
    }

    broker_task.await.expect("Broker task panicked");
}
