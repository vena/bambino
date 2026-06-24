//! # Camera & Video Streaming Systems Module
//!
//! Exposes interfaces and parsers for extraction of video/image feeds from
//! physical Bambu Lab printers [REF-CAM-RTSPS].
//!
//! Physical printers support camera data extraction via two distinct interfaces
//! to accommodate varied onboard network processor workloads:
//!
//! 1. **Implicit TLS RTSPS Stream (Port 322)**: Supported on higher-capability CoreXY and H2/IDEX
//!    platforms. Employs H.264 video streams requiring standard RTSP handshakes and Digest
//!    Authentication challenges.
//! 2. **Proprietary Binary JPEG Stream (Port 6000)**: Supported on constrained microcontrollers
//!    (such as ESP32-based P1 and A1 lines). Delivers discrete JPEG frames over a lightweight,
//!    custom-wrapped TCP stream to minimize memory and processing overhead [REF-CAM-BINARY].

pub mod binary;
pub mod rtsps;

pub const CAMERA_PORT_RTSPS: u16 = 322;
pub const CAMERA_PORT_BINARY_JPEG: u16 = 6000;

/// Defines the operational camera interface type associated with the printer hardware line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraProtocol {
    /// RTSP stream wrapped in implicit TLS on Port 322 (X1, X2D, P2S, and H2 series).
    Rtsps,
    /// Custom binary TCP packet loop returning JPEG frames on Port 6000 (P1 and A1 series).
    BinaryJpeg,
}

impl CameraProtocol {
    /// Returns the standard TCP port associated with the physical interface.
    pub fn default_port(&self) -> u16 {
        match self {
            CameraProtocol::Rtsps => CAMERA_PORT_RTSPS,
            CameraProtocol::BinaryJpeg => CAMERA_PORT_BINARY_JPEG,
        }
    }
}
