//! # Binary Camera Protocol Integration Tests
//!
//! Validates the handshaking and frame extraction logic of the proprietary
//! Port 6000 binary JPEG stream (`BambuBinaryCameraStream`).
//!
//! Evaluates the client against the `mock_camera` server over an isolated
//! in-memory duplex stream, ensuring that JPEG magic marker bounds and payload
//! length descriptors are accurately translated.

mod common;

use tokio::io::DuplexStream;

use bambino::camera::binary::BambuBinaryCameraStream;
use bambino::io::TokioIo;

use common::mock_camera::run_mock_camera_server;

#[tokio::test]
async fn test_binary_camera_handshake_and_streaming() {
    let access_code = "87654321";
    let (client_stream, server_stream) = tokio::io::duplex(8192);

    // 1. Spawn the background mock camera server
    // Command the mock server to emit exactly 3 sequential mock frames
    let server_handle = tokio::spawn(run_mock_camera_server(server_stream, access_code, 3));

    // 2. Initialize the Client
    // We wrap the raw duplex stream in `TokioIo` to satisfy `AsyncIo` trait bounds.
    let mut camera_client: BambuBinaryCameraStream<TokioIo<DuplexStream>> =
        BambuBinaryCameraStream::new(TokioIo(client_stream));

    // 3. Execute Authentication Handshake
    // This transmits the 80-byte block. The mock server will panic and fail the test
    // if the magic identifiers or access code do not match expectations.
    camera_client
        .authenticate(access_code)
        .await
        .expect("Failed to negotiate binary stream authentication handshake");

    // 4. Extract and verify frames continuously
    let mut frame_buf = Vec::new();

    // Frame 1
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

    // Frame 2
    camera_client
        .read_next_frame(&mut frame_buf)
        .await
        .expect("Failed to read second camera frame");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);
    let inner_str = core::str::from_utf8(&frame_buf[2..frame_buf.len() - 2])
        .expect("Camera frame inner payload is not valid UTF-8");
    assert_eq!(inner_str, "MOCK_JPEG_PAYLOAD_1");

    // Frame 3
    camera_client
        .read_next_frame(&mut frame_buf)
        .await
        .expect("Failed to read third camera frame");
    assert_eq!(frame_buf[0..2], [0xFF, 0xD8]);
    let inner_str = core::str::from_utf8(&frame_buf[2..frame_buf.len() - 2])
        .expect("Camera frame inner payload is not valid UTF-8");
    assert_eq!(inner_str, "MOCK_JPEG_PAYLOAD_2");

    // 5. Verify Stream Exhaustion
    // The server was instructed to send exactly 3 frames, then cleanly drop the socket.
    // Reading a 4th frame should result in a connection error, not a parse panic.
    let eof_result = camera_client.read_next_frame(&mut frame_buf).await;
    assert!(
        eof_result.is_err(),
        "Expected network termination error upon stream exhaustion"
    );

    // Ensure the mock server ran to completion cleanly
    server_handle
        .await
        .expect("Background mock camera server panicked");
}
