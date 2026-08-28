---
paths:
  - "src/client/connect.rs"
  - "src/client/camera.rs"
  - "src/camera/mod.rs"
  - "src/quirks/**"
---

Camera integration is a third `CameraRawIO`/`CameraTls`/`CameraFactory` trio on `PrinterClient`, mirroring FTPS's shape (defaulted to dummy types). `ensure_camera()` checks `model.quirks().camera_protocol() == CameraProtocol::BinaryJpeg` before checking whether `.with_camera()` was configured — an RTSPS model (X1/X2/H2/P2S) fails with an unsupported-connection-type error immediately, never dials. `disconnect_camera()` resets `self.camera` to `None` so a dead stream doesn't get stuck behind `ensure_camera()`'s `is_some()` short-circuit.

`connect_all()` deliberately diverges here: on an RTSPS model it reports the camera as `None` ("not attempted") in `ConnectAllOutcome` rather than returning the error `ensure_camera()` returns. This is not an oversight and must not be "fixed" into agreeing with `ensure_camera()`. `connect_all()` connects every *applicable* channel, and an RTSPS camera is a channel that does not apply to that printer rather than a failure — surfacing it as `Err` would hand every X1/X2/H2/P2S consumer a guaranteed error on an otherwise clean connect. `ensure_camera()` keeps the error because there the caller asked for the camera specifically and silence would be wrong.
