//! # Client Coordinator — Connection Lifecycle / Reconnect Tests
//!
//! Split from `client_test.rs` Phase 18 section (see issue #35).

mod common;

use std::sync::Arc;
use tokio::sync::Mutex;

use bambino::client::{
    DummyFactory, DummyTimer, DummyTls, PrinterClient,
};
use bambino::error::Error;
use bambino::ftps::FtpsClient;
use bambino::io::TokioIo;
use bambino::identity::PrinterIdentity;
use bambino::models::PrinterModel;
use bambino::mqtt::MqttClient;

use common::io::{DummyTlsConnector, HostCapturingTlsConnector, MockDataStreamFactory};
use common::client::{connect_test_client, SERIAL};
use common::mock_ftps;
use common::mock_mqtt::{
    handle_mqtt_handshake, read_puback, read_publish_payload, send_publish_payload,
};

#[tokio::test]
async fn test_ensure_mqtt_reseed_skipped_without_real_clock() {
    // The wall-clock sequence-counter reseed in ensure_mqtt() is meant to stop two
    // independent sessions connecting to the same printer from both starting at the same
    // fixed counter. Under DummyTimer (the documented, first-class default when
    // .with_timer() isn't chained), now_millis() always returns 0, so reseeding
    // unconditionally would collide every default-configured client onto the same seed —
    // exactly the bug this guard prevents. Verify the first command after a lazy
    // ensure_mqtt() connect still carries the untouched default sequence ID (10001).
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory::new(data_container);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json["print"]["sequence_id"], "10001",
            "DummyTimer has no real clock — reseed must be skipped, not collapse every \
             default-configured client onto the same wall-clock seed"
        );
    });

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    );

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("mock broker task panicked");
}

#[tokio::test]
async fn test_first_lazy_command_carries_a_reseeded_sequence_id_with_a_real_clock() {
    // The complement of the DummyTimer test above. dispatch() used to mint the sequence ID
    // before publish_request() ran ensure_mqtt(), so the wall-clock reseed landed one command
    // too late and the *first* command of every lazily-connecting session still published the
    // fixed 10001 — precisely the cross-session collision the reseed exists to prevent, since
    // MQTT connects lazily by default and "construct, then immediately send" is the common
    // shape. With a real TimerProvider the first command must already be reseeded.
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory::new(data_container);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
        let json = read_publish_payload(&mut server_stream).await;
        assert_ne!(
            json["print"]["sequence_id"], "10001",
            "with a real clock the reseed must complete before the first command's \
             sequence ID is minted, not after it"
        );
    });

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new());

    client.send_gcode("G28").await.expect("send_gcode failed");

    broker_task.await.expect("mock broker task panicked");
}

#[tokio::test]
async fn test_disconnect_and_attach_mqtt_recovers_dead_session() {
    // Before disconnect_mqtt()/attach_mqtt() existed, a dead MQTT session (a
    // tick_zombie_check()-detected zombie, a transport error) had no supported recovery
    // path — ensure_mqtt()'s is_some() short-circuit kept handing back the same broken
    // stream forever, unlike disconnect_camera()/attach_camera() and
    // disconnect_storage()/attach_storage(). Verify disconnect clears the slot and attach
    // reinstalls a fresh connected client that telemetry keeps working through.
    let (client_stream_a, mut server_stream_a) = tokio::io::duplex(8192);
    let broker_task_a = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a).await;
    });
    let mut client = connect_test_client(TokioIo(client_stream_a), SERIAL, PrinterModel::P1S).await;
    assert!(client.is_mqtt_connected());
    broker_task_a.await.expect("First broker task panicked");

    client
        .disconnect_mqtt()
        .await
        .expect("disconnect_mqtt should succeed");
    assert!(
        !client.is_mqtt_connected(),
        "disconnect_mqtt must clear self.mqtt"
    );

    let (client_stream_b, mut server_stream_b) = tokio::io::duplex(8192);
    let topic = format!("device/{SERIAL}/report");
    let broker_task_b = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_b).await;
        send_publish_payload(
            &mut server_stream_b,
            &topic,
            9001,
            br#"{"print":{"wifi_signal":"-60dBm"}}"#,
        )
        .await;
        read_puback(&mut server_stream_b).await;
    });
    let mqtt_client_b = MqttClient::connect(
        TokioIo(client_stream_b),
        &PrinterIdentity { ip: String::new(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
        .await
        .expect("second MQTT connect handshake failed");
    client.attach_mqtt(mqtt_client_b);
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
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory::new(data_container.clone());

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_ftps(DummyTlsConnector, factory, DummyTimer);

    for attempt in 1..=2 {
        let result = client.storage().await;
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
async fn test_disconnect_storage_clears_ftps_for_clean_reconnect() {
    // `disconnect_storage()` (review/client.md Phase 5) must leave `self.ftps` as `None`
    // afterward, so a later `storage()` call falls through to `ensure_ftps()`'s existing
    // "FTPS not configured" error instead of ever handing back the poisoned client that
    // `FtpsClient::disconnect()` leaves behind (review/ftps.md Phase 2/7).
    //
    // The FTPS client is genuinely poisoned first, via a control-channel transport failure
    // (`.claude/rules/ftps-poisoning.md`) — without that this test only reproved that
    // `ftps_config` is consumed on first connect, an unrelated invariant already covered
    // elsewhere, and broken code resetting `self.ftps` on the ordinary-disconnect path only
    // would still have passed.
    let (client_control, server_control) = tokio::io::duplex(8192);

    // `ensure_ftps()` fetches its raw control stream via the factory, so the mock data
    // stream is preloaded with the client side of the duplex pair up front.
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_control))));
    let factory = MockDataStreamFactory::new(data_container.clone());

    // Acks the handshake, reads DELE, then drops the control stream without replying.
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dele_connection_drop(
        server_control,
        data_container.clone(),
    ));

    let mut client = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_ftps(DummyTlsConnector, factory, DummyTimer);

    let ftps = client
        .storage()
        .await
        .expect("first storage() call should connect via the mock FTPS handshake");
    assert!(matches!(
        ftps.delete_file("/model/job.3mf").await,
        Err(Error::Network(_))
    ));
    // Poisoned now: `storage()` still hands back this same instance (`ensure_ftps()`'s
    // `is_some()` short-circuit), and every operation through it fails.
    let through_poisoned = client
        .storage()
        .await
        .expect("storage() short-circuits to the existing, now-poisoned client")
        .get_file_size("/model/job.3mf")
        .await;
    assert!(
        matches!(through_poisoned, Err(Error::ProtocolViolation(_))),
        "a poisoned FTPS client must keep failing until it is replaced, got {:?}",
        through_poisoned
    );
    assert!(client.is_ftps_connected());

    client
        .disconnect_storage()
        .await
        .expect("disconnect_storage should succeed");
    assert!(
        !client.is_ftps_connected(),
        "disconnect_storage must clear self.ftps"
    );

    // ftps_config was already consumed by the first storage() call, so this must surface
    // the clear "not configured" error, not a stale/poisoned reconnect.
    let result = client.storage().await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation (\"FTPS not configured\") after disconnect_storage, got {:?}",
        result.map(|_| ())
    );

    server_handle.await.expect("Mock server panicked");

    // The documented recovery path: a freshly connected client installed via attach_storage()
    // works, proving disconnect_storage() left the slot genuinely reusable rather than just
    // having consumed a one-shot config.
    let (fresh_control, fresh_server_control) = tokio::io::duplex(8192);
    let fresh_container = Arc::new(Mutex::new(None));
    let fresh_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        fresh_server_control,
        fresh_container.clone(),
    ));

    let fresh_ftps = FtpsClient::connect(
        TokioIo(fresh_control),
        DummyTlsConnector,
        MockDataStreamFactory::new(fresh_container),
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
        DummyTimer,
        false,
    )
    .await
    .expect("fresh FTPS handshake failed");

    client.attach_storage(fresh_ftps);
    assert!(client.is_ftps_connected());
    client
        .disconnect_storage()
        .await
        .expect("disconnect_storage on the reattached session should succeed");

    fresh_handle.await.expect("Fresh mock server panicked");
}

#[tokio::test]
async fn test_camera_trio_unconfigured_error() {
    // No test exercised the camera trio's "not configured" branch — the same case
    // FTPS's disconnect_storage/re-storage() test above covers for the FTPS trio
    // (see "FTPS not configured" a few tests up). A PrinterClient that never called
    // .with_camera()/.attach_camera() must fail read_camera_frame()/camera() with a clear
    // ProtocolViolation, not a panic or a misleading dial-level error.
    let mut client = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    );
    assert!(!client.is_camera_connected());

    let mut frame_buf = Vec::new();
    let result = client.read_camera_frame(&mut frame_buf).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation (\"Camera not configured\") on an unconfigured client, got {:?}",
        result.map(|_| ())
    );
    assert!(!client.is_camera_connected());
}

#[tokio::test]
async fn test_ensure_mqtt_bounds_post_dial_handshake_by_connect_timeout() {
    // review/client.md Phase 1: `ensure_mqtt()`'s race must cover the full
    // dial+TLS+`MqttClient::connect()` handshake, not just dial+TLS. Simulate a peer
    // that completes TCP/TLS but never sends CONNACK — the duplex's server side is left
    // idle forever, so any read from the client side blocks indefinitely unless the
    // handshake itself is inside the timeout race.
    let (client_stream, _server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory::new(data_container);

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(1);

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
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory::new(data_container);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(0);

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
    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory::new(data_container);

    let (connector, captured_host) = HostCapturingTlsConnector::new();
    let mut client = PrinterClient::new(
        connector,
        factory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_timer(bambino::io::tokio::TokioTimer::new())
    .with_connect_timeout(1);

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), client.connect_mqtt()).await;

    assert_eq!(
        captured_host.lock().await.as_deref(),
        Some(SERIAL),
        "MQTT TLS connect must use the serial, not the IP, as SNI/identity"
    );
}

/// `from_mqtt()`-constructed clients have empty `ip`/`access_code` (no host config
/// was ever supplied) — calling `.with_ftps()` on one used to silently succeed and only fail
/// opaquely at actual FTPS connect time. Must now panic immediately at the builder call site.
#[tokio::test]
#[should_panic(expected = "with_ftps() requires a real ip/access_code")]
async fn test_with_ftps_panics_on_from_mqtt_client() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);
    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;
    });
    let client = connect_test_client(TokioIo(client_stream), SERIAL, PrinterModel::P1S).await;

    let _ = client.with_ftps(
        bambino::client::dummy::DummyTls,
        bambino::client::dummy::DummyFactory,
        bambino::client::dummy::DummyTimer,
    );

    broker_task.await.expect("mock broker task panicked");
}
