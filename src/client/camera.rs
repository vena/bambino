#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::camera::binary::BinaryCameraStream;
use crate::error::Error;
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
    FtpsTimer,
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
        FtpsTimer,
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
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Injects a pre-connected [`BinaryCameraStream`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the camera
    /// connection. For lazy connection, use [`.with_camera()`](Self::with_camera).
    pub fn attach_camera(&mut self, camera: BinaryCameraStream<CameraTls::Stream>) {
        self.camera = Some(camera);
    }

    /// Returns direct access to the underlying [`BinaryCameraStream`], auto-connecting if needed.
    ///
    /// Requires prior camera configuration via [`.with_camera()`](Self::with_camera) or
    /// [`.attach_camera()`](Self::attach_camera). Returns `Error::ProtocolViolation`
    /// immediately for RTSPS models — see `ensure_camera()`'s doc
    /// comment.
    pub async fn camera(&mut self) -> Result<&mut BinaryCameraStream<CameraTls::Stream>, Error> {
        self.ensure_camera().await?;
        Ok(self
            .camera
            .as_mut()
            .expect("ensure_camera() just verified self.camera is Some"))
    }

    /// Reads the next camera frame, auto-connecting (and authenticating) if needed.
    ///
    /// Bounds the read against `self.timer` (see
    /// `BinaryCameraStream::read_next_frame_with_timer`), mirroring
    /// [`poll_telemetry()`](Self::poll_telemetry)'s relationship to
    /// [`.mqtt()`](Self::mqtt).
    pub async fn read_camera_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), Error> {
        self.ensure_camera().await?;
        self.camera
            .as_mut()
            .expect("ensure_camera() just verified self.camera is Some")
            .read_next_frame_with_timer(frame_buf, &self.timer, CAMERA_READ_TIMEOUT_SECS * 1000)
            .await
    }

    /// Disconnects the camera session, if one exists, and clears it from the client.
    ///
    /// A dead stream (`ConnectionReset`, bad markers, etc.) would otherwise leave `self.camera`
    /// stuck `Some(...)` forever, since `ensure_camera()`'s `is_some()` short-circuit would
    /// keep handing back the same broken stream.
    ///
    /// There is no protocol-level teardown on `BinaryCameraStream` to call, but the TLS session
    /// underneath it is shut down properly before the slot is cleared —
    /// [`TlsConnector::close`](crate::io::TlsConnector::close) sends `close_notify` so the
    /// printer sees an orderly teardown rather than a truncated stream (GitHub issue #293).
    /// Failure there is logged and ignored: the connection is going away either way.
    ///
    /// Idempotent, and unlike [`disconnect_storage()`](Self::disconnect_storage) this *can* be
    /// reconnected — `ensure_camera()` never consumes `camera_config` (nothing is moved out of
    /// it), so the next camera call redials.
    pub async fn disconnect_camera(&mut self) -> Result<(), Error> {
        if let Some((tls, _)) = self.camera_config.as_ref()
            && let Some(mut camera) = self.camera.take()
            && let Err(e) = tls.close(camera.stream_mut()).await
        {
            log::debug!("camera TLS close failed: {e:?}");
        }
        self.camera = None;
        Ok(())
    }
}
