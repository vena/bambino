*[bambino](../../index.md) / [camera](../index.md) / [rtsps](index.md)*

---

# Module `rtsps`

# RTSPS Stream Helpers (Port 322)

Utilities for integrating with the RTSPS video stream on higher-capability Bambu Lab
printers (X1, X2, H2, P2S series). These printers host a local RTSP server wrapped in
implicit TLS on port 322, using Digest authentication with the printer's LAN access code.

This module does **not** implement an RTSP client or TLS proxy. It provides building
blocks for callers integrating with external media frameworks (FFmpeg, GStreamer, VLC):

- [`build_rtsps_url`](#build-rtsps-url) — generates the authenticated RTSPS URL for direct consumption
- [`rewrite_rtsp_request_uri`](#rewrite-rtsp-request-uri) — rewrites proxy-local URIs for Digest auth correctness
- [`RtpTimestampCorrector`](#rtptimestampcorrector) — fixes frozen RTP timestamps on affected P2S firmware

# RTSPS proxy architecture

The printer's RTSPS server uses a self-signed TLS certificate that standard media players
cannot validate. The common integration pattern is a local decryption proxy:

1. A proxy listens on `127.0.0.1:<local_port>` accepting plain `rtsp://` connections
2. The media player connects to `rtsp://127.0.0.1:<local_port>/streaming/live/1`
3. The proxy wraps traffic in TLS and forwards to `rtsps://<printer_ip>:322/...`

RTSP Digest authentication hashes include the request-line URI. The printer expects
`rtsps://<printer_ip>:322/...` but the player sends `rtsp://127.0.0.1:...`.
[`rewrite_rtsp_request_uri`](#rewrite-rtsp-request-uri) rewrites the request-line/URI text so a proxy that acts as
its own independent RTSP client toward the printer (computing its own Digest response
against the rewritten URI) sends the correct URI. **It does not recompute or repair an
already-computed Digest `Authorization` header** — a transparent relay that forwards the
player's original `Authorization` header verbatim will still get a 401, because that
header's `response=` hash was computed by the player against its own local URI and this
function has no way to update it (see the function's own doc comment for detail).

# P2S RTP timestamp freeze

P2S printers on firmware `01.02.00.00` have an encoder bug where every H.264 frame
carries the same RTP timestamp (~0.06s). Decoders interpret non-advancing timestamps as
duplicates and drop frames, freezing the video. [`RtpTimestampCorrector`](#rtptimestampcorrector) replaces the
frozen timestamps with host-computed values on the standard 90 kHz RTP clock. Use
[`ModelQuirks::requires_wallclock_rtsp_timestamps()`](crate::quirks::ModelQuirks::requires_wallclock_rtsp_timestamps)
to check whether the connected model needs this correction.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`RtpTimestampCorrector`](#rtptimestampcorrector) | struct | Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes. |
| [`build_rtsps_url`](#build-rtsps-url) | fn | Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream. |
| [`rewrite_rtsp_request_uri`](#rewrite-rtsp-request-uri) | fn | Rewrites a plain `rtsp://` proxy URI to the printer's `rtsps://` endpoint. |

## Types

### `RtpTimestampCorrector`

```rust
struct RtpTimestampCorrector {
    // [REDACTED: Private Fields]
}
```

Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.

#### Implementations

- <span id="rtptimestampcorrector-init"></span>`fn init(embedded_rtp: u32) -> Self`

  Initializes the corrector by capturing the stream's first embedded RTP timestamp as the base coordinate for all subsequent corrections.

  This preserves alignment with the SDP stream definition.

- <span id="rtptimestampcorrector-correct"></span>`fn correct(&self, elapsed_secs: f64) -> u32`

  Computes the corrected RTP timestamp from host-observed elapsed time.

#### Trait Implementations


---

## Functions

### `build_rtsps_url`

```rust
fn build_rtsps_url(ip: &str, access_code: &str) -> Result<String, crate::error::Error>
```

**Types:** [`Error`](../../error/index.md#error)

Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream.

The returned URL can be passed directly to media frameworks that support RTSPS with
Digest authentication, or used as the target endpoint for a local decryption proxy
(see module-level docs for the proxy pattern).

# Errors

Returns [`Error::ProtocolViolation`] if `access_code` is empty or contains any
character outside ASCII letters/digits. Genuine printer-issued LAN access codes are
always 8 uppercase ASCII alphanumeric characters, so a rejection here almost always
means a copy-paste mistake (stray whitespace, a trailing newline) rather than a
valid-but-unusual code — surfacing it as an error catches that mistake instead of
silently building a malformed URL.

Also returns [`Error::ProtocolViolation`] if `ip` does not parse as a valid IPv4 or
IPv6 address. Without this check, an `ip` containing an embedded `@` (e.g.
`"1.2.3.4@attacker.example.com"`, spoofable by any device on the LAN via SSDP/mDNS
discovery) would place everything up to the last `@` into the URL's userinfo component,
redirecting the connection — and the LAN access code — to an attacker-controlled host.

### `rewrite_rtsp_request_uri`

```rust
fn rewrite_rtsp_request_uri(request_uri: &str, printer_ip: &str) -> Result<String, crate::error::Error>
```

**Types:** [`Error`](../../error/index.md#error)

Rewrites a plain `rtsp://` proxy URI to the printer's `rtsps://` endpoint.

When running a local decryption proxy (see module-level docs), media players send
requests to `rtsp://127.0.0.1:<local_port>/...`. RTSP Digest authentication includes
the request-line URI in its hash, so the printer expects `rtsps://<ip>:322/...`. This
function performs pure text surgery on the request-line/URI: it replaces the scheme and
host while preserving the path and query string, nothing else.

**This function does not repair an already-computed Digest `Authorization` header.** It
never sees an `Authorization` header, a nonce, a realm, or the access code, so it cannot
compute or correct an HA1/HA2/`response=` MD5 value. It is only useful to a proxy that
acts as its own independent RTSP client toward the printer — i.e. one that computes its
own Digest response against the rewritten URI returned here. A transparent-relay proxy
that forwards the player's original `Authorization` header verbatim will still receive a
401: that header's `response=` value was computed by the player against its own local
(`rtsp://127.0.0.1:...`) URI, and nothing here updates it to match the rewritten one.

If the input does not start with `rtsp://` (e.g. it's already `rtsps://`), it is returned
unchanged.

This function expects proxy-generated URIs with a simple `rtsp://host:port/path` structure.
It is not a general-purpose URI parser.

# Errors

Returns [`Error::ProtocolViolation`] if `printer_ip` does not parse as a valid IPv4 or
IPv6 address — the same check [`build_rtsps_url`](#build-rtsps-url) applies to its own `ip` parameter, and
for the same reason: a `printer_ip` containing `@` or `/` (e.g. sourced from a
spoofable SSDP/mDNS discovery response, same as [`build_rtsps_url`](#build-rtsps-url)'s hazard) could
otherwise redirect the proxy's outbound connection or produce a malformed URI. This
function has no other caller in this crate to rely on for pre-validation — it's called
once per incoming request in a proxy's hot path, but IP-string parsing is cheap enough
that re-validating here is not a meaningful cost.

