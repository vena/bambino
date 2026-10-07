//! # Client Coordinator — Connection Lifecycle / Reconnect Tests
//!
//! Split from the "Command-response round-trip tests" section of the former
//! `client_test.rs` (see issue #35).

use std::sync::Arc;
use tokio::sync::Mutex;

use bambino::client::{DummyFactory, DummyTimer, DummyTls, PrinterClient};
use bambino::error::Error;
use bambino::ftps::FtpsClient;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use crate::common::client::{
    SERIAL, connect_idle_client, connect_test_mqtt, spawn_broker, test_identity,
};
use crate::common::io::{
    CloseCountingTlsConnector, DummyTlsConnector, HostCapturingTlsConnector, MockDataStreamFactory,
};
use crate::common::mock_ftps;
use crate::common::mock_mqtt::{ReportPublisher, handle_mqtt_handshake, read_publish_payload};

#[tokio::test]
async fn test_connect_all_reports_rtsps_camera_as_not_attempted() {
    // ensure_camera() returns ProtocolViolation on an RTSPS model, but connect_all() must
    // report that channel as None ("not attempted") instead. An RTSPS camera is a channel
    // that does not apply to this printer rather than a failure, and surfacing it as an
    // Err would hand every P2S/X2D consumer a guaranteed error on an otherwise clean
    // connect. This is the one place connect_all() deliberately diverges from the
    // sequential path, so it gets its own test.

    let (mut client, broker_task) = connect_idle_client(PrinterModel::P2S).await;
    let outcome = client.connect_all().await;

    assert!(
        outcome.camera.is_none(),
        "RTSPS camera must be reported as not attempted, got {:?}",
        outcome.camera
    );
    assert!(
        outcome.mqtt.is_none(),
        "MQTT was already connected via from_mqtt(), so it must not be re-attempted"
    );
    assert!(
        outcome.ftps.is_none(),
        "FTPS was never configured, so it must be reported as not attempted, not as an error"
    );

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_connect_all_connects_mqtt_and_skips_unconfigured_channels() {
    // The common consumer shape: MQTT configured for a lazy dial, no .with_ftps() and no
    // .with_camera(). Configuration is the selection, so the two unconfigured channels are
    // skipped silently and only MQTT is dialled — and it must actually end up installed,
    // not merely reported as Ok.
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // connect_all() must run the same per-connection steps as ensure_mqtt(), including the
        // connect-time pushall that refills the telemetry cache (issue #287).
        let pushall = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            pushall["pushing"]["command"], "pushall",
            "connect_all() must publish the connect-time pushall"
        );
    });
    let (factory, _) = MockDataStreamFactory::with_stream(stream);

    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S));

    assert!(
        !client.is_mqtt_connected(),
        "precondition: lazy, not yet dialled"
    );
    let outcome = client.connect_all().await;

    assert!(
        matches!(outcome.mqtt, Some(Ok(()))),
        "MQTT should have been dialled and connected, got {:?}",
        outcome.mqtt
    );
    assert!(
        client.is_mqtt_connected(),
        "a Some(Ok(())) outcome must mean the session is installed on the client"
    );
    assert!(outcome.ftps.is_none(), "FTPS was never configured");
    assert!(
        outcome.camera.is_none(),
        "camera was never configured, even though P1S uses the BinaryJpeg protocol"
    );

    broker_task.await.expect("broker task panicked");
}

#[tokio::test]
async fn test_ensure_mqtt_reseed_skipped_without_real_clock() {
    // The wall-clock sequence-counter reseed in ensure_mqtt() is meant to stop two
    // independent sessions connecting to the same printer from both starting at the same
    // fixed counter. Under DummyTimer (the documented, first-class default when
    // .with_timer() isn't chained), now_millis() always returns 0, so reseeding
    // unconditionally would collide every default-configured client onto the same seed —
    // exactly the bug this guard prevents. Verify the first command after a lazy
    // ensure_mqtt() connect still carries the untouched default sequence ID (30001).
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // `ensure_mqtt()` publishes a connect-time pushall before returning, so it — not the
        // caller's command — mints the first sequence ID of the session.
        let pushall = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            pushall["pushing"]["sequence_id"], "30001",
            "DummyTimer has no real clock — reseed must be skipped, not collapse every \
             default-configured client onto the same wall-clock seed"
        );
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json["print"]["sequence_id"], "30002",
            "the caller's first command must continue the untouched default sequence"
        );
    });
    let (factory, _) = MockDataStreamFactory::with_stream(stream);

    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S));

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("mock broker task panicked");
}

#[tokio::test]
async fn test_first_lazy_command_carries_a_reseeded_sequence_id_with_a_real_clock() {
    // The complement of the DummyTimer test above. dispatch() used to mint the sequence ID
    // before publish_request() ran ensure_mqtt(), so the wall-clock reseed landed one command
    // too late and the *first* command of every lazily-connecting session still published the
    // fixed 30001 — precisely the cross-session collision the reseed exists to prevent, since
    // MQTT connects lazily by default and "construct, then immediately send" is the common
    // shape. With a real TimerProvider the first command must already be reseeded.
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
        handle_mqtt_handshake(&mut server_stream).await;
        // The connect-time pushall is itself minted after the reseed, so it must already be
        // off the fixed default — as must the caller's command behind it.
        let pushall = read_publish_payload(&mut server_stream).await;
        assert_ne!(
            pushall["pushing"]["sequence_id"], "30001",
            "with a real clock the reseed must complete before any sequence ID is minted"
        );
        let json = read_publish_payload(&mut server_stream).await;
        assert_ne!(
            json["print"]["sequence_id"], "30001",
            "with a real clock the reseed must complete before the first command's \
             sequence ID is minted, not after it"
        );
        let seq: u64 = json["print"]["sequence_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (30_000..i32::MAX as u64).contains(&seq),
            "a reseeded id must stay above the printer's push_status counter and BambuStudio's \
             reserved 20000..30000 range, got {seq}"
        );
    });
    let (factory, _) = MockDataStreamFactory::with_stream(stream);

    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S))
            .with_timer(bambino::io::tokio::TokioTimer::new());

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("mock broker task panicked");
}

/// Regression test for GitHub issue #293: `disconnect_mqtt()` must shut the TLS session down
/// through [`TlsConnector::close`] before releasing it, not just drop it.
///
/// Dropping alone already returns the memory — the slot goes to `None` in the same call — but
/// it sends no `close_notify`, so the printer sees a truncated connection on every ordinary
/// disconnect. `TlsConnector::close` defaults to a no-op, so forgetting the call is silent;
/// this counts it.
#[tokio::test]
async fn test_disconnect_mqtt_closes_the_tls_session() {
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let (connector, closes) = CloseCountingTlsConnector::new();
    let mut client = PrinterClient::new(
        connector,
        MockDataStreamFactory::empty(),
        test_identity(PrinterModel::P1S),
    );
    client
        .attach_mqtt(connect_test_mqtt(stream, SERIAL).await)
        .await;
    broker_task.await.expect("mock broker task panicked");

    client.disconnect_mqtt().await;
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "disconnect_mqtt must close the TLS session before dropping it"
    );
}

#[tokio::test]
async fn test_disconnect_and_attach_mqtt_recovers_dead_session() {
    // Before disconnect_mqtt()/attach_mqtt() existed, a dead MQTT session (a
    // tick_zombie_check()-detected zombie, a transport error) had no supported recovery
    // path — ensure_mqtt()'s is_some() short-circuit kept handing back the same broken
    // stream forever, unlike disconnect_camera()/attach_camera() and
    // disconnect_ftps()/attach_ftps(). Verify disconnect clears the slot and attach
    // reinstalls a fresh connected client that telemetry keeps working through.
    let (mut client, broker_task_a) = connect_idle_client(PrinterModel::P1S).await;
    assert!(client.is_mqtt_connected());
    broker_task_a.await.expect("First broker task panicked");

    client.disconnect_mqtt().await;
    assert!(
        !client.is_mqtt_connected(),
        "disconnect_mqtt must clear self.mqtt"
    );

    let mut reports = ReportPublisher::new(SERIAL);
    let (stream_b, broker_task_b) = spawn_broker(|mut server_stream_b| async move {
        handle_mqtt_handshake(&mut server_stream_b).await;
        // attach_mqtt runs the same connect-time pushall a dialled session gets (#346).
        let pushall = read_publish_payload(&mut server_stream_b).await;
        assert_eq!(pushall["pushing"]["command"], "pushall");
        reports
            .publish(
                &mut server_stream_b,
                br#"{"print":{"wifi_signal":"-60dBm"}}"#,
            )
            .await;
    });
    client
        .attach_mqtt(connect_test_mqtt(stream_b, SERIAL).await)
        .await;
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
    let factory = MockDataStreamFactory::empty();

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        test_identity(PrinterModel::P1S),
    )
    .with_ftps(DummyTlsConnector, factory, DummyTimer);

    for attempt in 1..=2 {
        let result = client.ftps().await;
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
async fn test_disconnect_ftps_clears_ftps_for_clean_reconnect() {
    // A poisoned FTPS session must not be handed back: `ftps()` disconnects it, its parts go
    // back into the FTPS config, and the next call redials (#448).
    //
    // The FTPS client is genuinely poisoned first, via a control-channel transport failure
    // (`.claude/rules/ftps-poisoning.md`) — without that this test only reproved that
    // `ftps_config` is consumed on first connect, an unrelated invariant already covered
    // elsewhere, and broken code resetting `self.ftps` on the ordinary-disconnect path only
    // would still have passed.
    let (client_control, server_control) = tokio::io::duplex(8192);

    // `ensure_ftps()` fetches its raw control stream via the factory, so the mock data
    // stream is preloaded with the client side of the duplex pair up front.
    let (factory, data_container) =
        MockDataStreamFactory::with_stream(TokioIo::new(client_control));

    // Acks the handshake, reads DELE, then drops the control stream without replying.
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dele_connection_drop(
        server_control,
    ));

    let mut client = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
        .with_ftps(DummyTlsConnector, factory, DummyTimer);

    let ftps = client
        .ftps()
        .await
        .expect("first ftps() call should connect via the mock FTPS handshake");
    assert!(matches!(
        ftps.delete_file("/model/job.3mf").await,
        Err(Error::Network(_))
    ));
    // Poisoned now: it no longer counts as connected.
    assert!(!client.is_ftps_connected());
    server_handle.await.expect("Mock server panicked");

    // The next `ftps()` replaces the poisoned session: `disconnect_ftps()` returns its parts to
    // the FTPS config, and the same factory dials a fresh control stream.
    let (fresh_control, fresh_server_control) = tokio::io::duplex(8192);
    *data_container.lock().await = Some(TokioIo::new(fresh_control));
    let fresh_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(fresh_server_control));
    client
        .ftps()
        .await
        .expect("ftps() should redial over the poisoned session");
    assert!(client.is_ftps_connected());

    client.disconnect_ftps().await;
    assert!(!client.is_ftps_connected());
    fresh_handle.await.expect("Fresh mock server panicked");
}

#[tokio::test]
async fn test_attach_ftps_installs_a_connected_session() {
    let (fresh_control, fresh_server_control) = tokio::io::duplex(8192);
    let fresh_container = Arc::new(Mutex::new(None));
    let fresh_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(fresh_server_control));
    let identity = test_identity(PrinterModel::P1S);
    let fresh_ftps = FtpsClient::connect(
        TokioIo::new(fresh_control),
        DummyTlsConnector,
        MockDataStreamFactory::new(fresh_container),
        identity.clone(),
        DummyTimer,
        bambino::ftps::TlsVersionCheck::Enforce,
    )
    .await
    .expect("fresh FTPS handshake failed");

    let mut client = PrinterClient::new(DummyTls, DummyFactory, identity).with_ftps(
        DummyTlsConnector,
        MockDataStreamFactory::empty(),
        DummyTimer,
    );
    client.attach_ftps(fresh_ftps).await;
    assert!(client.is_ftps_connected());
    client.disconnect_ftps().await;
    fresh_handle.await.expect("Fresh mock server panicked");
}

#[tokio::test]
async fn test_camera_trio_unconfigured_error() {
    // No test exercised the camera trio's "not configured" branch — the same case
    // FTPS's disconnect_ftps/re-ftps() test above covers for the FTPS trio
    // (see "FTPS not configured" a few tests up). A PrinterClient that never called
    // .with_camera()/.attach_camera() must fail read_camera_frame()/camera() with a clear
    // ProtocolViolation, not a panic or a misleading dial-level error.
    let mut client = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S));
    assert!(!client.is_camera_connected());

    let result = client.read_camera_frame().await;
    assert!(
        matches!(result, Err(Error::NotConfigured(_))),
        "expected NotConfigured on an unconfigured client, got {:?}",
        result.map(|_| ())
    );
    assert!(!client.is_camera_connected());
}

#[tokio::test]
async fn test_ensure_mqtt_bounds_post_dial_handshake_by_connect_timeout() {
    // `ensure_mqtt()`'s connect-timeout race must cover the full
    // dial+TLS+`MqttClient::connect()` handshake, not just dial+TLS (see
    // `.claude/rules/connect-timeouts.md`). Simulate a peer
    // that completes TCP/TLS but never sends CONNACK — the duplex's server side is left
    // idle forever, so any read from the client side blocks indefinitely unless the
    // handshake itself is inside the timeout race.
    let (client_stream, _server_stream) = tokio::io::duplex(8192);
    let (factory, _) = MockDataStreamFactory::with_stream(TokioIo::new(client_stream));

    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S))
            .with_timer(bambino::io::tokio::TokioTimer::new())
            .with_connect_timeout(Some(std::time::Duration::from_secs(1)));

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
    let (stream, broker_task) = spawn_broker(|mut server_stream| async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });
    let (factory, _) = MockDataStreamFactory::with_stream(stream);

    let mut client =
        PrinterClient::new(DummyTlsConnector, factory, test_identity(PrinterModel::P1S))
            .with_timer(bambino::io::tokio::TokioTimer::new())
            .with_connect_timeout(None);

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
    let (factory, _) = MockDataStreamFactory::with_stream(TokioIo::new(client_stream));

    let (connector, captured_host) = HostCapturingTlsConnector::new();
    let mut client = PrinterClient::new(connector, factory, test_identity(PrinterModel::P1S))
        .with_timer(bambino::io::tokio::TokioTimer::new())
        .with_connect_timeout(Some(std::time::Duration::from_secs(1)));

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect_mqtt()).await;

    assert_eq!(
        captured_host.lock().await.as_deref(),
        Some(SERIAL),
        "MQTT TLS connect must use the serial, not the IP, as SNI/identity"
    );
}

#[tokio::test]
async fn test_with_ftps_on_from_mqtt_client_reports_not_configured() {
    // A from_mqtt() client has no ip or access code to dial FTPS with. with_ftps() used to
    // panic at the builder; the first FTPS call now returns NotConfigured instead (#449).
    let (client, broker_task) = connect_idle_client(PrinterModel::P1S).await;

    let mut client = client.with_ftps(
        crate::common::io::DummyTlsConnector,
        MockDataStreamFactory::empty(),
        bambino::client::dummy::DummyTimer,
    );
    let result = client.connect_ftps().await;
    assert!(
        matches!(result, Err(Error::NotConfigured(_))),
        "expected NotConfigured, got {result:?}"
    );

    broker_task.await.expect("mock broker task panicked");
}
