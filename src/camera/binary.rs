//! # Chamber Image Binary JPEG Socket Protocol (Port 6000)
//!
//! Handles connection handshakes and payload processing for constrained printer lines
//! (P1 and A1 series) transmitting discrete camera frames over raw TLS TCP sockets [REF-CAM-BINARY].
//!
//! **Handshake Architecture [REF-CAM-BINARY]:**
//! Upon establishing a TLS session, the connecting client must immediately transmit a
//! packed 80-byte authentication packet formatted in little-endian order. If the handshake is
//! accepted, the physical machine begins continuously writing raw JPEG frames prefixed with
//! a standard 16-byte length descriptor.
//!
//! **Flow Integrity Guards:**
//! 1. Verifies that incoming payloads conform strictly to JPEG magic start (`FF D8`) and
//!    end (`FF D9`) markers before returning buffers to upstream applications to insulate
//!    against decoding crashes.
//! 2. Clamps incoming frame sizes to a reasonable upper boundary (10MB) to protect against
//!    unbounded memory allocation crashes on low-resource environments if transport stream
//!    corruption occurs.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError};

pub(crate) const CAMERA_HANDSHAKE_SIZE: usize = 80;
pub(crate) const CAMERA_HANDSHAKE_MAGIC: u32 = 64;
pub(crate) const CAMERA_HANDSHAKE_COMMAND_ID: u32 = 12288;
pub(crate) const CAMERA_USERNAME_OFFSET: usize = 16;
pub(crate) const CAMERA_PASSWORD_OFFSET: usize = 48;
pub(crate) const CAMERA_PASSWORD_MAX_LEN: usize = 32;
pub(crate) const CAMERA_FRAME_HEADER_SIZE: usize = 16;
pub(crate) const CAMERA_FRAME_MAX_SIZE: usize = 10 * 1024 * 1024;
pub(crate) const JPEG_MARKER_SOI_HIGH: u8 = 0xFF;
pub(crate) const JPEG_MARKER_SOI_LOW: u8 = 0xD8;
pub(crate) const JPEG_MARKER_EOI_HIGH: u8 = 0xFF;
pub(crate) const JPEG_MARKER_EOI_LOW: u8 = 0xD9;

/// Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].
///
/// **Byte Ordering Specifications:**
/// * Offset 0-3 (4 bytes): Magic identifier header (`0x00000040` / 64)
/// * Offset 4-7 (4 bytes): Control operation Command ID (`0x00003000` / 12288)
/// * Offset 8-15 (8 bytes): Zero-padding block
/// * Offset 16-47 (32 bytes): Null-padded ASCII username (`"bblp"`)
/// * Offset 48-79 (32 bytes): Null-padded ASCII LAN access code
pub fn build_handshake_packet(
    access_code: &str,
) -> Result<[u8; CAMERA_HANDSHAKE_SIZE], BambuError> {
    let mut packet = [0u8; CAMERA_HANDSHAKE_SIZE];

    packet[0..4].copy_from_slice(&CAMERA_HANDSHAKE_MAGIC.to_le_bytes());
    packet[4..8].copy_from_slice(&CAMERA_HANDSHAKE_COMMAND_ID.to_le_bytes());

    let username = b"bblp";
    packet[CAMERA_USERNAME_OFFSET..CAMERA_USERNAME_OFFSET + username.len()]
        .copy_from_slice(username);

    let code_bytes = access_code.as_bytes();
    if code_bytes.len() > CAMERA_PASSWORD_MAX_LEN {
        return Err(BambuError::ProtocolViolation(
            "Access code length exceeds maximum 32-byte authorization boundary".into(),
        ));
    }
    packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + code_bytes.len()]
        .copy_from_slice(code_bytes);

    Ok(packet)
}

/// Abstract state controller parsing incoming frame buffers from raw Port 6000 streams.
pub struct BambuBinaryCameraStream<IO: AsyncIo> {
    stream: IO,
}

impl<IO: AsyncIo> BambuBinaryCameraStream<IO> {
    /// Instantiates a camera parser wrapper surrounding an active secure stream socket.
    pub fn new(stream: IO) -> Self {
        Self { stream }
    }

    /// Transmits the 80-byte authentication handshake to activate the continuous frame-push process.
    pub async fn authenticate(&mut self, access_code: &str) -> Result<(), BambuError> {
        let handshake = build_handshake_packet(access_code)?;
        self.stream
            .write_all(&handshake)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        Ok(())
    }

    /// Asynchronously extracts the next complete frame from the stream.
    ///
    /// Refills the user-supplied `Vec<u8>` to minimize memory churn during high-frequency
    /// image extraction operations.
    pub async fn read_next_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), BambuError> {
        // Read 16-byte frame metadata descriptor
        let mut header = [0u8; CAMERA_FRAME_HEADER_SIZE];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;

        // Extract little-endian payload size N from first 4 bytes
        let size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;

        // Bounded allocation check to guard against memory allocation overflow attacks
        if size > CAMERA_FRAME_MAX_SIZE {
            return Err(BambuError::ProtocolViolation(
                "Extracted JPEG frame size exceeds safety allocation limit (10MB)".into(),
            ));
        }

        if size == 0 {
            return Err(BambuError::ProtocolViolation(
                "Acquired empty frame payload descriptor".into(),
            ));
        }

        // Fetch exactly N payload bytes representing raw image data
        frame_buf.resize(size, 0);
        self.stream
            .read_exact(frame_buf)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;

        // Validate frame bounds to protect downstream graphic engines against decoding crashes
        if size < 4
            || frame_buf[0] != JPEG_MARKER_SOI_HIGH
            || frame_buf[1] != JPEG_MARKER_SOI_LOW
            || frame_buf[size - 2] != JPEG_MARKER_EOI_HIGH
            || frame_buf[size - 1] != JPEG_MARKER_EOI_LOW
        {
            return Err(BambuError::ProtocolViolation(
                "Acquired stream packet lacks valid JPEG magic marker boundaries".into(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_packet_construction() {
        let packet = build_handshake_packet("ABCDEF12").unwrap();

        assert_eq!(packet[0..4], 64u32.to_le_bytes());
        assert_eq!(packet[4..8], 12288u32.to_le_bytes());
        assert_eq!(&packet[16..20], b"bblp");
        assert_eq!(packet[20], 0);
        assert_eq!(&packet[48..56], b"ABCDEF12");
        assert_eq!(packet[56], 0);
    }

    #[test]
    fn test_handshake_max_length_access_code() {
        let code = "A".repeat(CAMERA_PASSWORD_MAX_LEN);
        let packet = build_handshake_packet(&code).unwrap();
        assert_eq!(
            &packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + 32],
            code.as_bytes()
        );
    }

    #[test]
    fn test_handshake_oversized_access_code() {
        let code = "A".repeat(CAMERA_PASSWORD_MAX_LEN + 1);
        let result = build_handshake_packet(&code);
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::*;
        use crate::io::TokioIo;

        fn make_frame_header(size: u32) -> Vec<u8> {
            let mut header = vec![0u8; CAMERA_FRAME_HEADER_SIZE];
            header[0..4].copy_from_slice(&size.to_le_bytes());
            header
        }

        #[tokio::test]
        async fn test_read_frame_oversized() {
            let data = make_frame_header((CAMERA_FRAME_MAX_SIZE + 1) as u32);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_zero_size() {
            let data = make_frame_header(0);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_invalid_jpeg_markers() {
            let mut data = make_frame_header(4);
            data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
        }
    }
}
