#[cfg(not(feature = "std"))]
use alloc::string::String;

use core::marker::PhantomData;

use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};
use crate::models::BambuModel;
use crate::mqtt::BambuMqttClient;

use super::{DEFAULT_COMMAND_TIMEOUT_SECS, INITIAL_SEQUENCE_ID, PreConnected, PrinterClient};

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(feature = "std")]
use std::collections::VecDeque;

impl<IO, Timer, RawIO, Tls, Factory> PrinterClient<PreConnected<IO>, Timer, RawIO, Tls, Factory>
where
    IO: AsyncIo,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Instantiates a coordinator client holding both active MQTTS and implicit FTPS sessions.
    pub fn new_with_storage(
        mqtt_client: BambuMqttClient<IO>,
        ftps_client: BambuFtpsClient<RawIO, Tls, Factory>,
        timer: Timer,
        serial: &str,
        model: BambuModel,
    ) -> Self {
        Self {
            mqtt: Some(mqtt_client),
            ftps: Some(ftps_client),
            connector: PreConnected(PhantomData),
            timer,
            serial: String::from(serial),
            ip: String::new(),
            access_code: String::new(),
            model,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            pending_messages: VecDeque::new(),
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
        }
    }
}

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Connects and registers an FTPS client on demand if not set during initialization.
    pub fn attach_storage(&mut self, ftps_client: BambuFtpsClient<RawIO, Tls, Factory>) {
        self.ftps = Some(ftps_client);
    }

    /// Exposes a reference to the active FTPS client.
    ///
    /// Returns `None` if storage capabilities have not been attached.
    pub fn storage(&mut self) -> Option<&mut BambuFtpsClient<RawIO, Tls, Factory>> {
        self.ftps.as_mut()
    }
}
