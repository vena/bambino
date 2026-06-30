//! # Client Coordinator Behavioral Integration Tests
//!
//! Validates the safety boundaries, temperature clamps, fan step calculations, and
//! G-code wrapping heuristics implemented inside `PrinterClient`.
//!
//! Evaluates the client against an inline, in-memory duplex stream mock to ensure
//! exact verification of the generated raw JSON payloads and raw G-code arrays.

mod common;

use bambino::client::{CalibrationOption, FanTarget, PrintSpeed, PrintStatus, PrinterClient};
use bambino::error::BambuError;
use bambino::io::TokioIo;
use bambino::models::BambuModel;
use bambino::mqtt::{BambuMqttClient, PrintJobConfig};

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "00M000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    // CoreXY Bed-on-Z initialization
    let mut client_x1c = PrinterClient::from_mqtt(mqtt_client, "00M000000000000", BambuModel::X1C);

    // Assert public serial and model getters expose the correct fields
    assert_eq!(client_x1c.serial(), "00M000000000000");
    assert_eq!(client_x1c.model(), BambuModel::X1C);

    // Bed-on-Z Safety Guard Verification: home_z_only_danger must return ModelMismatch
    let err_res = client_x1c.home_axes(true).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch(_))));

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

    let mut client_a1 = PrinterClient::from_mqtt(mqtt_client_a1, "039000000000000", BambuModel::A1);

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mut client_x1e = PrinterClient::from_mqtt(mqtt_client, "00M000000000000", BambuModel::X1E);

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

    let mut client_x1c =
        PrinterClient::from_mqtt(mqtt_client_x1c, "00M000000000000", BambuModel::X1C);

    let err_res = client_x1c.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch(_))));

    // Open-frame model check (A1 — no sensor, no heater)
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
    });

    let mqtt_client_a1 =
        BambuMqttClient::connect(TokioIo(client_stream_a1), "039000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed for A1");

    let mut client_a1 = PrinterClient::from_mqtt(mqtt_client_a1, "039000000000000", BambuModel::A1);

    let err_res = client_a1.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch(_))));

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

    let mut client_p1s = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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
    assert!(matches!(err_res, Err(BambuError::ModelMismatch(_))));

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

    let mut client_x2 =
        PrinterClient::from_mqtt(mqtt_client_x2, "20P000000000000", BambuModel::X2D);

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mut client_h2d = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::H2D);

    client_h2d
        .set_airduct_mode(bambino::mqtt::commands::AirductMode::Cooling)
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

    let mut client_a1 = PrinterClient::from_mqtt(mqtt_client_a1, "039000000000000", BambuModel::A1);

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

    let mut client_p1s =
        PrinterClient::from_mqtt(mqtt_client_p1s, "01P000000000000", BambuModel::P1S);

    assert!(matches!(
        client_p1s
            .set_airduct_mode(bambino::mqtt::commands::AirductMode::Cooling)
            .await,
        Err(BambuError::ModelMismatch(_))
    ));
    assert!(matches!(
        client_p1s.set_prompt_sound(true).await,
        Err(BambuError::ModelMismatch(_))
    ));
    assert!(matches!(
        client_p1s.set_buzzer_mode(1).await,
        Err(BambuError::ModelMismatch(_))
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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    // Unsafe partial homing on bed-on-Z must be rejected by send_gcode
    let err = client.send_gcode("G28 Z").await;
    assert!(matches!(err, Err(BambuError::ModelMismatch(_))));

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "00M000000000000", BambuModel::X1E);

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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
    });

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "20P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "20P000000000000", BambuModel::X2D);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client
        .clear_print_error()
        .await
        .expect("clear_print_error failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_toggle_led_wire_payload() {
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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client
        .toggle_led("chamber_light", true)
        .await
        .expect("toggle_led on failed");
    client
        .toggle_led("chamber_light", false)
        .await
        .expect("toggle_led off failed");

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client
        .change_filament(0, 1, 1, -1, -1)
        .await
        .expect("change_filament failed");

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
        assert_eq!(json_start["print"]["dry_temp"], 55);
        assert_eq!(json_start["print"]["dry_time"], 480);
        assert_eq!(json_start["print"]["filament"], "PA-CF");

        let json_stop = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_stop["print"]["command"], "ams_filament_drying");
        assert_eq!(json_stop["print"]["mode"], 0);
    });

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client
        .start_drying(128, 55, 480, true, "PA-CF")
        .await
        .expect("start_drying failed");
    client.stop_drying(128).await.expect("stop_drying failed");

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client.scan_rfid(0, 2).await.expect("scan_rfid failed");

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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client
        .select_k_profile(0, 1, 4, "GFA01", "0.4")
        .await
        .expect("select_k_profile failed");

    broker_task.await.expect("Broker task panicked");
}

const K_PROFILE_RESPONSE: &str = r#"{"print":{"command":"extrusion_cali_get","sequence_id":"10002","nozzle_diameter":"0.4","filaments":[{"cali_idx":4,"filament_id":"GFA01","nozzle_diameter":"0.4","nozzle_id":"HS00-0.4","extruder_id":0,"name":"Test PLA","k_value":"0.022000","setting_id":"PF12345678901234567"}]}}"#;

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
            K_PROFILE_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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
            K_PROFILE_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);
    client.set_k_profile_primed(true);

    let resp = client
        .get_k_profiles()
        .await
        .expect("get_k_profiles failed");
    assert_eq!(resp.print.command, "extrusion_cali_get");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_sequence_id_wrapping() {
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

    let mqtt_client =
        BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
            .await
            .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, "01P000000000000", BambuModel::P1S);

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("Broker task panicked");
}

// ============================================================================
// Command-response round-trip tests (Phase 18)
// ============================================================================

const VERSION_RESPONSE: &str = r#"{"info":{"command":"get_version","sequence_id":"10001","module":[{"product_name":"Bambu Lab P1S","name":"ota","hw_ver":"OTA","sw_ver":"01.09.00.00","sn":"01P000000000001","visible":true},{"name":"esp32","sw_ver":"01.02.03.04","sn":"01P000000000002"}]}}"#;

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

    let result = client.wait_for_homing().await;
    assert!(
        matches!(result, Err(BambuError::Timeout)),
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

    let mqtt_client = BambuMqttClient::connect(TokioIo(client_stream), SERIAL, "12345678")
        .await
        .expect("MQTT connect handshake failed");

    let mut client = PrinterClient::from_mqtt(mqtt_client, SERIAL, BambuModel::P1S);

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
