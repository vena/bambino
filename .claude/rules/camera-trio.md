---
paths:
  - "src/client/connect.rs"
  - "src/client/camera.rs"
  - "src/camera/mod.rs"
  - "src/quirks/**"
---

Camera integration is a third `CameraRawIO`/`CameraTls`/`CameraFactory` trio on `PrinterClient`, mirroring FTPS's shape (defaulted to dummy types). `ensure_camera()` checks `model.quirks().camera_protocol() == CameraProtocol::BinaryJpeg` before checking whether `.with_camera()` was configured — an RTSPS model (X1/X2/H2/P2S) fails with an unsupported-connection-type error immediately, never dials. `disconnect_camera()` resets `self.camera` to `None` so a dead stream doesn't get stuck behind `ensure_camera()`'s `is_some()` short-circuit.
