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
//! 2. Clamps incoming frame sizes to a reasonable upper boundary (5MB) to protect against
//!    unbounded memory allocation crashes on low-resource environments if transport stream
//!    corruption occurs.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError};

/// Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].
///
/// **Byte Ordering Specifications:**
/// * Offset 0-3 (4 bytes): Magic identifier header (`0x00000040` / 64)
/// * Offset 4-7 (4 bytes): Control operation Command ID (`0x00003000` / 12288)
/// * Offset 8-15 (8 bytes): Zero-padding block
/// * Offset 16-47 (32 bytes): Null-padded ASCII username (`"bblp"`)
/// * Offset 48-79 (32 bytes): Null-padded ASCII LAN access code
pub fn build_handshake_packet(access_code: &str) -> Result<[u8; 80], BambuError> {
    let mut packet = [0u8; 80];

    // Magic Header definition
    packet[0..4].copy_from_slice(&64u32.to_le_bytes());

    // Connection Command ID registration
    packet[4..8].copy_from_slice(&12288u32.to_le_bytes());

    // Fill Username buffer block (null-terminated)
    let username = b"bblp";
    packet[16..16 + username.len()].copy_from_slice(username);

    // Validate and write password/LAN access code buffer block (null-terminated)
    let code_bytes = access_code.as_bytes();
    if code_bytes.len() > 32 {
        return Err(BambuError::ProtocolViolation(
            "Access code length exceeds maximum 32-byte authorization boundary",
        ));
    }
    packet[48..48 + code_bytes.len()].copy_from_slice(code_bytes);

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
        let mut header = [0u8; 16];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;

        // Extract little-endian payload size N from first 4 bytes
        let size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;

        // Bounded allocation check to guard against memory allocation overflow attacks
        if size > 5 * 1024 * 1024 {
            return Err(BambuError::ProtocolViolation(
                "Extracted JPEG frame size exceeds safety allocation limit (5MB)",
            ));
        }

        if size == 0 {
            return Err(BambuError::ProtocolViolation(
                "Acquired empty frame payload descriptor",
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
            || frame_buf[0] != 0xFF
            || frame_buf[1] != 0xD8
            || frame_buf[size - 2] != 0xFF
            || frame_buf[size - 1] != 0xD9
        {
            return Err(BambuError::ProtocolViolation(
                "Acquired stream packet lacks valid JPEG magic marker boundaries",
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

        // Validate magic header payload
        assert_eq!(packet[0..4], 64u32.to_le_bytes());

        // Validate control command payload
        assert_eq!(packet[4..8], 12288u32.to_le_bytes());

        // Validate Null-padded username block
        assert_eq!(&packet[16..20], b"bblp");
        assert_eq!(packet[20], 0);

        // Validate Null-padded access code
        assert_eq!(&packet[48..56], b"ABCDEF12");
        assert_eq!(packet[56], 0);
    }
}
