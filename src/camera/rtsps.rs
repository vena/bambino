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
//! `rtsps://<printer_ip>:322/...` but the player sends `rtsp://127.0.0.1:...`.
//! [`rewrite_rtsp_request_uri`] rewrites the request-line/URI text so a proxy that acts as
//! its own independent RTSP client toward the printer (computing its own Digest response
//! against the rewritten URI) sends the correct URI. **It does not recompute or repair an
//! already-computed Digest `Authorization` header** — a transparent relay that forwards the
//! player's original `Authorization` header verbatim will still get a 401, because that
//! header's `response=` hash was computed by the player against its own local URI and this
//! function has no way to update it (see the function's own doc comment for detail).
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

use crate::camera::CAMERA_PORT_RTSPS;
use crate::error::Error;

pub(crate) const RTP_CLOCK_FREQUENCY_HZ: u32 = 90000;

/// Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream.
///
/// The returned URL can be passed directly to media frameworks that support RTSPS with
/// Digest authentication, or used as the target endpoint for a local decryption proxy
/// (see module-level docs for the proxy pattern).
///
/// # Errors
///
/// Returns [`Error::ProtocolViolation`] if `access_code` is empty or contains any
/// character outside ASCII letters/digits. Genuine printer-issued LAN access codes are
/// always 8 uppercase ASCII alphanumeric characters, so a rejection here almost always
/// means a copy-paste mistake (stray whitespace, a trailing newline) rather than a
/// valid-but-unusual code — surfacing it as an error catches that mistake instead of
/// silently building a malformed URL.
///
/// Also returns [`Error::ProtocolViolation`] if `ip` does not parse as a valid IPv4 or
/// IPv6 address. Without this check, an `ip` containing an embedded `@` (e.g.
/// `"1.2.3.4@attacker.example.com"`, spoofable by any device on the LAN via SSDP/mDNS
/// discovery) would place everything up to the last `@` into the URL's userinfo component,
/// redirecting the connection — and the LAN access code — to an attacker-controlled host.
pub fn build_rtsps_url(ip: &str, access_code: &str) -> Result<String, Error> {
    if access_code.is_empty() || !access_code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::ProtocolViolation(
            "access_code must be a non-empty ASCII alphanumeric string".into(),
        ));
    }
    let Ok(ip_addr) = ip.parse::<core::net::IpAddr>() else {
        return Err(Error::ProtocolViolation(
            "ip must be a valid IPv4 or IPv6 address".into(),
        ));
    };
    // RFC 3986 §3.2.2: an IPv6 literal used as a URI host must be bracketed, or its colons
    // are indistinguishable from the port separator to a conforming URI parser.
    let host = if ip_addr.is_ipv6() {
        format!("[{}]", ip)
    } else {
        String::from(ip)
    };
    Ok(format!(
        "rtsps://bblp:{}@{}:{}/streaming/live/1",
        access_code, host, CAMERA_PORT_RTSPS
    ))
}

/// Rewrites a plain `rtsp://` proxy URI to the printer's `rtsps://` endpoint.
///
/// When running a local decryption proxy (see module-level docs), media players send
/// requests to `rtsp://127.0.0.1:<local_port>/...`. RTSP Digest authentication includes
/// the request-line URI in its hash, so the printer expects `rtsps://<ip>:322/...`. This
/// function performs pure text surgery on the request-line/URI: it replaces the scheme and
/// host while preserving the path and query string, nothing else.
///
/// **This function does not repair an already-computed Digest `Authorization` header.** It
/// never sees an `Authorization` header, a nonce, a realm, or the access code, so it cannot
/// compute or correct an HA1/HA2/`response=` MD5 value. It is only useful to a proxy that
/// acts as its own independent RTSP client toward the printer — i.e. one that computes its
/// own Digest response against the rewritten URI returned here. A transparent-relay proxy
/// that forwards the player's original `Authorization` header verbatim will still receive a
/// 401: that header's `response=` value was computed by the player against its own local
/// (`rtsp://127.0.0.1:...`) URI, and nothing here updates it to match the rewritten one.
///
/// If the input does not start with `rtsp://` (e.g. it's already `rtsps://`), it is returned
/// unchanged.
///
/// This function expects proxy-generated URIs with a simple `rtsp://host:port/path` structure.
/// It is not a general-purpose URI parser.
///
/// # Errors
///
/// Returns [`Error::ProtocolViolation`] if `printer_ip` does not parse as a valid IPv4 or
/// IPv6 address — the same check [`build_rtsps_url`] applies to its own `ip` parameter, and
/// for the same reason: a `printer_ip` containing `@` or `/` (e.g. sourced from a
/// spoofable SSDP/mDNS discovery response, same as [`build_rtsps_url`]'s hazard) could
/// otherwise redirect the proxy's outbound connection or produce a malformed URI. This
/// function has no other caller in this crate to rely on for pre-validation — it's called
/// once per incoming request in a proxy's hot path, but IP-string parsing is cheap enough
/// that re-validating here is not a meaningful cost.
pub fn rewrite_rtsp_request_uri(request_uri: &str, printer_ip: &str) -> Result<String, Error> {
    let Ok(printer_ip_addr) = printer_ip.parse::<core::net::IpAddr>() else {
        return Err(Error::ProtocolViolation(
            "printer_ip must be a valid IPv4 or IPv6 address".into(),
        ));
    };
    // RFC 3986 §3.2.2: bracket IPv6 literals, matching build_rtsps_url.
    let host = if printer_ip_addr.is_ipv6() {
        format!("[{}]", printer_ip)
    } else {
        String::from(printer_ip)
    };
    if let Some(remainder) = request_uri.strip_prefix("rtsp://") {
        let mut split = remainder.splitn(2, '/');
        if let Some(_host) = split.next() {
            let path = split.next().unwrap_or("");
            return Ok(format!("rtsps://{}:{}/{}", host, CAMERA_PORT_RTSPS, path));
        }
    }
    Ok(String::from(request_uri))
}

/// Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.
pub struct RtpTimestampCorrector {
    base_timestamp: u32,
    frequency_hz: u32,
}

impl RtpTimestampCorrector {
    /// Initializes the corrector by capturing the stream's first embedded RTP timestamp as the base coordinate for all subsequent corrections.
    /// This preserves alignment with the SDP stream definition.
    pub fn new(embedded_rtp: u32) -> Self {
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
        let url = build_rtsps_url("192.168.1.150", "12345678").unwrap();
        assert_eq!(
            url,
            "rtsps://bblp:12345678@192.168.1.150:322/streaming/live/1"
        );
    }

    #[test]
    fn test_build_rtsps_url_rejects_empty_access_code() {
        assert!(build_rtsps_url("192.168.1.150", "").is_err());
    }

    #[test]
    fn test_build_rtsps_url_rejects_non_alphanumeric_access_code() {
        assert!(build_rtsps_url("192.168.1.150", "1234@678").is_err());
        assert!(build_rtsps_url("192.168.1.150", "1234 678").is_err());
        assert!(build_rtsps_url("192.168.1.150", "1234\n678").is_err());
    }

    #[test]
    fn test_build_rtsps_url_rejects_ip_with_embedded_at() {
        assert!(build_rtsps_url("1.2.3.4@attacker.example.com", "12345678").is_err());
    }

    #[test]
    fn test_build_rtsps_url_rejects_non_ip_hostname() {
        assert!(build_rtsps_url("attacker.example.com", "12345678").is_err());
    }

    #[test]
    fn test_build_rtsps_url_accepts_ipv6() {
        // BUG-005: an unbracketed IPv6 literal is malformed per RFC 3986 §3.2.2 — its colons
        // are indistinguishable from the port separator to a conforming URI parser.
        let url = build_rtsps_url("fe80::1", "12345678").unwrap();
        assert_eq!(url, "rtsps://bblp:12345678@[fe80::1]:322/streaming/live/1");
    }

    #[test]
    fn test_rtsp_proxy_uri_rewrite() {
        let incoming_uri = "rtsp://127.0.0.1:8554/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(incoming_uri, "192.168.1.150").unwrap();
        assert_eq!(rewritten, "rtsps://192.168.1.150:322/streaming/live/1");
    }

    #[test]
    fn test_rewrite_uri_with_query_string() {
        let uri = "rtsp://127.0.0.1:8554/streaming/live/1?token=abc&quality=high";
        let rewritten = rewrite_rtsp_request_uri(uri, "10.0.0.5").unwrap();
        assert_eq!(
            rewritten,
            "rtsps://10.0.0.5:322/streaming/live/1?token=abc&quality=high"
        );
    }

    #[test]
    fn test_rewrite_uri_already_rtsps_returns_unchanged() {
        let uri = "rtsps://192.168.1.150:322/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(uri, "10.0.0.5").unwrap();
        assert_eq!(rewritten, uri);
    }

    #[test]
    fn test_rewrite_uri_no_path() {
        let uri = "rtsp://127.0.0.1:8554";
        let rewritten = rewrite_rtsp_request_uri(uri, "192.168.1.150").unwrap();
        assert_eq!(rewritten, "rtsps://192.168.1.150:322/");
    }

    #[test]
    fn test_rewrite_uri_rejects_ip_with_embedded_at() {
        let uri = "rtsp://127.0.0.1:8554/streaming/live/1";
        assert!(rewrite_rtsp_request_uri(uri, "1.2.3.4@attacker.example.com").is_err());
    }

    #[test]
    fn test_rewrite_uri_brackets_ipv6_printer_ip() {
        // BUG-005: same RFC 3986 §3.2.2 bracketing requirement as build_rtsps_url.
        let uri = "rtsp://127.0.0.1:8554/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(uri, "fe80::1").unwrap();
        assert_eq!(rewritten, "rtsps://[fe80::1]:322/streaming/live/1");
    }

    #[test]
    fn test_timestamp_freezing_correction() {
        let corrector = RtpTimestampCorrector::new(54000);

        // Frame at t=0: base timestamp returned via wrapping_add(0)
        assert_eq!(corrector.correct(0.0), 54000);

        // Frame at t=1.5s: delta = 1.5 * 90000 = 135000
        assert_eq!(corrector.correct(1.5), 189000);

        // Frame at t=2.0s: delta = 2.0 * 90000 = 180000
        assert_eq!(corrector.correct(2.0), 234000);
    }

    #[test]
    fn test_timestamp_corrector_wraps_after_13_hours() {
        let corrector = RtpTimestampCorrector::new(0);

        // 50000 seconds (~13.9 hours) at 90kHz = 4,500,000,000 which exceeds u32::MAX
        // (4,294,967,296) and must wrap modulo 2^32, not saturate at u32::MAX.
        // Independently hand-computed (not via the implementation's own formula):
        // 4,500,000,000 - 4,294,967,296 = 205,032,704.
        let ts = corrector.correct(50000.0);
        assert_eq!(ts, 205_032_704u32);
        assert_ne!(ts, u32::MAX, "must wrap, not saturate");
    }

    #[test]
    fn test_rewrite_uri_does_not_match_embedded_rtsp_substring() {
        // Regression: a `find`-based prefix check would match "rtsp://"
        // wherever it appears in the string, not just at the start. `strip_prefix` only
        // matches at position 0, so a redirect-style URL that merely contains the
        // substring later on must be returned unchanged.
        let uri = "https://example.com/redirect?to=rtsp://192.168.1.150/streaming/live/1";
        let rewritten = rewrite_rtsp_request_uri(uri, "192.168.1.150").unwrap();
        assert_eq!(rewritten, uri);
    }
}
