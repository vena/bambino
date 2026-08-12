//! # Client Coordinator — Negative / Failure Path Tests
//!
//! Split from `client_test.rs` (see issue #35).

mod common;


use bambino::client::{
    CalibrationOption,
    PrintSpeed,
};
use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use bambino::mqtt::PrintJobConfig;

use common::client::connect_test_client;
use common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

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
async fn test_start_drying_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1C).await;

    let result = client.start_drying(999, 55, 8, 0, true, 20, false, "PA-CF").await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_stop_drying_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1C).await;

    let result = client.stop_drying(16).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

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

