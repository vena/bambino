//! # RTSPS Stream Configuration & Real-Time Correction (Port 322)
//!
//! Exposes helper utilities for establishing secure video streams over Port 322 [REF-CAM-RTSPS].
//! Includes helper utilities to generate authorization-compliant RTSPS endpoints,
//! perform proxy request-line rewrites, and resolve P2S static RTP timestamp freezing bugs.
//!
//! **P2S RTP Timestamp Freeze Quirk [REF-CAM-RTSPS]:**
//! Certain premium printers (P2S series) running firmware tracks like `01.02.00.00` suffer
//! from an embedded encoder bug where every H.264 frame is stamped with a static RTP timestamp
//! of approximately `0.06` seconds. Standard decoders (such as FFmpeg/GStreamer) interpret
//! non-advancing stream markers as duplicates and drop subsequent frames, causing video freezes.
//!
//! To circumvent this, the `RtpTimestampCorrector` tracks packet arrival intervals on the host
//! and synthesizes monotonically advancing timestamps mapped to the standard 90,000 Hz video clock
//! required by modern media frameworks.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

pub(crate) const RTP_CLOCK_FREQUENCY_HZ: u32 = 90000;

/// Formats the standard implicit TLS RTSPS connection path utilized by Bambu Lab printers [REF-CAM-RTSPS].
pub fn build_rtsps_url(ip: &str, access_code: &str) -> String {
    format!("rtsps://bblp:{}@{}:322/streaming/live/1", access_code, ip)
}

/// Rewrites a plain RTSP request-line URL to its secure RTSPS counterpart in-transit.
///
/// **Why this is required [REF-CAM-RTSPS]:**
/// Plain-to-secure decryption proxies wrapped around local mediaplayer instances receive plain
/// connections (`rtsp://127.0.0.1:<local_port>`). However, when calculating cryptographic Digest
/// hashes, the printer's broker verifies the request-line URI. If the URI string mismatches the
/// printer's secure destination target, authorization fails with `401 Unauthorized`.
///
/// This helper replaces localhost references with the remote printer target while leaving
/// query strings and transport blocks unmodified.
pub fn rewrite_rtsp_request_uri(request_line: &str, printer_ip: &str) -> String {
    if let Some(start_idx) = request_line.find("rtsp://") {
        let remainder = &request_line[start_idx + 7..];
        // Split by first slash to separate host segment from path segment
        let mut split = remainder.splitn(2, '/');
        if let Some(_host) = split.next() {
            let path = split.next().unwrap_or("");
            return format!("rtsps://{}:322/{}", printer_ip, path);
        }
    }
    String::from(request_line)
}

/// Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.
pub struct RtpTimestampCorrector {
    base_timestamp: u32,
    has_initiated: bool,
    frequency_hz: u32,
}

impl Default for RtpTimestampCorrector {
    fn default() -> Self {
        Self::new()
    }
}

impl RtpTimestampCorrector {
    /// Instantiates a corrector mapping frame deltas to the standard 90,000 Hz video stream clock.
    pub fn new() -> Self {
        Self {
            base_timestamp: 0,
            has_initiated: false,
            frequency_hz: RTP_CLOCK_FREQUENCY_HZ,
        }
    }

    /// Computes the corrected RTP timestamp sequence number based on host-observed arrival deltas.
    ///
    /// * `elapsed_secs`: Total accumulated seconds since the first stream packet arrived.
    /// * `embedded_rtp`: The raw timestamp parsed from the stream packet.
    ///
    /// If the stream has just initialized, we preserve the original `embedded_rtp` as our base
    /// coordinate to ensure alignment with standard SDP stream definitions.
    pub fn correct_timestamp(&mut self, elapsed_secs: f64, embedded_rtp: u32) -> u32 {
        if !self.has_initiated {
            self.base_timestamp = embedded_rtp;
            self.has_initiated = true;
            return embedded_rtp;
        }

        // Multiply elapsed host seconds against 90kHz clock scale
        let raw = elapsed_secs * self.frequency_hz as f64;
        let rtp_delta = (raw + 0.5) as u32;

        self.base_timestamp.wrapping_add(rtp_delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_rtsps_url() {
        let url = build_rtsps_url("192.168.1.150", "12345678");
        assert_eq!(
            url,
            "rtsps://bblp:12345678@192.168.1.150:322/streaming/live/1"
        );
    }

    #[test]
    fn test_rtsp_proxy_uri_rewrite() {
        // Mock incoming proxy header from player
        let incoming_uri = "rtsp://127.0.0.1:8554/streaming/live/1";
        let target_ip = "192.168.1.150";

        let rewritten = rewrite_rtsp_request_uri(incoming_uri, target_ip);
        assert_eq!(rewritten, "rtsps://192.168.1.150:322/streaming/live/1");
    }

    #[test]
    fn test_timestamp_freezing_correction() {
        let mut corrector = RtpTimestampCorrector::new();

        // Frame 1: Embedded RTP starts at some arbitrary value
        let corrected_1 = corrector.correct_timestamp(0.0, 54000);
        assert_eq!(corrected_1, 54000);

        // Frame 2: Arrives after 1.5 seconds. Embedded RTP remains stuck at 54000
        let corrected_2 = corrector.correct_timestamp(1.5, 54000);
        // Delta = 1.5 * 90000 = 135000 clock units. Expected = 54000 + 135000 = 189000
        assert_eq!(corrected_2, 189000);

        // Frame 3: Arrives after 2.0 seconds. Embedded RTP still stuck at 54000
        let corrected_3 = corrector.correct_timestamp(2.0, 54000);
        // Delta = 2.0 * 90000 = 180000 clock units. Expected = 54000 + 180000 = 234000
        assert_eq!(corrected_3, 234000);
    }
}
