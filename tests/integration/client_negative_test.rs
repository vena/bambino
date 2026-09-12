//! # Client Coordinator — Negative / Failure Path Tests
//!
//! Split from `client_test.rs` (see issue #35).

use bambino::client::{CalibrationOption, PrintSpeed, PrintStatus};
use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use bambino::mqtt::PrintJobConfig;
use bambino::types::DryingMaterial;
use bambino::types::telemetry::AmsUnitModel;

use crate::common::client::connect_test_client;
use crate::common::mock_mqtt::{
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
    let mut client_p1s = connect_test_client(
        TokioIo(client_stream_p1s),
        "01P000000000000",
        PrinterModel::P1S,
    )
    .await;
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
    let mut client_h2d = connect_test_client(
        TokioIo(client_stream_h2d),
        "01P000000000000",
        PrinterModel::H2D,
    )
    .await;
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
        while crate::common::mock_mqtt::read_packet(&mut server_stream)
            .await
            .is_ok()
        {}
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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
        matches!(err, Err(Error::Backpressure)),
        "Expected Backpressure on command 201 (MqttClient::publish_command's documented \
         in-flight-saturation response). It must not be a timeout: saturation doesn't clear on \
         its own, so a caller's retry-on-timeout policy would spin forever. Got {:?}",
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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
        // P1S: single nozzle → nozzle_offset_cali forced to 0
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "20P000000000000", PrinterModel::X2D).await;

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
async fn test_start_print_single_nozzle_overrides_nozzle_offset_off() {
    // Regression: the quirk was consulted only inside `unwrap_or_else`, so an explicit
    // `.nozzle_offset_calibration(true)` sailed past the single-nozzle hardware gate and
    // serialized `nozzle_offset_cali: 1` to a printer with no second carriage. The quirk is a
    // hard ceiling, not a default.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["nozzle_offset_cali"], 0);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let config = PrintJobConfig::new(
        "job.3mf",
        "Metadata/plate_1.gcode",
        "Test Print",
        12345,
        "textured",
    )
    .nozzle_offset_calibration(true);
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .start_calibration(
            CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION,
        )
        .await
        .expect("start_calibration failed");

    broker_task.await.expect("Broker task panicked");
}

/// Issue #251: the firmware acks every option bit as `"success"` and silently queues nothing
/// for a routine it doesn't run, so `effective == 0` is the only thing stopping a calibration
/// request that runs nothing from being reported as success (closed #229). Nothing tested it.
#[tokio::test]
async fn test_start_calibration_rejects_a_fully_unsupported_request_without_publishing() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // The rejected calibration must never reach the wire, so the *first* frame the broker
        // sees has to be the follow-up command issued below.
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json["print"]["command"], "clean_print_error",
            "a fully-unsupported calibration request must publish nothing at all"
        );
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // The P1S mask is 0b0000_1110; neither NOZZLE_HEIGHT (16) nor HEATBED_THERMAL (32) is in it.
    let result = client
        .start_calibration(CalibrationOption::NOZZLE_HEIGHT | CalibrationOption::HEATBED_THERMAL)
        .await;
    assert!(
        matches!(result, Err(Error::ModelMismatch(_))),
        "expected ModelMismatch when no requested routine is supported, got {result:?}"
    );

    client
        .clear_print_error()
        .await
        .expect("clear_print_error failed");

    broker_task.await.expect("Broker task panicked");
}

/// Issue #251, the other branch: a partially-supported request is deliberately *not* an error —
/// it proceeds with the supported bits only, so a caller that ORs in every flag defensively
/// still gets the routines the model runs.
#[tokio::test]
async fn test_start_calibration_publishes_only_the_supported_bits() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "calibration");
        // BED_LEVELING (2) survives; NOZZLE_HEIGHT (16) is masked off for the P1S.
        assert_eq!(json["print"]["option"], 2);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .start_calibration(CalibrationOption::BED_LEVELING | CalibrationOption::NOZZLE_HEIGHT)
        .await
        .expect("a partially-supported calibration request must still run");

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

// Issue #253: `preheat_chamber` coordinates the airduct flap with the chamber heater and had
// no coverage of any of its four branches — including the reset-to-cooling case, whose absence
// would leave a PLA job inheriting the previous ABS job's heating flap.

#[tokio::test]
async fn test_preheat_chamber_sets_heating_flap_then_target_on_a_flap_and_heater_model() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // The flap must be sealed *before* the target is raised: its default cooling position
        // actively vents the chamber, so a heat request with the flap left cooling never
        // converges.
        let flap = read_publish_payload(&mut server_stream).await;
        assert_eq!(flap["print"]["command"], "set_airduct");
        assert_eq!(flap["print"]["modeId"], 1); // Heating

        let heat = read_publish_payload(&mut server_stream).await;
        assert_eq!(heat["print"]["command"], "gcode_line");
        assert_eq!(heat["print"]["param"], "M141 S50\n");
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "09400000000000", PrinterModel::H2D).await;

    client
        .preheat_chamber(50)
        .await
        .expect("preheat_chamber failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_preheat_chamber_resets_the_flap_to_cooling_on_a_zero_target() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let flap = read_publish_payload(&mut server_stream).await;
        assert_eq!(flap["print"]["command"], "set_airduct");
        assert_eq!(
            flap["print"]["modeId"], 0,
            "a zero target must reset the flap to cooling — the flap persists across jobs"
        );

        let heat = read_publish_payload(&mut server_stream).await;
        assert_eq!(heat["print"]["param"], "M141 S0\n");
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "09400000000000", PrinterModel::H2D).await;

    client
        .preheat_chamber(0)
        .await
        .expect("preheat_chamber failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_preheat_chamber_on_a_flap_only_model_stops_after_the_flap() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // A P2S has the flap but no active chamber heater, so "stop venting for cooling" is
        // actionable while `M141` is not — and must not be sent.
        let flap = read_publish_payload(&mut server_stream).await;
        assert_eq!(flap["print"]["command"], "set_airduct");
        assert_eq!(flap["print"]["modeId"], 0);

        let next = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            next["print"]["command"], "clean_print_error",
            "no M141 may follow the flap command on a heaterless model"
        );
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "N7000000000000", PrinterModel::P2S).await;

    client
        .preheat_chamber(0)
        .await
        .expect("preheat_chamber failed");
    client
        .clear_print_error()
        .await
        .expect("clear_print_error failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_preheat_chamber_on_a_flap_only_model_rejects_heat_without_touching_the_flap() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // The caller wanted heat this model cannot make; the flap is left alone, so the first
        // frame on the wire is the follow-up command.
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "clean_print_error");
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "N7000000000000", PrinterModel::P2S).await;

    let result = client.preheat_chamber(50).await;
    assert!(
        matches!(result, Err(Error::ModelMismatch(_))),
        "a heat request on a heaterless model must fail, got {result:?}"
    );

    client
        .clear_print_error()
        .await
        .expect("clear_print_error failed");

    broker_task.await.expect("Broker task panicked");
}

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(0, 1, -1, -1, None)
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(1, 2, -1, -1, None)
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(255, 254, -1, -1, None)
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.change_filament(99, 1, -1, -1, None).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

/// Issue #252: `slot_id == 254` is the external-spool load sentinel and is only meaningful
/// against an external-spool `ams_id`. The earlier `ams_id >= 16` form also admitted the AMS-HT
/// bus range 128..=135, where a pair like `(130, 254)` derived a correct `target` but shipped a
/// nonsensical slot to real hardware (closed #9). Every existing test used `(255, 254)`.
#[tokio::test]
async fn test_change_filament_rejects_external_spool_sentinel_on_an_ams_ht_bus_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.change_filament(130, 254, -1, -1, None).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "slot_id 254 against an AMS-HT bus id must be rejected, got {result:?}"
    );

    broker_task.await.expect("Broker task panicked");
}

/// Issue #252, second half: `extruder_id` reaches the wire. Every other `change_filament` test
/// passes `None`, so the field that decides which hotend an FTS-equipped machine feeds — and
/// whose absence makes such a machine discard the command in silence — was never asserted.
#[tokio::test]
async fn test_change_filament_routes_extruder_id_onto_the_wire() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_change_filament");
        assert_eq!(json["print"]["extruder_id"], 1);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client
        .change_filament(0, 1, -1, -1, Some(1))
        .await
        .expect("change_filament with an explicit extruder_id failed");

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    client
        .dry(128)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PA-CF")
        .send()
        .await
        .expect("start_drying failed");
    client.stop_drying(128).await.expect("stop_drying failed");

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_drying_rejects_temperature_outside_ams_unit_range() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    // No AMS snapshot has been polled, so the unit-model gate passes through and the range
    // falls back to the `ams_id`-derived one: 45-85 for an AMS-HT address, 45-65 otherwise.
    // Out-of-range is rejected at both ends rather than clamped — silently rewriting a
    // caller's 0°C into the 45°C floor would start a heating cycle nobody asked for.
    for (ams_id, temp) in [(128, 200), (128, 20), (0, 200), (0, 20), (0, 0)] {
        let err = client
            .dry(ams_id)
            .temp(temp)
            .duration_hours(8)
            .humidity(0)
            .rotate_tray(true)
            .cooling_temp(20)
            .close_power_conflict(false)
            .filament("PA-CF")
            .send()
            .await
            .expect_err("start_drying must reject an out-of-range temperature");
        assert!(
            matches!(err, Error::InvalidArgument(_)),
            "ams_id {ams_id} temp {temp} gave {err:?}"
        );
    }

    // 85°C is in range for an AMS-HT but out of range for a standard-AMS address.
    let err = client
        .dry(0)
        .temp(85)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect_err("85°C must be rejected at a standard-AMS address");
    assert!(matches!(err, Error::InvalidArgument(_)));

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

/// A reported `fun2` bit 5 outranks the model's own rule, end to end through the client.
///
/// **The payload here is synthetic and no P1S produces it.** The P1 family sends neither `fun`
/// nor `fun2` (#241, `reference/03_mqtt_telemetry.md`), so this does not document P1S behavior —
/// it exercises the precedence rule using the model whose rule is most strongly `false`, which
/// makes an override the clearest possible signal. Read it as "a reported bit wins", not as
/// "a P1S can report this".
#[tokio::test]
async fn test_reported_fun2_overrides_the_model_rule() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // fun2 "20" = bit 5 set, plus an AMS 2 Pro so the unit gate passes too.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4503,
            br#"{"print":{"fun2":"20","ams":{"ams":[{"id":"0","temp":"25","humidity":"4","info":"3"}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_filament_drying");
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    // Before any telemetry, the P1S screen-only rule stands.
    assert!(!client.supports_ams_remote_drying());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    assert!(client.supports_ams_remote_drying());
    client
        .dry(0)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect("a P1S reporting fun2 bit 5 must be allowed to dry");

    broker_task.await.expect("Broker task panicked");
}

/// The inverse: a printer reporting bit 5 *clear* is refused even where the quirk says `true`.
#[tokio::test]
async fn test_reported_fun2_can_refuse_where_the_quirk_allows() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // fun2 "00" = bit 5 clear. An X1E's quirk default is true.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4504,
            br#"{"print":{"fun2":"00","ams":{"ams":[{"id":"0","temp":"25","humidity":"4","info":"3"}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1E).await;
    assert!(client.supports_ams_remote_drying());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    // There is one answer to this question, not two that can disagree (#240): asking the quirk
    // directly with the client's own context gives the same result as the client shorthand.
    assert!(
        !client
            .quirks()
            .supports_ams_remote_drying(&client.quirk_context())
    );
    assert!(!client.supports_ams_remote_drying());
    assert!(!client.capabilities().supports_ams_remote_drying());
    let err = client
        .dry(0)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect_err("a printer reporting fun2 bit 5 clear must be refused");
    assert!(matches!(err, Error::ModelMismatch(_)), "{err:?}");

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

/// The builder's unset defaults are what reaches the wire, so they are pinned here rather than
/// only described in doc comments. `cooling_temp` in particular defaults to BambuStudio's own
/// fallback of 50 (`AMSDryControl.cpp:813`), not to zero.
#[tokio::test]
async fn test_dry_builder_defaults_reach_the_wire() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_filament_drying");
        assert_eq!(json["print"]["temp"], 55);
        assert_eq!(json["print"]["duration"], 8);
        // Unset by the caller — these are the builder's defaults.
        assert_eq!(json["print"]["humidity"], 0);
        assert_eq!(json["print"]["rotate_tray"], false);
        assert_eq!(json["print"]["cooling_temp"], 50);
        assert_eq!(json["print"]["close_power_conflict"], false);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    client
        .dry(128)
        .temp(55)
        .duration_hours(8)
        .send()
        .await
        .expect("dry() with only temp and duration set must publish");

    broker_task.await.expect("Broker task panicked");
}

/// `.material()` fills temperature, duration, cooling temperature and the filament name from one
/// choice — the reason the builder exists. PETG on an AMS 2 Pro is 65 °C for 12 h idle, with a
/// 60 °C softening temperature as the wire `cooling_temp`.
#[tokio::test]
async fn test_dry_builder_material_fills_four_fields() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["temp"], 65);
        assert_eq!(json["print"]["duration"], 12);
        assert_eq!(json["print"]["cooling_temp"], 60);
        assert_eq!(json["print"]["filament"], "PETG");
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    client
        .dry(128)
        .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
        .send()
        .await
        .expect("material() must supply temp, duration, cooling_temp and filament");

    broker_task.await.expect("Broker task panicked");
}

/// `.printing()` re-reads the lower while-printing column: PETG drops 65 -> 55 °C.
#[tokio::test]
async fn test_dry_builder_printing_column() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["temp"], 55);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    client
        .dry(128)
        .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
        .printing(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
        .send()
        .await
        .expect("printing() must re-read the while-printing column");

    broker_task.await.expect("Broker task panicked");
}

/// An unset temperature or duration is refused rather than defaulted. Picking one silently would
/// start a real heating cycle nobody asked for, which is the same reasoning that made #234 reject
/// out-of-range temperatures instead of clamping them.
#[tokio::test]
async fn test_dry_builder_refuses_unset_temp_or_duration() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    let err = client
        .dry(128)
        .duration_hours(8)
        .send()
        .await
        .expect_err("an unset temperature must be refused");
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    let err = client
        .dry(128)
        .temp(55)
        .send()
        .await
        .expect_err("an unset duration must be refused");
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    // A material with no parameters for this unit leaves them unset rather than guessing, so
    // the same refusal applies — an AMS Lite has no drying chamber at all.
    let err = client
        .dry(0)
        .material(DryingMaterial::Petg, AmsUnitModel::AmsLite)
        .send()
        .await
        .expect_err("a material with no parameters for this unit must not publish a guess");
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

/// The builder inherits every gate, in the same order. A P1S refuses on host capability before
/// anything else, so a fully-configured cycle still never reaches the wire.
#[tokio::test]
async fn test_dry_builder_inherits_the_gates() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let err = client
        .dry(0)
        .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
        .send()
        .await
        .expect_err("P1S is screen-only — the builder must refuse like start_drying did");
    assert!(matches!(err, Error::ModelMismatch(_)), "{err:?}");

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

/// The core of #234: `ams_id` `0..=3` is shared by the original AMS, the AMS Lite and the
/// AMS 2 Pro, and only the last has a heater. Once telemetry identifies the attached unit, a
/// drying command to a heaterless one must be refused rather than acked into the void.
#[tokio::test]
async fn test_start_drying_refuses_heaterless_unit_once_observed() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // `info` bits 0-3 = 1 → the original 4-slot AMS. No drying chamber.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4500,
            br#"{"print":{"ams":{"ams":[{"id":"0","temp":"25","humidity":"4","info":"1"}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1E).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    // 55°C is inside the standard-AMS 45-65 range, so only the unit-model gate can reject this.
    let err = client
        .dry(0)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect_err("start_drying must refuse an original AMS — it has no heater");
    assert!(matches!(err, Error::ModelMismatch(_)), "{err:?}");

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

/// Same address, same temperature, but the unit reports as an AMS 2 Pro — it must publish.
#[tokio::test]
async fn test_start_drying_allows_observed_ams_2_pro() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // `info` bits 0-3 = 3 → AMS 2 Pro (BambuStudio `N3F`). Dries, 45-65°C.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4501,
            br#"{"print":{"ams":{"ams":[{"id":"0","temp":"25","humidity":"4","info":"3"}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_filament_drying");
        assert_eq!(json["print"]["temp"], 55);
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1E).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    client
        .dry(0)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect("start_drying must be allowed on an observed AMS 2 Pro");

    broker_task.await.expect("Broker task panicked");
}

/// An AMS 2 Pro tops out at 65°C even though the same request would be legal at an AMS-HT
/// address — the observed unit, not the address, sets the ceiling.
#[tokio::test]
async fn test_start_drying_range_follows_observed_unit_not_address() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // An AMS-HT bus address carrying an AMS 2 Pro's unit type. Contrived, but it pins
        // which of the two sources the range comes from.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4502,
            br#"{"print":{"ams":{"ams":[{"id":"128","temp":"25","humidity":"4","info":"3"}]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1E).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");

    // 80°C would pass the address-derived 45-85 fallback and fails the observed unit's 45-65.
    let err = client
        .dry(128)
        .temp(80)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PLA")
        .send()
        .await
        .expect_err("the observed AMS 2 Pro's 65°C ceiling must win over the AMS-HT address");
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_drying_rejects_external_spool() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    // 254/255 pass `is_valid_ams_id` (they are real addresses for change_filament), but an
    // external spool is a bracket with no heater, so drying can never act on one. Unlike the
    // 0..=3 case this needs no telemetry: the sentinels never appear in the `ams` array.
    for ams_id in [254, 255] {
        let err = client
            .dry(ams_id)
            .temp(55)
            .duration_hours(8)
            .humidity(0)
            .rotate_tray(true)
            .cooling_temp(20)
            .close_power_conflict(false)
            .filament("PLA")
            .send()
            .await
            .expect_err("start_drying must reject an external spool");
        assert!(matches!(err, Error::ModelMismatch(_)), "ams_id {ams_id}");
    }

    drop(client);
    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_start_drying_rejected_on_p1_screen_only_firmware() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // P1S firmware acks ams_filament_drying with `result: success` and then silently
    // discards it — no heater/fan activation, dry_status stays 0 — confirmed against real
    // hardware. start_drying() must reject before dispatch rather than send a command the
    // printer will accept-then-drop.
    let err = client
        .dry(0)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PA-CF")
        .send()
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    let result = client
        .dry(999)
        .temp(55)
        .duration_hours(8)
        .humidity(0)
        .rotate_tray(true)
        .cooling_temp(20)
        .close_power_conflict(false)
        .filament("PA-CF")
        .send()
        .await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_stop_drying_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::X1E).await;

    // 16 is the A2L AMS Lite's physical id and valid; 17 addresses nothing.
    let result = client.stop_drying(17).await;
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client.scan_rfid(0, 2).await.expect("scan_rfid failed");

    broker_task.await.expect("Broker task panicked");
}

/// Issue #271: every `ams_id`-taking command accepts the A2L-attached AMS Lite under both its
/// normalized id 6 and physical id 16, and sends the physical 16 with a local slot.
#[tokio::test]
async fn test_ams_commands_address_a2l_ams_lite() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_change_filament");
        assert_eq!(json["print"]["ams_id"], 16);
        assert_eq!(json["print"]["slot_id"], 1);
        assert_eq!(json["print"]["target"], 16);

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_get_rfid");
        assert_eq!(json["print"]["ams_id"], 16);
        assert_eq!(json["print"]["slot_id"], 2);

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_filament_drying");
        assert_eq!(json["print"]["ams_id"], 16);

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "extrusion_cali_sel");
        assert_eq!(json["print"]["ams_id"], 16);
        assert_eq!(json["print"]["tray_id"], 25);

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "ams_filament_drying");
        assert_eq!(json["print"]["ams_id"], 16);
        assert_eq!(json["print"]["mode"], 1);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::A2L).await;

    client
        .change_filament(6, 1, -1, -1, None)
        .await
        .expect("change_filament failed");
    client.scan_rfid(16, 2).await.expect("scan_rfid failed");
    client.stop_drying(6).await.expect("stop_drying failed");
    client
        .select_k_profile(6, 25, 4, "GFA01", "0.4")
        .await
        .expect("select_k_profile failed");
    // No AMS telemetry has arrived, so the unit-model gate passes the unobserved unit through.
    client
        .dry(16)
        .temp(50)
        .duration_hours(4)
        .send()
        .await
        .expect("drying cycle failed");

    broker_task.await.expect("Broker task panicked");
}

/// Issue #269: a printer whose `push_status` frames lack the new-protocol probe gets
/// `M620 R<global tray>`; one that shows the probe gets `ams_get_rfid`.
#[tokio::test]
async fn test_scan_rfid_selects_command_by_protocol_generation() {
    for (frame, new_protocol) in [
        (
            br#"{"print":{"command":"push_status","gcode_state":"IDLE"}}"#.as_slice(),
            false,
        ),
        (
            br#"{"print":{"command":"push_status","cfg":"0","fun":"0","aux":"0","stat":"0"}}"#
                .as_slice(),
            true,
        ),
        (
            br#"{"print":{"command":"push_status","flag3":512}}"#.as_slice(),
            true,
        ),
    ] {
        let (client_stream, mut server_stream) = tokio::io::duplex(8192);
        let topic = format!("device/{}/report", SERIAL);
        let frame = frame.to_vec();

        let broker_task = tokio::spawn(async move {
            handle_mqtt_handshake(&mut server_stream).await;
            send_publish_payload(&mut server_stream, &topic, 4601, &frame).await;
            read_puback(&mut server_stream).await;

            let json = read_publish_payload(&mut server_stream).await;
            if new_protocol {
                assert_eq!(json["print"]["command"], "ams_get_rfid");
                assert_eq!(json["print"]["ams_id"], 1);
                assert_eq!(json["print"]["slot_id"], 2);
            } else {
                assert_eq!(json["print"]["command"], "gcode_line");
                assert_eq!(json["print"]["param"], "M620 R6\n");
            }
        });

        let mut client =
            connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1C).await;
        client
            .poll_telemetry()
            .await
            .expect("poll_telemetry failed");
        client.scan_rfid(1, 2).await.expect("scan_rfid failed");

        broker_task.await.expect("Broker task panicked");
    }
}

/// Issue #269: the scan feeds filament to the reader, so it is refused while `tray_now` shows
/// filament loaded to the toolhead.
#[tokio::test]
async fn test_scan_rfid_refuses_with_filament_loaded() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        send_publish_payload(
            &mut server_stream,
            &topic,
            4602,
            br#"{"print":{"command":"push_status","ams":{"tray_now":"3","ams":[]}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X1C).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");
    let result = client.scan_rfid(0, 2).await;
    assert!(matches!(result, Err(Error::InvalidState(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_scan_rfid_rejects_invalid_ams_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    let result = client.select_k_profile(200, 200, 4, "GFA01", "0.4").await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_select_k_profile_rejects_standard_tray_id_above_15() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Only the valid (0, 15) call below dispatches — if the rejected (0, 16) call
        // had leaked a publish, this read would see tray_id 16 and fail the assert.
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "extrusion_cali_sel");
        assert_eq!(json["print"]["tray_id"], 15);
    });

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    // Standard-AMS global tray IDs are 0..=15 (4 units × 4 slots); 16..=103 used to
    // slip through and dispatch an address the firmware rejects or mis-routes.
    let result = client.select_k_profile(0, 16, 4, "GFA01", "0.4").await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));

    // The upper documented standard boundary (15) must still be accepted.
    let result = client.select_k_profile(0, 15, 4, "GFA01", "0.4").await;
    assert!(result.is_ok());

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
        .get_k_profiles(None, None)
        .await
        .expect("get_k_profiles failed");
    assert_eq!(resp.print.filaments.len(), 1);
    assert_eq!(resp.print.filaments[0].filament_id, "GFA01");

    // Second call skips prime (1 publish)
    let resp2 = client
        .get_k_profiles(None, None)
        .await
        .expect("get_k_profiles second call failed");
    assert_eq!(resp2.print.filaments.len(), 1);

    broker_task.await.expect("Broker task panicked");
}

/// Issue #264: `filament_id` must reach the wire on *both* the prime and the real query.
/// It used to be hardcoded `None`, so a filament-scoped query was only reachable by bypassing
/// this wrapper and hand-managing the priming quirk.
#[tokio::test]
async fn test_get_k_profiles_threads_filament_id_onto_the_wire() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_prime = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_prime["print"]["command"], "extrusion_cali_get");
        assert_eq!(json_prime["print"]["filament_id"], "GFA01");
        assert_eq!(json_prime["print"]["nozzle_diameter"], "0.4");

        let json_real = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_real["print"]["filament_id"], "GFA01");
        assert_eq!(json_real["print"]["nozzle_diameter"], "0.4");

        send_publish_payload(
            &mut server_stream,
            &topic,
            1000,
            K_PROFILE_RESPONSE.as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    let resp = client
        .get_k_profiles(Some("GFA01"), Some("0.4"))
        .await
        .expect("filament-scoped get_k_profiles failed");
    assert_eq!(resp.print.filaments.len(), 1);

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
        .get_k_profiles(None, None)
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
        .get_k_profiles(None, None)
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

    let mut client =
        connect_test_client(TokioIo(client_stream), "01P000000000000", PrinterModel::P1S).await;

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("Broker task panicked");
}

// ============================================================================
// Print-state gates (issue #231)
// ============================================================================

/// `skip_objects` must refuse once telemetry confirms no job is loaded — its object IDs are
/// `identify_id` values from the loaded 3MF, so they reference nothing when idle.
#[tokio::test]
async fn test_skip_objects_refuses_when_idle() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        send_publish_payload(
            &mut server_stream,
            &topic,
            4400,
            br#"{"print":{"gcode_state":"IDLE"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");
    assert_eq!(client.print_status(), Some(PrintStatus::Idle));

    assert!(matches!(
        client.skip_objects(vec![1, 2]).await,
        Err(Error::InvalidState(_))
    ));

    broker_task.await.expect("Broker task panicked");
}

/// `PAUSE` is a legitimate skip window (inspect a failed part, skip it, resume), so the gate
/// must let it through and actually publish.
#[tokio::test]
async fn test_skip_objects_allowed_when_paused() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        send_publish_payload(
            &mut server_stream,
            &topic,
            4401,
            br#"{"print":{"gcode_state":"PAUSE"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "skip_objects");
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");
    assert_eq!(client.print_status(), Some(PrintStatus::Paused));

    client
        .skip_objects(vec![3])
        .await
        .expect("skip_objects should be allowed while paused");

    broker_task.await.expect("Broker task panicked");
}

/// An empty `obj_list` cannot skip anything, so it is rejected as a bad argument before the
/// state gate is even consulted.
#[tokio::test]
async fn test_skip_objects_rejects_empty_object_ids() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    assert!(matches!(
        client.skip_objects(vec![]).await,
        Err(Error::InvalidArgument(_))
    ));

    broker_task.await.expect("Broker task panicked");
}

/// `pause`/`resume`/`stop` are deliberately **not** state-gated, and this guards that against a
/// well-meaning future "fix" that generalizes `skip_objects`' gate across the lifecycle calls.
///
/// Three independent reasons, all load-bearing: no upstream gates these on `gcode_state`; the
/// gate would read a *cached* value, so a stale `IDLE` would refuse a real abort; and the CLI's
/// `probe` sends pause/resume while idle on purpose to document firmware behavior, which a
/// client-side refusal would silently defeat.
#[tokio::test]
async fn test_lifecycle_commands_are_not_state_gated() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        send_publish_payload(
            &mut server_stream,
            &topic,
            4402,
            br#"{"print":{"gcode_state":"IDLE"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        for expected in ["pause", "resume", "stop"] {
            let json = read_publish_payload(&mut server_stream).await;
            assert_eq!(json["print"]["command"], expected);
        }
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry failed");
    // Cache now definitively reads Idle — the state a gate would refuse on.
    assert_eq!(client.print_status(), Some(PrintStatus::Idle));

    client
        .pause_print()
        .await
        .expect("pause_print must remain ungated");
    client
        .resume_print()
        .await
        .expect("resume_print must remain ungated");
    client
        .stop_print()
        .await
        .expect("stop_print must remain ungated");

    broker_task.await.expect("Broker task panicked");
}
