#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::camera::binary::BambuBinaryCameraStream;
use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::PrinterClient;
use crate::camera::binary::CAMERA_READ_TIMEOUT_SECS;

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
    CameraRawIO,
    CameraTls,
    CameraFactory,
>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Injects a pre-connected [`BambuBinaryCameraStream`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the camera
    /// connection. For lazy connection, use [`.with_camera()`](Self::with_camera).
    pub fn attach_camera(&mut self, camera: BambuBinaryCameraStream<CameraTls::Stream>) {
        self.camera = Some(camera);
    }

    /// Returns direct access to the underlying [`BambuBinaryCameraStream`], auto-connecting
    /// if needed.
    ///
    /// Requires prior camera configuration via [`.with_camera()`](Self::with_camera) or
    /// [`.attach_camera()`](Self::attach_camera). Returns `BambuError::ProtocolViolation`
    /// immediately for RTSPS models — see [`ensure_camera()`](Self::ensure_camera)'s doc
    /// comment.
    pub async fn camera(
        &mut self,
    ) -> Result<&mut BambuBinaryCameraStream<CameraTls::Stream>, BambuError> {
        self.ensure_camera().await?;
        Ok(self.camera.as_mut().unwrap())
    }

    /// Reads the next camera frame, auto-connecting (and authenticating) if needed.
    ///
    /// Bounds the read against `self.timer` (see
    /// `BambuBinaryCameraStream::read_next_frame_with_timer`), mirroring
    /// [`poll_telemetry()`](Self::poll_telemetry)'s relationship to
    /// [`.mqtt()`](Self::mqtt).
    pub async fn read_camera_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), BambuError> {
        self.ensure_camera().await?;
        self.camera
            .as_mut()
            .unwrap()
            .read_next_frame_with_timer(frame_buf, &self.timer, CAMERA_READ_TIMEOUT_SECS * 1000)
            .await
    }

    /// Disconnects the camera session, if one exists, and clears it from the client.
    ///
    /// Once `camera_config` is consumed by [`ensure_camera()`](Self::ensure_camera), a dead
    /// stream (`ConnectionReset`, bad markers, etc.) would otherwise leave `self.camera`
    /// stuck `Some(...)` forever, since `ensure_camera()`'s `is_some()` short-circuit would
    /// keep handing back the same broken stream. There is no protocol-level teardown on
    /// `BambuBinaryCameraStream` to call — this just clears the slot.
    ///
    /// Idempotent. Reconnecting requires a fresh [`.with_camera()`](Self::with_camera) on a
    /// new `PrinterClient`, the same caveat FTPS already documents for
    /// [`disconnect_storage()`](Self::disconnect_storage).
    pub async fn disconnect_camera(&mut self) -> Result<(), BambuError> {
        self.camera = None;
        Ok(())
    }
}
