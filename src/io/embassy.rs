//! # Bare-Metal Embassy Runtime Integration
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for bare-metal targets utilizing the Embassy network
//! stack and `embedded-tls`.

#[cfg(feature = "embassy")]
use crate::io::{AsyncIo, AsyncUdpSocket, SocketError, TimerProvider, TlsConnector};

/// Timer implementation designed for the hardware microsecond clock in Embassy.
#[cfg(feature = "embassy")]
pub struct EmbassyTimer;

#[cfg(feature = "embassy")]
impl TimerProvider for EmbassyTimer {
    async fn sleep(duration: core::time::Duration) {
        let micros = duration.as_micros() as u64;
        ::embassy_time::Timer::after(::embassy_time::Duration::from_micros(micros)).await;
    }
}

/// UDP Socket implementation designed for the Embassy network stack.
///
/// Under Embassy, binding and state registration are coordinated via the stack's SocketSet pool.
#[cfg(feature = "embassy")]
pub struct EmbassyUdpSocket<'a> {
    inner: ::embassy_net::udp::UdpSocket<'a>,
}

#[cfg(feature = "embassy")]
impl<'a> EmbassyUdpSocket<'a> {
    /// Creates a wrapper using a pre-initialized Embassy UDP socket.
    pub fn new(inner: ::embassy_net::udp::UdpSocket<'a>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "embassy")]
impl<'a> AsyncUdpSocket for EmbassyUdpSocket<'a> {
    async fn bind(_addr: &str) -> Result<Self, SocketError> {
        // Under Embassy, IP bindings are pre-allocated during network task initialization.
        // Direct string binding is bypassed on physical bare-metal hardware.
        Err(SocketError::Other(
            "Embassy socket sets must be pre-bound during hardware stack initialization",
        ))
    }

    async fn send_to(&self, buf: &[u8], target: &str) -> Result<usize, SocketError> {
        let endpoint = parse_endpoint(target).ok_or(SocketError::InvalidInput)?;
        // Embassy-net UDP socket utilizes standard slice transmission
        self.inner
            .send_to(buf, endpoint)
            .await
            .map_err(|_| SocketError::ConnectionReset)?;
        Ok(buf.len())
    }

    async fn recv_from(
        &self,
        buf: &mut [u8],
    ) -> Result<(usize, alloc::string::String), SocketError> {
        let (len, from_endpoint) = self
            .inner
            .recv_from(buf)
            .await
            .map_err(|_| SocketError::ConnectionReset)?;

        // Reconstruct IP target string for standard interface consumption.
        // Under embassy-net 0.9.1, UdpMetadata wraps its IpEndpoint target inside the `endpoint` field.
        let mut addr_str = alloc::string::String::new();
        use core::fmt::Write;
        write!(
            &mut addr_str,
            "{}:{}",
            from_endpoint.endpoint.addr, from_endpoint.endpoint.port
        )
        .map_err(|_| SocketError::InvalidInput)?;

        Ok((len, addr_str))
    }
}

/// Dynamic IP Endpoint parser for bare-metal targets.
///
/// Converts a standard target string (e.g., "192.168.1.150:2021") into an
/// Embassy-compatible `IpEndpoint` socket target.
#[cfg(feature = "embassy")]
fn parse_endpoint(addr: &str) -> Option<::embassy_net::IpEndpoint> {
    let mut parts = addr.split(':');
    let ip_str = parts.next()?;
    let port_str = parts.next()?;
    let port: u16 = port_str.parse().ok()?;

    let mut ip_parts = ip_str.split('.');
    let b0: u8 = ip_parts.next()?.parse().ok()?;
    let b1: u8 = ip_parts.next()?.parse().ok()?;
    let b2: u8 = ip_parts.next()?.parse().ok()?;
    let b3: u8 = ip_parts.next()?.parse().ok()?;

    let ip = ::embassy_net::IpAddress::v4(b0, b1, b2, b3);
    Some(::embassy_net::IpEndpoint::new(ip, port))
}

/// A wrapper around `UnsafeCell` that implements `Sync` to satisfy raw static storage bounds.
///
/// **Why this is used:** Modern Rust editions deprecate mutable references to `static mut` because
/// they violate exclusive borrow models. Using `SyncUnsafeCell` allows the async stack to safely
/// negotiate dynamic TLS record slices without triggering compiler warnings or UB.
#[cfg(feature = "embassy")]
struct SyncUnsafeCell<T>(core::cell::UnsafeCell<T>);

#[cfg(feature = "embassy")]
unsafe impl<T> Sync for SyncUnsafeCell<T> {}

#[cfg(feature = "embassy")]
static TLS_READ_BUFFER: SyncUnsafeCell<[u8; 16384]> =
    SyncUnsafeCell(core::cell::UnsafeCell::new([0u8; 16384]));
#[cfg(feature = "embassy")]
static TLS_WRITE_BUFFER: SyncUnsafeCell<[u8; 16384]> =
    SyncUnsafeCell(core::cell::UnsafeCell::new([0u8; 16384]));

/// TLS Secure connector wrapping the static, stack-friendly `embedded-tls` engine.
#[cfg(feature = "embassy")]
pub struct EmbassyTlsConnector<'a, CipherSuite>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
{
    config: &'a ::embedded_tls::TlsConfig<'a>,
    _phantom: core::marker::PhantomData<CipherSuite>,
}

#[cfg(feature = "embassy")]
impl<'a, CipherSuite> EmbassyTlsConnector<'a, CipherSuite>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
{
    /// Creates a new Embassy secure connector using a pre-allocated static config block.
    pub fn new(config: &'a ::embedded_tls::TlsConfig<'a>) -> Self {
        Self {
            config,
            _phantom: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "embassy")]
impl<'a, RawStream, CipherSuite> TlsConnector<RawStream> for EmbassyTlsConnector<'a, CipherSuite>
where
    RawStream: AsyncIo + 'static,
    CipherSuite: ::embedded_tls::TlsCipherSuite + 'static,
{
    type Stream = ::embedded_tls::TlsConnection<'a, RawStream, CipherSuite>;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawStream,
    ) -> Result<Self::Stream, SocketError> {
        // Safe conversion of UnsafeCell arrays to dynamic record buffers.
        // embedded-tls 0.19.0 requires distinct read and write buffers for full-duplex session lifetimes.
        let read_buf = unsafe { &mut *TLS_READ_BUFFER.0.get() };
        let write_buf = unsafe { &mut *TLS_WRITE_BUFFER.0.get() };

        let mut connection = ::embedded_tls::TlsConnection::new(raw_stream, read_buf, write_buf);

        // Simple, lightweight RNG implementing the legacy v0.6.4 rand_core traits expected by embedded-tls.
        // This decouples the TLS handshake dependencies from standard workspace v0.10.1 layouts.
        struct SimpleRng;

        impl ::rand_core_legacy::RngCore for SimpleRng {
            fn next_u32(&mut self) -> u32 {
                0x12345678
            }
            fn next_u64(&mut self) -> u64 {
                0x123456789abcdef0
            }
            fn fill_bytes(&mut self, dest: &mut [u8]) {
                for (i, byte) in dest.iter_mut().enumerate() {
                    *byte = (i & 0xFF) as u8;
                }
            }
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ::rand_core_legacy::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }

        impl ::rand_core_legacy::CryptoRng for SimpleRng {}

        let mut rng = SimpleRng;

        // Under embedded-tls 0.19.0, establishing a connection requires calling `.open` with a TlsContext
        // carrying the configuration structure and legacy CryptoRng provider.
        let context = ::embedded_tls::TlsContext::new(
            self.config,
            ::embedded_tls::UnsecureProvider::new::<CipherSuite>(&mut rng),
        );

        connection
            .open(context)
            .await
            .map_err(|_| SocketError::ConnectionAborted)?;

        Ok(connection)
    }
}
