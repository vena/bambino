*[bambino](../../index.md) / [camera](../index.md) / [binary](index.md)*

---

# Module `binary`

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

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`BambuBinaryCameraStream`](#bambubinarycamerastream) | struct | Abstract state controller parsing incoming frame buffers from raw Port 6000 streams. |
| [`build_handshake_packet`](#build-handshake-packet) | fn | Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY]. |
| [`CAMERA_PASSWORD_MAX_LEN`](#camera-password-max-len) | const | Maximum accepted access-code length for the camera handshake and RTSPS auth, in bytes. |

## Types

### `BambuBinaryCameraStream<IO: AsyncIo>`

```rust
struct BambuBinaryCameraStream<IO: AsyncIo> {
    // [REDACTED: Private Fields]
}
```

Abstract state controller parsing incoming frame buffers from raw Port 6000 streams.

Does not own dial/redial logic itself — a caller writing its own reconnect loop against
port 6000 must not redial immediately after a disconnect. The printer's port-6000 socket
accepts only one connection at a time; reopening before the prior TCP FIN completes can
orphan the old socket server-side until keepalive reaps it (~20 min stall). Confirmed
printer behavior (bambuddy `fix(camera) #2521`); add a delay or wait for the old socket
to fully close before redialing.

#### Implementations

- <span id="bambubinarycamerastream-new"></span>`fn new(stream: IO) -> Self`

  Instantiates a camera parser wrapper surrounding an active secure stream socket.

- <span id="bambubinarycamerastream-with-max-frame-size"></span>`fn with_max_frame_size(self, max: usize) -> Self`

  Overrides the maximum accepted frame size (default: `CAMERA_FRAME_MAX_SIZE`, 10MB).

- <span id="bambubinarycamerastream-authenticate"></span>`async fn authenticate(&mut self, identity: &PrinterIdentity) -> Result<(), Error>` — [`PrinterIdentity`](../../identity/index.md#printeridentity), [`Error`](../../error/index.md#error)

  Transmits the 80-byte authentication handshake to activate the continuous frame-push process.

- <span id="bambubinarycamerastream-read-next-frame"></span>`async fn read_next_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Asynchronously extracts the next complete frame from the stream.

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for BambuBinaryCameraStream<IO>`

##### `impl<E> AsTaggedImplicit<'a, E> for BambuBinaryCameraStream<IO>`


---

## Functions

### `build_handshake_packet`

```rust
fn build_handshake_packet(access_code: &str) -> Result<[u8; 80], crate::error::Error>
```

**Types:** [`Error`](../../error/index.md#error)

Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].

**Byte Ordering Specifications:**
* Offset 0-3 (4 bytes): Magic identifier header (`0x00000040` / 64)
* Offset 4-7 (4 bytes): Control operation Command ID (`0x00003000` / 12288)
* Offset 8-15 (8 bytes): Zero-padding block
* Offset 16-47 (32 bytes): Null-padded ASCII username (`"bblp"`)
* Offset 48-79 (32 bytes): Null-padded ASCII LAN access code


---

## Constants

### `CAMERA_PASSWORD_MAX_LEN`
```rust
const CAMERA_PASSWORD_MAX_LEN: usize = 32usize;
```

Maximum accepted access-code length for the camera handshake and RTSPS auth, in bytes.

