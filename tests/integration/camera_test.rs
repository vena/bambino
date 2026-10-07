//! # Binary Camera Protocol Integration Tests
//!
//! Validates the handshaking and frame extraction logic of the proprietary
//! Port 6000 binary JPEG stream (`BinaryCameraStream`).
//!
//! Evaluates the client against the `mock_camera` server over an isolated
//! in-memory duplex stream, ensuring that JPEG magic marker bounds and payload
//! length descriptors are accurately translated.

use tokio::io::DuplexStream;

use bambino::camera::binary::BinaryCameraStream;
use bambino::client::{DummyFactory, DummyTls, PrinterClient};
use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use crate::common::client::{ACCESS_CODE, spawn_broker, test_identity};
use crate::common::io::{CloseCountingTlsConnector, DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_camera::{
    mock_frame, run_mock_camera_server, run_mock_camera_server_closes_after_handshake,
    run_mock_camera_server_drops_mid_frame,
};

/// Wraps `stream` in a `BinaryCameraStream` and sends the handshake for [`ACCESS_CODE`].
///
/// `authenticate()` only confirms the handshake was written, so this succeeds whether or not
/// the server goes on to accept it.
async fn authenticated_camera(
    stream: TokioIo<DuplexStream>,
) -> BinaryCameraStream<TokioIo<DuplexStream>> {
    let mut camera = BinaryCameraStream::new(stream);
    camera
        .authenticate(ACCESS_CODE)
        .await
        .expect("Failed to negotiate binary stream authentication handshake");
    camera
}

#[tokio::test]
async fn test_binary_camera_handshake_and_streaming() {
    // Command the mock server to emit exactly 3 sequential mock frames. It panics, failing the
    // test, if the 80-byte handshake's magic identifiers or access code don't match.
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server(server, ACCESS_CODE, 3));
    let mut camera_client = authenticated_camera(stream).await;

    for i in 0..3 {
        let frame_buf = camera_client
            .read_next_frame()
            .await
            .unwrap_or_else(|e| panic!("Failed to read camera frame {i}: {e:?}"));
        assert_eq!(frame_buf, mock_frame(i));
    }

    // The server was instructed to send exactly 3 frames, then cleanly drop the socket.
    // Reading a 4th frame should result in a connection error, not a parse panic.
    let eof_result = camera_client.read_next_frame().await;
    assert!(
        eof_result.is_err(),
        "Expected network termination error upon stream exhaustion"
    );

    server_handle
        .await
        .expect("Background mock camera server panicked");
}

/// Exercises the full `PrinterClient` camera path (see `.claude/rules/camera-trio.md`):
/// `.with_camera()` lazy-connects on first `read_camera_frame()` call, dialing via the mock
/// factory, passing through the (pass-through) mock TLS connector, authenticating, and
/// reading a frame — analogous to
/// `client_reconnect_test.rs::test_disconnect_ftps_clears_ftps_for_clean_reconnect`'s
/// FTPS-through-`PrinterClient` pattern.
#[tokio::test]
async fn test_printer_client_camera_end_to_end() {
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server(server, ACCESS_CODE, 1));
    let (factory, _) = MockDataStreamFactory::with_stream(stream);

    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
        .with_camera(DummyTlsConnector, factory);

    assert!(!printer.is_camera_connected());
    let frame_buf = printer
        .read_camera_frame()
        .await
        .expect("read_camera_frame should connect, authenticate, and read the mock frame");

    assert!(printer.is_camera_connected());
    assert_eq!(frame_buf, mock_frame(0));

    server_handle.await.expect("Mock camera server panicked");
}

/// `ensure_camera()` must reject an RTSPS model immediately — before any dial, and even
/// without `.with_camera()` ever being called — per `.claude/rules/camera-trio.md`'s design:
/// the protocol check runs first, so an RTSPS model gets "this model doesn't support this
/// connection type," not "you forgot to configure it."
#[tokio::test]
async fn test_ensure_camera_rejects_rtsps_model_without_dialing() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::X1C));
    let result = printer.read_camera_frame().await;

    assert!(
        matches!(result, Err(Error::ModelMismatch(_))),
        "expected ModelMismatch for an RTSPS model, got {:?}",
        result.map(|_| ())
    );
    assert!(!printer.is_camera_connected());
}

/// `ensure_camera()` used to `.take()` `camera_config` before attempting the dial,
/// so a failed attempt permanently discarded it — every later call would then report the
/// misleading "Camera not configured" error instead of retrying. `MockDataStreamFactory`'s
/// `dial()` fails with `ConnectionRefused` whenever its stream container is empty, so two
/// consecutive calls over the same never-populated container must both fail the *same*
/// dial-level way, never degrading into "not configured" (which would mean the config got
/// dropped after the first attempt).
#[tokio::test]
async fn test_ensure_camera_retries_after_failed_dial() {
    let factory = MockDataStreamFactory::empty();

    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
        .with_camera(DummyTlsConnector, factory);
    for attempt in 1..=2 {
        let result = printer.read_camera_frame().await;
        assert!(
            matches!(result, Err(Error::Network(_))),
            "attempt {attempt}: expected the dial failure to surface as Network, not \
             degrade into \"Camera not configured\" from a config consumed on a prior failed \
             attempt, got {:?}",
            result.map(|_| ())
        );
    }
    assert!(!printer.is_camera_connected());
}

/// Full integration-level coverage of a rejected access code — `authenticate()`
/// only confirms the handshake bytes were written (see `src/camera/CLAUDE.md`), so the
/// actual rejection must surface on the following `read_next_frame()` call as a connection
/// error, not a hang or a misleading success.
#[tokio::test]
async fn test_binary_camera_rejected_handshake_surfaces_on_first_read() {
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server_closes_after_handshake(server, ACCESS_CODE));
    let mut camera_client = authenticated_camera(stream).await;
    let result = camera_client.read_next_frame().await;
    assert!(
        result.is_err(),
        "expected a connection error on the first read after a rejected handshake, got {:?}",
        result
    );

    server_handle
        .await
        .expect("Background mock camera server panicked");
}

/// Full integration-level coverage of a connection dropping partway through a
/// frame's declared payload — no existing unit test in `src/camera/binary.rs` exercises a
/// short read against an already-valid header (its tests cover fully-present-but-structurally-
/// invalid payloads instead).
#[tokio::test]
async fn test_binary_camera_mid_frame_disconnect_returns_error_not_panic() {
    // Header declares a 40-byte payload; server only ever writes 10 before dropping.
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server_drops_mid_frame(server, ACCESS_CODE, 40, 10));
    let mut camera_client = authenticated_camera(stream).await;
    let result = camera_client.read_next_frame().await;
    assert!(
        result.is_err(),
        "expected a connection error when the stream closes mid-payload, got {:?}",
        result
    );

    server_handle
        .await
        .expect("Background mock camera server panicked");
}

/// `attach_camera()`/`disconnect_camera()` were never exercised by any test. Verifies
/// attach makes the client immediately usable for frame reads, and disconnect clears the slot.
#[tokio::test]
async fn test_attach_and_disconnect_camera() {
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server(server, ACCESS_CODE, 1));
    let camera_stream = authenticated_camera(stream).await;

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        test_identity(PrinterModel::P1S),
    )
    .with_camera(DummyTlsConnector, MockDataStreamFactory::empty());
    assert!(!client.is_camera_connected());

    client.attach_camera(camera_stream).await;
    assert!(client.is_camera_connected());
    let frame_buf = client
        .read_camera_frame()
        .await
        .expect("attach_camera should leave an immediately-usable connected stream");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);

    client.disconnect_camera().await;
    assert!(
        !client.is_camera_connected(),
        "disconnect_camera must clear self.camera"
    );

    // GitHub issue #293: the slot being clear is not the whole teardown — the TLS session
    // underneath must be closed, not merely dropped. Verified against a counting connector in
    // `test_disconnect_camera_closes_the_tls_session` below; `DummyTlsConnector` here keeps the
    // trait's no-op default, so this test says nothing about it either way.

    server_handle
        .await
        .expect("Background mock camera server panicked");
}

/// Regression test for GitHub issue #293: `disconnect_camera()` must shut the TLS session down
/// through [`TlsConnector::close`] before releasing it.
///
/// The close runs through the connector still held in `camera_config` — which is why
/// `ensure_camera()` no longer clears that field on a successful connect. Nothing was moved out
/// of it there (unlike `ftps_config`, whose connector moves into the `FtpsClient`), so clearing
/// it only ever cost the teardown its connector and the client its ability to redial.
#[tokio::test]
async fn test_disconnect_camera_closes_the_tls_session() {
    let (stream, server_handle) =
        spawn_broker(|server| run_mock_camera_server(server, ACCESS_CODE, 1));
    let camera_stream = authenticated_camera(stream).await;

    let (connector, closes) = CloseCountingTlsConnector::new();
    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        test_identity(PrinterModel::P1S),
    )
    .with_camera(connector, MockDataStreamFactory::empty());
    client.attach_camera(camera_stream).await;

    client.disconnect_camera().await;
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "disconnect_camera must close the TLS session before dropping the stream"
    );

    server_handle.abort();
}
