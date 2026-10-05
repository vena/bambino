//! # Mock Port 6000 Binary Camera Server
//!
//! Provides a deterministic mock server that simulates the proprietary binary
//! image stream utilized by the constrained ESP32 lines (P1 and A1 series).
//!
//! **Behavioral Design:**
//! 1. Awaits the 80-byte connection handshake and checks it byte-for-byte against the
//!    library's own `build_handshake_packet`. The packet layout itself is checked
//!    independently by `binary.rs`'s `test_handshake_packet_construction`.
//! 2. Emits sequentially increasing "JPEG frames" prefaced with the 16-byte metadata
//!    header (containing the exact payload length).
//! 3. The emitted frames are mocked to contain valid JPEG magic start (`FF D8`) and
//!    end (`FF D9`) markers to satisfy the client-side safety guards.

use bambino::camera::binary::build_handshake_packet;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Reads the 80-byte handshake and asserts it is exactly what the client builds for `access_code`.
async fn expect_handshake(stream: &mut tokio::io::DuplexStream, access_code: &str) {
    let expected = build_handshake_packet(access_code).expect("test access code must be valid");
    let mut handshake = [0u8; 80];
    stream
        .read_exact(&mut handshake)
        .await
        .expect("Failed to read 80-byte handshake");
    assert_eq!(handshake, expected, "Unexpected handshake packet");
}

/// The 16-byte frame header: payload length, little-endian, then zero padding.
fn frame_header(payload_len: u32) -> [u8; 16] {
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&payload_len.to_le_bytes());
    header
}

/// Simulates the proprietary binary camera protocol emitted on Port 6000.
///
/// * `stream`: The server-side end of the duplex TCP control stream.
/// * `expected_access_code`: The LAN code used to validate the handshake payload.
/// * `frame_count`: The number of discrete mocked JPEG frames to push before disconnecting.
pub async fn run_mock_camera_server(
    mut stream: tokio::io::DuplexStream,
    expected_access_code: &str,
    frame_count: u32,
) {
    expect_handshake(&mut stream, expected_access_code).await;

    for i in 0..frame_count {
        let mut mock_image = vec![0xFF, 0xD8];
        mock_image.extend_from_slice(format!("MOCK_JPEG_PAYLOAD_{}", i).as_bytes());
        mock_image.extend_from_slice(&[0xFF, 0xD9]);

        stream
            .write_all(&frame_header(mock_image.len() as u32))
            .await
            .expect("Failed to write camera frame header");
        stream
            .write_all(&mock_image)
            .await
            .expect("Failed to write camera frame payload");
        stream.flush().await.expect("Failed to flush camera frame");
    }
}

/// Variant that reads and validates the handshake exactly like
/// [`run_mock_camera_server`], then closes the connection immediately instead of streaming any
/// frame — simulating a rejected access code. Per `src/camera/CLAUDE.md`, `authenticate()`
/// only confirms the handshake packet was *written*; a real rejection surfaces later as a
/// connection error from the first `read_next_frame()` call. This lets a test assert that
/// shape end-to-end instead of only at the unit level.
pub async fn run_mock_camera_server_closes_after_handshake(
    mut stream: tokio::io::DuplexStream,
    expected_access_code: &str,
) {
    expect_handshake(&mut stream, expected_access_code).await;
    // No frame data written — drop the stream immediately, as a printer would after
    // rejecting the access code.
}

/// Variant that streams a valid frame header advertising `payload_len` bytes, then
/// closes the connection after writing only `bytes_before_drop` of that payload — simulating a
/// network blip or printer-side disconnect mid-frame. No unit test in `src/camera/binary.rs`
/// covers a connection closing partway through a payload already declared by its header (its
/// existing tests cover oversized/zero-size/malformed-marker payloads that are fully present,
/// just structurally invalid).
pub async fn run_mock_camera_server_drops_mid_frame(
    mut stream: tokio::io::DuplexStream,
    expected_access_code: &str,
    payload_len: u32,
    bytes_before_drop: usize,
) {
    expect_handshake(&mut stream, expected_access_code).await;

    stream
        .write_all(&frame_header(payload_len))
        .await
        .expect("Failed to write camera frame header");
    stream
        .write_all(&vec![0xFFu8; bytes_before_drop])
        .await
        .expect("Failed to write partial camera frame payload");
    stream.flush().await.expect("Failed to flush partial frame");

    // Drop the stream without writing the remaining declared bytes.
}
