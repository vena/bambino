//! # Binary Camera Protocol Integration Tests
//!
//! Validates the handshaking and frame extraction logic of the proprietary
//! Port 6000 binary JPEG stream (`BambuBinaryCameraStream`).
//!
//! Evaluates the client against the `mock_camera` server over an isolated
//! in-memory duplex stream, ensuring that JPEG magic marker bounds and payload
//! length descriptors are accurately translated.

mod common;

use std::sync::Arc;
use tokio::io::DuplexStream;
use tokio::sync::Mutex;

use bambino::camera::binary::BambuBinaryCameraStream;
use bambino::client::{DummyFactory, DummyTls, PrinterClient};
use bambino::error::Error;
use bambino::io::TokioIo;
use bambino::identity::PrinterIdentity;
use bambino::models::PrinterModel;

use common::io::{DummyTlsConnector, MockDataStreamFactory};
use common::mock_camera::{
    run_mock_camera_server, run_mock_camera_server_closes_after_handshake,
    run_mock_camera_server_drops_mid_frame,
};

const SERIAL: &str = "01P000000000000";

#[tokio::test]
async fn test_binary_camera_handshake_and_streaming() {
    let access_code = "87654321";
    let (client_stream, server_stream) = tokio::io::duplex(8192);

    // Command the mock server to emit exactly 3 sequential mock frames
    let server_handle = tokio::spawn(run_mock_camera_server(server_stream, access_code, 3));

    // We wrap the raw duplex stream in `TokioIo` to satisfy `AsyncIo` trait bounds.
    let mut camera_client: BambuBinaryCameraStream<TokioIo<DuplexStream>> =
        BambuBinaryCameraStream::new(TokioIo(client_stream));

    // This transmits the 80-byte block. The mock server will panic and fail the test
    // if the magic identifiers or access code do not match expectations.
    camera_client
        .authenticate(&PrinterIdentity { ip: String::new(), serial: String::new(), access_code: access_code.to_string(), model: PrinterModel::P1S })
        .await
        .expect("Failed to negotiate binary stream authentication handshake");

    let mut frame_buf = Vec::new();

    camera_client
        .read_next_frame(&mut frame_buf)
        .await
        .expect("Failed to read first camera frame");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8], "Missing JPEG start marker");
    assert_eq!(
        frame_buf[frame_buf.len() - 2..],
        [0xFF, 0xD9],
        "Missing JPEG end marker"
    );
    let inner_str = core::str::from_utf8(&frame_buf[2..frame_buf.len() - 2])
        .expect("Camera frame inner payload is not valid UTF-8");
    assert_eq!(inner_str, "MOCK_JPEG_PAYLOAD_0");

    camera_client
        .read_next_frame(&mut frame_buf)
        .await
        .expect("Failed to read second camera frame");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);
    let inner_str = core::str::from_utf8(&frame_buf[2..frame_buf.len() - 2])
        .expect("Camera frame inner payload is not valid UTF-8");
    assert_eq!(inner_str, "MOCK_JPEG_PAYLOAD_1");

    camera_client
        .read_next_frame(&mut frame_buf)
        .await
        .expect("Failed to read third camera frame");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);
    let inner_str = core::str::from_utf8(&frame_buf[2..frame_buf.len() - 2])
        .expect("Camera frame inner payload is not valid UTF-8");
    assert_eq!(inner_str, "MOCK_JPEG_PAYLOAD_2");

    // The server was instructed to send exactly 3 frames, then cleanly drop the socket.
    // Reading a 4th frame should result in a connection error, not a parse panic.
    let eof_result = camera_client.read_next_frame(&mut frame_buf).await;
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
/// `tests/client_test.rs::test_disconnect_storage_clears_ftps_for_clean_reconnect`'s
/// FTPS-through-`PrinterClient` pattern.
#[tokio::test]
async fn test_printer_client_camera_end_to_end() {
    let access_code = "12345678";
    let (client_stream, server_stream) = tokio::io::duplex(8192);

    let data_container = Arc::new(Mutex::new(Some(TokioIo(client_stream))));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    let server_handle = tokio::spawn(run_mock_camera_server(server_stream, access_code, 1));

    let mut printer = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: access_code.to_string(), model: PrinterModel::P1S },
    )
    .with_camera(DummyTlsConnector, factory);

    assert!(!printer.is_camera_connected());

    let mut frame_buf = Vec::new();
    printer
        .read_camera_frame(&mut frame_buf)
        .await
        .expect("read_camera_frame should connect, authenticate, and read the mock frame");

    assert!(printer.is_camera_connected());
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8], "Missing JPEG start marker");
    assert_eq!(
        frame_buf[frame_buf.len() - 2..],
        [0xFF, 0xD9],
        "Missing JPEG end marker"
    );

    server_handle.await.expect("Mock camera server panicked");
}

/// `ensure_camera()` must reject an RTSPS model immediately — before any dial, and even
/// without `.with_camera()` ever being called — per `.claude/rules/camera-trio.md`'s design:
/// the protocol check runs first, so an RTSPS model gets "this model doesn't support this
/// connection type," not "you forgot to configure it."
#[tokio::test]
async fn test_ensure_camera_rejects_rtsps_model_without_dialing() {
    let mut printer = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::X1C },
    );

    let mut frame_buf = Vec::new();
    let result = printer.read_camera_frame(&mut frame_buf).await;

    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation for an RTSPS model, got {:?}",
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
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    let mut printer = PrinterClient::new(
        DummyTls,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: "12345678".into(), model: PrinterModel::P1S },
    )
    .with_camera(DummyTlsConnector, factory);

    let mut frame_buf = Vec::new();
    for attempt in 1..=2 {
        let result = printer.read_camera_frame(&mut frame_buf).await;
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
    let access_code = "87654321";
    let (client_stream, server_stream) = tokio::io::duplex(8192);

    let server_handle = tokio::spawn(run_mock_camera_server_closes_after_handshake(
        server_stream,
        access_code,
    ));

    let mut camera_client: BambuBinaryCameraStream<TokioIo<DuplexStream>> =
        BambuBinaryCameraStream::new(TokioIo(client_stream));

    camera_client
        .authenticate(&PrinterIdentity { ip: String::new(), serial: String::new(), access_code: access_code.to_string(), model: PrinterModel::P1S })
        .await
        .expect("authenticate() only confirms the handshake write, must still succeed here");

    let mut frame_buf = Vec::new();
    let result = camera_client.read_next_frame(&mut frame_buf).await;
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
    let access_code = "87654321";
    let (client_stream, server_stream) = tokio::io::duplex(8192);

    // Header declares a 40-byte payload; server only ever writes 10 before dropping.
    let server_handle = tokio::spawn(run_mock_camera_server_drops_mid_frame(
        server_stream,
        access_code,
        40,
        10,
    ));

    let mut camera_client: BambuBinaryCameraStream<TokioIo<DuplexStream>> =
        BambuBinaryCameraStream::new(TokioIo(client_stream));

    camera_client
        .authenticate(&PrinterIdentity { ip: String::new(), serial: String::new(), access_code: access_code.to_string(), model: PrinterModel::P1S })
        .await
        .expect("Failed to negotiate binary stream authentication handshake");

    let mut frame_buf = Vec::new();
    let result = camera_client.read_next_frame(&mut frame_buf).await;
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
    let access_code = "87654321";
    let (client_stream, server_stream) = tokio::io::duplex(8192);
    let server_handle = tokio::spawn(run_mock_camera_server(server_stream, access_code, 1));

    let mut camera_stream: BambuBinaryCameraStream<TokioIo<DuplexStream>> =
        BambuBinaryCameraStream::new(TokioIo(client_stream));
    camera_stream
        .authenticate(&PrinterIdentity { ip: String::new(), serial: String::new(), access_code: access_code.to_string(), model: PrinterModel::P1S })
        .await
        .expect("Failed to negotiate binary stream authentication handshake");

    let mut client = PrinterClient::new(
        DummyTlsConnector,
        DummyFactory,
        PrinterIdentity { ip: "127.0.0.1".into(), serial: SERIAL.into(), access_code: access_code.to_string(), model: PrinterModel::P1S },
    )
    .with_camera(
        DummyTlsConnector,
        MockDataStreamFactory {
            active_stream: Arc::new(Mutex::new(None)),
        },
    );
    assert!(!client.is_camera_connected());

    client.attach_camera(camera_stream);
    assert!(client.is_camera_connected());

    let mut frame_buf = Vec::new();
    client
        .read_camera_frame(&mut frame_buf)
        .await
        .expect("attach_camera should leave an immediately-usable connected stream");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);

    client
        .disconnect_camera()
        .await
        .expect("disconnect_camera should succeed");
    assert!(!client.is_camera_connected(), "disconnect_camera must clear self.camera");

    server_handle
        .await
        .expect("Background mock camera server panicked");
}
