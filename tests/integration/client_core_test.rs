//! # Client Coordinator — Core Behavioral Verification
//!
//! Split from `client_test.rs` (see issue #35); safety boundaries, temperature
//! clamps, fan step calculations, and G-code wrapping heuristics.

use bambino::client::{BuzzerMode, FanTarget};
use bambino::error::Error;
use bambino::models::PrinterModel;

use crate::common::client::{SERIAL, X1_SERIAL, connect_idle_client, with_broker};
use crate::common::mock_mqtt::{
    ReportPublisher, handle_mqtt_handshake, read_gcode_param, read_publish_payload,
};

// ============================================================================
// Core behavioral verification cases
// ============================================================================

#[tokio::test]
async fn test_homing_safety_interlocks() {
    // CoreXY Bed-on-Z initialization
    let (mut client_x1c, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1C,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Verify Bed-on-Z Safe Homing Command (Bare G28)
            assert_eq!(read_gcode_param(&mut server_stream).await, "G28\n");
        },
    )
    .await;

    // Assert public serial and model getters expose the correct fields
    assert_eq!(client_x1c.serial(), X1_SERIAL);
    assert_eq!(client_x1c.model(), PrinterModel::X1C);

    // Bed-on-Z Safety Guard Verification: home_z_only_danger must return ModelMismatch
    let err_res = client_x1c.home_z_only().await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Standard homing should succeed with bare G28
    client_x1c.home_all().await.expect("G28 homing failed");

    // Bed-Slinger initialization

    let (mut client_a1, broker_task_a1) = with_broker(
        "039000000000000",
        PrinterModel::A1,
        |mut server_stream_a1| async move {
            handle_mqtt_handshake(&mut server_stream_a1).await;
            // Verify Bed-Slinger homing parameters write G28 Z to the stream
            assert_eq!(read_gcode_param(&mut server_stream_a1).await, "G28 Z\n");
        },
    )
    .await;

    // Bed-Slingers do not share upward bed collision hazards; G28 Z homing is permitted
    client_a1
        .home_z_only()
        .await
        .expect("A1 Z-only homing failed");

    broker_task.await.expect("X1C broker task panicked");
    broker_task_a1.await.expect("A1 broker task panicked");
}

#[tokio::test]
async fn test_kinematic_and_extrusion_moves() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Read Z move packet: BambuStudio's M211 save/enable/restore and reference coordinate wraps
            assert_eq!(
                read_gcode_param(&mut server_stream).await,
                "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode\nM211 R\n"
            );

            // Read X move packet: plain relative move G91 -> G0 -> G90
            assert_eq!(read_gcode_param(&mut server_stream).await, "G91\nG0 X-15.50 F6000\nG90\n");

            // Read relative manual extrusion packet
            assert_eq!(read_gcode_param(&mut server_stream).await, "M83\nG0 E10.00 F900\n");
            })
        .await;

    client
        .move_relative(bambino::client::Axis::Z, 10.0, 3000)
        .await
        .expect("Z move failed");
    client
        .move_relative(bambino::client::Axis::X, -15.5, 6000)
        .await
        .expect("X move failed");
    client.extrude(10.0, 900).await.expect("Extrusion failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_move_relative_zero_distance_is_noop() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Only the non-zero X move below should reach the wire — the zero-distance Z and X
            // calls must short-circuit before publishing anything. If either zero-distance call
            // incorrectly published, this would be the first packet read instead, and the
            // assertion below would fail on mismatched params.
            let json_x = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_x["print"]["param"], "G91\nG0 X5.00 F1000\nG90\n");
            json_x
        })
        .await;

    // Zero-distance Z move: must be a no-op (Ok(None), no travel-limit error, no wire traffic) —
    // not the misleading "exceeds model travel limits" error `relative_z_move_gcode` would
    // otherwise collapse it into (it returns the same empty string for zero and out-of-range).
    let z_result = client
        .move_relative(bambino::client::Axis::Z, 0.0, 3000)
        .await
        .expect("zero-distance Z move should succeed as a no-op");
    assert_eq!(
        z_result, None,
        "a no-op move publishes nothing, so has no handle"
    );

    // Zero-distance X move: same no-op contract, off the Z-only travel-limit code path.
    let x_zero_result = client
        .move_relative(bambino::client::Axis::X, 0.0, 1000)
        .await
        .expect("zero-distance X move should succeed as a no-op");
    assert_eq!(
        x_zero_result, None,
        "a no-op move publishes nothing, so has no handle"
    );

    // Non-zero move on the same client still publishes normally, and its handle names exactly
    // what reached the wire (issue #283).
    let handle = client
        .move_relative(bambino::client::Axis::X, 5.0, 1000)
        .await
        .expect("non-zero X move failed")
        .expect("a published move must return a handle");

    let json_x = broker_task.await.expect("Broker task panicked");
    assert_eq!(handle.command(), "gcode_line");
    assert_eq!(
        json_x["print"]["sequence_id"],
        handle.sequence_id().to_string(),
        "the handle must carry the sequence_id the printer will echo, not the MQTT packet id"
    );
    assert_eq!(handle.ack(), bambino::client::AckExpectation::Echoes);
}

#[tokio::test]
async fn test_move_relative_z_still_rejects_out_of_range_distance() {
    let (mut client, broker_task) = connect_idle_client(PrinterModel::P1S).await;

    // P1S z_max is 256.0mm — a non-zero distance exceeding that must still surface the
    // travel-limit error, confirming the zero-distance short-circuit didn't swallow this case.
    let result = client
        .move_relative(bambino::client::Axis::Z, 300.0, 3000)
        .await;
    assert!(matches!(result, Err(Error::ModelMismatch(_))));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_move_relative_x_rejects_out_of_range_distance() {
    // X/Y moves previously had no distance cap at all, unlike Z. P1S x_max is
    // 256.0mm — a distance exceeding that must be rejected the same way Z's out-of-range
    // case already is.

    let (mut client, broker_task) = connect_idle_client(PrinterModel::P1S).await;

    let result = client
        .move_relative(bambino::client::Axis::X, 300.0, 3000)
        .await;
    assert!(matches!(result, Err(Error::ModelMismatch(_))));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_thermal_guards_and_temperatures() {
    let (mut client_x1e, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1E,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S60\n");

            assert_eq!(read_gcode_param(&mut server_stream).await, "M104 T0 S220\n");

            // Active chamber temperature verification (X1E has active PTC heater)
            assert_eq!(read_gcode_param(&mut server_stream).await, "M141 S45\n");
        },
    )
    .await;

    client_x1e
        .set_bed_temperature(60)
        .await
        .expect("Bed temp set failed");
    client_x1e
        .set_nozzle_temperature(0, 220)
        .await
        .expect("Nozzle temp set failed");

    // Chamber temperature should succeed on models with active PTC heaters
    client_x1e
        .set_chamber_temperature(45)
        .await
        .expect("Chamber temp set failed");

    // X1C has a chamber sensor but no active heater — M141 must be rejected

    let (mut client_x1c, broker_task_x1c) = with_broker(
        X1_SERIAL,
        PrinterModel::X1C,
        |mut server_stream_x1c| async move {
            handle_mqtt_handshake(&mut server_stream_x1c).await;
        },
    )
    .await;

    let err_res = client_x1c.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Open-frame model check (A1 — no sensor, no heater)

    let (mut client_a1, broker_task_a1) = with_broker(
        "039000000000000",
        PrinterModel::A1,
        |mut server_stream_a1| async move {
            handle_mqtt_handshake(&mut server_stream_a1).await;
        },
    )
    .await;

    let err_res = client_a1.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    broker_task.await.expect("X1E broker task panicked");
    broker_task_x1c.await.expect("X1C broker task panicked");
    broker_task_a1.await.expect("A1 broker task panicked");
}

// X1C's bed_temp_max ceiling is voltage-dependent (110°C @220V, 120°C @110V, per
// src/quirks/models/x1.rs's X1C_BED_TEMP_MAX), derived from cached home_flag telemetry — but
// every existing bed-clamping test above only exercises X1E, whose ceiling ignores the
// parameter entirely. A regression that swapped the two constants or flipped the None-case
// fallback direction would pass every existing test untouched.
#[tokio::test]
async fn test_x1c_bed_temp_ceiling_voltage_dependent() {
    let (mut client, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1C,
        |mut server_stream| async move {
            let mut reports = ReportPublisher::new(X1_SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // No home_flag observed yet — must clamp to the conservative 220V-region default.
            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S110\n");

            // home_flag bit 3 set -> confirmed 220V region.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":8}}"#)
                .await;

            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S110\n");

            // home_flag bit 3 clear -> confirmed 110V region, higher ceiling.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":0}}"#)
                .await;

            assert_eq!(read_gcode_param(&mut server_stream).await, "M140 S120\n");
        },
    )
    .await;

    assert_eq!(client.is_220v_power(), None);
    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed pre-telemetry");

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag=8");
    assert_eq!(client.is_220v_power(), Some(true));
    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed at confirmed 220V");

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag=0");
    assert_eq!(client.is_220v_power(), Some(false));
    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed at confirmed 110V");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_cooling_fans_and_peripheral_switches() {
    let (mut client_p1s, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            assert_eq!(read_gcode_param(&mut server_stream).await, "M106 P1 S127\n"); // 50% PWM

            assert_eq!(read_gcode_param(&mut server_stream).await, "M106 P2 S255\n"); // 100% PWM
        })
        .await;

    client_p1s
        .set_fan_speed(FanTarget::PartCooling, 50)
        .await
        .expect("Part cooling fan set failed");
    client_p1s
        .set_fan_speed(FanTarget::AuxiliaryLeft, 100)
        .await
        .expect("Auxiliary left fan set failed");

    // Verify the second left-side auxiliary fan (port 10) is restricted on non-X2D models
    let err_res = client_p1s
        .set_fan_speed(FanTarget::AuxiliaryLeft2, 80)
        .await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Verify the second left-side auxiliary fan is supported on X2D (port 10)

    let (mut client_x2, broker_task_x2) = with_broker(
        "20P000000000000",
        PrinterModel::X2D,
        |mut server_stream_x2| async move {
            handle_mqtt_handshake(&mut server_stream_x2).await;
            assert_eq!(
                read_gcode_param(&mut server_stream_x2).await,
                "M106 P10 S204\n"
            ); // 80% PWM
        },
    )
    .await;

    client_x2
        .set_fan_speed(FanTarget::AuxiliaryLeft2, 80)
        .await
        .expect("X2D second left auxiliary fan set failed");

    broker_task.await.expect("P1S broker task panicked");
    broker_task_x2.await.expect("X2D broker task panicked");
}

#[tokio::test]
async fn test_set_fan_speed_clamps_above_100_percent() {
    // set_fan_speed's speed_percent > 100 clamp path was never exercised — every
    // existing call in this file used values <= 100. 150% must clamp to the same 255 PWM
    // value 100% produces, not overflow or wrap.

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            assert_eq!(read_gcode_param(&mut server_stream).await, "M106 P1 S255\n"); // clamped to 100% PWM
        })
        .await;

    client
        .set_fan_speed(FanTarget::PartCooling, 150)
        .await
        .expect("Part cooling fan set failed");

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_chamber_exhaust_fan_success_and_model_mismatch() {
    // FanTarget::ChamberExhaust was never exercised through set_fan_speed, unlike its
    // 3 sibling fan targets above (PartCooling, AuxiliaryLeft, AuxiliaryLeft2). Mirrors the
    // AuxiliaryLeft2 success (X2D)/mismatch (P1S) pair in
    // test_cooling_fans_and_peripheral_switches, using H2D for the success case since chamber
    // exhaust is an H2-series/X2D feature.

    let (mut client_h2d, broker_task) = with_broker(
        "09P000000000000",
        PrinterModel::H2D,
        |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            assert_eq!(read_gcode_param(&mut server_stream).await, "M106 P3 S204\n"); // 80% PWM
        },
    )
    .await;

    client_h2d
        .set_fan_speed(FanTarget::ChamberExhaust, 80)
        .await
        .expect("H2D chamber exhaust fan set failed");

    broker_task.await.expect("H2D broker task panicked");

    // Verify chamber exhaust fan is restricted on a model without one (P1S).
    let (mut client_p1s, broker_task_p1s) = connect_idle_client(PrinterModel::P1S).await;

    let err_res = client_p1s
        .set_fan_speed(FanTarget::ChamberExhaust, 80)
        .await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    broker_task_p1s.await.expect("P1S broker task panicked");
}

#[tokio::test]
async fn test_queue_lifecycle_control_blocks() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            let json_pause = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_pause["print"]["command"], "pause");

            let json_resume = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_resume["print"]["command"], "resume");

            let json_stop = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_stop["print"]["command"], "stop");
        })
        .await;

    client.pause_print().await.expect("Pause failed");
    client.resume_print().await.expect("Resume failed");
    client.stop_print().await.expect("Stop failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_peripheral_signals_and_climate_controls() {
    // H2D supports airduct + buzzer

    let (mut client_h2d, broker_task) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            let json_airduct = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_airduct["print"]["command"], "set_airduct");
            assert_eq!(json_airduct["print"]["modeId"], 0);

            let json_buzzer = read_publish_payload(&mut server_stream).await;
            assert_eq!(json_buzzer["print"]["command"], "buzzer_ctrl");
            assert_eq!(json_buzzer["print"]["mode"], 2);
        })
        .await;

    client_h2d
        .set_airduct_mode(bambino::mqtt::commands::AirductMode::Cooling)
        .await
        .expect("Airduct mode set failed");
    client_h2d
        .set_buzzer_mode(BuzzerMode::Chirp)
        .await
        .expect("Buzzer mode set failed");

    broker_task.await.expect("H2D broker task panicked");

    // A1 supports prompt sound

    let (mut client_a1, broker_task_a1) = with_broker(
        "039000000000000",
        PrinterModel::A1,
        |mut server_stream_a1| async move {
            handle_mqtt_handshake(&mut server_stream_a1).await;

            let json_sound = read_publish_payload(&mut server_stream_a1).await;
            assert_eq!(json_sound["print"]["command"], "print_option");
            assert_eq!(json_sound["print"]["sound_enable"], true);
        },
    )
    .await;

    client_a1
        .set_prompt_sound(true)
        .await
        .expect("Prompt sound set failed");

    broker_task_a1.await.expect("A1 broker task panicked");

    // P1S supports none of these

    let (mut client_p1s, broker_task_p1s) = connect_idle_client(PrinterModel::P1S).await;

    assert!(matches!(
        client_p1s
            .set_airduct_mode(bambino::mqtt::commands::AirductMode::Cooling)
            .await,
        Err(Error::ModelMismatch(_))
    ));
    assert!(matches!(
        client_p1s.set_prompt_sound(true).await,
        Err(Error::ModelMismatch(_))
    ));
    assert!(matches!(
        client_p1s.set_buzzer_mode(BuzzerMode::Alarm).await,
        Err(Error::ModelMismatch(_))
    ));

    broker_task_p1s.await.expect("P1S broker task panicked");
}

/// The error-dialog commands echo the job's `job_id`, cached from telemetry (sent as a number by
/// the firmware, as BambuStudio's `get_longlong_val` reads it), and encode `err` in decimal —
/// except `uiop`, which is 8-digit hex.
#[tokio::test]
async fn test_error_dialog_commands_reach_the_wire() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            ReportPublisher::new(SERIAL)
                .publish(
                    &mut server_stream,
                    br#"{"print":{"job_id":4242,"print_error":83935248}}"#,
                )
                .await;

            let ignore = read_publish_payload(&mut server_stream).await;
            assert_eq!(ignore["print"]["command"], "ignore");
            assert_eq!(ignore["print"]["err"], "83935248");
            assert_eq!(ignore["print"]["param"], "reserve");
            assert_eq!(ignore["print"]["job_id"], "4242");

            let dismiss = read_publish_payload(&mut server_stream).await;
            assert_eq!(dismiss["print"]["command"], "idle_ignore");
            assert_eq!(dismiss["print"]["type"], 1);

            let close = read_publish_payload(&mut server_stream).await;
            assert_eq!(close["system"]["command"], "uiop");
            assert_eq!(close["system"]["err"], "0500C010");

            let refresh = read_publish_payload(&mut server_stream).await;
            assert_eq!(refresh["print"]["command"], "refresh_nozzle");
        })
        .await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    client
        .ignore_error_and_resume(0x0500_C010_u32)
        .await
        .expect("ignore");
    client
        .dismiss_error(0x0500_C010_u32, bambino::client::IdleIgnoreScope::Permanent)
        .await
        .expect("idle_ignore");
    client
        .close_error_dialog(0x0500_C010_u32)
        .await
        .expect("uiop");
    client.refresh_nozzle().await.expect("refresh_nozzle");

    broker_task.await.expect("Broker task panicked");
}
