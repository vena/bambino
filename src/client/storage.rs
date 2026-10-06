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
    /// connection. For lazy connection, use [`.with_ftps()`](Self::with_ftps). On a
    /// [`from_mqtt()`](Self::from_mqtt) client, whose FTPS type parameters are placeholders,
    /// use [`.with_attached_ftps()`](Self::with_attached_ftps) instead.
    ///
    /// A session already in the slot is disconnected first, as
    /// [`disconnect_ftps()`](Self::disconnect_ftps) does, so its TLS session is closed
    /// rather than dropped mid-stream.
    pub async fn attach_ftps(
        &mut self,
        ftps_client: FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>,
    ) {
        let _ = self.disconnect_ftps().await;
        self.ftps = Some(ftps_client);
    }

    /// Returns direct access to the underlying [`FtpsClient`], auto-connecting if needed.
    ///
    /// Requires prior FTPS configuration via [`.with_ftps()`](Self::with_ftps) or
    /// [`.attach_ftps()`](Self::attach_ftps). A session that a transport failure poisoned is
    /// disconnected and redialed here rather than handed back.
    pub async fn ftps(
        &mut self,
    ) -> Result<&mut FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, Error> {
        self.ensure_ftps().await?;
        Ok(self
            .ftps
            .as_mut()
            .expect("ensure_ftps() just verified self.ftps is Some"))
    }

    /// Disconnects the FTPS session, if one exists, keeping its configuration for a reconnect.
    ///
    /// `FtpsClient::disconnect()` hands back the TLS connector, factory and timer; they go back
    /// into this client's FTPS configuration, so the next [`ftps()`](Self::ftps) or
    /// [`connect_ftps()`](Self::connect_ftps) dials a fresh session, as the camera channel does.
    ///
    /// Idempotent — a no-op if no FTPS session is active. Always returns `Ok(())`; kept
    /// fallible for API symmetry with [`connect_ftps()`](Self::connect_ftps) and to leave room
    /// for a fallible teardown step in the future without a breaking signature change.
    pub async fn disconnect_ftps(&mut self) -> Result<(), Error> {
        if let Some(client) = self.ftps.take() {
            self.ftps_config = Some(client.disconnect().await);
        }
        Ok(())
    }
}
