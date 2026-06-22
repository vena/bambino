//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters for the P2S platform.
//!
//! **FTPS TLS 1.3 Ticket Failure [REF-FTPS-CONN]:**
//! The embedded vsFTPd daemon fails to process asynchronous session-ticket
//! resumption on TLS 1.3 data channels, resulting in premature transfer truncation.
//! Security configurations must force TLS 1.2 on passive ports.

/// Forces TLS v1.2 restriction to avoid data channel session-close races
pub fn force_tls_v12_for_ftps() -> bool {
    true
}

/// Constant frame rate camera sync parameters to resolve RTP timestamp freezing bugs [REF-CAM-RTSPS]
pub fn requires_wallclock_rtsp_timestamps() -> bool {
    true
}
