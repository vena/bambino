use crate::ftps::FtpDataStreamFactory;
use crate::io::{TimerProvider, TlsConnector};

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DummyRawIo;

impl embedded_io_async::ErrorType for DummyRawIo {
    type Error = embedded_io_async::ErrorKind;
}

impl embedded_io_async::Read for DummyRawIo {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(0)
    }
}

impl embedded_io_async::Write for DummyRawIo {
    async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        Ok(0)
    }
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[doc(hidden)]
pub struct DummyTls;

impl TlsConnector<DummyRawIo> for DummyTls {
    type Stream = DummyRawIo;
    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        _raw_stream: DummyRawIo,
    ) -> Result<Self::Stream, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

#[doc(hidden)]
pub struct DummyFactory;

impl FtpDataStreamFactory<DummyRawIo> for DummyFactory {
    async fn create_data_stream(
        &self,
        _host: &str,
        _port: u16,
    ) -> Result<DummyRawIo, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DummyTimer;

impl TimerProvider for DummyTimer {
    async fn sleep(&self, _duration: core::time::Duration) {}
    fn now_millis(&self) -> u64 {
        0
    }
}
