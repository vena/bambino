//! # End-to-End Telemetry Accessor Replay
//!
//! Phase 2 of `TELEMETRY_TEST_PLAN.md`: replays a real P1S wire capture through the actual
//! stateful `PrinterClient` telemetry pipeline (MQTT framing -> `poll_telemetry()` ->
//! `update_telemetry_cache()`), one message at a time, exercising every public telemetry
//! accessor after each poll. Every other telemetry test drives a single hand-written or
//! single-real-message fixture; this is the only one that replays a full sequence through
//! the real cache the way a live `PrinterClient` session does, so a bug that only manifests
//! after N messages of accumulated state has coverage.
//!
//! Chose the existing mock-MQTT-broker harness (already proven by
//! `mqtt_test.rs::test_mqtt_client_lifecycle_and_telemetry` and
//! `client_test.rs`'s `PrinterClient::from_mqtt` tests) over refactoring
//! `update_telemetry_cache` to take `&mut TelemetryCache` explicitly: this session's Phase 1
//! sweep confirmed only one new bug (not a merge-logic shape) and left five
//! `needs-verification`, well under the three-plus-instances threshold this crate's
//! quirks-engine precedent uses to justify a shared-strategy refactor.

mod common;

use bambino::client::PrinterClient;
use bambino::identity::PrinterIdentity;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use bambino::mqtt::MqttClient;

use common::mock_mqtt::{handle_mqtt_handshake, read_puback, send_publish_payload};

const SERIAL: &str = "01P000000000000";

/// Generously-wide plausibility bound for any single-value temperature accessor here, in °C.
/// Not a precision spec — a sanity net catching a broken composite-temperature unpack (which
/// tends to produce values in the tens of thousands, not merely "a bit off").
const PLAUSIBLE_MAX_TEMP_C: u16 = 500;

#[tokio::test]
async fn test_p1s_print_sequence_full_replay_accessors_stay_sane() {
    let capture = include_str!("mocks/P1S_print_sequence.ndjson");
    let lines: Vec<String> = capture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();
    assert!(
        !lines.is_empty(),
        "capture fixture is empty — nothing to replay"
    );

    let (client_stream, mut server_stream) = tokio::io::duplex(1 << 16);
    let topic = format!("device/{}/report", SERIAL);

    let broker_lines = lines.clone();
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        for (i, line) in broker_lines.iter().enumerate() {
            send_publish_payload(
                &mut server_stream,
                &topic,
                2000u16.wrapping_add(i as u16),
                line.as_bytes(),
            )
            .await;
            read_puback(&mut server_stream).await;
        }
    });

    let mqtt_client = MqttClient::connect(
        TokioIo(client_stream),
        &PrinterIdentity { ip: String::new(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
        .await
        .expect("MQTT connect handshake failed");
    let mut client = PrinterClient::from_mqtt(mqtt_client, PrinterModel::P1S);

    for (i, _line) in lines.iter().enumerate() {
        client
            .poll_telemetry()
            .await
            .unwrap_or_else(|e| panic!("poll_telemetry failed at message {i}: {e:?}"));

        // Every public telemetry accessor must be callable without panicking, and any
        // numeric value it returns must stay within a generous plausibility bound.
        let _ = client.print_status();
        let _ = client.is_door_open();
        let _ = client.active_fault();

        let progress = client.print_progress();
        if let Some(percent) = progress.percent {
            assert!(
                (-1..=100).contains(&percent),
                "mc_percent implausible at message {i}: {percent}"
            );
        }
        if let Some(layer) = progress.layer_num {
            assert!(layer >= 0, "layer_num implausible at message {i}: {layer}");
        }
        if let Some(total) = progress.total_layers {
            assert!(
                total >= 0,
                "total_layers implausible at message {i}: {total}"
            );
        }

        let (bed_actual, bed_target) = client.bed_temperatures();
        assert!(
            bed_actual < PLAUSIBLE_MAX_TEMP_C && bed_target < PLAUSIBLE_MAX_TEMP_C,
            "bed_temperatures implausible at message {i}: ({bed_actual}, {bed_target})"
        );

        for (id, actual, target) in client.nozzle_temperatures() {
            assert!(
                actual < PLAUSIBLE_MAX_TEMP_C && target < PLAUSIBLE_MAX_TEMP_C,
                "nozzle_temperatures[{id}] implausible at message {i}: ({actual}, {target})"
            );
        }

        if let Some((actual, target)) = client.chamber_temperature() {
            assert!(
                actual < PLAUSIBLE_MAX_TEMP_C && target < PLAUSIBLE_MAX_TEMP_C,
                "chamber_temperature implausible at message {i}: ({actual}, {target})"
            );
        }

        let _ = client.ams();
        let _ = client.vt_tray();
        let _ = client.vir_slot();
        let _ = client.hms();
        let _ = client.active_hms_alerts();

        for fan in [
            client.part_cooling_fan_speed(),
            client.auxiliary_left_fan_speed(),
            client.chamber_exhaust_fan_speed(),
            client.heatbreak_fan_speed(),
            client.auxiliary_left2_fan_speed(),
        ] {
            if let Some(pct) = fan {
                assert!(pct <= 100, "fan speed implausible at message {i}: {pct}");
            }
        }

        let _ = client.print_speed();
        let _ = client.print_speed_magnitude();
        let _ = client.wifi_signal();
        let _ = client.is_ethernet_active_via_wifi_signal();
    }

    drop(client);
    broker_task.await.expect("mock broker task panicked");
}
