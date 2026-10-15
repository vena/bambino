//! `PrinterClient`'s port-6000 file tunnel session [REF-FILESVC].

use crate::error::Error;
use crate::file_tunnel::FileTunnel;
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
    /// Returns the port-6000 file tunnel, connecting first if needed.
    ///
    /// Requires [`.with_file_tunnel()`](Self::with_file_tunnel). Refuses with
    /// [`Error::ModelMismatch`] without dialing when the printer doesn't serve the tunnel (see
    /// [`Capabilities::file_tunnel_support`](super::Capabilities::file_tunnel_support)): every
    /// chamber-image model, and any printer that reported `fun` bit 31 clear. A printer assumed to
    /// serve it that fails the handshake returns that error, which is the signal to use FTPS.
    ///
    /// The returned handle borrows this client and bounds every call with its timer; chain
    /// [`.with_timer()`](Self::with_timer), or a printer that ignores the login is only detected
    /// when it closes the connection after 60 s. The session stays open after the call; close it
    /// with [`disconnect_file_tunnel()`](Self::disconnect_file_tunnel) when done, since each live
    /// TLS session costs memory on embedded targets.
    pub async fn file_tunnel(&mut self) -> Result<FileTunnel<'_, CameraTls::Stream, Timer>, Error> {
        self.ensure_file_tunnel().await?;
        Ok(FileTunnel {
            client: self
                .file_tunnel
                .as_mut()
                .expect("ensure_file_tunnel() just connected the file tunnel"),
            timer: &self.timer,
        })
    }

    /// Eagerly establishes the file tunnel session.
    ///
    /// Idempotent: returns `Ok(())` if already connected. Not part of
    /// [`connect_all()`](Self::connect_all), which already opens three TLS sessions.
    pub async fn connect_file_tunnel(&mut self) -> Result<(), Error> {
        self.ensure_file_tunnel().await
    }

    /// Returns whether a usable file tunnel session is open (a poisoned one is not).
    pub fn is_file_tunnel_connected(&self) -> bool {
        self.file_tunnel
            .as_ref()
            .is_some_and(|client| !client.is_poisoned())
    }

    /// Closes and drops the file tunnel session, if one is open.
    ///
    /// Sends `close_notify` through the configured connector, then drops the stream, which is what
    /// frees its memory (`.claude/rules/tls-session-teardown.md`); a poisoned session is dropped
    /// without the close. Idempotent, and the next file tunnel call redials.
    pub async fn disconnect_file_tunnel(&mut self) {
        let Some(client) = self.file_tunnel.take() else {
            return;
        };
        match self.camera_config.as_ref() {
            Some((tls, _)) => client.disconnect(tls, &self.timer).await,
            None => drop(client),
        }
    }
}
