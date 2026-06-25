//! # RTSPS Stream Helpers (Port 322)
//!
//! Utilities for integrating with the RTSPS video stream on higher-capability Bambu Lab
//! printers (X1, X2, H2, P2S series). These printers host a local RTSP server wrapped in
//! implicit TLS on port 322, using Digest authentication with the printer's LAN access code.
//!
//! This module does **not** implement an RTSP client or TLS proxy. It provides building
//! blocks for callers integrating with external media frameworks (FFmpeg, GStreamer, VLC):
//!
//! - [`build_rtsps_url`] — generates the authenticated RTSPS URL for direct consumption
//! - [`rewrite_rtsp_request_uri`] — rewrites proxy-local URIs for Digest auth correctness
//! - [`RtpTimestampCorrector`] — fixes frozen RTP timestamps on affected P2S firmware
//!
//! # RTSPS proxy architecture
//!
//! The printer's RTSPS server uses a self-signed TLS certificate that standard media players
//! cannot validate. The common integration pattern is a local decryption proxy:
//!
//! 1. A proxy listens on `127.0.0.1:<local_port>` accepting plain `rtsp://` connections
//! 2. The media player connects to `rtsp://127.0.0.1:<local_port>/streaming/live/1`
//! 3. The proxy wraps traffic in TLS and forwards to `rtsps://<printer_ip>:322/...`
//!
//! RTSP Digest authentication hashes include the request-line URI. The printer expects
//! `rtsps://<printer_ip>:322/...` but the player sends `rtsp://127.0.0.1:...`. If the
//! proxy forwards the URI verbatim, the hash mismatches and the printer returns 401.
//! [`rewrite_rtsp_request_uri`] performs the in-flight rewrite to fix this.
//!
//! # P2S RTP timestamp freeze
//!
//! P2S printers on firmware `01.02.00.00` have an encoder bug where every H.264 frame
//! carries the same RTP timestamp (~0.06s). Decoders interpret non-advancing timestamps as
//! duplicates and drop frames, freezing the video. [`RtpTimestampCorrector`] replaces the
//! frozen timestamps with host-computed values on the standard 90 kHz RTP clock. Use
//! [`ModelQuirks::requires_wallclock_rtsp_timestamps()`](crate::quirks::ModelQuirks::requires_wallclock_rtsp_timestamps)
//! to check whether the connected model needs this correction.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

pub(crate) const RTP_CLOCK_FREQUENCY_HZ: u32 = 90000;

/// Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream.
///
/// The returned URL can be passed directly to media frameworks that support RTSPS with
/// Digest authentication, or used as the target endpoint for a local decryption proxy
/// (see module-level docs for the proxy pattern).
pub fn build_rtsps_url(ip: &str, access_code: &str) -> String {
    format!("rtsps://bblp:{}@{}:322/streaming/live/1", access_code, ip)
}

/// Rewrites a plain `rtsp://` proxy URI to the printer's `rtsps://` endpoint.
///
/// When running a local decryption proxy (see module-level docs), media players send
/// requests to `rtsp://127.0.0.1:<local_port>/...`. RTSP Digest authentication includes
/// the request-line URI in its hash, so the printer expects `rtsps://<ip>:322/...`. This
/// function performs the in-flight rewrite, replacing the scheme and host while preserving
/// the path and query string.
///
/// If the input does not contain `rtsp://` (e.g. it's already `rtsps://`), it is returned
/// unchanged.
///
/// This function expects proxy-generated URIs with a simple `rtsp://host:port/path` structure.
/// It is not a general-purpose URI parser.
pub fn rewrite_rtsp_request_uri(request_uri: &str, printer_ip: &str) -> String {
    if let Some(start_idx) = request_uri.find("rtsp://") {
        let remainder = &request_uri[start_idx + 7..];
        let mut split = remainder.splitn(2, '/');
        if let Some(_host) = split.next() {
            let path = split.next().unwrap_or("");
            return format!("rtsps://{}:322/{}", printer_ip, path);
        }
    }
    String::from(request_uri)
}

/// Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.
pub struct RtpTimestampCorrector {
    base_timestamp: u32,
    frequency_hz: u32,
}

impl RtpTimestampCorrector {
    /// Initializes the corrector by capturing the stream's first embedded RTP timestamp
    /// as the base coordinate for all subsequent corrections. This preserves alignment
    /// with the SDP stream definition.
    pub fn init(embedded_rtp: u32) -> Self {
        Self {
            base_timestamp: embedded_rtp,
            frequency_hz: RTP_CLOCK_FREQUENCY_HZ,
        }
    }

    /// Computes the corrected RTP timestamp from host-observed elapsed time.
    ///
    /// * `elapsed_secs`: Total accumulated seconds since the first stream packet arrived.
    pub fn correct(&self, elapsed_secs: f64) -> u32 {
        let raw = elapsed_secs * self.frequency_hz as f64;
        // Truncate via u64 intermediate to preserve wrapping semantics for streams
        // exceeding ~13.25 hours (where f64 -> u32 would saturate at u32::MAX).
        let rtp_delta = (raw + 0.5) as u64 as u32;
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
        let incoming_uri = "rtsp://127.0.0.1:8554/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(incoming_uri, "192.168.1.150");
        assert_eq!(rewritten, "rtsps://192.168.1.150:322/streaming/live/1");
    }

    #[test]
    fn test_rewrite_uri_with_query_string() {
        let uri = "rtsp://127.0.0.1:8554/streaming/live/1?token=abc&quality=high";
        let rewritten = rewrite_rtsp_request_uri(uri, "10.0.0.5");
        assert_eq!(
            rewritten,
            "rtsps://10.0.0.5:322/streaming/live/1?token=abc&quality=high"
        );
    }

    #[test]
    fn test_rewrite_uri_already_rtsps_returns_unchanged() {
        let uri = "rtsps://192.168.1.150:322/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(uri, "10.0.0.5");
        assert_eq!(rewritten, uri);
    }

    #[test]
    fn test_rewrite_uri_no_path() {
        let uri = "rtsp://127.0.0.1:8554";
        let rewritten = rewrite_rtsp_request_uri(uri, "192.168.1.150");
        assert_eq!(rewritten, "rtsps://192.168.1.150:322/");
    }

    #[test]
    fn test_timestamp_freezing_correction() {
        let corrector = RtpTimestampCorrector::init(54000);

        // Frame at t=0: base timestamp returned via wrapping_add(0)
        assert_eq!(corrector.correct(0.0), 54000);

        // Frame at t=1.5s: delta = 1.5 * 90000 = 135000
        assert_eq!(corrector.correct(1.5), 189000);

        // Frame at t=2.0s: delta = 2.0 * 90000 = 180000
        assert_eq!(corrector.correct(2.0), 234000);
    }

    #[test]
    fn test_timestamp_corrector_wraps_after_13_hours() {
        let corrector = RtpTimestampCorrector::init(0);

        // 50000 seconds (~13.9 hours) at 90kHz = 4,500,000,000 which exceeds u32::MAX
        // Should wrap correctly, not saturate
        let ts = corrector.correct(50000.0);
        let expected = (50000.0 * 90000.0 + 0.5) as u64 as u32;
        assert_eq!(ts, expected);
        assert_ne!(ts, u32::MAX, "must wrap, not saturate");
    }
}
