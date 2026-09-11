//! # Mock Port 6000 Binary Camera Server
//!
//! Provides a deterministic mock server that simulates the proprietary binary
//! image stream utilized by the constrained ESP32 lines (P1 and A1 series).
//!
//! **Behavioral Design:**
//! 1. Awaits and validates the strict 80-byte little-endian connection handshake
//!    packet, ensuring the authentication credentials conform to the standard layout.
//! 2. Emits sequentially increasing "JPEG frames" prefaced with the 16-byte metadata
//!    header (containing the exact payload length).
//! 3. The emitted frames are mocked to contain valid JPEG magic start (`FF D8`) and
//!    end (`FF D9`) markers to satisfy the client-side safety guards.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    let mut handshake = [0u8; 80];
    stream
        .read_exact(&mut handshake)
        .await
        .expect("Failed to read 80-byte handshake");

    assert_eq!(
        &handshake[0..4],
        64u32.to_le_bytes(),
        "Invalid handshake magic header"
    );

    assert_eq!(
        &handshake[4..8],
        12288u32.to_le_bytes(),
        "Invalid handshake command header"
    );

    assert_eq!(&handshake[16..20], b"bblp", "Invalid handshake username");

    let code_len = expected_access_code.len();
    assert_eq!(
        &handshake[48..48 + code_len],
        expected_access_code.as_bytes(),
        "Invalid handshake access code"
    );

    for i in 0..frame_count {
        let mut mock_image = vec![0xFF, 0xD8];

        let inner_data = format!("MOCK_JPEG_PAYLOAD_{}", i);
        mock_image.extend_from_slice(inner_data.as_bytes());

        mock_image.extend_from_slice(&[0xFF, 0xD9]);

        let payload_size = mock_image.len() as u32;

        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&payload_size.to_le_bytes());

        stream
            .write_all(&header)
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
    let mut handshake = [0u8; 80];
    stream
        .read_exact(&mut handshake)
        .await
        .expect("Failed to read 80-byte handshake");

    let code_len = expected_access_code.len();
    assert_eq!(
        &handshake[48..48 + code_len],
        expected_access_code.as_bytes(),
        "Invalid handshake access code"
    );

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
    let mut handshake = [0u8; 80];
    stream
        .read_exact(&mut handshake)
        .await
        .expect("Failed to read 80-byte handshake");

    let code_len = expected_access_code.len();
    assert_eq!(
        &handshake[48..48 + code_len],
        expected_access_code.as_bytes(),
        "Invalid handshake access code"
    );

    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&payload_len.to_le_bytes());
    stream
        .write_all(&header)
        .await
        .expect("Failed to write camera frame header");

    let partial_payload = vec![0xFFu8; bytes_before_drop];
    stream
        .write_all(&partial_payload)
        .await
        .expect("Failed to write partial camera frame payload");
    stream.flush().await.expect("Failed to flush partial frame");

    // Drop the stream without writing the remaining declared bytes.
}
