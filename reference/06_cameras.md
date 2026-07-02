# Chapter 6: Video Streaming & Chamber Image Handshakes

---

### 6.1 Network Boundary & Interface Parameters [REF-CAM-RTSPS]

Bambu Lab printers support video streaming via two different protocol interfaces, divided cleanly by the processing capabilities of the model's core network processor:

1.  **RTSPS Interface (Port 322)**: Supported on high-capability models (`X1`, `X1C`, `X1E`, `X2D`, `P2S`, and `H2` series). These printers host a local RTSP server wrapped in TLS (RTSPS) with digest authentication.
2.  **Chamber Image Interface (Port 6000)**: Supported on constrained models (`P1P`, `P1S`, `A1`, `A1 Mini`, and `A2L`). These printers utilize a proprietary, low-overhead binary TCP socket protocol that delivers discrete JPEG frames [REF-CAM-BINARY].

---

### 6.2 Over-the-Wire Telemetry Payload Schema (The Read Stream)

The state of the camera is reported on the report topic under the `"print"` or `"ipcam"` structure:
*   `ipcam_dev`: Internal identifier or state of the hardware camera module.
*   `ipcam_record`: Indicates whether the local user stream or camera live feed is active (`"enable"` or `"disable"`).
*   `timelapse`: Indicates whether frame-by-layer timelapse recording is active (`"enable"` or `"disable"`).
*   `rtsp_url`: The RTSPS streaming URL (e.g. `"rtsps://192.168.1.64/streaming/live/1"`) or `"disable"` when RTSP streaming is turned off. On the H2 series, Port 322 is closed by default in factory firmware and this field reports `"disable"` until manually enabled via the physical touchscreen interface. Clients should check this field before attempting an RTSPS connection.

---

### 6.3 Over-the-Wire Control Command Schema (The Write Stream)

#### Chamber Image Protocol Handshake (Port 6000) [REF-CAM-BINARY]
To establish a connection and begin streaming frames from a P1/A1 printer, the client must initiate a TLS handshake over Port 6000, and immediately transmit an **80-byte binary authentication packet**.

The packet is structured in little-endian byte ordering as follows:

| Offset (Bytes) | Size (Bytes) | Data Type | Field Value / Description |
| :--- | :--- | :--- | :--- |
| `0 - 3` | 4 | `uint32` | `0x00000040` (Magic Header) |
| `4 - 7` | 4 | `uint32` | `0x00003000` (Command ID: 12288) |
| `8 - 15` | 8 | `bytes` | Zero-padding (`\x00` * 8) |
| `16 - 47` | 32 | `string` | `"bblp"` (Null-padded ASCII username) |
| `48 - 79` | 32 | `string` | `<access_code>` (Null-padded ASCII access code) |

Once authenticated, the printer continuously streams JPEG payloads back over the socket. Each frame is preceded by a **16-byte header**:
*   **Bytes 0-3**: `uint32` payload size $N$ (little-endian). This represents the size of the subsequent raw JPEG data.
*   **Bytes 4-15**: Padding / Metadata (zeros).

The client must parse the size $N$, validate that $N$ does not exceed a sanity limit of 10MB (to guard against malformed headers causing unbounded allocation), read exactly $N$ bytes of data, and verify that the payload starts with the JPEG magic marker `\xff\xd8` and ends with `\xff\xd9` before displaying the frame.

#### RTSPS Protocol Handshake (Port 322)
RTSPS connection URLs are formatted as:

```text
rtsps://bblp:<access_code>@<printer_ip>:322/streaming/live/1
```

Because the printer's embedded RTSP server utilizes implicit TLS (RTSPS) with custom certificate parameters, plain TCP decrypting proxies may be constructed locally to interface with standard stream parsers. Under this proxy architecture, RTSP request-line URLs must be rewritten in transit:
1.  Plain RTSP requests directed to the proxy (`rtsp://127.0.0.1:<local_port>`) are wrapped in SSL/TLS and forwarded to Port 322.
2.  RTSP request-line URLs must be rewritten in transit from `rtsp://127.0.0.1:<local_port>` to `rtsps://<printer_ip>:322` before reaching the printer, so a proxy that acts as its own independent RTSP client toward the printer sends its outbound request against the correct URI.

**This URI rewrite alone does not repair an already-computed Digest `Authorization` header.** The `response=` value in a Digest `Authorization` header is a hash computed by the client (the media player) against the URI *it* used, before the proxy sees the request. Rewriting only the request-line URI does not change that already-computed hash. A proxy that merely relays the player's original `Authorization` header verbatim (rather than independently authenticating to the printer itself, using the access code, as its own RTSP client) will still receive a 401 from the printer, because the printer computes its own expected hash against the rewritten URI and the two will disagree. `bambino`'s `rewrite_rtsp_request_uri` implements only the request-line text rewrite described above — it has no access to the Digest nonce/realm/access code and cannot recompute the `response=` hash.

---

### 6.4 Mechanical & Firmware Quirks

#### P2S Frame & Timestamp Freezing Quirk (Firmware `01.02.00.00`)
P2S printers running firmware `01.02.00.00` suffer from two distinct RTSP stream implementation bugs:
1.  **Slow Keyframe Pacing**: The initial metadata packet is too small for standard format probing. Demuxers must be configured with a larger probe size (at least `1,048,576` bytes / 1MB) and extended format analysis duration to correctly identify the stream structure.
2.  **Non-Advancing RTP Timestamps**: Every frame is erroneously stamped with a static timestamp of approximately `0.06` seconds. If the demuxer utilizes Constant Frame Rate (CFR) conversion with default stream-embedded timestamps, it will interpret every frame after the first as a duplicate and drop them, causing the video stream to freeze.
    *   *Mitigation*: The stream must be processed using the wall-clock packet arrival times rather than trusting the stream-embedded RTP clock ticks.
