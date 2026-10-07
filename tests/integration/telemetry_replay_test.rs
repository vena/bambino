//! # End-to-End Telemetry Accessor Replay
//!
//! Replays a real P1S wire capture through the actual
//! stateful `PrinterClient` telemetry pipeline (MQTT framing -> `poll_telemetry()` ->
//! `update_telemetry_cache()`), one message at a time: the numeric accessors are bounds-checked
//! after each poll, and the status sequence and final cached state are asserted against what the
//! capture contains. Every other telemetry test drives a single hand-written or
//! single-real-message fixture; this is the only one that replays a full sequence through
//! the real cache the way a live `PrinterClient` session does, so a bug that only manifests
//! after N messages of accumulated state has coverage.
//!
//! Chose the existing mock-MQTT-broker harness (already proven by
//! `mqtt_test.rs::test_mqtt_client_lifecycle_and_telemetry` and the `client_*_test.rs` files'
//! `PrinterClient::from_mqtt` setup in `common/client.rs`) over refactoring
//! `update_telemetry_cache` to take `&mut TelemetryCache` explicitly: the gap-finding sweep
//! that preceded this test (upstream parsers cross-referenced against captures) confirmed only
//! one new bug (not a merge-logic shape) and left five `needs-verification`, well under the
//! three-plus-instances threshold this crate's quirks-engine precedent uses to justify a
//! shared-strategy refactor.

use bambino::client::{FanTarget, HeaterTemps, NozzleTemps, PrintStatus, TelemetryEvent};
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use bambino::mqtt::report_topic;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;

use crate::common::client::{SERIAL, TestClient, with_broker};
use crate::common::mock_mqtt::{handle_mqtt_handshake, read_puback, send_publish_payload};

/// Generously-wide plausibility bound for any single-value temperature accessor here, in °C.
/// Not a precision spec — a sanity net catching a broken composite-temperature unpack (which
/// tends to produce values in the tens of thousands, not merely "a bit off").
const PLAUSIBLE_MAX_TEMP_C: u16 = 500;

/// Connects a `model` client to a broker that publishes each of `payloads` in order, waiting for each PUBACK.
async fn replay_client(
    model: PrinterModel,
    payloads: Vec<Vec<u8>>,
) -> (TestClient<TokioIo<DuplexStream>>, JoinHandle<()>) {
    with_broker(SERIAL, model, |mut server| async move {
        handle_mqtt_handshake(&mut server).await;
        let topic = report_topic(SERIAL);
        for (i, payload) in payloads.iter().enumerate() {
            send_publish_payload(&mut server, &topic, 2000u16.wrapping_add(i as u16), payload)
                .await;
            read_puback(&mut server).await;
        }
    })
    .await
}

#[tokio::test]
async fn test_p1s_print_sequence_full_replay_accessors_stay_sane() {
    let capture = include_str!("../mocks/P1S_print_sequence.ndjson");
    let lines: Vec<&str> = capture.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        !lines.is_empty(),
        "capture fixture is empty — nothing to replay"
    );

    let (mut client, broker_task) = replay_client(
        PrinterModel::P1S,
        lines.iter().map(|l| l.as_bytes().to_vec()).collect(),
    )
    .await;

    // P1S has no chamber sensor, so this accessor is None for the whole replay; the chamber
    // decode is covered by the X1C test below.
    assert!(client.chamber_temperature().is_none());

    let mut reports_parsed = 0usize;
    let mut statuses: Vec<PrintStatus> = Vec::new();

    for i in 0..lines.len() {
        let event = client
            .poll_telemetry()
            .await
            .unwrap_or_else(|e| panic!("poll_telemetry failed at message {i}: {e:?}"));

        // Without this the whole test is vacuous: both a command-echo false positive and an
        // outright deserialization failure yield `Ok(TelemetryEvent::Unknown(..))` rather than
        // panicking, and every assertion below either reads a default/zero value or sits inside
        // an `if let Some(..)` that is trivially satisfied when the cache never updates.
        //
        // The capture interleaves genuine state reports with command echoes (`project_file`
        // and friends), and `Unknown` is the correct outcome for an echo. So the per-line
        // expectation is derived from the fixture itself, and the totals are compared after
        // the loop.
        if matches!(event, TelemetryEvent::Report(..)) {
            reports_parsed += 1;
        }

        if let Some(status) = client.print_status()
            && statuses.last() != Some(&status)
        {
            statuses.push(status);
        }

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

        if let Some(bed) = client.bed_temperatures() {
            assert!(
                bed.actual < PLAUSIBLE_MAX_TEMP_C && bed.target < PLAUSIBLE_MAX_TEMP_C,
                "bed_temperatures implausible at message {i}: {bed:?}"
            );
        }

        for NozzleTemps { id, actual, target } in client.nozzle_temperatures() {
            assert!(
                actual < PLAUSIBLE_MAX_TEMP_C && target < PLAUSIBLE_MAX_TEMP_C,
                "nozzle_temperatures[{id}] implausible at message {i}: ({actual}, {target})"
            );
        }

        for pct in [
            client.fan_speed(FanTarget::PartCooling),
            client.fan_speed(FanTarget::AuxiliaryLeft),
            client.fan_speed(FanTarget::ChamberExhaust),
            client.heatbreak_fan_speed(),
            client.fan_speed(FanTarget::AuxiliaryLeft2),
        ]
        .into_iter()
        .flatten()
        {
            assert!(pct <= 100, "fan speed implausible at message {i}: {pct}");
        }
    }

    // A line is a command echo (legitimately `Unknown`) if its `print` object carries a
    // `command` field naming something other than a status push. Everything else in this
    // capture is a state report and must have parsed as one.
    let expected_reports = lines
        .iter()
        .filter(|line| {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                return false;
            };
            !matches!(
                v.get("print").and_then(|p| p.get("command")).and_then(|c| c.as_str()),
                Some(cmd) if cmd != "push_status"
            )
        })
        .count();
    assert!(
        reports_parsed > 0,
        "no replayed message parsed as a Report at all"
    );
    assert_eq!(
        reports_parsed, expected_reports,
        "every non-command-echo line should parse as a Report"
    );

    // The capture is one whole job: the previous job's FINISH, back to IDLE, then this job.
    assert_eq!(
        statuses,
        [
            PrintStatus::Finished,
            PrintStatus::Idle,
            PrintStatus::Preparing,
            PrintStatus::Running,
            PrintStatus::Finished,
        ]
    );

    // State the capture leaves behind: a healthy single-nozzle P1S on Wi-Fi with one AMS.
    assert_eq!(client.active_fault(), None);
    assert!(client.hms().is_some_and(<[_]>::is_empty));
    assert!(client.active_hms_alerts().is_empty());
    assert_eq!(client.ams().map(|ams| ams.ams.len()), Some(1));
    assert!(client.vt_tray().is_some());
    assert!(client.vir_slot().is_none());
    assert!(client.wifi_signal().is_some());
    assert!(!client.is_ethernet_active_via_wifi_signal());

    drop(client);
    broker_task.await.expect("mock broker task panicked");
}

#[tokio::test]
async fn test_x1c_chamber_temperature_decode() {
    let (mut client, broker_task) = replay_client(
        PrinterModel::X1C,
        vec![
            // Direct temperature (≤ 500): target assumed 0°C.
            br#"{"print":{"chamber_temper":35.5}}"#.to_vec(),
            // Composite-packed temperature: upper 16 bits = target, lower 16 bits = actual.
            br#"{"print":{"chamber_temper":65571.0}}"#.to_vec(),
        ],
    )
    .await;

    // Chamber-equipped model: None before any chamber_temper is observed.
    assert_eq!(client.chamber_temperature(), None);

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse the direct-temperature report");
    // 35.5 rounds to 36 (#461).
    assert_eq!(
        client.chamber_temperature(),
        Some(HeaterTemps {
            actual: 36,
            target: 0
        })
    );

    client
        .poll_telemetry()
        .await
        .expect("poll_telemetry should parse the composite-temperature report");
    assert_eq!(
        client.chamber_temperature(),
        Some(HeaterTemps {
            actual: 35,
            target: 1
        })
    );

    drop(client);
    broker_task.await.expect("mock broker task panicked");
}
