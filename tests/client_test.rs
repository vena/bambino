//! # Client Coordinator Behavioral Integration Tests
//!
//! Validates the safety boundaries, temperature clamps, fan step calculations, and
//! G-code wrapping heuristics implemented inside `PrinterClient`.
//!
//! Evaluates the client against an inline, in-memory duplex stream mock to ensure
//! exact verification of the generated raw JSON payloads and raw G-code arrays.

mod common;

use std::sync::Arc;
use tokio::sync::Mutex;

use bambino::client::{
    BuzzerMode, CalibrationOption, DummyFactory, DummyTimer, DummyTls, FanTarget, PrintProgress,
    PrintSpeed, PrintStatus, PrinterClient,
};
use bambino::diagnostics::DecodedPrintError;
use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::identity::PrinterIdentity;
use bambino::models::PrinterModel;
use bambino::mqtt::{MqttClient, PrintJobConfig};

use common::io::{DummyTlsConnector, HostCapturingTlsConnector, MockDataStreamFactory};
use common::client::connect_test_client;
use common::mock_ftps;
use common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

// ============================================================================
// Core behavioral verification cases
// ============================================================================

#[tokio::test]
async fn test_homing_safety_interlocks() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Verify Bed-on-Z Safe Homing Command (Bare G28)
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "gcode_line");
        assert_eq!(json["print"]["param"], "G28\n");
    });

    // CoreXY Bed-on-Z initialization
    let mut client_x1c = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

    // Assert public serial and model getters expose the correct fields
    assert_eq!(client_x1c.serial(), "00M000000000000");
    assert_eq!(client_x1c.model(), PrinterModel::X1C);

    // Bed-on-Z Safety Guard Verification: home_z_only_danger must return ModelMismatch
    let err_res = client_x1c.home_axes(true).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Standard homing should succeed with bare G28
    client_x1c
        .home_axes(false)
        .await
        .expect("G28 homing failed");

    // Bed-Slinger initialization
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
        // Verify Bed-Slinger homing parameters write G28 Z to the stream
        let json = read_publish_payload(&mut server_stream_a1).await;
        assert_eq!(json["print"]["command"], "gcode_line");
        assert_eq!(json["print"]["param"], "G28 Z\n");
    });

    let mut client_a1 = connect_test_client(TokioIo(client_stream_a1), "039000000000000", PrinterModel::A1).await;

    // Bed-Slingers do not share upward bed collision hazards; G28 Z homing is permitted
    client_a1
        .home_axes(true)
        .await
        .expect("A1 Z-only homing failed");

    broker_task.await.expect("X1C broker task panicked");
    broker_task_a1.await.expect("A1 broker task panicked");
}

#[tokio::test]
async fn test_kinematic_and_extrusion_moves() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read Z move packet: must carry safety limits M211 S1 and reference coordinate wraps
        let json_z = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json_z["print"]["param"],
            "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode\n"
        );

        // Read X move packet: plain relative move G91 -> G0 -> G90
        let json_x = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_x["print"]["param"], "G91\nG0 X-15.50 F6000\nG90\n");

        // Read relative manual extrusion packet
        let json_e = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_e["print"]["param"], "M83\nG0 E10.00 F900\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .move_relative('z', 10.0, 3000)
        .await
        .expect("Z move failed");
    client
        .move_relative('x', -15.5, 6000)
        .await
        .expect("X move failed");
    client.extrude(10.0, 900).await.expect("Extrusion failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_move_relative_zero_distance_is_noop() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Only the non-zero X move below should reach the wire — the zero-distance Z and X
        // calls must short-circuit before publishing anything. If either zero-distance call
        // incorrectly published, this would be the first packet read instead, and the
        // assertion below would fail on mismatched params.
        let json_x = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_x["print"]["param"], "G91\nG0 X5.00 F1000\nG90\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // Zero-distance Z move: must be a no-op (Ok(0), no travel-limit error, no wire traffic) —
    // not the misleading "exceeds model travel limits" error `relative_z_move_gcode` would
    // otherwise collapse it into (it returns the same empty string for zero and out-of-range).
    let z_result = client
        .move_relative('z', 0.0, 3000)
        .await
        .expect("zero-distance Z move should succeed as a no-op");
    assert_eq!(z_result, 0, "no-op move should return sentinel packet id 0");

    // Zero-distance X move: same no-op contract, off the Z-only travel-limit code path.
    let x_zero_result = client
        .move_relative('x', 0.0, 1000)
        .await
        .expect("zero-distance X move should succeed as a no-op");
    assert_eq!(
        x_zero_result, 0,
        "no-op move should return sentinel packet id 0"
    );

    // Non-zero move on the same client still publishes normally.
    client
        .move_relative('x', 5.0, 1000)
        .await
        .expect("non-zero X move failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_move_relative_z_still_rejects_out_of_range_distance() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // P1S z_max is 256.0mm — a non-zero distance exceeding that must still surface the
    // travel-limit error, confirming the zero-distance short-circuit didn't swallow this case.
    let result = client.move_relative('z', 300.0, 3000).await;
    assert!(matches!(result, Err(Error::ModelMismatch(_))));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_move_relative_x_rejects_out_of_range_distance() {
    // X/Y moves previously had no distance cap at all, unlike Z. P1S x_max is
    // 256.0mm — a distance exceeding that must be rejected the same way Z's out-of-range
    // case already is.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.move_relative('x', 300.0, 3000).await;
    assert!(matches!(result, Err(Error::ModelMismatch(_))));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_thermal_guards_and_temperatures() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_bed = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_bed["print"]["param"], "M140 S60\n");

        let json_nozzle = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_nozzle["print"]["param"], "M104 T0 S220\n");

        // Active chamber temperature verification (X1E has active PTC heater)
        let json_chamber = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_chamber["print"]["param"], "M141 S45\n");
    });

    let mut client_x1e = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1E).await;

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
    let (client_stream_x1c, mut server_stream_x1c) = tokio::io::duplex(8192);
    let broker_task_x1c = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_x1c).await;
    });

    let mut client_x1c = connect_test_client(TokioIo(client_stream_x1c), "00M000000000000", PrinterModel::X1C).await;

    let err_res = client_x1c.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Open-frame model check (A1 — no sensor, no heater)
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
    });

    let mut client_a1 = connect_test_client(TokioIo(client_stream_a1), "039000000000000", PrinterModel::A1).await;

    let err_res = client_a1.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    broker_task.await.expect("X1E broker task panicked");
    broker_task_x1c.await.expect("X1C broker task panicked");
    broker_task_a1.await.expect("A1 broker task panicked");
}

// X1C's bed_temp_max ceiling is voltage-dependent (110°C @220V, 120°C @110V, per
// src/quirks/models/x1.rs's x1c_bed_temp_max), derived from cached home_flag telemetry — but
// every existing bed-clamping test above only exercises X1E, whose ceiling ignores the
// parameter entirely. A regression that swapped the two constants or flipped the None-case
// fallback direction would pass every existing test untouched.
#[tokio::test]
async fn test_x1c_bed_temp_ceiling_voltage_dependent() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", "00M000000000000");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // No home_flag observed yet — must clamp to the conservative 220V-region default.
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "M140 S110\n");

        // home_flag bit 3 set -> confirmed 220V region.
        send_publish_payload(
            &mut server_stream,
            &topic,
            2000,
            br#"{"print":{"home_flag":8}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "M140 S110\n");

        // home_flag bit 3 clear -> confirmed 110V region, higher ceiling.
        send_publish_payload(
            &mut server_stream,
            &topic,
            2001,
            br#"{"print":{"home_flag":0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "M140 S120\n");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed pre-telemetry");

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag=8");
    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed at confirmed 220V");

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag=0");
    client
        .set_bed_temperature(999)
        .await
        .expect("bed temp set should succeed at confirmed 110V");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_cooling_fans_and_peripheral_switches() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_cf = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_cf["print"]["param"], "M106 P1 S127\n"); // 50% PWM

        let json_aux = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_aux["print"]["param"], "M106 P2 S255\n"); // 100% PWM
    });

    let mut client_p1s = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client_p1s
        .set_fan_speed(FanTarget::PartCooling, 50)
        .await
        .expect("Part cooling fan set failed");
    client_p1s
        .set_fan_speed(FanTarget::AuxiliaryLeft, 100)
        .await
        .expect("Auxiliary left fan set failed");

    // Verify right auxiliary cooling fan is restricted on non-X2D models
    let err_res = client_p1s
        .set_fan_speed(FanTarget::AuxiliaryRight, 80)
        .await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    // Verify right auxiliary cooling fan is supported on X2D model (using Port 10)
    let (client_stream_x2, mut server_stream_x2) = tokio::io::duplex(8192);
    let broker_task_x2 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_x2).await;
        let json_aux_r = read_publish_payload(&mut server_stream_x2).await;
        assert_eq!(json_aux_r["print"]["param"], "M106 P10 S204\n"); // 80% PWM
    });

    let mut client_x2 = connect_test_client(TokioIo(client_stream_x2), "20P000000000000", PrinterModel::X2D).await;

    client_x2
        .set_fan_speed(FanTarget::AuxiliaryRight, 80)
        .await
        .expect("X2D auxiliary right fan set failed");

    broker_task.await.expect("P1S broker task panicked");
    broker_task_x2.await.expect("X2D broker task panicked");
}

#[tokio::test]
async fn test_set_fan_speed_clamps_above_100_percent() {
    // set_fan_speed's speed_percent > 100 clamp path was never exercised — every
    // existing call in this file used values <= 100. 150% must clamp to the same 255 PWM
    // value 100% produces, not overflow or wrap.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "M106 P1 S255\n"); // clamped to 100% PWM
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .set_fan_speed(FanTarget::PartCooling, 150)
        .await
        .expect("Part cooling fan set failed");

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_chamber_exhaust_fan_success_and_model_mismatch() {
    // FanTarget::ChamberExhaust was never exercised through set_fan_speed, unlike its
    // 3 sibling fan targets above (PartCooling, AuxiliaryLeft, AuxiliaryRight). Mirrors the
    // AuxiliaryRight success (X2D)/mismatch (P1S) pair in
    // test_cooling_fans_and_peripheral_switches, using H2D for the success case since chamber
    // exhaust is an H2-series/X2D feature.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["param"], "M106 P3 S204\n"); // 80% PWM
    });

    let mut client_h2d = connect_test_client(TokioIo(client_stream), "09P000000000000", PrinterModel::H2D).await;

    client_h2d
        .set_fan_speed(FanTarget::ChamberExhaust, 80)
        .await
        .expect("H2D chamber exhaust fan set failed");

    broker_task.await.expect("H2D broker task panicked");

    // Verify chamber exhaust fan is restricted on a model without one (P1S).
    let (client_stream_p1s, mut server_stream_p1s) = tokio::io::duplex(8192);
    let broker_task_p1s = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_p1s).await;
    });
    let mut client_p1s = connect_test_client(TokioIo(client_stream_p1s), "01P000000000000", PrinterModel::P1S).await;

    let err_res = client_p1s.set_fan_speed(FanTarget::ChamberExhaust, 80).await;
    assert!(matches!(err_res, Err(Error::ModelMismatch(_))));

    broker_task_p1s.await.expect("P1S broker task panicked");
}

#[tokio::test]
async fn test_queue_lifecycle_control_blocks() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_pause = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_pause["print"]["command"], "pause");

        let json_resume = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_resume["print"]["command"], "resume");

        let json_stop = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_stop["print"]["command"], "stop");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client.pause_print().await.expect("Pause failed");
    client.resume_print().await.expect("Resume failed");
    client.stop_print().await.expect("Stop failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_peripheral_signals_and_climate_controls() {
    // H2D supports airduct + buzzer
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_airduct = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_airduct["print"]["command"], "set_airduct");
        assert_eq!(json_airduct["print"]["modeId"], 0);

        let json_buzzer = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_buzzer["print"]["command"], "buzzer_ctrl");
        assert_eq!(json_buzzer["print"]["mode"], 2);
    });

    let mut client_h2d = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::H2D).await;

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
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);

    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;

        let json_sound = read_publish_payload(&mut server_stream_a1).await;
        assert_eq!(json_sound["print"]["command"], "print_option");
        assert_eq!(json_sound["print"]["sound_enable"], true);
    });

    let mut client_a1 = connect_test_client(TokioIo(client_stream_a1), "039000000000000", PrinterModel::A1).await;

    client_a1
        .set_prompt_sound(true)
        .await
        .expect("Prompt sound set failed");

    broker_task_a1.await.expect("A1 broker task panicked");

    // P1S supports none of these
    let (client_stream_p1s, mut server_stream_p1s) = tokio::io::duplex(8192);
    let broker_task_p1s = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_p1s).await;
    });

    let mut client_p1s = connect_test_client(TokioIo(client_stream_p1s), "01P000000000000", PrinterModel::P1S).await;

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

// ============================================================================
// Negative / Failure Path Tests
// ============================================================================

#[tokio::test]
async fn test_set_nozzle_temperature_validates_nozzle_id() {
    // Single-nozzle model: nozzle_id 1 must be rejected.
    let (client_stream_p1s, mut server_stream_p1s) = tokio::io::duplex(8192);
    let broker_task_p1s = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_p1s).await;
    });
    let mut client_p1s = connect_test_client(TokioIo(client_stream_p1s), "01P000000000000", PrinterModel::P1S).await;
    assert!(matches!(
        client_p1s.set_nozzle_temperature(1, 220).await,
        Err(Error::ModelMismatch(_))
    ));
    broker_task_p1s.await.expect("P1S broker task panicked");

    // IDEX model: nozzle_id 1 (secondary carriage) must be accepted.
    let (client_stream_h2d, mut server_stream_h2d) = tokio::io::duplex(8192);
    let broker_task_h2d = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_h2d).await;
        let json = read_publish_payload(&mut server_stream_h2d).await;
        assert_eq!(json["print"]["param"], "M104 T1 S220\n");
    });
    let mut client_h2d = connect_test_client(TokioIo(client_stream_h2d), "01P000000000000", PrinterModel::H2D).await;
    client_h2d
        .set_nozzle_temperature(1, 220)
        .await
        .expect("IDEX secondary nozzle should be accepted");
    broker_task_h2d.await.expect("H2D broker task panicked");
}

#[tokio::test]
async fn test_in_flight_saturation() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1_048_576);

    let _broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read and discard all incoming PUBLISH packets without sending PUBACKs
        while common::mock_mqtt::read_packet(&mut server_stream)
            .await
            .is_ok()
        {}
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // Fill the in-flight queue to capacity (200 commands)
    for i in 0..200 {
        client
            .send_gcode("G28")
            .await
            .unwrap_or_else(|e| panic!("Command {} should succeed but got: {:?}", i, e));
    }

    // The 201st command must be rejected due to in-flight saturation
    let err = client.send_gcode("G28").await;
    assert!(
        matches!(
            err,
            Err(Error::Network(bambino::io::SocketError::TimedOut))
        ),
        "Expected Network(TimedOut) on command 201 (MqttClient::publish_command's \
         documented in-flight-saturation response), got {:?}",
        err
    );
}

#[tokio::test]
async fn test_connection_drop_during_operation() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // Drop the server stream immediately after handshake
        drop(server_stream);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    broker_task.await.expect("Broker task panicked");

    // After the server stream is dropped, publish attempts should fail with a network error
    let result = client.send_gcode("G28").await;
    assert!(
        result.is_err(),
        "Expected network error after connection drop"
    );
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected Error::Network, got {:?}",
        result
    );
}

// Print Job, Speed, Calibration, Error Clear & LED Tests

#[tokio::test]
async fn test_start_print_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "project_file");
        assert_eq!(json["print"]["file"], "job.3mf");
        assert_eq!(json["print"]["param"], "Metadata/plate_1.gcode");
        assert_eq!(json["print"]["subtask_name"], "Test Print");
        assert_eq!(json["print"]["bed_type"], "textured");
        assert_eq!(json["print"]["url"], "ftp://job.3mf");
        assert_eq!(json["print"]["use_ams"], false);
        assert_eq!(json["print"]["ams_mapping"], "");
        assert_eq!(json["print"]["bed_leveling"], true);
        assert_eq!(json["print"]["vibration_cali"], true);
        assert_eq!(json["print"]["timelapse"], true);
        assert_eq!(json["print"]["layer_inspect"], true);
        // P1S: single nozzle → nozzle_offset_cali defaults to 0
        assert_eq!(json["print"]["nozzle_offset_cali"], 0);
        // PrintJobConfig::new() defaults run_flow_calibration to true (README-documented
        // default), which from_config() serializes as extrude_cali_flag: 1.
        assert_eq!(json["print"]["extrude_cali_flag"], 1);
        // flow_cali/profile_id/project_id/task_id, previously missing entirely.
        // subtask_id/project_id/task_id all share one value (see ProjectFilePayload's
        // project_id doc comment for why).
        assert_eq!(json["print"]["flow_cali"], true);
        assert_eq!(json["print"]["profile_id"], "0");
        let subtask_id = json["print"]["subtask_id"].clone();
        assert_eq!(json["print"]["project_id"], subtask_id);
        assert_eq!(json["print"]["task_id"], subtask_id);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let config = PrintJobConfig::new(
        "job.3mf",
        "Metadata/plate_1.gcode",
        "Test Print",
        1718626458000,
        "textured",
    );
    client
        .start_print(&config)
        .await
        .expect("start_print failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_print_idex_nozzle_offset_default() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        // X2D: IDEX → nozzle_offset_cali defaults to 1
        assert_eq!(json["print"]["nozzle_offset_cali"], 1);
        assert_eq!(json["print"]["ams_mapping"], serde_json::json!([0, -1]));
        assert_eq!(json["print"]["use_ams"], true);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "20P000000000000", PrinterModel::X2D).await;

    let config = PrintJobConfig::new(
        "job.3mf",
        "Metadata/plate_1.gcode",
        "IDEX Print",
        12345,
        "textured",
    )
    .with_ams(vec![0, -1]);
    client
        .start_print(&config)
        .await
        .expect("start_print failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_set_print_speed_all_levels() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        for (expected_param, label) in [
            ("1", "Silent"),
            ("2", "Standard"),
            ("3", "Sport"),
            ("4", "Ludicrous"),
        ] {
            let json = read_publish_payload(&mut server_stream).await;
            assert_eq!(
                json["print"]["command"], "print_speed",
                "Failed on {}",
                label
            );
            assert_eq!(
                json["print"]["param"], expected_param,
                "Failed on {}",
                label
            );
        }
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    for level in [
        PrintSpeed::Silent,
        PrintSpeed::Standard,
        PrintSpeed::Sport,
        PrintSpeed::Ludicrous,
    ] {
        client
            .set_print_speed(level)
            .await
            .expect("set_print_speed failed");
    }

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_skip_objects_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "skip_objects");
        assert_eq!(json["print"]["obj_list"], serde_json::json!([0, 3, 7]));
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .skip_objects(vec![0, 3, 7])
        .await
        .expect("skip_objects failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_calibration_combined_flags() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "calibration");
        // BED_LEVELING (2) | VIBRATION_COMPENSATION (4) = 6
        assert_eq!(json["print"]["option"], 6);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .start_calibration(
            CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION,
        )
        .await
        .expect("start_calibration failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_clear_print_error_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "clean_print_error");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .clear_print_error()
        .await
        .expect("clear_print_error failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_set_led_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["system"]["command"], "ledctrl");
        assert_eq!(json["system"]["led_node"], "chamber_light");
        assert_eq!(json["system"]["led_mode"], "on");

        let json_off = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_off["system"]["led_mode"], "off");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .set_led("chamber_light", true)
        .await
        .expect("set_led on failed");
    client
        .set_led("chamber_light", false)
        .await
        .expect("set_led off failed");

    broker_task.await.expect("Broker task panicked");
}

// AMS Control Tests

#[tokio::test]
async fn test_change_filament_load_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_change_filament");
        assert_eq!(json["print"]["ams_id"], 0);
        assert_eq!(json["print"]["slot_id"], 1);
        assert_eq!(json["print"]["target"], 1);
        assert_eq!(json["print"]["curr_temp"], -1);
        assert_eq!(json["print"]["tar_temp"], -1);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(0, 1, -1, -1)
        .await
        .expect("change_filament failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_change_filament_derives_target_for_nonzero_ams_unit() {
    // For any standard AMS unit other than 0, target is the flat global tray ID
    // (ams_id*4 + slot_id), not slot_id — ams_id 0 previously masked this since the two
    // values coincide there.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["ams_id"], 1);
        assert_eq!(json["print"]["slot_id"], 2);
        assert_eq!(json["print"]["target"], 6); // 1*4 + 2, not 2
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(1, 2, -1, -1)
        .await
        .expect("change_filament failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_change_filament_derives_target_for_external_spool() {
    // An external-spool load's target is the ams_id itself (255), not slot_id
    // (254) — the reference doc's worked examples for this case were previously wrong too.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["ams_id"], 255);
        assert_eq!(json["print"]["slot_id"], 254);
        assert_eq!(json["print"]["target"], 255);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(255, 254, -1, -1)
        .await
        .expect("change_filament failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_change_filament_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.change_filament(99, 1, -1, -1).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_drying_lifecycle_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_start = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_start["print"]["command"], "ams_filament_drying");
        assert_eq!(json_start["print"]["ams_id"], 128);
        assert_eq!(json_start["print"]["mode"], 1);
        assert_eq!(json_start["print"]["temp"], 55);
        assert_eq!(json_start["print"]["duration"], 8);
        assert_eq!(json_start["print"]["filament"], "PA-CF");

        let json_stop = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_stop["print"]["command"], "ams_filament_drying");
        assert_eq!(json_stop["print"]["mode"], 0);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1C).await;

    client
        .start_drying(128, 55, 8, 0, true, 20, false, "PA-CF")
        .await
        .expect("start_drying failed");
    client.stop_drying(128).await.expect("stop_drying failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_drying_clamps_temperature_to_ams_unit_ceiling() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // AMS-HT (ams_id 128) is rated to 85°C — a requested 200°C must clamp to 85, not
        // the AMS 2 Pro / standard-AMS ceiling of 65°C.
        let json_ht = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_ht["print"]["temp"], 85);

        // A standard AMS unit (ams_id 0) is rated to 65°C — a requested 200°C must clamp
        // to 65.
        let json_standard = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_standard["print"]["temp"], 65);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1C).await;

    client
        .start_drying(128, 200, 8, 0, true, 20, false, "PA-CF")
        .await
        .expect("start_drying (AMS-HT) failed");
    client
        .start_drying(0, 200, 8, 0, true, 20, false, "PLA")
        .await
        .expect("start_drying (standard AMS) failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_drying_rejected_on_p1_screen_only_firmware() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // P1S firmware acks ams_filament_drying with `result: success` and then silently
    // discards it — no heater/fan activation, dry_status stays 0 — confirmed against real
    // hardware. start_drying() must reject before dispatch rather than send a command the
    // printer will accept-then-drop.
    let err = client
        .start_drying(0, 55, 8, 0, true, 20, false, "PA-CF")
        .await
        .expect_err("start_drying must reject on P1 (screen-only AMS drying)");
    assert!(matches!(err, Error::ModelMismatch(_)));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_scan_rfid_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_get_rfid");
        assert_eq!(json["print"]["ams_id"], 0);
        assert_eq!(json["print"]["slot_id"], 2);
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client.scan_rfid(0, 2).await.expect("scan_rfid failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_scan_rfid_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.scan_rfid(255, 2).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_select_k_profile_wire_payload() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "extrusion_cali_sel");
        assert_eq!(json["print"]["ams_id"], 0);
        assert_eq!(json["print"]["tray_id"], 1);
        assert_eq!(json["print"]["cali_idx"], 4);
        assert_eq!(json["print"]["filament_id"], "GFA01");
        assert_eq!(json["print"]["nozzle_diameter"], "0.4");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .select_k_profile(0, 1, 4, "GFA01", "0.4")
        .await
        .expect("select_k_profile failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_select_k_profile_rejects_invalid_combo() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.select_k_profile(200, 200, 4, "GFA01", "0.4").await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

const K_PROFILE_RESPONSE: &str = r#"{"print":{"command":"extrusion_cali_get","sequence_id":"10002","nozzle_diameter":"0.4","filaments":[{"cali_idx":4,"filament_id":"GFA01","nozzle_diameter":"0.4","nozzle_id":"HS00-0.4","extruder_id":0,"name":"Test PLA","k_value":"0.022000","setting_id":"PF12345678901234567"}]}}"#;

// Second `get_k_profiles()` call on an already-primed client: sequence ID advances
// past the first call's prime (10001) and real query (10002).
const K_PROFILE_RESPONSE_SECOND_CALL: &str = r#"{"print":{"command":"extrusion_cali_get","sequence_id":"10003","nozzle_diameter":"0.4","filaments":[{"cali_idx":4,"filament_id":"GFA01","nozzle_diameter":"0.4","nozzle_id":"HS00-0.4","extruder_id":0,"name":"Test PLA","k_value":"0.022000","setting_id":"PF12345678901234567"}]}}"#;

// Manual-prime-skip call: only the real query is sent, so it lands on the first
// sequence ID issued (10001), not the second (10002) that auto-priming would consume.
const K_PROFILE_RESPONSE_NO_PRIME: &str = r#"{"print":{"command":"extrusion_cali_get","sequence_id":"10001","nozzle_diameter":"0.4","filaments":[{"cali_idx":4,"filament_id":"GFA01","nozzle_diameter":"0.4","nozzle_id":"HS00-0.4","extruder_id":0,"name":"Test PLA","k_value":"0.022000","setting_id":"PF12345678901234567"}]}}"#;

// Correct command but a sequence ID that belongs to nobody — simulates a stray
// response from another MQTT client (Orca/Studio/a second instance of us) querying
// the same printer concurrently. `poll_until` must not consume it. Filament content
// deliberately differs from the real response so a test that wrongly accepts this
// decoy fails on content, not just on a missed assertion.
const K_PROFILE_RESPONSE_DECOY_SEQ: &str = r#"{"print":{"command":"extrusion_cali_get","sequence_id":"99999","nozzle_diameter":"0.4","filaments":[{"cali_idx":9,"filament_id":"DECOY01","nozzle_diameter":"0.4","nozzle_id":"HS00-0.4","extruder_id":0,"name":"Decoy PLA","k_value":"0.099000","setting_id":"PF99999999999999999"}]}}"#;

const SERIAL: &str = "01P000000000000";

#[tokio::test]
async fn test_get_k_profiles_auto_priming() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // First call: auto-prime sends two extrusion_cali_get commands
        let json_prime = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_prime["print"]["command"], "extrusion_cali_get");

        let json_real = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_real["print"]["command"], "extrusion_cali_get");

        // Send response and wait for client's PUBACK
        send_publish_payload(
            &mut server_stream,
            &topic,
            1000,
            K_PROFILE_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;

        // Second call: already primed, only one command
        let json_second = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_second["print"]["command"], "extrusion_cali_get");

        // Send response for second call
        send_publish_payload(
            &mut server_stream,
            &topic,
            1001,
            K_PROFILE_RESPONSE_SECOND_CALL.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    // First call triggers auto-prime (2 publishes)
    let resp = client
        .get_k_profiles()
        .await
        .expect("get_k_profiles failed");
    assert_eq!(resp.print.filaments.len(), 1);
    assert_eq!(resp.print.filaments[0].filament_id, "GFA01");

    // Second call skips prime (1 publish)
    let resp2 = client
        .get_k_profiles()
        .await
        .expect("get_k_profiles second call failed");
    assert_eq!(resp2.print.filaments.len(), 1);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_get_k_profiles_manual_prime_skip() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // With manual priming, only one command should be sent
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "extrusion_cali_get");

        // Send response and wait for client's PUBACK
        send_publish_payload(
            &mut server_stream,
            &topic,
            1000,
            K_PROFILE_RESPONSE_NO_PRIME.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client.set_k_profile_primed(true);

    let resp = client
        .get_k_profiles()
        .await
        .expect("get_k_profiles failed");
    assert_eq!(resp.print.command, "extrusion_cali_get");

    broker_task.await.expect("Broker task panicked");
}

// Phase 9: sequence ID correlation hygiene

#[tokio::test]
async fn test_get_k_profiles_ignores_mismatched_sequence_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Manual priming, only one command should be sent
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "extrusion_cali_get");

        // A decoy response with the right command but a sequence ID that doesn't
        // belong to us (e.g. a second MQTT client querying the same printer).
        send_publish_payload(
            &mut server_stream,
            &topic,
            1000,
            K_PROFILE_RESPONSE_DECOY_SEQ.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;

        // Then the real response, correctly sequenced.
        send_publish_payload(
            &mut server_stream,
            &topic,
            1001,
            K_PROFILE_RESPONSE_NO_PRIME.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client.set_k_profile_primed(true);

    let resp = client
        .get_k_profiles()
        .await
        .expect("get_k_profiles should skip the decoy and find the real response");

    // Must be the real response's filament, not the decoy's.
    assert_eq!(resp.print.filaments[0].filament_id, "GFA01");
    assert_ne!(resp.print.filaments[0].filament_id, "DECOY01");

    broker_task.await.expect("Broker task panicked");
}

// This only exercises a single command from a freshly-constructed client (sequence
// ID 10001), so it can't seed sequence_counter near TASK_ID_MAX to actually trigger wraparound
// — that field is pub(crate), invisible to this external integration test. It still verifies a
// real invariant (every wire sequence_id fits in i32), just not wraparound itself; the
// wraparound math is covered directly by
// mqtt::commands::tests::test_clamp_task_id_wraps_near_max, colocated with clamp_task_id().
#[tokio::test]
async fn test_sequence_id_fits_in_i32() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        let seq: u64 = json["print"]["sequence_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(seq <= i32::MAX as u64, "Sequence ID must fit in i32");
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("Broker task panicked");
}

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

#[tokio::test]
async fn test_print_status_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            4300,
            br#"{"print":{"gcode_state":"RUNNING"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            4301,
            br#"{"print":{"gcode_state":"BOGUS_STATE"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    // No telemetry observed yet — cache must read as unknown-state, not a stale guess.
    assert_eq!(client.print_status(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse gcode_state report");
    assert_eq!(client.print_status(), Some(PrintStatus::Running));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second report");
    assert_eq!(client.print_status(), Some(PrintStatus::Unknown));

    broker_task.await.expect("Broker task panicked");
}

// Phase 14: is_door_open / active_fault telemetry accessors

#[tokio::test]
async fn test_door_open_none_on_sensorless_model() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bit 23 set — would read as "open" on a sensor-equipped model.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5000,
            br#"{"print":{"home_flag":8388608}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // P1S has no door sensor.
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.is_door_open(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse home_flag report");

    // Sensorless model must stay None regardless of the observed register.
    assert_eq!(client.is_door_open(), None);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_door_open_cache_from_telemetry_on_sensor_equipped_model() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", "00M000000000000");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bit 23 set: door open.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5100,
            br#"{"print":{"home_flag":8388608}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Bit 23 clear: door closed.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5101,
            br#"{"print":{"home_flag":0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // X1C has a door sensor, read from home_flag bit 23.
    let mut client = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

    // No telemetry observed yet — cache must read as unknown, not "closed".
    assert_eq!(client.is_door_open(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first home_flag report");
    assert_eq!(client.is_door_open(), Some(true));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second home_flag report");
    assert_eq!(client.is_door_open(), Some(false));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_door_open_cache_survives_message_omitting_home_flag() {
    // last_door_open used to be overwritten unconditionally on every telemetry
    // message, ignoring the same absent-field staleness contract every other cache field
    // respects. A print-carrying message that omits home_flag (X1C's door-sensor field)
    // must leave a previously-observed "door open" cached, not reset it to Some(false).
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", "00M000000000000");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bit 23 set: door open.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5110,
            br#"{"print":{"home_flag":8388608}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // A print-carrying message with no home_flag at all (e.g. an incremental update
        // only touching an unrelated field) must not reset the cached door state.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5111,
            br#"{"print":{"mc_percent":42}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first home_flag report");
    assert_eq!(client.is_door_open(), Some(true));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second, home_flag-omitting report");
    assert_eq!(
        client.is_door_open(),
        Some(true),
        "a message omitting home_flag must not reset the cached door-open state"
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_active_fault_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // print_error = 83902476 decimal -> 0x0500400C, a genuine fault.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5200,
            br#"{"print":{"print_error":83902476}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Register reads back to 0 — no fault.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5201,
            br#"{"print":{"print_error":0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    // No telemetry observed yet.
    assert_eq!(client.active_fault(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first print_error report");
    assert_eq!(
        client.active_fault(),
        Some(DecodedPrintError {
            short_code: "0500_400C".to_string(),
            module_id: 0x05,
            is_genuine_fault: true,
        })
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second print_error report");
    // 0 collapses to None — same as "never observed" from the caller's perspective.
    assert_eq!(client.active_fault(), None);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_print_progress_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5300,
            br#"{"print":{"mc_percent":42,"mc_remaining_time":30,"layer_num":5,"total_layer_num":100}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Only `mc_percent` present this time — the other three fields must stay cached.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5301,
            br#"{"print":{"mc_percent":50}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.print_progress(), PrintProgress::default());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first progress report");
    assert_eq!(
        client.print_progress(),
        PrintProgress {
            percent: Some(42),
            remaining_secs: Some(30),
            layer_num: Some(5),
            total_layers: Some(100),
        }
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second progress report");
    assert_eq!(
        client.print_progress(),
        PrintProgress {
            percent: Some(50),
            remaining_secs: Some(30),
            layer_num: Some(5),
            total_layers: Some(100),
        }
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_bed_temperatures_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5400,
            br#"{"print":{"bed_temper":60.0,"bed_target_temper":65.0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Only `bed_temper` present this time — target must stay cached at 65.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5401,
            br#"{"print":{"bed_temper":61.0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.bed_temperatures(), (0, 0));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first bed temperature report");
    assert_eq!(client.bed_temperatures(), (60, 65));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second bed temperature report");
    assert_eq!(client.bed_temperatures(), (61, 65));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_ams_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5500,
            br#"{"print":{"ams":{"ams_exist_bits":"1","tray_exist_bits":"3"}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // A report with no `ams` key at all must leave the cache untouched.
        send_publish_payload(&mut server_stream, &topic, 5501, br#"{"print":{}}"#).await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert!(client.ams().is_none());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first AMS report");
    assert_eq!(
        client.ams().and_then(|ams| ams.ams_exist_bits.as_deref()),
        Some("1")
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second (ams-less) report");
    assert_eq!(
        client.ams().and_then(|ams| ams.ams_exist_bits.as_deref()),
        Some("1")
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_vt_tray_and_vir_slot_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5600,
            br#"{"print":{"vt_tray":{"id":"254"},"vir_slot":[{"id":"0"},{"id":"1"}]}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert!(client.vt_tray().is_none());
    assert!(client.vir_slot().is_none());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse vt_tray/vir_slot report");
    assert_eq!(client.vt_tray().and_then(|t| t.id.as_deref()), Some("254"));
    assert_eq!(
        client
            .vir_slot()
            .map(|slots| slots.iter().map(|s| s.id.as_deref()).collect::<Vec<_>>()),
        Some(vec![Some("0"), Some("1")])
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_nozzle_temperatures_cache_single_nozzle_model() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5700,
            br#"{"print":{"nozzle_temper":200.0,"nozzle_target_temper":210.0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.nozzle_temperatures(), vec![(0, 0, 0)]);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse nozzle temperature report");
    assert_eq!(client.nozzle_temperatures(), vec![(0, 200, 210)]);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_printing_tray_global_id_prefers_snow_field() {
    // printing_tray_global_id() decodes device.extruder.info[active].snow directly,
    // no ams_extruder_map needed.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Extruder 0 (right/main): state selects active_extruder_index()=1 (left), snow
        // routes it to ams_id=2, slot_id=1 (raw = (2<<8)|1 = 513).
        send_publish_payload(
            &mut server_stream,
            &topic,
            5700,
            br#"{"device":{"extruder":{"info":[
                {"id":0,"snow":65535},
                {"id":1,"snow":513}
            ],"state":18}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.printing_tray_global_id(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse extruder report");
    // ams_id=2, slot_id=1 -> global tray id = 2*4 + 1 = 9
    assert_eq!(client.printing_tray_global_id(), Some(9));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_nozzle_temperatures_cache_idex_flat_field_routing_quirk() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // IDEX hardware present (`device.nozzle.info` has 2 entries) but no live
        // `device.extruder.info` temps yet — the flat-field routing quirk applies:
        // nozzle_temper (100) is nozzle 1 (left) actual, nozzle_target_temper (220) is
        // nozzle 0 (right) target.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5701,
            br#"{"print":{"device":{"nozzle":{"info":[{"id":0},{"id":1}]}},"nozzle_temper":100.0,"nozzle_target_temper":220.0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::H2D).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse IDEX nozzle report");
    assert_eq!(client.nozzle_temperatures(), vec![(0, 0, 220), (1, 100, 0)]);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_chamber_temperature_cache() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Composite-packed: (60 << 16) | 50 = actual 50, target 60.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5702,
            br#"{"print":{"chamber_temper":3932210.0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // P1S has no chamber heater/sensor — always None regardless of telemetry.
    let mut sensorless_client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    assert_eq!(sensorless_client.chamber_temperature(), None);
    sensorless_client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse chamber temperature report");
    assert_eq!(sensorless_client.chamber_temperature(), None);

    let (client_stream2, mut server_stream2) = tokio::io::duplex(8192);
    let topic2 = format!("device/{SERIAL}/report");
    let broker_task2 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream2).await;
        send_publish_payload(
            &mut server_stream2,
            &topic2,
            5703,
            br#"{"print":{"chamber_temper":3932210.0}}"#,
        )
        .await;
        read_puback(&mut server_stream2).await;
    });
    let mut heated_client = connect_test_client(TokioIo(client_stream2), SERIAL, PrinterModel::H2D).await;

    assert_eq!(heated_client.chamber_temperature(), Some((0, 0)));
    heated_client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse chamber temperature report");
    assert_eq!(heated_client.chamber_temperature(), Some((50, 60)));

    broker_task.await.expect("Broker task panicked");
    broker_task2.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_hms_cache_and_active_alerts() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // One genuine fault (attr 0x05000100 / code 0x0001400C) and one cancellation
        // echo (attr 0x05000100 / code 0x0001400E) that must be filtered out.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5704,
            br#"{"print":{"hms":[{"attr":83886336,"code":81932},{"attr":83886336,"code":81934}]}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert!(client.hms().is_none());
    assert!(client.active_hms_alerts().is_empty());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse HMS report");
    assert_eq!(client.hms().map(|h| h.len()), Some(2));

    let active = client.active_hms_alerts();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].short_code, "0500_400C");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_sanitized_ams_clears_stale_fields_without_mutating_raw_cache() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // A tray in state 9 (empty) that still carries stale material fields from a
        // previously loaded spool — the exact case `ams()`'s doc comment says stays raw
        // and `sanitized_ams()` scrubs.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5705,
            br#"{"print":{"ams":{"ams":[{"id":"0","temp":"25.0","humidity":"3","tray":[{"id":"0","state":9,"tray_type":"PLA","tray_color":"FF0000FF","remain":42}]}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert!(client.ams().is_none());
    assert!(client.sanitized_ams().is_none());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse AMS report");

    let raw_tray = &client.ams().unwrap().ams[0].tray.as_ref().unwrap()[0];
    assert_eq!(
        raw_tray.tray_type.as_deref(),
        Some("PLA"),
        "ams() must stay raw — stale material fields are never proactively scrubbed"
    );
    assert_eq!(raw_tray.remain, Some(42));

    let sanitized = client.sanitized_ams().unwrap();
    let sanitized_tray = &sanitized.ams[0].tray.as_ref().unwrap()[0];
    assert_eq!(
        sanitized_tray.tray_type, None,
        "sanitized_ams() must clear stale material fields for an empty-state tray"
    );
    assert_eq!(sanitized_tray.remain, Some(-1));

    // Confirm sanitized_ams() didn't mutate the cache it read from.
    let raw_tray_again = &client.ams().unwrap().ams[0].tray.as_ref().unwrap()[0];
    assert_eq!(raw_tray_again.tray_type.as_deref(), Some("PLA"));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_fan_speed_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5705,
            br#"{"print":{"cooling_fan_speed":"15","big_fan1_speed":"8","big_fan2_speed":"0","heatbreak_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":75}]}}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // H2D uses step-encoded (not percentage) fan telemetry for the primary four fans,
    // unlike X2D (which reports percentages directly — see `X2Quirks::reports_auxiliary_fan_percentage`).
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::H2D).await;

    assert_eq!(client.part_cooling_fan_speed(), None);
    assert_eq!(client.auxiliary_right_fan_speed(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(client.part_cooling_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_left_fan_speed(), Some(53));
    assert_eq!(client.chamber_exhaust_fan_speed(), Some(0));
    assert_eq!(client.heatbreak_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_right_fan_speed(), Some(75));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_print_speed_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5706,
            br#"{"print":{"spd_lvl":3,"spd_mag":124}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.print_speed(), None);
    assert_eq!(client.print_speed_magnitude(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse print speed report");
    assert_eq!(client.print_speed(), Some(PrintSpeed::Sport));
    assert_eq!(client.print_speed_magnitude(), Some(124));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_wifi_signal_cache_from_telemetry() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5707,
            br#"{"print":{"wifi_signal":"-52dBm"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5708,
            br#"{"print":{"wifi_signal":"-90dBm"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    assert_eq!(client.wifi_signal(), None);
    assert!(!client.is_ethernet_active_via_wifi_signal());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first wifi_signal report");
    assert_eq!(client.wifi_signal(), Some("-52dBm"));
    assert!(!client.is_ethernet_active_via_wifi_signal());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second wifi_signal report");
    assert_eq!(client.wifi_signal(), Some("-90dBm"));
    assert!(client.is_ethernet_active_via_wifi_signal());

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_ensure_mqtt_reseed_skipped_without_real_clock() {
    // The wall-clock sequence-counter reseed in ensure_mqtt() is meant to stop two
    // independent sessions connecting to the same printer from both starting at the same
    // fixed counter. Under DummyTimer (the documented, first-class default when
    // .with_timer() isn't chained), now_millis() always returns 0, so reseeding
    // unconditionally would collide every default-configured client onto the same seed —
    // exactly the bug this guard prevents. Verify the first command after a lazy
    // ensure_mqtt() connect still carries the untouched default sequence ID (10001).
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory {
        active_stream: data_container,
    };

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json["print"]["sequence_id"], "10001",
            "DummyTimer has no real clock — reseed must be skipped, not collapse every \
             default-configured client onto the same wall-clock seed"
        );
    });

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    );

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("mock broker task panicked");
}

#[tokio::test]
async fn test_disconnect_and_attach_mqtt_recovers_dead_session() {
    // Before disconnect_mqtt()/attach_mqtt() existed, a dead MQTT session (a
    // tick_zombie_check()-detected zombie, a transport error) had no supported recovery
    // path — ensure_mqtt()'s is_some() short-circuit kept handing back the same broken
    // stream forever, unlike disconnect_camera()/attach_camera() and
    // disconnect_storage()/attach_storage(). Verify disconnect clears the slot and attach
    // reinstalls a fresh connected client that telemetry keeps working through.
    let (client_stream_a, mut server_stream_a) = tokio::io::duplex(8192);
    let broker_task_a = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a).await;
    });
    let mut client = connect_test_client(TokioIo(client_stream_a), SERIAL, PrinterModel::P1S).await;
    assert!(client.is_mqtt_connected());
    broker_task_a.await.expect("First broker task panicked");

    client
        .disconnect_mqtt()
        .await
        .expect("disconnect_mqtt should succeed");
    assert!(
        !client.is_mqtt_connected(),
        "disconnect_mqtt must clear self.mqtt"
    );

    let (client_stream_b, mut server_stream_b) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");
    let broker_task_b = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_b).await;
        send_publish_payload(
            &mut server_stream_b,
            &topic,
            9001,
            br#"{"print":{"wifi_signal":"-60dBm"}}"#,
        )
        .await;
        read_puback(&mut server_stream_b).await;
    });
    let mqtt_client_b = MqttClient::connect(
        TokioIo(client_stream_b),
        &PrinterIdentity { ip: String::new(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
        .await
        .expect("second MQTT connect handshake failed");
    client.attach_mqtt(mqtt_client_b);
    assert!(
        client.is_mqtt_connected(),
        "attach_mqtt must reinstall a session"
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should work over the reattached session");
    assert_eq!(client.wifi_signal(), Some("-60dBm"));

    broker_task_b.await.expect("Second broker task panicked");
}

#[tokio::test]
async fn test_ensure_ftps_retries_after_failed_dial() {
    // ensure_ftps() used to .take() ftps_config before attempting the dial, so a
    // failed attempt (including a connect_timeout_secs timeout on a slow LAN) permanently
    // discarded it — every later call would then report the misleading "FTPS not
    // configured" error instead of retrying. MockDataStreamFactory's dial() fails with
    // ConnectionRefused whenever its stream container is empty, so two consecutive calls
    // over the same never-populated container must both fail the *same* dial-level way,
    // never degrading into "not configured" (which would mean the config got dropped after
    // the first attempt).
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_ftps(DummyTlsConnector, factory, DummyTimer);

    for attempt in 1..=2 {
        let result = client.storage().await;
        assert!(
            matches!(result, Err(Error::Network(_))),
            "attempt {attempt}: expected the dial failure to surface as Network, not \
             degrade into \"FTPS not configured\" from a config consumed on a prior failed \
             attempt, got {:?}",
            result.map(|_| ())
        );
    }
}

#[tokio::test]
async fn test_disconnect_storage_clears_ftps_for_clean_reconnect() {
    // `disconnect_storage()` (review/client.md Phase 5) must leave `self.ftps` as `None`
    // afterward, so a later `storage()` call falls through to `ensure_ftps()`'s existing
    // "FTPS not configured" error instead of ever handing back the now-poisoned client that
    // `BambuFtpsClient::disconnect()` leaves behind (review/ftps.md Phase 2/7).
    let (client_control, server_control) = tokio::io::duplex(8192);

    // `ensure_ftps()` fetches its raw control stream via the factory, so the mock data
    // stream is preloaded with the client side of the duplex pair up front.
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_control))));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        data_container.clone(),
    ));

    let mut client = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_ftps(DummyTlsConnector, factory, DummyTimer);

    client
        .storage()
        .await
        .expect("first storage() call should connect via the mock FTPS handshake");
    assert!(client.is_ftps_connected());

    client
        .disconnect_storage()
        .await
        .expect("disconnect_storage should succeed");
    assert!(
        !client.is_ftps_connected(),
        "disconnect_storage must clear self.ftps"
    );

    // ftps_config was already consumed by the first storage() call, so this must surface
    // the clear "not configured" error, not a stale/poisoned reconnect.
    let result = client.storage().await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation (\"FTPS not configured\") after disconnect_storage, got {:?}",
        result.map(|_| ())
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_camera_trio_unconfigured_error() {
    // No test exercised the camera trio's "not configured" branch — the same case
    // FTPS's disconnect_storage/re-storage() test above covers for the FTPS trio
    // (see "FTPS not configured" a few tests up). A PrinterClient that never called
    // .with_camera()/.attach_camera() must fail read_camera_frame()/camera() with a clear
    // ProtocolViolation, not a panic or a misleading dial-level error.
    let mut client = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    );
    assert!(!client.is_camera_connected());

    let mut frame_buf = Vec::new();
    let result = client.read_camera_frame(&mut frame_buf).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation (\"Camera not configured\") on an unconfigured client, got {:?}",
        result.map(|_| ())
    );
    assert!(!client.is_camera_connected());
}

#[tokio::test]
async fn test_ensure_mqtt_bounds_post_dial_handshake_by_connect_timeout() {
    // review/client.md Phase 1: `ensure_mqtt()`'s race must cover the full
    // dial+TLS+`MqttClient::connect()` handshake, not just dial+TLS. Simulate a peer
    // that completes TCP/TLS but never sends CONNACK — the duplex's server side is left
    // idle forever, so any read from the client side blocks indefinitely unless the
    // handshake itself is inside the timeout race.
    let (client_stream, _server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory {
        active_stream: data_container,
    };

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(1);

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect_mqtt())
        .await
        .expect("connect_mqtt() must return within the 5s test safety margin, not hang forever");

    assert!(
        matches!(result, Err(Error::Network(_))),
        "expected a bounded Network(TimedOut) once connect_timeout_secs elapses \
         mid-CONNACK-handshake, got {:?}",
        result.map(|_| ())
    );
}

#[tokio::test]
async fn test_with_connect_timeout_zero_disables_timeout() {
    // connect_timeout_secs == 0 used to race against timer.sleep(Duration::from_secs(0)),
    // which resolves near-instantly and wins the race against the dial+TLS+handshake future on
    // nearly every attempt — making `0` mean "always fail immediately" instead of "disabled,"
    // unlike the sibling `command_timeout_secs` field's documented "0 disables" convention.
    // With a real (non-stalled) peer completing the handshake, connect_mqtt() must now succeed.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory {
        active_stream: data_container,
    };

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(0);

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect_mqtt())
        .await
        .expect("connect_mqtt() must return within the 5s test safety margin, not hang forever");

    assert!(
        result.is_ok(),
        "connect_timeout_secs == 0 must disable the timeout, not fail immediately, got {:?}",
        result.map(|_| ())
    );

    broker_task.await.expect("mock broker task panicked");
}

/// Regression test for `.claude/rules/tls-identity-sni.md`: `ensure_mqtt()`'s TLS connect
/// must send the printer's serial as SNI/identity, never the IP.
#[tokio::test]
async fn test_ensure_mqtt_connects_tls_with_serial_not_ip() {
    let (client_stream, _server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory {
        active_stream: data_container,
    };

    let (connector, captured_host) = HostCapturingTlsConnector::new();
    let mut client = PrinterClient::new(
        connector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(1);

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect_mqtt()).await;

    assert_eq!(
        captured_host.lock().await.as_deref(),
        Some(SERIAL),
        "MQTT TLS connect must use the serial, not the IP, as SNI/identity"
    );
}

/// `from_mqtt()`-constructed clients have empty `ip`/`access_code` (no host config
/// was ever supplied) — calling `.with_ftps()` on one used to silently succeed and only fail
/// opaquely at actual FTPS connect time. Must now panic immediately at the builder call site.
#[tokio::test]
#[should_panic(expected = "with_ftps() requires a real ip/access_code")]
async fn test_with_ftps_panics_on_from_mqtt_client() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });
    let client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let _ = client.with_ftps(
        bambino::client::dummy::DummyTls,
        bambino::client::dummy::DummyFactory,
        bambino::client::dummy::DummyTimer,
    );

    broker_task.await.expect("mock broker task panicked");
}
