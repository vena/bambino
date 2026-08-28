//! # Client Coordinator — Session/Polling/Homing Round-Trip Tests
//!
//! Split from `client_test.rs` Phase 18 section (see issue #35).

mod common;

use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use common::client::{SERIAL, connect_test_client};
use common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

#[tokio::test]
async fn test_request_pushall() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["pushing"]["command"], "pushall");
        assert!(json["pushing"]["sequence_id"].is_string());
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    client
        .request_pushall()
        .await
        .expect("request_pushall failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_home_flag_cache_and_advisory_warnings() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // X and Y homed (bits 0-1), Z not homed (bit 2 clear).
        send_publish_payload(
            &mut server_stream,
            &topic,
            2000,
            br#"{"print":{"home_flag":3}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Unhomed Z move and extrude must still be dispatched — advisory only, not a gate.
        let json_z = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_z["print"]["command"], "gcode_line");

        let json_e = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_e["print"]["param"], "M83\nG0 E5.00 F500\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    // No telemetry observed yet — cache must read as unknown, not "unhomed".
    assert_eq!(client.is_axis_homed('x'), None);
    assert_eq!(client.is_all_axes_homed(), None);

    let event = client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag report");
    assert!(event.report().is_some());

    assert_eq!(client.is_axis_homed('x'), Some(true));
    assert_eq!(client.is_axis_homed('y'), Some(true));
    assert_eq!(client.is_axis_homed('z'), Some(false));
    assert_eq!(client.is_axis_homed('e'), None);
    assert_eq!(client.is_all_axes_homed(), Some(false));

    client
        .move_relative('z', 5.0, 1000)
        .await
        .expect("move_relative should proceed despite unhomed Z");
    client
        .extrude(5.0, 500)
        .await
        .expect("extrude should proceed despite unhomed axes");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_home_flag_bit31_set_deserializes_as_negative_wire_value() {
    // 0x80000003 as a signed 32-bit int is -2147483645; the wire sends this negative form
    // ([REF-HOMEFLAG]) and it must mask back to the same bit pattern rather than failing the
    // whole telemetry message's deserialize (issue #49).
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            2500,
            br#"{"print":{"home_flag":-2147483645}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let event = client
        .poll_telemetry()
        .await
        .expect("negative home_flag must still parse via deserialize_signed_as_u32");
    assert!(event.report().is_some());

    assert_eq!(client.is_axis_homed('x'), Some(true));
    assert_eq!(client.is_axis_homed('y'), Some(true));
    assert_eq!(client.is_axis_homed('z'), Some(false));

    broker_task.await.expect("Broker task panicked");
}

// Phase 8: wait_for_homing

#[tokio::test]
async fn test_wait_for_homing_resolves_after_dip() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Already-homed reading must not resolve the call on its own.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4000,
            br#"{"print":{"home_flag":7}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Dip: not all axes homed mid-cycle.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4001,
            br#"{"print":{"home_flag":3}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Recovery: all axes homed again — this is the reading that should resolve.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4002,
            br#"{"print":{"home_flag":7}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    client
        .wait_for_homing()
        .await
        .expect("wait_for_homing should resolve after observing a dip followed by recovery");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_wait_for_homing_resolves_when_already_in_progress() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // First observed reading is already mid-home (e.g. touchscreen-triggered before
        // this client started watching) — must still count as the dip.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4100,
            br#"{"print":{"home_flag":1}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            4101,
            br#"{"print":{"home_flag":7}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    client
        .wait_for_homing()
        .await
        .expect("wait_for_homing should resolve on a join-in-progress external home");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_wait_for_homing_times_out_without_dip() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Axes stay fully homed for the entire window — no dip is ever observed, so
        // wait_for_homing must exhaust the message-count safety valve and time out
        // rather than resolving on the first (or any) all-homed reading.
        for i in 0..200u16 {
            send_publish_payload(
                &mut server_stream,
                &topic,
                4200 + i,
                br#"{"print":{"home_flag":7}}"#,
            )
            .await;
            read_puback(&mut server_stream).await;
        }
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let result = client.wait_for_homing().await;
    assert!(
        matches!(result, Err(Error::Timeout)),
        "expected timeout when no dip is ever observed, got {:?}",
        result
    );

    broker_task.await.expect("Broker task panicked");
}
