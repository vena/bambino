//! # Client Coordinator Behavioral Integration Tests
//!
//! Validates the safety boundaries, temperature clamps, fan step calculations, and
//! G-code wrapping heuristics implemented inside `PrinterClient`.
//!
//! Evaluates the client against an inline, in-memory duplex stream mock to ensure
//! exact verification of the generated raw JSON payloads and raw G-code arrays.

mod common;

use bambino::client::{FanTarget, PrinterClient};
use bambino::error::BambuError;
use bambino::io::TokioIo;
use bambino::models::BambuModel;
use bambino::mqtt::BambuMqttClient;

use common::mock_mqtt::{handle_mqtt_handshake, read_publish_payload};

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "00M000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    // CoreXY Bed-on-Z initialization
    let mut client_x1c = PrinterClient::new(mqtt_client, "00M000000000000", BambuModel::X1C);

    // Assert public serial and model getters expose the correct fields
    assert_eq!(client_x1c.serial(), "00M000000000000");
    assert_eq!(client_x1c.model(), BambuModel::X1C);

    // Bed-on-Z Safety Guard Verification: home_z_only_danger must return ModelMismatch
    let err_res = client_x1c.home_axes(true).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

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

    let mqtt_client_a1 =
        BambuMqttClient::connect(TokioIo(client_stream_a1), "039000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for A1");

    let mut client_a1 = PrinterClient::new(mqtt_client_a1, "039000000000000", BambuModel::A1);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

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
async fn test_thermal_guards_and_temperatures() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bed temperature verification
        let json_bed = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_bed["print"]["param"], "M140 S60\n");

        // Nozzle temperature verification
        let json_nozzle = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_nozzle["print"]["param"], "M104 T0 S220\n");

        // Active chamber temperature verification (X1E has active PTC heater)
        let json_chamber = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_chamber["print"]["param"], "M141 S45\n");
    });

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "00M000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client_x1e = PrinterClient::new(mqtt_client, "00M000000000000", BambuModel::X1E);

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

    let mqtt_client_x1c =
        BambuMqttClient::connect(TokioIo(client_stream_x1c), "00M000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for X1C");

    let mut client_x1c = PrinterClient::new(mqtt_client_x1c, "00M000000000000", BambuModel::X1C);

    let err_res = client_x1c.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    // Open-frame model check (A1 — no sensor, no heater)
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
    });

    let mqtt_client_a1 =
        BambuMqttClient::connect(TokioIo(client_stream_a1), "039000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for A1");

    let mut client_a1 = PrinterClient::new(mqtt_client_a1, "039000000000000", BambuModel::A1);

    let err_res = client_a1.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    broker_task.await.expect("X1E broker task panicked");
    broker_task_x1c.await.expect("X1C broker task panicked");
    broker_task_a1.await.expect("A1 broker task panicked");
}

#[tokio::test]
async fn test_cooling_fans_and_peripheral_switches() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Verify part cooling fan (M106 P1)
        let json_cf = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_cf["print"]["param"], "M106 P1 S127\n"); // 50% PWM

        // Left auxiliary fan verification (M106 P2)
        let json_aux = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_aux["print"]["param"], "M106 P2 S255\n"); // 100% PWM
    });

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client_p1s = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

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
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    // Verify right auxiliary cooling fan is supported on X2D model (using Port 10)
    let (client_stream_x2, mut server_stream_x2) = tokio::io::duplex(8192);
    let broker_task_x2 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_x2).await;
        let json_aux_r = read_publish_payload(&mut server_stream_x2).await;
        assert_eq!(json_aux_r["print"]["param"], "M106 P10 S204\n"); // 80% PWM
    });

    let mqtt_client_x2 =
        BambuMqttClient::connect(TokioIo(client_stream_x2), "20P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for X2D");

    let mut client_x2 = PrinterClient::new(mqtt_client_x2, "20P000000000000", BambuModel::X2D);

    client_x2
        .set_fan_speed(FanTarget::AuxiliaryRight, 80)
        .await
        .expect("X2D auxiliary right fan set failed");

    broker_task.await.expect("P1S broker task panicked");
    broker_task_x2.await.expect("X2D broker task panicked");
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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client_h2d = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::H2D);

    client_h2d
        .set_airduct_mode(true)
        .await
        .expect("Airduct mode set failed");
    client_h2d
        .set_buzzer_mode(2)
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

    let mqtt_client_a1 =
        BambuMqttClient::connect(TokioIo(client_stream_a1), "039000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for A1");

    let mut client_a1 = PrinterClient::new(mqtt_client_a1, "039000000000000", BambuModel::A1);

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

    let mqtt_client_p1s =
        BambuMqttClient::connect(TokioIo(client_stream_p1s), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for P1S");

    let mut client_p1s = PrinterClient::new(mqtt_client_p1s, "01P000000000000", BambuModel::P1S);

    assert!(matches!(
        client_p1s.set_airduct_mode(true).await,
        Err(BambuError::ModelMismatch)
    ));
    assert!(matches!(
        client_p1s.set_prompt_sound(true).await,
        Err(BambuError::ModelMismatch)
    ));
    assert!(matches!(
        client_p1s.set_buzzer_mode(1).await,
        Err(BambuError::ModelMismatch)
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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    // Unsafe partial homing on bed-on-Z must be rejected by send_gcode
    let err = client.send_gcode("G28 Z").await;
    assert!(matches!(err, Err(BambuError::ModelMismatch)));

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "00M000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "00M000000000000", BambuModel::X1E);

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

// ============================================================================
// Negative / Failure Path Tests
// ============================================================================

#[tokio::test]
async fn test_in_flight_saturation() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1_048_576);

    let _broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read and discard all incoming PUBLISH packets without sending PUBACKs
        loop {
            match common::mock_mqtt::read_packet(&mut server_stream).await {
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

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
        err.is_err(),
        "Expected in-flight saturation error on command 201"
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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    broker_task.await.expect("Broker task panicked");

    // After the server stream is dropped, publish attempts should fail with a network error
    let result = client.send_gcode("G28").await;
    assert!(
        result.is_err(),
        "Expected network error after connection drop"
    );
    assert!(
        matches!(result, Err(BambuError::NetworkError(_))),
        "Expected BambuError::NetworkError, got {:?}",
        result
    );
}
