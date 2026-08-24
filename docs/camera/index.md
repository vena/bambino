*[bambino](../index.md) / [camera](index.md)*

---

# Module `camera`

# Camera & Video Streaming

Bambu Lab printers expose camera feeds through two protocols:

1. **Binary JPEG (Port 6000)** — A1, A1 Mini, A2L, and P1 series. A lightweight binary protocol that
   streams discrete JPEG frames over TLS. This module provides a complete client
   ([`BinaryCameraStream`](binary/index.md#binarycamerastream)) that handles the handshake and frame extraction.

2. **RTSPS (Port 322)** — X1, X2, H2, and P2S series. An RTSP server behind implicit TLS
   with Digest authentication. This module provides helper utilities ([`rtsps`](rtsps/index.md)) for
   integrating with external media frameworks (FFmpeg, GStreamer, VLC), including URL
   generation, proxy URI rewriting, and P2S timestamp correction. It does **not** include
   an RTSP client or TLS proxy — see the [`rtsps`](rtsps/index.md) module docs for the proxy architecture.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`binary`](binary/index.md) | mod | # Chamber Image Binary JPEG Socket Protocol (Port 6000) |
| [`rtsps`](rtsps/index.md) | mod | # RTSPS Stream Helpers (Port 322) |
| [`CameraProtocol`](#cameraprotocol) | enum | Which camera streaming protocol a printer model uses. |
| [`CAMERA_PORT_BINARY_JPEG`](#camera-port-binary-jpeg) | const | Default port for binary JPEG camera streams (A1, A1 Mini, A2L, and P1 series). |
| [`CAMERA_PORT_RTSPS`](#camera-port-rtsps) | const | Default port for RTSPS camera streams (X1, X2, H2, P2S series). |

## Modules

- [`binary`](binary/index.md) — # Chamber Image Binary JPEG Socket Protocol (Port 6000)
- [`rtsps`](rtsps/index.md) — # RTSPS Stream Helpers (Port 322)


---

## Types

### `CameraProtocol`

```rust
enum CameraProtocol {
    Rtsps,
    BinaryJpeg,
}
```

Which camera streaming protocol a printer model uses.

#### Variants

- **`Rtsps`**

  RTSP stream wrapped in implicit TLS on Port 322 (X1, X2D, P2S, and H2 series).

- **`BinaryJpeg`**

  Custom binary TCP packet loop returning JPEG frames on Port 6000 (P1 and A1 series, including A2L).

#### Implementations

- <span id="cameraprotocol-default-port"></span>`fn default_port(&self) -> u16`

  Returns the standard TCP port associated with the physical interface.

#### Trait Implementations

##### `impl Clone for CameraProtocol`

- <span id="cameraprotocol-clone"></span>`fn clone(&self) -> CameraProtocol` — [`CameraProtocol`](#cameraprotocol)

##### `impl Copy for CameraProtocol`

##### `impl Debug for CameraProtocol`

- <span id="cameraprotocol-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CameraProtocol`

##### `impl Hash for CameraProtocol`

- <span id="cameraprotocol-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CameraProtocol`

- <span id="cameraprotocol-partialeq-eq"></span>`fn eq(&self, other: &CameraProtocol) -> bool` — [`CameraProtocol`](#cameraprotocol)


---

## Constants

### `CAMERA_PORT_BINARY_JPEG`
```rust
const CAMERA_PORT_BINARY_JPEG: u16 = 6_000u16;
```

Default port for binary JPEG camera streams (A1, A1 Mini, A2L, and P1 series).

The printer accepts only one connection to this port at a time. A caller redialing it
immediately after disconnecting can orphan the prior socket server-side until keepalive
reaps it (~20 min stall) — wait for the old connection to fully close, or add a delay,
before reconnecting. See [`BinaryCameraStream`](binary/index.md#binarycamerastream)'s doc comment.

### `CAMERA_PORT_RTSPS`
```rust
const CAMERA_PORT_RTSPS: u16 = 322u16;
```

Default port for RTSPS camera streams (X1, X2, H2, P2S series).

