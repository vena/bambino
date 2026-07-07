**bambino > camera > rtsps**

# Module: camera::rtsps

## Contents

**Structs**

- [`RtpTimestampCorrector`](#rtptimestampcorrector) - Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.

**Functions**

- [`build_rtsps_url`](#build_rtsps_url) - Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream.
- [`rewrite_rtsp_request_uri`](#rewrite_rtsp_request_uri) - Rewrites a plain `rtsp://` proxy URI to the printer's `rtsps://` endpoint.

---

## bambino::camera::rtsps::RtpTimestampCorrector

*Struct*

Corrects frozen stream-embedded timestamps to prevent duplicate frame drop freezes.

**Methods:**

- `fn init(embedded_rtp: u32) -> Self` - Initializes the corrector by capturing the stream's first embedded RTP timestamp as the base coordinate for all subsequent corrections.
- `fn correct(self: &Self, elapsed_secs: f64) -> u32` - Computes the corrected RTP timestamp from host-observed elapsed time.



## bambino::camera::rtsps::build_rtsps_url

*Function*

Builds the authenticated RTSPS URL for a Bambu Lab printer's video stream.

The returned URL can be passed directly to media frameworks that support RTSPS with
Digest authentication, or used as the target endpoint for a local decryption proxy
(see module-level docs for the proxy pattern).

# Errors

Returns [`BambuError::ProtocolViolation`] if `access_code` is empty or contains any
character outside ASCII letters/digits. Genuine printer-issued LAN access codes are
always 8 uppercase ASCII alphanumeric characters, so a rejection here almost always
means a copy-paste mistake (stray whitespace, a trailing newline) rather than a
valid-but-unusual code — surfacing it as an error catches that mistake instead of
silently building a malformed URL.

Also returns [`BambuError::ProtocolViolation`] if `ip` does not parse as a valid IPv4 or
IPv6 address. Without this check, an `ip` containing an embedded `@` (e.g.
`"1.2.3.4@attacker.example.com"`, spoofable by any device on the LAN via SSDP/mDNS
discovery) would place everything up to the last `@` into the URL's userinfo component,
redirecting the connection — and the LAN access code — to an attacker-controlled host.

```rust
fn build_rtsps_url(ip: &str, access_code: &str) -> Result<String, crate::error::BambuError>
```



## bambino::camera::rtsps::rewrite_rtsp_request_uri

*Function*

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

Returns [`BambuError::ProtocolViolation`] if `printer_ip` does not parse as a valid IPv4 or
IPv6 address — the same check [`build_rtsps_url`] applies to its own `ip` parameter, and
for the same reason: a `printer_ip` containing `@` or `/` (e.g. sourced from a
spoofable SSDP/mDNS discovery response, same as [`build_rtsps_url`]'s hazard) could
otherwise redirect the proxy's outbound connection or produce a malformed URI. This
function has no other caller in this crate to rely on for pre-validation — it's called
once per incoming request in a proxy's hot path, but IP-string parsing is cheap enough
that re-validating here is not a meaningful cost.

```rust
fn rewrite_rtsp_request_uri(request_uri: &str, printer_ip: &str) -> Result<String, crate::error::BambuError>
```



