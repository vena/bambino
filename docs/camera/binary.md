**bambino > camera > binary**

# Module: camera::binary

## Contents

**Structs**

- [`BambuBinaryCameraStream`](#bambubinarycamerastream) - Abstract state controller parsing incoming frame buffers from raw Port 6000 streams.

**Functions**

- [`build_handshake_packet`](#build_handshake_packet) - Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].

---

## bambino::camera::binary::BambuBinaryCameraStream

*Struct*

Abstract state controller parsing incoming frame buffers from raw Port 6000 streams.

**Generic Parameters:**
- IO

**Methods:**

- `fn new(stream: IO) -> Self` - Instantiates a camera parser wrapper surrounding an active secure stream socket.
- `fn with_max_frame_size(self: Self, max: usize) -> Self` - Overrides the maximum accepted frame size (default: `CAMERA_FRAME_MAX_SIZE`, 10MB).
- `fn authenticate(self: & mut Self, access_code: &str) -> Result<(), BambuError>` - Transmits the 80-byte authentication handshake to activate the continuous frame-push process.
- `fn read_next_frame(self: & mut Self, frame_buf: & mut Vec<u8>) -> Result<(), BambuError>` - Asynchronously extracts the next complete frame from the stream.



## bambino::camera::binary::build_handshake_packet

*Function*

Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].

**Byte Ordering Specifications:**
* Offset 0-3 (4 bytes): Magic identifier header (`0x00000040` / 64)
* Offset 4-7 (4 bytes): Control operation Command ID (`0x00003000` / 12288)
* Offset 8-15 (8 bytes): Zero-padding block
* Offset 16-47 (32 bytes): Null-padded ASCII username (`"bblp"`)
* Offset 48-79 (32 bytes): Null-padded ASCII LAN access code

```rust
fn build_handshake_packet(access_code: &str) -> Result<[u8; 80], crate::error::BambuError>
```



