//! # Client Coordinator — G-code Safety Validation
//!
//! Split from `client_test.rs` (see issue #35).

use bambino::error::Error;
use bambino::models::PrinterModel;

use crate::common::client::{SERIAL, X1_SERIAL, with_broker};
use crate::common::mock_mqtt::{handle_mqtt_handshake, read_gcode_param};

// ============================================================================
// G-code Safety Validation Tests
// ============================================================================

#[tokio::test]
async fn test_send_gcode_rejects_unsafe_homing() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Only safe G28 should arrive
            assert_eq!(read_gcode_param(&mut server_stream).await, "G28\n");
        })
        .await;

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
async fn test_send_gcode_rejects_over_limit_heater_targets() {
    // Regression (#353): send_gcode checked only G28, so raw heater commands bypassed every
    // ceiling the typed setters clamp to.

    let (mut client, broker_task) = with_broker(
        SERIAL,
        PrinterModel::A1Mini,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Only the in-range bed target should arrive
            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S60\n");
        },
    )
    .await;

    for gcode in [
        "M140 S200",
        "M190 S81",
        "M104 S500",
        "G91\nM109 S999",
        "M141 S60",
    ] {
        let err = client.send_gcode(gcode).await;
        assert!(
            matches!(err, Err(Error::ModelMismatch(_))),
            "{gcode:?} should be rejected on A1 Mini, got {err:?}"
        );
    }

    client
        .send_gcode("M140 S60")
        .await
        .expect("In-range bed target should pass");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_send_gcode_raw_bypasses_safety() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Raw mode should send the unsafe command through
            assert_eq!(read_gcode_param(&mut server_stream).await, "G28 Z\n");
        })
        .await;

    // send_gcode_raw should bypass safety checks
    client
        .send_gcode_raw("G28 Z")
        .await
        .expect("Raw G-code should bypass safety");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_temperature_clamping() {
    let (mut client, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1E,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Bed temp 500 should be clamped to X1E max (110)
            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S110\n");

            // Nozzle temp 999 should be clamped to X1E max (320)
            assert_eq!(read_gcode_param(&mut server_stream).await, "M104 T0 S320\n");

            // Chamber temp 200 should be clamped to X1E max (60)
            assert_eq!(read_gcode_param(&mut server_stream).await, "M141 S60\n");
        },
    )
    .await;

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

    let (mut client, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1E,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S0\n");

            assert_eq!(read_gcode_param(&mut server_stream).await, "M104 T0 S0\n");

            assert_eq!(read_gcode_param(&mut server_stream).await, "M141 S0\n");
        },
    )
    .await;

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
