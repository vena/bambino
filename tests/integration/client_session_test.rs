//! # Client Coordinator — Session/Polling/Homing Round-Trip Tests
//!
//! Split from the "Command-response round-trip tests" section of the former
//! `client_test.rs` (see issue #35).

use bambino::error::Error;
use bambino::models::PrinterModel;

use crate::common::client::{SERIAL, with_broker};
use crate::common::mock_mqtt::{
    ReportPublisher, handle_mqtt_handshake, read_gcode_param, read_publish_payload,
};

#[tokio::test]
async fn test_request_pushall() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            let json = read_publish_payload(&mut server_stream).await;
            assert_eq!(json["pushing"]["command"], "pushall");
            assert!(json["pushing"]["sequence_id"].is_string());
        })
        .await;

    client
        .request_pushall()
        .await
        .expect("request_pushall failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_home_flag_cache_and_advisory_warnings() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // X and Y homed (bits 0-1), Z not homed (bit 2 clear).
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":3}}"#)
                .await;

            // Unhomed Z move and extrude must still be dispatched — advisory only, not a gate.
            let json_z = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_z["print"]["command"], "gcode_line");

            assert_eq!(
                read_gcode_param(&mut server_stream).await,
                "M83\nG0 E5.00 F500\n"
            );
        })
        .await;

    // No telemetry observed yet — cache must read as unknown, not "unhomed".
    assert_eq!(client.is_axis_homed(bambino::client::Axis::X), None);
    assert_eq!(client.is_all_axes_homed(), None);

    let event = client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag report");
    assert!(event.report().is_some());

    assert_eq!(client.is_axis_homed(bambino::client::Axis::X), Some(true));
    assert_eq!(client.is_axis_homed(bambino::client::Axis::Y), Some(true));
    assert_eq!(client.is_axis_homed(bambino::client::Axis::Z), Some(false));
    assert_eq!(client.is_all_axes_homed(), Some(false));

    client
        .move_relative(bambino::client::Axis::Z, 5.0, 1000)
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

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"home_flag":-2147483645}}"#,
                )
                .await;
        })
        .await;

    let event = client
        .poll_telemetry()
        .await
        .expect("negative home_flag must still parse via deserialize_signed_as_u32");
    assert!(event.report().is_some());

    assert_eq!(client.is_axis_homed(bambino::client::Axis::X), Some(true));
    assert_eq!(client.is_axis_homed(bambino::client::Axis::Y), Some(true));
    assert_eq!(client.is_axis_homed(bambino::client::Axis::Z), Some(false));

    broker_task.await.expect("Broker task panicked");
}

// wait_for_homing

#[tokio::test]
async fn test_wait_for_homing_resolves_after_dip() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // Already-homed reading must not resolve the call on its own.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":7}}"#)
                .await;

            // Dip: not all axes homed mid-cycle.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":3}}"#)
                .await;

            // Recovery: all axes homed again — this is the reading that should resolve.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":7}}"#)
                .await;
        })
        .await;

    client
        .wait_for_homing()
        .await
        .expect("wait_for_homing should resolve after observing a dip followed by recovery");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_wait_for_homing_resolves_when_already_in_progress() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // First observed reading is already mid-home (e.g. touchscreen-triggered before
            // this client started watching) — must still count as the dip.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":1}}"#)
                .await;

            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":7}}"#)
                .await;
        })
        .await;

    client
        .wait_for_homing()
        .await
        .expect("wait_for_homing should resolve on a join-in-progress external home");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_wait_for_homing_times_out_without_dip() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // Axes stay fully homed for the entire window — no dip is ever observed, so
            // wait_for_homing must exhaust the message-count safety valve and time out
            // rather than resolving on the first (or any) all-homed reading.
            for _ in 0..200 {
                reports
                    .publish(&mut server_stream, br#"{"print":{"home_flag":7}}"#)
                    .await;
            }
        })
        .await;

    let result = client.wait_for_homing().await;
    assert!(
        matches!(result, Err(Error::Timeout)),
        "expected timeout when no dip is ever observed, got {:?}",
        result
    );

    broker_task.await.expect("Broker task panicked");
}
