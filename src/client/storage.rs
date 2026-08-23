use crate::error::Error;
use crate::ftps::FtpsClient;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::PrinterClient;

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
    /// Injects a pre-connected [`FtpsClient`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the FTPS
    /// connection. For lazy connection, use [`.with_ftps()`](Self::with_ftps).
    pub fn attach_storage(
        &mut self,
        ftps_client: FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>,
    ) {
        self.ftps = Some(ftps_client);
    }

    /// Returns direct access to the underlying [`FtpsClient`], auto-connecting if needed.
    ///
    /// Requires prior FTPS configuration via [`.with_ftps()`](Self::with_ftps) or
    /// [`.attach_storage()`](Self::attach_storage).
    pub async fn storage(
        &mut self,
    ) -> Result<&mut FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, Error> {
        self.ensure_ftps().await?;
        Ok(self.ftps.as_mut().expect("ensure_ftps() just verified self.ftps is Some"))
    }

    /// Disconnects the FTPS session, if one exists, and clears it from the client.
    ///
    /// `FtpsClient::disconnect()` is `&mut self` (non-consuming) and always poisons
    /// itself on the way out (see its doc comment) — every subsequent call on that instance
    /// would fail with `ProtocolViolation`. Without this method, nothing ever resets
    /// `self.ftps` back to `None`, so a later [`storage()`](Self::storage) call would
    /// short-circuit `ensure_ftps()`'s `is_some()` check and hand back the now-poisoned
    /// client, surfacing a confusing low-level error instead of a clear one.
    ///
    /// `disconnect_storage()` takes `self.ftps`, disconnects it, and leaves the slot `None`.
    /// The next `storage()` call then falls through to `ensure_ftps()`'s existing "FTPS not
    /// configured" error (if `ftps_config` was already consumed by an earlier connect) rather
    /// than ever returning a poisoned client. Reconnecting still requires fresh FTPS
    /// configuration — [`.with_ftps()`](Self::with_ftps) on a new `PrinterClient`, or
    /// [`.attach_storage()`](Self::attach_storage) — since `ftps_config` is consumed on first
    /// connection.
    ///
    /// Idempotent — a no-op if no FTPS session is active. Always returns `Ok(())`; kept
    /// fallible for API symmetry with [`connect_ftps()`](Self::connect_ftps) and to leave room
    /// for a fallible teardown step in the future without a breaking signature change.
    pub async fn disconnect_storage(&mut self) -> Result<(), Error> {
        if let Some(mut client) = self.ftps.take() {
            client.disconnect().await;
        }
        Ok(())
    }
}
