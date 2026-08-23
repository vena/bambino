//! # Camera & Video Streaming
//!
//! Bambu Lab printers expose camera feeds through two protocols:
//!
//! 1. **Binary JPEG (Port 6000)** — A1, A1 Mini, A2L, and P1 series. A lightweight binary protocol that
//!    streams discrete JPEG frames over TLS. This module provides a complete client
//!    ([`binary::BinaryCameraStream`]) that handles the handshake and frame extraction.
//!
//! 2. **RTSPS (Port 322)** — X1, X2, H2, and P2S series. An RTSP server behind implicit TLS
//!    with Digest authentication. This module provides helper utilities ([`rtsps`]) for
//!    integrating with external media frameworks (FFmpeg, GStreamer, VLC), including URL
//!    generation, proxy URI rewriting, and P2S timestamp correction. It does **not** include
//!    an RTSP client or TLS proxy — see the [`rtsps`] module docs for the proxy architecture.

pub mod binary;
pub mod rtsps;

/// Default port for RTSPS camera streams (X1, X2, H2, P2S series).
pub const CAMERA_PORT_RTSPS: u16 = 322;
/// Default port for binary JPEG camera streams (A1, A1 Mini, A2L, and P1 series).
///
/// The printer accepts only one connection to this port at a time. A caller redialing it
/// immediately after disconnecting can orphan the prior socket server-side until keepalive
/// reaps it (~20 min stall) — wait for the old connection to fully close, or add a delay,
/// before reconnecting. See [`binary::BinaryCameraStream`]'s doc comment.
pub const CAMERA_PORT_BINARY_JPEG: u16 = 6000;

/// Which camera streaming protocol a printer model uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraProtocol {
    /// RTSP stream wrapped in implicit TLS on Port 322 (X1, X2D, P2S, and H2 series).
    Rtsps,
    /// Custom binary TCP packet loop returning JPEG frames on Port 6000 (P1 and A1 series, including A2L).
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
