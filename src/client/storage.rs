use crate::error::BambuError;
use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};

use super::PrinterClient;

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Injects a pre-connected [`BambuFtpsClient`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the FTPS
    /// connection. For lazy connection, use [`.with_ftps()`](Self::with_ftps).
    pub fn attach_storage(&mut self, ftps_client: BambuFtpsClient<RawIO, Tls, Factory>) {
        self.ftps = Some(ftps_client);
    }

    /// Returns direct access to the underlying [`BambuFtpsClient`], auto-connecting
    /// if needed.
    ///
    /// Requires prior FTPS configuration via [`.with_ftps()`](Self::with_ftps) or
    /// [`.attach_storage()`](Self::attach_storage).
    pub async fn storage(
        &mut self,
    ) -> Result<&mut BambuFtpsClient<RawIO, Tls, Factory>, BambuError> {
        self.ensure_ftps().await?;
        Ok(self.ftps.as_mut().unwrap())
    }
}
