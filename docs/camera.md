**bambino > camera**

# Module: camera

## Contents

**Modules**

- [`binary`](#binary) - # Chamber Image Binary JPEG Socket Protocol (Port 6000)
- [`rtsps`](#rtsps) - # RTSPS Stream Helpers (Port 322)

**Enums**

- [`CameraProtocol`](#cameraprotocol) - Which camera streaming protocol a printer model uses.

**Constants**

- [`CAMERA_PORT_BINARY_JPEG`](#camera_port_binary_jpeg) - Default port for binary JPEG camera streams (A1, A1 Mini, A2L, and P1 series).
- [`CAMERA_PORT_RTSPS`](#camera_port_rtsps) - Default port for RTSPS camera streams (X1, X2, H2, P2S series).

---

## bambino::camera::CAMERA_PORT_BINARY_JPEG

*Constant*: `u16`

Default port for binary JPEG camera streams (A1, A1 Mini, A2L, and P1 series).



## bambino::camera::CAMERA_PORT_RTSPS

*Constant*: `u16`

Default port for RTSPS camera streams (X1, X2, H2, P2S series).



## bambino::camera::CameraProtocol

*Enum*

Which camera streaming protocol a printer model uses.

**Variants:**
- `Rtsps` - RTSP stream wrapped in implicit TLS on Port 322 (X1, X2D, P2S, and H2 series).
- `BinaryJpeg` - Custom binary TCP packet loop returning JPEG frames on Port 6000 (P1 and A1 series, including A2L).

**Methods:**

- `fn default_port(self: &Self) -> u16` - Returns the standard TCP port associated with the physical interface.

**Traits:** Copy, Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> CameraProtocol`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **PartialEq**
  - `fn eq(self: &Self, other: &CameraProtocol) -> bool`



## Module: binary

# Chamber Image Binary JPEG Socket Protocol (Port 6000)

Handles connection handshakes and payload processing for constrained printer lines
(P1 and A1 series, including A2L) transmitting discrete camera frames over raw TLS TCP sockets [REF-CAM-BINARY].

**Handshake Architecture [REF-CAM-BINARY]:**
Upon establishing a TLS session, the connecting client must immediately transmit a
packed 80-byte authentication packet formatted in little-endian order. If the handshake is
accepted, the physical machine begins continuously writing raw JPEG frames prefixed with
a standard 16-byte length descriptor.

**Flow Integrity Guards:**
1. Verifies that incoming payloads conform strictly to JPEG magic start (`FF D8`) and
   end (`FF D9`) markers before returning buffers to upstream applications to insulate
   against decoding crashes.
2. Clamps incoming frame sizes to a reasonable upper boundary (10MB by default) to protect
   against unbounded memory allocation crashes on low-resource environments if transport
   stream corruption occurs. Use [`BambuBinaryCameraStream::with_max_frame_size`] to lower
   this cap on constrained (`no_std`/Embassy) targets.



## Module: rtsps

# RTSPS Stream Helpers (Port 322)

Utilities for integrating with the RTSPS video stream on higher-capability Bambu Lab
printers (X1, X2, H2, P2S series). These printers host a local RTSP server wrapped in
implicit TLS on port 322, using Digest authentication with the printer's LAN access code.

This module does **not** implement an RTSP client or TLS proxy. It provides building
blocks for callers integrating with external media frameworks (FFmpeg, GStreamer, VLC):

- [`build_rtsps_url`] — generates the authenticated RTSPS URL for direct consumption
- [`rewrite_rtsp_request_uri`] — rewrites proxy-local URIs for Digest auth correctness
- [`RtpTimestampCorrector`] — fixes frozen RTP timestamps on affected P2S firmware

# RTSPS proxy architecture

The printer's RTSPS server uses a self-signed TLS certificate that standard media players
cannot validate. The common integration pattern is a local decryption proxy:

1. A proxy listens on `127.0.0.1:<local_port>` accepting plain `rtsp://` connections
2. The media player connects to `rtsp://127.0.0.1:<local_port>/streaming/live/1`
3. The proxy wraps traffic in TLS and forwards to `rtsps://<printer_ip>:322/...`

RTSP Digest authentication hashes include the request-line URI. The printer expects
`rtsps://<printer_ip>:322/...` but the player sends `rtsp://127.0.0.1:...`.
[`rewrite_rtsp_request_uri`] rewrites the request-line/URI text so a proxy that acts as
its own independent RTSP client toward the printer (computing its own Digest response
against the rewritten URI) sends the correct URI. **It does not recompute or repair an
already-computed Digest `Authorization` header** — a transparent relay that forwards the
player's original `Authorization` header verbatim will still get a 401, because that
header's `response=` hash was computed by the player against its own local URI and this
function has no way to update it (see the function's own doc comment for detail).

# P2S RTP timestamp freeze

P2S printers on firmware `01.02.00.00` have an encoder bug where every H.264 frame
carries the same RTP timestamp (~0.06s). Decoders interpret non-advancing timestamps as
duplicates and drop frames, freezing the video. [`RtpTimestampCorrector`] replaces the
frozen timestamps with host-computed values on the standard 90 kHz RTP clock. Use
[`ModelQuirks::requires_wallclock_rtsp_timestamps()`](crate::quirks::ModelQuirks::requires_wallclock_rtsp_timestamps)
to check whether the connected model needs this correction.



