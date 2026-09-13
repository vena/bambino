//! # Command Outcome Tests
//!
//! Every echoing command a `PrinterClient` publishes must end in exactly one
//! `CommandOutcome`, delivered through `poll_telemetry()` as `TelemetryEvent::Command` or
//! returned by `await_ack()` — never both, never neither (issues #282, #284, #285).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bambino::client::{AckExpectation, CommandOutcome, CommandRefusal, TelemetryEvent};
use bambino::error::Error;
use bambino::io::{TimerError, TimerProvider, TokioIo};
use bambino::models::PrinterModel;
use tokio::io::DuplexStream;

use crate::common::client::{SERIAL, connect_test_client};
use crate::common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

/// A real-clock timer whose `now_millis()` only moves when a test advances it, so a deadline
/// can be crossed without sleeping through it.
#[derive(Clone, Default)]
struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    fn advance_ms(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl TimerProvider for ManualClock {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        tokio::time::sleep(duration).await;
        Ok(())
    }

    fn now_millis(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn report_topic() -> String {
    format!("device/{SERIAL}/report")
}

/// Reads the client's next command and returns its `(wrapper, command, sequence_id)`.
async fn read_command(stream: &mut DuplexStream) -> (String, String, String) {
    let json = read_publish_payload(stream).await;
    let (wrapper, inner) = json.as_object().unwrap().iter().next().unwrap();
    (
        wrapper.clone(),
        inner["command"].as_str().unwrap().to_string(),
        inner["sequence_id"].as_str().unwrap().to_string(),
    )
}

async fn publish(stream: &mut DuplexStream, packet_id: u16, payload: &str) {
    send_publish_payload(stream, &report_topic(), packet_id, payload.as_bytes()).await;
    read_puback(stream).await;
}

#[tokio::test]
async fn test_echo_resolves_its_command_with_the_decoded_verdict() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let (_, command, seq) = read_command(&mut server_stream).await;
        publish(
            &mut server_stream,
            1,
            r#"{"print":{"command":"push_status","sequence_id":"58","home_flag":7}}"#,
        )
        .await;
        publish(
            &mut server_stream,
            2,
            &format!(r#"{{"print":{{"command":"{command}","sequence_id":"{seq}","result":"success","reason":"success"}}}}"#),
        )
        .await;

        // A system-wrapped refusal answers the second command.
        let (wrapper, command, seq) = read_command(&mut server_stream).await;
        assert_eq!(wrapper, "system");
        publish(
            &mut server_stream,
            3,
            &format!(r#"{{"system":{{"command":"{command}","sequence_id":"{seq}","result":"failed","reason":"mqtt message verify failed"}}}}"#),
        )
        .await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let gcode = client.send_gcode("G28").await.expect("send_gcode failed");
    assert!(matches!(
        client.poll_telemetry().await.unwrap(),
        TelemetryEvent::Report(..)
    ));
    match client.poll_telemetry().await.unwrap() {
        TelemetryEvent::Command(resolution, raw) => {
            assert_eq!(resolution.handle, gcode);
            assert_eq!(resolution.outcome, CommandOutcome::Accepted);
            assert!(raw.is_some(), "an echo-decided outcome carries its message");
        }
        other => panic!("expected the gcode_line outcome, got {other:?}"),
    }

    let led = client
        .set_led("chamber_light", true)
        .await
        .expect("set_led failed");
    match client.poll_telemetry().await.unwrap() {
        TelemetryEvent::Command(resolution, _) => {
            assert_eq!(resolution.handle, led);
            assert_eq!(
                resolution.outcome,
                CommandOutcome::Refused(CommandRefusal {
                    result: Some("failed".into()),
                    reason: Some("mqtt message verify failed".into()),
                    err_code: None,
                    errno: None,
                })
            );
        }
        other => panic!("expected the ledctrl outcome, got {other:?}"),
    }

    broker_task.await.expect("broker task panicked");
}

/// Issue #282: a `system`-wrapped echo deserializes as an empty `TelemetryReport`, so it used
/// to surface as `TelemetryEvent::Report`. An echo for a `sequence_id` this client never sent
/// — another client's command on the shared report topic — must be `Unknown`.
#[tokio::test]
async fn test_foreign_echo_under_any_wrapper_is_unknown() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        publish(
            &mut server_stream,
            1,
            r#"{"system":{"command":"ledctrl","sequence_id":"20001","result":"success","reason":"success"}}"#,
        )
        .await;
        publish(
            &mut server_stream,
            2,
            r#"{"print":{"command":"pause","sequence_id":"0","result":"success"}}"#,
        )
        .await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    for _ in 0..2 {
        let event = client.poll_telemetry().await.unwrap();
        assert!(
            matches!(event, TelemetryEvent::Unknown(_)),
            "a foreign command echo must be Unknown, got {event:?}"
        );
    }

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_unanswered_command_times_out_at_its_deadline() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        read_command(&mut server_stream).await;
        // No echo: the printer never answers.
    });

    let clock = ManualClock::default();
    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S)
        .await
        .with_timer(clock.clone());
    client.set_command_timeout(5);

    let handle = client.pause_print().await.expect("pause_print failed");
    broker_task.await.expect("broker task panicked");

    clock.advance_ms(5_000);
    match client.poll_telemetry().await.unwrap() {
        TelemetryEvent::Command(resolution, raw) => {
            assert_eq!(resolution.handle, handle);
            assert_eq!(resolution.outcome, CommandOutcome::TimedOut);
            assert!(raw.is_none(), "no message produced a timeout");
        }
        other => panic!("expected a timeout, got {other:?}"),
    }
    assert_eq!(
        client.await_ack(&handle).await.unwrap(),
        CommandOutcome::TimedOut,
        "await_ack answers from the outcome the event loop already received"
    );
}

#[tokio::test]
async fn test_disconnect_resolves_pending_commands_as_connection_lost() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        read_command(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    let handle = client.resume_print().await.expect("resume_print failed");
    broker_task.await.expect("broker task panicked");

    client.disconnect_mqtt().await.unwrap();
    // Delivered before any reconnect is attempted — this from_mqtt() client cannot redial.
    match client.poll_telemetry().await.unwrap() {
        TelemetryEvent::Command(resolution, None) => {
            assert_eq!(resolution.handle, handle);
            assert_eq!(resolution.outcome, CommandOutcome::ConnectionLost);
        }
        other => panic!("expected ConnectionLost, got {other:?}"),
    }
}

#[tokio::test]
async fn test_await_ack_returns_the_outcome_once_and_keeps_other_traffic() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let (_, command, seq) = read_command(&mut server_stream).await;
        publish(
            &mut server_stream,
            1,
            r#"{"print":{"command":"push_status","sequence_id":"59","gcode_state":"RUNNING"}}"#,
        )
        .await;
        publish(
            &mut server_stream,
            2,
            &format!(r#"{{"print":{{"command":"{command}","sequence_id":"{seq}","errno":-2,"soft_temp":45}}}}"#),
        )
        .await;
        publish(
            &mut server_stream,
            3,
            r#"{"print":{"command":"push_status","sequence_id":"60","gcode_state":"RUNNING"}}"#,
        )
        .await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    let handle = client
        .change_filament(0, 1, 220, 220, None)
        .await
        .expect("change_filament failed");

    let outcome = client.await_ack(&handle).await.expect("await_ack failed");
    assert!(
        matches!(
            outcome,
            CommandOutcome::Refused(CommandRefusal {
                errno: Some(-2),
                ..
            })
        ),
        "got {outcome:?}"
    );

    // The telemetry read while waiting is still delivered, and the outcome is not repeated.
    for _ in 0..2 {
        let event = client.poll_telemetry().await.unwrap();
        assert!(
            matches!(event, TelemetryEvent::Report(..)),
            "expected buffered and live telemetry, got {event:?}"
        );
    }
    assert!(
        matches!(
            client.await_ack(&handle).await,
            Err(Error::InvalidArgument(_))
        ),
        "an outcome already returned by await_ack is not held a second time"
    );

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_pushall_settles_on_publish() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        read_command(&mut server_stream).await;
    });

    let mut client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;
    let handle = client
        .request_pushall()
        .await
        .expect("request_pushall failed");
    assert_eq!(handle.ack(), AckExpectation::SettlesOnPublish);
    assert_eq!(
        client.await_ack(&handle).await.unwrap(),
        CommandOutcome::SettledOnPublish
    );

    broker_task.await.expect("broker task panicked");
}
