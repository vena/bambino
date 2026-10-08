//! # Client Coordinator — Telemetry Cache Round-Trip Tests
//!
//! Split from the "Command-response round-trip tests" section of the former
//! `client_test.rs` (see issue #35).

use bambino::client::HeaterTemps;
use bambino::client::{
    AirPurificationMode, DoorOpenCheck, IdleHeatingProtection, NozzleBlobDetectMode,
    XcamHaltSensitivity, XcamModule,
};
use bambino::client::{PrintProgress, PrintSpeed, PrintStatus, PrinterClient, TelemetryEvent};
use bambino::diagnostics::DecodedPrintError;
use bambino::error::Error;
use bambino::models::PrinterModel;
use bambino::quirks::Support;

use crate::common::client::{
    SERIAL, X1_SERIAL, connect_idle_client, connect_test_mqtt, spawn_broker, test_identity,
    with_broker,
};
use crate::common::io::{DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_mqtt::{ReportPublisher, handle_mqtt_handshake, read_publish_payload};

#[tokio::test]
async fn test_print_status_cache_from_telemetry() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"gcode_state":"RUNNING"}}"#,
                )
                .await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"gcode_state":"BOGUS_STATE"}}"#,
                )
                .await;
        })
        .await;

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

// is_door_open / active_fault telemetry accessors

#[tokio::test]
async fn test_door_open_none_on_sensorless_model() {
    // P1S has no door sensor.
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // Bit 23 set — would read as "open" on a sensor-equipped model.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":8388608}}"#)
                .await;
        })
        .await;

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
    // X1C has a door sensor, read from home_flag bit 23.
    let (mut client, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1C,
        |mut server_stream| async move {
            let mut reports = ReportPublisher::new(X1_SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // Bit 23 set: door open.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":8388608}}"#)
                .await;

            // Bit 23 clear: door closed.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":0}}"#)
                .await;
        },
    )
    .await;

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

    let (mut client, broker_task) = with_broker(
        X1_SERIAL,
        PrinterModel::X1C,
        |mut server_stream| async move {
            let mut reports = ReportPublisher::new(X1_SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // Bit 23 set: door open.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":8388608}}"#)
                .await;

            // A print-carrying message with no home_flag at all (e.g. an incremental update
            // only touching an unrelated field) must not reset the cached door state.
            reports
                .publish(&mut server_stream, br#"{"print":{"mc_percent":42}}"#)
                .await;
        },
    )
    .await;

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
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            // print_error = 83902476 decimal -> 0x0500400C, a genuine fault.
            reports
                .publish(&mut server_stream, br#"{"print":{"print_error":83902476}}"#)
                .await;

            // Register reads back to 0 — no fault.
            reports
                .publish(&mut server_stream, br#"{"print":{"print_error":0}}"#)
                .await;
        })
        .await;

    // No telemetry observed yet.
    assert_eq!(client.active_fault(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first print_error report");
    assert_eq!(
        client.active_fault(),
        Some(DecodedPrintError {
            code: 0x0500_400C,
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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"mc_percent":42,"mc_remaining_time":30,"layer_num":5,"total_layer_num":100}}"#,
                )
                .await;

            // Only `mc_percent` present this time — the other three fields must stay cached.
            reports.publish(&mut server_stream, br#"{"print":{"mc_percent":50}}"#).await;
            })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"layer_num":5,"total_layer_num":100}}"#,
                )
                .await;

            // End-of-print: firmware resets total_layer_num to 0. That is not a real layer
            // count, and must not overwrite the cached 100.
            reports
                .publish(&mut server_stream, br#"{"print":{"total_layer_num":0}}"#)
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"bed_temper":60.0,"bed_target_temper":65.0}}"#,
                )
                .await;

            // Only `bed_temper` present this time — target must stay cached at 65.
            reports
                .publish(&mut server_stream, br#"{"print":{"bed_temper":61.0}}"#)
                .await;
        })
        .await;

    assert_eq!(client.bed_temperatures(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse first bed temperature report");
    assert_eq!(
        client.bed_temperatures(),
        Some(HeaterTemps {
            actual: 60,
            target: 65
        })
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse second bed temperature report");
    assert_eq!(
        client.bed_temperatures(),
        Some(HeaterTemps {
            actual: 61,
            target: 65
        })
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_ams_cache_from_telemetry() {
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"ams":{"ams_exist_bits":"1","tray_exist_bits":"3"}}}"#,
                )
                .await;

            // A report with no `ams` key at all must leave the cache untouched.
            reports
                .publish(&mut server_stream, br#"{"print":{}}"#)
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"vt_tray":{"id":"254"},"vir_slot":[{"id":"0"},{"id":"1"}]}}"#,
                )
                .await;
        })
        .await;

    assert!(client.vt_tray().is_none());
    assert!(client.vir_slot().is_none());

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse vt_tray/vir_slot report");
    assert_eq!(client.vt_tray().map(|t| t.id.as_str()), Some("254"));
    assert_eq!(
        client
            .vir_slot()
            .map(|slots| slots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()),
        Some(vec!["0", "1"])
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_vt_tray_and_vir_slot_partial_push_preserves_cached_fields() {
    // issue #43: a partial id-only push must not wholesale-clobber prior tray_type/tray_color/
    // etc., and a vir_slot push carrying only one extruder's entry must not drop the other.
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Seed full fields for vt_tray and both vir_slot entries.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{
                "vt_tray":{"id":"254","tray_type":"PLA","tray_color":"FF0000FF","remain":80},
                "vir_slot":[
                    {"id":"0","tray_type":"PLA","remain":80},
                    {"id":"1","tray_type":"PETG","remain":60}
                ]
            }}"#,
                )
                .await;

            // Follow-up: id-only vt_tray, and a vir_slot array carrying only id "0".
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"vt_tray":{"id":"254"},"vir_slot":[{"id":"0","remain":70}]}}"#,
                )
                .await;
        })
        .await;

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
    let slot0 = vir_slot.iter().find(|s| s.id == "0").unwrap();
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
    let slot1 = vir_slot.iter().find(|s| s.id == "1").unwrap();
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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"nozzle_temper":200.0,"nozzle_target_temper":210.0}}"#,
                )
                .await;
        })
        .await;

    assert_eq!(
        client
            .nozzle_temperatures()
            .iter()
            .map(|t| (t.id, t.actual, t.target))
            .collect::<Vec<_>>(),
        vec![]
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse nozzle temperature report");
    assert_eq!(
        client
            .nozzle_temperatures()
            .iter()
            .map(|t| (t.id, t.actual, t.target))
            .collect::<Vec<_>>(),
        vec![(0, 200, 210)]
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_printing_tray_global_id_prefers_snow_field() {
    // printing_tray_global_id() decodes device.extruder.info[active].snow directly,
    // no ams_extruder_map needed.
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Extruder 0 (right/main): state selects active_extruder_index()=1 (left), snow
            // routes it to ams_id=2, slot_id=1 (raw = (2<<8)|1 = 513).
            reports
                .publish(
                    &mut server_stream,
                    br#"{"device":{"extruder":{"info":[
                {"id":0,"snow":65535},
                {"id":1,"snow":513}
            ],"state":18}}}"#,
                )
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::A2L, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Same extruder shape as the sibling test above (state 18 selects index 1), with snow
            // routing to the AMS Lite's physical id 16, slot 2: raw = (16 << 8) | 2 = 4098.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"device":{"extruder":{"info":[
                {"id":0,"snow":65535},
                {"id":1,"snow":4098}
            ],"state":18}}}"#,
                )
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // IDEX hardware present (`device.nozzle.info` has 2 entries) but no live
            // `device.extruder.info` temps yet — the flat-field routing quirk applies:
            // nozzle_temper (100) is nozzle 1 (left) actual, nozzle_target_temper (220) is
            // nozzle 0 (right) target.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"device":{"nozzle":{"info":[{"id":0},{"id":1}]}},"nozzle_temper":100.0,"nozzle_target_temper":220.0}}"#,
                )
                .await;
            })
        .await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse IDEX nozzle report");
    assert_eq!(
        client
            .nozzle_temperatures()
            .iter()
            .map(|t| (t.id, t.actual, t.target))
            .collect::<Vec<_>>(),
        vec![(0, 0, 220), (1, 100, 0)]
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_chamber_temperature_cache() {
    let mut reports = ReportPublisher::new(SERIAL);

    // P1S has no chamber heater/sensor — always None regardless of telemetry.
    let (mut sensorless_client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // Composite-packed: (60 << 16) | 50 = actual 50, target 60.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"chamber_temper":3932210.0}}"#,
                )
                .await;
        })
        .await;
    assert_eq!(sensorless_client.chamber_temperature(), None);
    sensorless_client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse chamber temperature report");
    assert_eq!(sensorless_client.chamber_temperature(), None);

    let mut reports2 = ReportPublisher::new(SERIAL);
    let (mut heated_client, broker_task2) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream2| async move {
            handle_mqtt_handshake(&mut server_stream2).await;
            reports2
                .publish(
                    &mut server_stream2,
                    br#"{"print":{"chamber_temper":3932210.0}}"#,
                )
                .await;
        })
        .await;

    assert_eq!(heated_client.chamber_temperature(), None);
    heated_client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse chamber temperature report");
    assert_eq!(
        heated_client.chamber_temperature(),
        Some(HeaterTemps {
            actual: 50,
            target: 60
        })
    );

    broker_task.await.expect("Broker task panicked");
    broker_task2.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_hms_cache_and_active_alerts() {
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // One genuine fault (attr 0x05000100 / code 0x0001400C) and one cancellation
            // echo (attr 0x05000100 / code 0x0001400E) that must be filtered out.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"hms":[{"attr":83886336,"code":81932},{"attr":83886336,"code":81934}]}}"#,
                )
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // A tray in state 9 (empty) that still carries stale material fields from a
            // previously loaded spool — the exact case `ams()`'s doc comment says stays raw
            // and `sanitized_ams()` scrubs.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"ams":{"ams":[{"id":"0","temp":"25.0","humidity":"3","tray":[{"id":"0","state":9,"tray_type":"PLA","tray_color":"FF0000FF","remain":42}]}]}}}"#,
                )
                .await;
            })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    // The four flat fan keys are step-encoded (0-15) on every model, including P2S/X2D — see
    // test_fan_speed_cache_from_telemetry_x2d_step_encoded below.
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"cooling_fan_speed":"15","big_fan1_speed":"8","big_fan2_speed":"0","heatbreak_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":75}]}}}}"#,
                )
                .await;
            })
        .await;

    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::PartCooling),
        None
    );
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft2),
        None
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::PartCooling),
        Some(100)
    );
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft),
        Some(53)
    );
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::ChamberExhaust),
        Some(0)
    );
    assert_eq!(client.heatbreak_fan_speed(), Some(100));
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft2),
        Some(75)
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_fan_speed_cache_from_telemetry_x2d_step_encoded() {
    let mut reports = ReportPublisher::new(SERIAL);

    // Regression for #38: X2D/P2S previously decoded the four flat fan keys as
    // already-percentage (ModelQuirks::reports_auxiliary_fan_percentage), reading ~6.7x too low.
    // They must step-decode identically to every other model — only the id-160 airduct part
    // (auxiliary_left2_fan_speed) is a true wire percentage.
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::X2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"cooling_fan_speed":"15","big_fan1_speed":"8","big_fan2_speed":"0","heatbreak_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":75}]}}}}"#,
                )
                .await;
            })
        .await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::PartCooling),
        Some(100)
    );
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft),
        Some(53)
    );
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::ChamberExhaust),
        Some(0)
    );
    assert_eq!(client.heatbreak_fan_speed(), Some(100));
    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft2),
        Some(75)
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_auxiliary_left2_fan_negative_state_is_none() {
    let mut reports = ReportPublisher::new(SERIAL);

    // A negative `state` is a firmware sentinel for "off/unknown"; it must report
    // None, not be masked into 100% by `& 0xFF`.
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::X2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"cooling_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":-1}]}}}}"#,
                )
                .await;
            })
        .await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft2),
        None
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_auxiliary_left2_fan_packed_state_decodes_low_byte() {
    let mut reports = ReportPublisher::new(SERIAL);

    // The low-8-bit mask must still apply to a non-negative state (BambuStudio's
    // DevFan::ParseV3_0 get_flag_bits(state, 0, 8); bambuddy's identical `& 0xFF`).
    // Without it a packed value clamps to 100 instead of decoding to its real percentage.
    // The negative-sentinel guard above and this mask are both required and must stay in
    // that order — fixing either alone reintroduced the other's bug once already.
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::X2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            // 306 == 0x132: percentage 50 in the low byte, a flag bit set above it.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"cooling_fan_speed":"15","device":{"airduct":{"parts":[{"id":160,"state":306}]}}}}"#,
                )
                .await;
            })
        .await;

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse fan speed report");

    assert_eq!(
        client.fan_speed(bambino::client::FanTarget::AuxiliaryLeft2),
        Some(50)
    );

    broker_task.await.expect("Broker task panicked");
}

#[tokio::test]
async fn test_print_speed_cache_from_telemetry() {
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"spd_lvl":3,"spd_mag":124}}"#,
                )
                .await;
        })
        .await;

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
    let mut reports = ReportPublisher::new(SERIAL);

    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(&mut server_stream, br#"{"print":{"wifi_signal":"-52dBm"}}"#)
                .await;

            reports
                .publish(&mut server_stream, br#"{"print":{"wifi_signal":"-90dBm"}}"#)
                .await;
        })
        .await;

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
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
                let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;

            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"push_status","home_flag":7,"gcode_state":"RUNNING"}}"#,
                )
                .await;

            // An `extrusion_cali_get` reply carrying fields that overlap genuine telemetry. If the
            // gate let this through, it would clobber both cached values with the echo's contents.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"extrusion_cali_get","sequence_id":"10001","home_flag":0,"gcode_state":"FAILED"}}"#,
                )
                .await;
            })
        .await;

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

/// A full status report: `home_flag` plus enough filler keys to clear `FULL_REPORT_MIN_KEYS`.
fn full_report(home_flag: u32) -> String {
    let filler: String = (0..31).map(|i| format!(r#","filler{i}":0"#)).collect();
    format!(r#"{{"print":{{"home_flag":{home_flag}{filler}}}}}"#)
}

/// `print_option` settings read `home_flag` until a `cfg` arrives, then `cfg` alone; once `cfg`
/// has been seen, a heartbeat's partial `home_flag` changes nothing; a reported support bit lifts
/// the setter gate past the model rule.
#[tokio::test]
async fn test_print_option_settings_read_cfg_over_home_flag() {
    // home_flag: Filament Backup on (10), prompt sound on (17) and supported (18), tangle
    // detection supported (19) and on (20), air-print detection on (28).
    const HOME_FLAG: u32 = 1 << 10 | 1 << 17 | 1 << 18 | 1 << 19 | 1 << 20 | 1 << 28;
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;
            reports
                .publish(&mut server_stream, full_report(HOME_FLAG).as_bytes())
                .await;
            // cfg: auto-recovery on (16), smart blob detection auto (43-44 = 2), air
            // purification outside (36-37 = 2); prompt sound, backup and tangle off.
            reports
                .publish(&mut server_stream, br#"{"print":{"cfg":"0x102000010000"}}"#)
                .await;
            // A heartbeat from a printer that sends cfg: too few keys to be a full report, and
            // its home_flag is partial.
            reports
                .publish(&mut server_stream, br#"{"print":{"home_flag":0}}"#)
                .await;
            let sound = read_publish_payload(&mut server_stream).await;
            assert_eq!(sound["print"]["command"], "print_option");
            assert_eq!(sound["print"]["sound_enable"], false);
        })
        .await;
    assert_eq!(client.prompt_sound_enabled(), None);
    assert_eq!(
        client.capabilities().prompt_sound_support(),
        Support::Inferred(false)
    );

    client.poll_telemetry().await.expect("home_flag report");
    assert_eq!(client.prompt_sound_enabled(), Some(true));
    assert_eq!(client.filament_backup_enabled(), Some(true));
    assert_eq!(client.filament_tangle_detect_enabled(), Some(true));
    assert_eq!(client.auto_recovery_enabled(), Some(false));
    assert_eq!(client.air_print_detect_enabled(), Some(true));
    assert_eq!(client.smart_nozzle_blob_detect_mode(), None);
    assert_eq!(client.air_purification_mode(), None);
    let caps = client.capabilities();
    assert_eq!(caps.prompt_sound_support(), Support::Reported(true));
    assert_eq!(
        caps.filament_tangle_detect_support(),
        Support::Reported(true)
    );
    assert_eq!(caps.air_print_detect_support(), Support::Reported(false));
    assert_eq!(caps.air_purification_support(), Support::Assumed(false));
    assert!(matches!(
        client
            .set_air_purification(AirPurificationMode::Inside)
            .await,
        Err(Error::ModelMismatch(_))
    ));

    client.poll_telemetry().await.expect("cfg report");
    assert_eq!(client.prompt_sound_enabled(), Some(false));
    assert_eq!(client.filament_backup_enabled(), Some(false));
    assert_eq!(client.filament_tangle_detect_enabled(), Some(false));
    assert_eq!(client.auto_recovery_enabled(), Some(true));
    // cfg doesn't carry air-print detection, so home_flag still answers it.
    assert_eq!(client.air_print_detect_enabled(), Some(true));
    assert_eq!(
        client.smart_nozzle_blob_detect_mode(),
        Some(NozzleBlobDetectMode::Auto)
    );
    assert_eq!(
        client.air_purification_mode(),
        Some(AirPurificationMode::Outside)
    );

    client.poll_telemetry().await.expect("heartbeat");
    assert_eq!(
        client.air_print_detect_enabled(),
        Some(true),
        "once cfg has been seen, a heartbeat's partial home_flag must not answer"
    );
    assert_eq!(
        client.capabilities().prompt_sound_support(),
        Support::Reported(true)
    );

    // The P1S model rule says no prompt sound, but this printer reported support.
    client
        .set_prompt_sound(false)
        .await
        .expect("reported support must lift the model rule");
    broker_task.await.expect("Broker task panicked");
}

/// On a printer that sends no `cfg` (P1, A1), a small diff frame carries a complete `home_flag`,
/// so it updates the settings getters without waiting for a full report. The frame shape follows
/// `tests/mocks/P1S_print_sequence.ndjson`.
#[tokio::test]
async fn test_p1s_diff_frame_home_flag_updates_settings() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;
            reports
                .publish(&mut server_stream, full_report(0).as_bytes())
                .await;
            // Auto-recovery (bit 4) switched on at the printer's screen.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"push_status","msg":1,"sequence_id":"2","home_flag":16,"nozzle_temper":25.0}}"#,
                )
                .await;
        })
        .await;
    client.poll_telemetry().await.expect("full report");
    assert_eq!(client.auto_recovery_enabled(), Some(false));
    client.poll_telemetry().await.expect("diff frame");
    assert_eq!(client.auto_recovery_enabled(), Some(true));
    broker_task.await.expect("Broker task panicked");
}

/// An accepted `print_option` reply sets the getter at once for a supported setting, until the
/// next status frame carrying settings replaces it (#615). Unsupported settings and refused
/// replies are ignored, whichever client sent the command.
#[tokio::test]
async fn test_print_option_reply_feeds_getters_until_next_status_frame() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            let mut reports = ReportPublisher::new(SERIAL);
            handle_mqtt_handshake(&mut server_stream).await;
            // Full report: auto-recovery (bit 4) off, prompt sound unsupported (bit 18 clear).
            reports
                .publish(&mut server_stream, full_report(0).as_bytes())
                .await;
            let sent = read_publish_payload(&mut server_stream).await;
            let seq = sent["print"]["sequence_id"].as_str().unwrap().to_owned();
            // Our own command's reply, echoing the setting as BambuStudio's command builds it.
            reports
                .publish(
                    &mut server_stream,
                    format!(
                        r#"{{"print":{{"command":"print_option","sequence_id":"{seq}","option":1,"auto_recovery":true,"result":"success","reason":"success"}}}}"#
                    )
                    .as_bytes(),
                )
                .await;
            // Another client's replies: an unsupported setting, and a refused one.
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"print_option","sequence_id":"7","sound_enable":true,"result":"success"}}"#,
                )
                .await;
            reports
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"print_option","sequence_id":"8","auto_switch_filament":true,"result":"fail"}}"#,
                )
                .await;
            // A stale full report: the printer hasn't applied the change yet.
            reports
                .publish(&mut server_stream, full_report(0).as_bytes())
                .await;
        })
        .await;

    client.poll_telemetry().await.expect("full report");
    assert_eq!(client.auto_recovery_enabled(), Some(false));
    client
        .set_auto_recovery(true)
        .await
        .expect("auto-recovery send");

    for _ in 0..3 {
        client.poll_telemetry().await.expect("reply");
    }
    assert_eq!(client.auto_recovery_enabled(), Some(true));
    assert_eq!(
        client.prompt_sound_enabled(),
        Some(false),
        "a reply for a setting the P1S doesn't support must not change the getter"
    );
    assert_eq!(
        client.filament_backup_enabled(),
        Some(false),
        "a refused reply must not change the getter"
    );

    client.poll_telemetry().await.expect("stale full report");
    assert_eq!(
        client.auto_recovery_enabled(),
        Some(false),
        "the next status frame replaces the reply's value, stale or not"
    );
    broker_task.await.expect("Broker task panicked");
}

/// Door-open check, idle heating protection and stored sent files read back from `cfg`, and
/// their setters reach the wire under their own wrappers.
#[tokio::test]
async fn test_safety_and_storage_settings_read_cfg_and_send() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            // cfg: store sent files (19), door check pause (20-21 = 2), idle heating protection
            // unavailable (32-33 = 2). fun: door check (12) and idle heating protection (62).
            ReportPublisher::new(SERIAL)
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"push_status","cfg":"200280000","fun":"4000000000001000"}}"#,
                )
                .await;
            let door = read_publish_payload(&mut server_stream).await;
            assert_eq!(door["system"]["command"], "set_door_stat");
            assert_eq!(door["system"]["config"], 1);
            let idle = read_publish_payload(&mut server_stream).await;
            assert_eq!(
                idle["print"]["command"],
                "set_against_continued_heating_mode"
            );
            assert_eq!(idle["print"]["enable"], true);
            let store = read_publish_payload(&mut server_stream).await;
            assert_eq!(store["system"]["command"], "print_cache_set");
            assert_eq!(store["system"]["config"], false);
        })
        .await;
    assert_eq!(client.door_open_check(), None);
    assert!(matches!(
        client.set_idle_heating_protection(true).await,
        Err(Error::ModelMismatch(_))
    ));

    client.poll_telemetry().await.expect("cfg report");
    assert_eq!(client.store_sent_files_enabled(), Some(true));
    assert_eq!(client.door_open_check(), Some(DoorOpenCheck::PausePrint));
    assert_eq!(
        client.idle_heating_protection(),
        Some(IdleHeatingProtection::Unavailable)
    );
    assert_eq!(
        client.capabilities().idle_heating_protection_support(),
        Support::Reported(true)
    );

    client
        .set_door_open_check(DoorOpenCheck::Warn)
        .await
        .expect("door check send");
    client
        .set_idle_heating_protection(true)
        .await
        .expect("idle heating protection send");
    client
        .set_store_sent_files(false)
        .await
        .expect("store sent files send");
    broker_task.await.expect("Broker task panicked");
}

/// A P1S has no door sensor and no stored-files profile entry, so both setters refuse.
#[tokio::test]
async fn test_safety_and_storage_setters_refuse_on_p1s() {
    let (mut client, broker_task) = connect_idle_client(PrinterModel::P1S).await;
    assert!(matches!(
        client.set_door_open_check(DoorOpenCheck::Warn).await,
        Err(Error::ModelMismatch(_))
    ));
    assert!(matches!(
        client.set_store_sent_files(true).await,
        Err(Error::ModelMismatch(_))
    ));
    broker_task.await.expect("Broker task panicked");
}

/// `xcam_control_set` is gated per detector, rejects a sensitivity a module takes none of, and
/// reaches the wire under the `xcam` wrapper; first-layer inspection reads `cfg` bit 12.
#[tokio::test]
async fn test_xcam_detector_setter_and_first_layer_getter() {
    let (mut client, broker_task) =
        with_broker(SERIAL, PrinterModel::H2D, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            // fun bit 42 (spaghetti) set; cfg bit 12 (first-layer inspection) set.
            ReportPublisher::new(SERIAL)
                .publish(
                    &mut server_stream,
                    br#"{"print":{"command":"push_status","cfg":"1000","fun":"40000000000"}}"#,
                )
                .await;
            let set = read_publish_payload(&mut server_stream).await;
            assert_eq!(set["xcam"]["command"], "xcam_control_set");
            assert_eq!(set["xcam"]["module_name"], "spaghetti_detector");
            assert_eq!(set["xcam"]["control"], true);
            assert_eq!(set["xcam"]["halt_print_sensitivity"], "low");
        })
        .await;
    assert!(matches!(
        client
            .set_xcam_detector(XcamModule::SpaghettiDetector, true, None)
            .await,
        Err(Error::ModelMismatch(_))
    ));
    client.poll_telemetry().await.expect("report");
    assert_eq!(client.first_layer_inspection_enabled(), Some(true));
    assert!(matches!(
        client
            .set_xcam_detector(XcamModule::FodCheck, true, Some(XcamHaltSensitivity::High))
            .await,
        Err(Error::InvalidArgument(_))
    ));
    client
        .set_xcam_detector(
            XcamModule::SpaghettiDetector,
            true,
            Some(XcamHaltSensitivity::Low),
        )
        .await
        .expect("spaghetti detector send");
    broker_task.await.expect("Broker task panicked");
}

// Issue #254: connection-scoped telemetry must not survive an MQTT reconnect.

/// `home_flag` bits X|Y|Z set (0x07) plus the 220 V mains bit (0x08).
const HOME_FLAG_XYZ_HOMED_220V: u32 = 0x0F;

#[tokio::test]
async fn test_home_flag_goes_cold_across_reconnect_but_mains_region_persists() {
    let (mut client, first_broker) =
        with_broker(SERIAL, PrinterModel::P1S, |mut server_stream| async move {
            handle_mqtt_handshake(&mut server_stream).await;
            ReportPublisher::new(SERIAL)
                .publish(
                    &mut server_stream,
                    format!(r#"{{"print":{{"home_flag":{HOME_FLAG_XYZ_HOMED_220V}}}}}"#).as_bytes(),
                )
                .await;
        })
        .await;
    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse the home_flag report");
    assert_eq!(client.is_all_axes_homed(), Some(true));
    assert_eq!(client.is_axis_homed(bambino::client::Axis::X), Some(true));
    assert_eq!(client.is_220v_power(), Some(true));
    first_broker.await.expect("first broker task panicked");

    client.disconnect_mqtt().await;

    // The disconnect's cause may be the very event that lost homing, and firmware re-sends
    // only *changed* fields — so a flag from the previous connection is not evidence about
    // this machine any more, and would never self-correct if it happens not to change.
    assert_eq!(
        client.is_all_axes_homed(),
        None,
        "a home_flag observed on a previous connection must not read as a confident Some"
    );
    assert_eq!(client.is_axis_homed(bambino::client::Axis::X), None);

    // ...but the mains region is a fixed property of the physical printer, so it deliberately
    // does NOT share the homing accessors' fate. A "just clear the whole cache" refactor would
    // reset this to None and silently drop the bed ceiling to the conservative 110 °C clamp.
    assert_eq!(
        client.is_220v_power(),
        Some(true),
        "mains wiring cannot change across a reconnect to the same unit"
    );

    let (stream_2, second_broker) = spawn_broker(|mut server_stream_2| async move {
        handle_mqtt_handshake(&mut server_stream_2).await;
        // attach_mqtt runs the same connect-time pushall a dialled session gets (#346).
        let pushall = read_publish_payload(&mut server_stream_2).await;
        assert_eq!(pushall["pushing"]["command"], "pushall");
        ReportPublisher::new(SERIAL)
            .publish(
                &mut server_stream_2,
                format!(r#"{{"print":{{"home_flag":{HOME_FLAG_XYZ_HOMED_220V}}}}}"#).as_bytes(),
            )
            .await;
    });

    let reconnected = connect_test_mqtt(stream_2, SERIAL).await;
    client.attach_mqtt(reconnected).await;
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
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
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

    let (factory, _) = MockDataStreamFactory::with_stream(stream);
    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S));

    // Lazy connect: `home_axes` dials through `ensure_mqtt()`, which is where the pushall
    // belongs — the reconnect path in the bug report goes through it, not through an explicit
    // `connect_mqtt()`.
    client.home_all().await.expect("G28 homing failed");

    broker_task.await.expect("Broker task panicked");
}
