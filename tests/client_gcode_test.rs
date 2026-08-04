//! # Client Coordinator — G-code Safety Validation
//!
//! Split from `client_test.rs` (see issue #35).

mod common;


use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use common::client::connect_test_client;
use common::mock_mqtt::{
    handle_mqtt_handshake, read_publish_payload,
};

// ============================================================================
// G-code Safety Validation Tests
// ============================================================================

#[tokio::test]
async fn test_send_gcode_rejects_unsafe_homing() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Only safe G28 should arrive
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "G28\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // Unsafe partial homing on bed-on-Z must be rejected by send_gcode
    let err = client.send_gcode("G28 Z").await;
    assert!(matches!(err, Err(Error::ModelMismatch(_))));

    // Safe bare G28 must pass
    client
        .send_gcode("G28")
        .await
        .expect("Safe G28 should pass");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_send_gcode_raw_bypasses_safety() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Raw mode should send the unsafe command through
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "G28 Z\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // send_gcode_raw should bypass safety checks
    client
        .send_gcode_raw("G28 Z")
        .await
        .expect("Raw G-code should bypass safety");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_temperature_clamping() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bed temp 500 should be clamped to X1E max (110)
        let json_bed = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_bed["print"]["param"], "M140 S110\n");

        // Nozzle temp 999 should be clamped to X1E max (320)
        let json_nozzle = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_nozzle["print"]["param"], "M104 T0 S320\n");

        // Chamber temp 200 should be clamped to X1E max (60)
        let json_chamber = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_chamber["print"]["param"], "M141 S60\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1E).await;

    client
        .set_bed_temperature(500)
        .await
        .expect("Bed temp clamp failed");
    client
        .set_nozzle_temperature(0, 999)
        .await
        .expect("Nozzle temp clamp failed");
    client
        .set_chamber_temperature(200)
        .await
        .expect("Chamber temp clamp failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_temperature_clamping_lower_bound() {
    // set_bed_temperature/set_nozzle_temperature/set_chamber_temperature clamp only above
    // max — a 0 ("turn heater off") request must pass through unchanged, not get pulled up
    // to some floor. Every other clamp test in this file only sends values above max.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_bed = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_bed["print"]["param"], "M140 S0
");

        let json_nozzle = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_nozzle["print"]["param"], "M104 T0 S0
");

        let json_chamber = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_chamber["print"]["param"], "M141 S0
");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1E).await;

    client
        .set_bed_temperature(0)
        .await
        .expect("Bed temp floor failed");
    client
        .set_nozzle_temperature(0, 0)
        .await
        .expect("Nozzle temp floor failed");
    client
        .set_chamber_temperature(0)
        .await
        .expect("Chamber temp floor failed");

    broker_task.await.expect("Broker task panicked");
}

