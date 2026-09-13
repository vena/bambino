//! # Client Coordinator — Telemetry Cache Round-Trip Tests
//!
//! Split from `client_test.rs` Phase 18 section (see issue #35).

use std::sync::Arc;
use tokio::sync::Mutex;

use bambino::client::{PrintProgress, PrintSpeed, PrintStatus, PrinterClient, TelemetryEvent};
use bambino::diagnostics::DecodedPrintError;
use bambino::identity::PrinterIdentity;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use crate::common::client::{SERIAL, connect_test_client, connect_test_mqtt};
use crate::common::io::{DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

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
    let mut client =
        connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

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

    let mut client =
        connect_test_client(TokioIo(client_stream), "00M000000000000", PrinterModel::X1C).await;

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
            // The wire sends 30 *minutes*; the cache converts it to seconds.
            remaining_secs: Some(1800),
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
            // Cached from the first push, still in seconds.
            remaining_secs: Some(1800),
            layer_num: Some(5),
            total_layers: Some(100),
        }
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_print_progress_total_layers_zero_does_not_clobber_cache() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5310,
            br#"{"print":{"layer_num":5,"total_layer_num":100}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // End-of-print: firmware resets total_layer_num to 0. That is not a real layer
        // count, and must not overwrite the cached 100.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5311,
            br#"{"print":{"total_layer_num":0}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first progress report");
    assert_eq!(client.print_progress().total_layers, Some(100));

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse total_layer_num reset report");
    // Guards poll_telemetry's `&& total_layers > 0` cache-merge condition (issue #30).
    // dashboard.rs's own end-of-print test only covers the CLI's rendering, not this path.
    assert_eq!(client.print_progress().total_layers, Some(100));

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
async fn test_vt_tray_and_vir_slot_partial_push_preserves_cached_fields() {
    // issue #43: a partial id-only push must not wholesale-clobber prior tray_type/tray_color/
    // etc., and a vir_slot push carrying only one extruder's entry must not drop the other.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Seed full fields for vt_tray and both vir_slot entries.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5610,
            br#"{"print":{
                "vt_tray":{"id":"254","tray_type":"PLA","tray_color":"FF0000FF","remain":80},
                "vir_slot":[
                    {"id":"0","tray_type":"PLA","remain":80},
                    {"id":"1","tray_type":"PETG","remain":60}
                ]
            }}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // Follow-up: id-only vt_tray, and a vir_slot array carrying only id "0".
        send_publish_payload(
            &mut server_stream,
            &topic,
            5611,
            br#"{"print":{"vt_tray":{"id":"254"},"vir_slot":[{"id":"0","remain":70}]}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first vt_tray/vir_slot report");
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second (partial) vt_tray/vir_slot report");

    let vt_tray = client.vt_tray().expect("vt_tray must still be cached");
    assert_eq!(
        vt_tray.tray_type.as_deref(),
        Some("PLA"),
        "id-only follow-up must not clobber cached tray_type"
    );
    assert_eq!(
        vt_tray.remain,
        Some(80),
        "id-only follow-up must not clobber cached remain"
    );

    let vir_slot = client.vir_slot().expect("vir_slot must still be cached");
    assert_eq!(
        vir_slot.len(),
        2,
        "partial push must not drop the other extruder's cached tray"
    );
    let slot0 = vir_slot
        .iter()
        .find(|s| s.id.as_deref() == Some("0"))
        .unwrap();
    assert_eq!(
        slot0.remain,
        Some(70),
        "matched entry must merge in the new remain value"
    );
    assert_eq!(
        slot0.tray_type.as_deref(),
        Some("PLA"),
        "matched entry must preserve tray_type"
    );
    let slot1 = vir_slot
        .iter()
        .find(|s| s.id.as_deref() == Some("1"))
        .unwrap();
    assert_eq!(
        slot1.tray_type.as_deref(),
        Some("PETG"),
        "unreferenced entry must survive untouched"
    );
    assert_eq!(slot1.remain, Some(60));

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

/// Issue #258: an AMS Lite attached to an A2L reports physical unit id 16 — not the 0 the same
/// unit uses as an A1's only AMS, because on an A2L it sits alongside up to four shared-pool
/// units already holding ids 0-3. The `snow` decode path did not normalize it, so
/// `resolve_global_tray_id(16, slot)` fell through to `None` and this accessor reported "no
/// active tray" while the machine was printing from that unit.
#[tokio::test]
async fn test_printing_tray_global_id_normalizes_the_ams_lite_on_a2l_unit_id() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Same extruder shape as the sibling test above (state 18 selects index 1), with snow
        // routing to the AMS Lite's physical id 16, slot 2: raw = (16 << 8) | 2 = 4098.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5701,
            br#"{"device":{"extruder":{"info":[
                {"id":0,"snow":65535},
                {"id":1,"snow":4098}
            ],"state":18}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::A2L).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse extruder report");

    // Normalized 16 -> 6, so 6*4 + 2 = 26. BambuStudio reaches the same 26 by a different
    // route, keying on its AMS_LITE_MIXED unit type and computing a hardcoded 24 + slot.
    assert_eq!(
        client.printing_tray_global_id(),
        Some(26),
        "an AMS Lite on an A2L must resolve, not read as no-active-tray"
    );

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
    let mut sensorless_client =
        connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
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
    let mut heated_client =
        connect_test_client(TokioIo(client_stream2), SERIAL, PrinterModel::H2D).await;

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

    // The four flat fan keys are step-encoded (0-15) on every model, including P2S/X2D — see
    // test_fan_speed_cache_from_telemetry_x2d_step_encoded below.
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::H2D).await;

    assert_eq!(client.part_cooling_fan_speed(), None);
    assert_eq!(client.auxiliary_left2_fan_speed(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(client.part_cooling_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_left_fan_speed(), Some(53));
    assert_eq!(client.chamber_exhaust_fan_speed(), Some(0));
    assert_eq!(client.heatbreak_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_left2_fan_speed(), Some(75));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_fan_speed_cache_from_telemetry_x2d_step_encoded() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5706,
            br#"{"print":{"cooling_fan_speed":"15","big_fan1_speed":"8","big_fan2_speed":"0","heatbreak_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":75}]}}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // Regression for #38: X2D/P2S previously decoded the four flat fan keys as
    // already-percentage (ModelQuirks::reports_auxiliary_fan_percentage), reading ~6.7x too low.
    // They must step-decode identically to every other model — only the id-160 airduct part
    // (auxiliary_left2_fan_speed) is a true wire percentage.
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X2D).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(client.part_cooling_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_left_fan_speed(), Some(53));
    assert_eq!(client.chamber_exhaust_fan_speed(), Some(0));
    assert_eq!(client.heatbreak_fan_speed(), Some(100));
    assert_eq!(client.auxiliary_left2_fan_speed(), Some(75));

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_auxiliary_left2_fan_negative_state_is_none() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            5707,
            br#"{"print":{"cooling_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":-1}]}}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // A negative `state` is a firmware sentinel for "off/unknown"; it must report
    // None, not be masked into 100% by `& 0xFF`.
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X2D).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(client.auxiliary_left2_fan_speed(), None);

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_auxiliary_left2_fan_packed_state_decodes_low_byte() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // 306 == 0x132: percentage 50 in the low byte, a flag bit set above it.
        send_publish_payload(
            &mut server_stream,
            &topic,
            5708,
            br#"{"print":{"cooling_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":306}]}}}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    // The low-8-bit mask must still apply to a non-negative state (BambuStudio's
    // DevFan::ParseV3_0 get_flag_bits(state, 0, 8); bambuddy's identical `& 0xFF`).
    // Without it a packed value clamps to 100 instead of decoding to its real percentage.
    // The negative-sentinel guard above and this mask are both required and must stay in
    // that order — fixing either alone reintroduced the other's bug once already.
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::X2D).await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(client.auxiliary_left2_fan_speed(), Some(50));

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

/// Issue #262: a command-echo response shares the `print` envelope and several field names
/// with genuine telemetry, so `poll_telemetry()`'s command-echo check has to route it to
/// `Unknown` *and* leave the cache alone. Nothing asserted the second half.
#[tokio::test]
async fn test_command_echo_is_unknown_and_leaves_the_cache_untouched() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let topic = format!("device/{}/report", SERIAL);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        send_publish_payload(
            &mut server_stream,
            &topic,
            4500,
            br#"{"print":{"command":"push_status","home_flag":7,"gcode_state":"RUNNING"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;

        // An `extrusion_cali_get` reply carrying fields that overlap genuine telemetry. If the
        // gate let this through, it would clobber both cached values with the echo's contents.
        send_publish_payload(
            &mut server_stream,
            &topic,
            4501,
            br#"{"print":{"command":"extrusion_cali_get","sequence_id":"10001","home_flag":0,"gcode_state":"FAILED"}}"#,
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let first = client.poll_telemetry().await.expect("first poll failed");
    assert!(matches!(first, TelemetryEvent::Report(..)));
    assert_eq!(client.is_all_axes_homed(), Some(true));
    assert_eq!(client.print_status(), Some(PrintStatus::Running));

    let echo = client.poll_telemetry().await.expect("echo poll failed");
    assert!(
        matches!(echo, TelemetryEvent::Unknown(_)),
        "a command echo for a sequence_id this client never sent must route to Unknown"
    );
    assert_eq!(
        client.is_all_axes_homed(),
        Some(true),
        "a command echo must not clobber the cached home_flag"
    );
    assert_eq!(
        client.print_status(),
        Some(PrintStatus::Running),
        "a command echo must not clobber the cached gcode_state"
    );

    broker_task.await.expect("Broker task panicked");
}

// Issue #254: connection-scoped telemetry must not survive an MQTT reconnect.

/// `home_flag` bits X|Y|Z set (0x07) plus the 220 V mains bit (0x08).
const HOME_FLAG_XYZ_HOMED_220V: u32 = 0x0F;

#[tokio::test]
async fn test_home_flag_goes_cold_across_reconnect_but_mains_region_persists() {
    let topic = format!("device/{}/report", SERIAL);

    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let first_topic = topic.clone();
    let first_broker = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        send_publish_payload(
            &mut server_stream,
            &first_topic,
            4400,
            format!(r#"{{"print":{{"home_flag":{HOME_FLAG_XYZ_HOMED_220V}}}}}"#).as_bytes(),
        )
        .await;
        read_puback(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse the home_flag report");
    assert_eq!(client.is_all_axes_homed(), Some(true));
    assert_eq!(client.is_axis_homed('x'), Some(true));
    assert_eq!(client.is_220v_power(), Some(true));
    first_broker.await.expect("first broker task panicked");

    client
        .disconnect_mqtt()
        .await
        .expect("disconnect_mqtt is infallible");

    // The disconnect's cause may be the very event that lost homing, and firmware re-sends
    // only *changed* fields — so a flag from the previous connection is not evidence about
    // this machine any more, and would never self-correct if it happens not to change.
    assert_eq!(
        client.is_all_axes_homed(),
        None,
        "a home_flag observed on a previous connection must not read as a confident Some"
    );
    assert_eq!(client.is_axis_homed('x'), None);

    // ...but the mains region is a fixed property of the physical printer, so it deliberately
    // does NOT share the homing accessors' fate. A "just clear the whole cache" refactor would
    // reset this to None and silently drop the bed ceiling to the conservative 110 °C clamp.
    assert_eq!(
        client.is_220v_power(),
        Some(true),
        "mains wiring cannot change across a reconnect to the same unit"
    );

    let (client_stream_2, mut server_stream_2) = tokio::io::duplex(8192);
    let second_broker = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_2).await;
        send_publish_payload(
            &mut server_stream_2,
            &topic,
            4401,
            format!(r#"{{"print":{{"home_flag":{HOME_FLAG_XYZ_HOMED_220V}}}}}"#).as_bytes(),
        )
        .await;
        read_puback(&mut server_stream_2).await;
    });

    let reconnected = connect_test_mqtt(TokioIo(client_stream_2), SERIAL, PrinterModel::P1S).await;
    client.attach_mqtt(reconnected);
    assert_eq!(
        client.is_all_axes_homed(),
        None,
        "re-attaching must not resurrect the pre-disconnect flag"
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse the post-reconnect home_flag report");
    assert_eq!(
        client.is_all_axes_homed(),
        Some(true),
        "a flag observed on the current connection must read normally again"
    );
    second_broker.await.expect("second broker task panicked");
}

#[tokio::test]
async fn test_lazy_connect_publishes_pushall_before_the_callers_own_command() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Firmware broadcasts carry only changed fields, so a connection that never asks for a
        // full state dump may never see an unchanged value at all. Every reference client
        // (BambuStudio, ha-bambulab, bambuddy) requests one from its own connect handler.
        let connect_frame = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            connect_frame["pushing"]["command"], "pushall",
            "connection establishment must request a full state dump first"
        );

        let caller_frame = read_publish_payload(&mut server_stream).await;
        assert_eq!(caller_frame["print"]["command"], "gcode_line");
    });

    let factory = MockDataStreamFactory::new(Arc::new(Mutex::new(Some(TokioIo(client_stream)))));
    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity {
            ip: "127.0.0.1".to_string(),
            serial: SERIAL.to_string(),
            access_code: "12345678".to_string(),
            model: PrinterModel::P1S,
        },
    );

    // Lazy connect: `home_axes` dials through `ensure_mqtt()`, which is where the pushall
    // belongs — the reconnect path in the bug report goes through it, not through an explicit
    // `connect_mqtt()`.
    client.home_axes(false).await.expect("G28 homing failed");

    broker_task.await.expect("Broker task panicked");
}
