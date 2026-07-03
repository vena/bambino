//! Zero-cost dummy implementations for [`PrinterClient`](super::PrinterClient)'s type parameters.
//!
//! These let you create an MQTT-only `PrinterClient` without specifying concrete FTPS,
//! TLS, or timer types. They're the defaults — you'll never need to reference them directly
//! unless you're building a fully custom client configuration.

use core::marker::PhantomData;

use crate::io::{AsyncIo, RawStreamFactory, SocketError, TimerError, TimerProvider, TlsConnector};

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
        _raw_stream: DummyRawIo,
    ) -> Result<Self::Stream, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

#[doc(hidden)]
pub struct DummyFactory;

impl RawStreamFactory<DummyRawIo> for DummyFactory {
    async fn dial(&self, _host: &str, _port: u16) -> Result<DummyRawIo, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct DummyTimer;

impl TimerProvider for DummyTimer {
    async fn sleep(&self, _duration: core::time::Duration) -> Result<(), TimerError> {
        Ok(())
    }
    fn now_millis(&self) -> u64 {
        0
    }
    /// `false` — `sleep()` above completes instantly regardless of the requested
    /// duration, so racing an I/O read against it (as `src/mqtt/client.rs`'s
    /// `poll_wire` does for the stalled-read fix) would resolve to "timed out" on
    /// virtually every call instead of providing real protection. See
    /// [`TimerProvider::has_real_clock`]'s doc comment for the full reasoning.
    fn has_real_clock(&self) -> bool {
        false
    }
}

/// Marker type used as both the `Tls` and `Factory` slot for a `PrinterClient` wrapping an
/// already-connected [`BambuMqttClient`](crate::mqtt::BambuMqttClient) (`from_mqtt()`, used by
/// tests and Embassy). `ensure_mqtt()` short-circuits on `self.mqtt.is_some()` before either
/// impl below is ever called, so `TlsConnector::connect`'s identity passthrough is never
/// exercised for real, and `RawStreamFactory::dial` is genuinely unreachable.
#[doc(hidden)]
pub struct PreConnected<IO: AsyncIo>(pub(crate) PhantomData<IO>);

impl<IO: AsyncIo> TlsConnector<IO> for PreConnected<IO> {
    type Stream = IO;

    async fn connect(&self, _host: &str, raw_stream: IO) -> Result<IO, SocketError> {
        Ok(raw_stream)
    }
}

impl<IO: AsyncIo> RawStreamFactory<IO> for PreConnected<IO> {
    async fn dial(&self, _host: &str, _port: u16) -> Result<IO, SocketError> {
        Err(SocketError::NotConnected)
    }
}
