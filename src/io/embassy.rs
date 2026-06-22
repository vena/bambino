//! # Bare-Metal Embassy Runtime Integration
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport, 
//! and Timer interfaces for bare-metal targets utilizing the Embassy network 
//! stack and `embedded-tls`.

#[cfg(feature = "embassy")]
use crate::io::{AsyncIo, AsyncUdpSocket, TlsConnector, TimerProvider, SocketError};

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
        Err(SocketError::Other("Embassy socket sets must be pre-bound during hardware stack initialization"))
    }

    async fn send_to(&self, buf: &[u8], target: &str) -> Result<usize, SocketError> {
        let endpoint = parse_endpoint(target).ok_or(SocketError::InvalidInput)?;
        // Embassy-net UDP socket utilizes standard slice transmission
        self.inner.send_to(buf, endpoint).await
            .map_err(|_| SocketError::ConnectionReset)?;
        Ok(buf.len())
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, alloc::string::String), SocketError> {
        let (len, from_endpoint) = self.inner.recv_from(buf).await
            .map_err(|_| SocketError::ConnectionReset)?;
            
        // Reconstruct IP target string for standard interface consumption
        let mut addr_str = alloc::string::String::new();
        use core::fmt::Write;
        write!(&mut addr_str, "{}:{}", from_endpoint.addr, from_endpoint.port)
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

/// TLS Secure connector wrapping the static, stack-friendly `embedded-tls` engine.
#[cfg(feature = "embassy")]
pub struct EmbassyTlsConnector<'a, CipherSuite>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
{
    config: &'a ::embedded_tls::TlsConfig<'a, CipherSuite>,
}

#[cfg(feature = "embassy")]
impl<'a, CipherSuite> EmbassyTlsConnector<'a, CipherSuite>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
{
    /// Creates a new Embassy secure connector using a pre-allocated static config block.
    pub fn new(config: &'a ::embedded_tls::TlsConfig<'a, CipherSuite>) -> Self {
        Self { config }
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
        let mut connection = ::embedded_tls::TlsConnection::new(raw_stream, self.config);
        
        // Execute zero-alloc handshake directly on the pre-allocated buffer channel
        connection.handshake(::embedded_tls::HandshakeType::Client)
            .await
            .map_err(|_| SocketError::ConnectionAborted)?;
            
        Ok(connection)
    }
}