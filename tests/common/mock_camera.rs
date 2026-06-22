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
    // 1. Await 80-byte handshake payload
    let mut handshake = [0u8; 80];
    stream
        .read_exact(&mut handshake)
        .await
        .expect("Failed to read 80-byte handshake");

    // Validate Magic Identifier (64 / 0x00000040)
    assert_eq!(
        &handshake[0..4],
        64u32.to_le_bytes(),
        "Invalid handshake magic header"
    );

    // Validate Command Identifier (12288 / 0x00003000)
    assert_eq!(
        &handshake[4..8],
        12288u32.to_le_bytes(),
        "Invalid handshake command header"
    );

    // Validate Username (`"bblp"`)
    assert_eq!(&handshake[16..20], b"bblp", "Invalid handshake username");

    // Validate Access Code
    let code_len = expected_access_code.len();
    assert_eq!(
        &handshake[48..48 + code_len],
        expected_access_code.as_bytes(),
        "Invalid handshake access code"
    );

    // 2. Stream Mock JPEG Frames
    for i in 0..frame_count {
        // Construct a mock payload mapping to typical JPEG structures
        // Format: [FF D8] + [Arbitrary Data] + [FF D9]
        let mut mock_image = vec![0xFF, 0xD8];

        // Add some arbitrary internal payload data to simulate frame variance
        let inner_data = format!("MOCK_JPEG_PAYLOAD_{}", i);
        mock_image.extend_from_slice(inner_data.as_bytes());

        // Append terminal JPEG magic markers
        mock_image.extend_from_slice(&[0xFF, 0xD9]);

        let payload_size = mock_image.len() as u32;

        // Construct 16-byte metadata header (First 4 bytes = little-endian payload size)
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&payload_size.to_le_bytes());

        // Emit header followed by the binary frame
        stream.write_all(&header).await.unwrap();
        stream.write_all(&mock_image).await.unwrap();
        stream.flush().await.unwrap();
    }
}
