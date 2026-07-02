//! # Bare-Metal Embassy Runtime Integration
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for bare-metal targets utilizing the Embassy network
//! stack and `embedded-tls`.

#[cfg(feature = "embassy")]
use crate::ftps::FtpDataStreamFactory;
#[cfg(feature = "embassy")]
use crate::io::{
    AsyncIo, AsyncUdpSocket, SocketError, TimerError, TimerProvider, TlsConnector, TlsVersion,
};

/// Timer implementation designed for the hardware microsecond clock in Embassy.
#[cfg(feature = "embassy")]
pub struct EmbassyTimer;

#[cfg(feature = "embassy")]
impl TimerProvider for EmbassyTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        let micros = duration.as_micros() as u64;
        ::embassy_time::Timer::after(::embassy_time::Duration::from_micros(micros)).await;
        Ok(())
    }

    fn now_millis(&self) -> u64 {
        ::embassy_time::Instant::now().as_millis()
    }
}

/// UDP Socket implementation designed for the Embassy network stack.
///
/// Under Embassy, binding and state registration are coordinated via the stack's SocketSet
/// pool at boot time, so this type only implements [`AsyncUdpSocket`] (send/recv on an
/// already-existing socket) — it deliberately does not implement `BindableUdpSocket`,
/// since embassy-net's `UdpSocket::new()` requires pre-allocated buffer slices and its
/// `bind()` takes a typed `IpListenEndpoint`, not a `SocketAddr`. Construct one with
/// [`EmbassyUdpSocket::new()`] from an already-bound `embassy_net::udp::UdpSocket`.
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
    async fn send_to(
        &self,
        buf: &[u8],
        target: core::net::SocketAddr,
    ) -> Result<usize, SocketError> {
        // smoltcp's `From<SocketAddr> for IpEndpoint` requires both "proto-ipv4" and
        // "proto-ipv6"; this crate only enables "proto-ipv4" (SSDP is IPv4-only — multicast
        // 239.255.255.250 and broadcast 255.255.255.255 have no IPv6 equivalent), so convert
        // via `SocketAddrV4` explicitly and reject V6 targets.
        let endpoint: ::embassy_net::IpEndpoint = match target {
            core::net::SocketAddr::V4(v4) => v4.into(),
            core::net::SocketAddr::V6(_) => return Err(SocketError::InvalidInput),
        };
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
    ) -> Result<(usize, core::net::SocketAddr), SocketError> {
        let (len, from_endpoint) = self
            .inner
            .recv_from(buf)
            .await
            .map_err(|_| SocketError::ConnectionReset)?;

        // Under embassy-net 0.9.1, UdpMetadata wraps its IpEndpoint target inside the
        // `endpoint` field. `smoltcp::wire::IpEndpoint` converts directly to
        // `core::net::SocketAddr` — no string round-trip needed.
        Ok((len, from_endpoint.endpoint.into()))
    }
}

/// Wrapper around an `embedded-tls` connection over an Embassy-supplied buffer pair.
///
/// No longer guards a process-wide static — the read/write buffers are owned by the
/// [`EmbassyTlsConnector`] that produced this stream (see that type's doc comment).
#[cfg(feature = "embassy")]
pub struct EmbassyTlsStream<
    'a,
    RawStream: AsyncIo,
    CipherSuite: ::embedded_tls::TlsCipherSuite + 'static,
> {
    connection: ::embedded_tls::TlsConnection<'a, RawStream, CipherSuite>,
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::ErrorType
    for EmbassyTlsStream<'a, S, C>
{
    type Error = <::embedded_tls::TlsConnection<'a, S, C> as embedded_io_async::ErrorType>::Error;
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::Read
    for EmbassyTlsStream<'a, S, C>
{
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.connection.read(buf).await
    }
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::Write
    for EmbassyTlsStream<'a, S, C>
{
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.connection.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.connection.flush().await
    }
}

/// TLS Secure connector wrapping the `embedded-tls` engine over caller-supplied buffers.
///
/// Generic over `Rng`: callers must provide a platform-appropriate RNG implementation
/// (e.g., hardware TRNG peripheral). The RNG must implement the legacy `rand_core` v0.6
/// traits expected by `embedded-tls` v0.19.
///
/// **Buffer ownership.** `embedded-tls` needs a read and write scratch buffer for the
/// lifetime of a TLS session (16KB apiece is a reasonable default, matching TLS's max
/// record size). Earlier versions of this connector hid two such buffers behind
/// process-wide statics, which meant a second concurrent connection (e.g. FTPS's control
/// and data channels, opened at the same time) would panic. There is no such thing as a
/// concurrency-safe *global* buffer pair, so this connector takes its buffers from the
/// caller instead: construct one `EmbassyTlsConnector` per concurrent connection you need,
/// each with its own `&'a mut [u8]` pair, and the caller's board-RAM budget decides how
/// many can exist at once. `connect()` takes the buffers out of the connector on first use
/// (`Option::take`) — calling `connect()` again on the same connector without a fresh one
/// returns `SocketError::Other` instead of a second, aliased borrow.
#[cfg(feature = "embassy")]
pub struct EmbassyTlsConnector<'a, CipherSuite, Rng>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
    Rng: ::rand_core_legacy::CryptoRng + ::rand_core_legacy::RngCore,
{
    config: &'a ::embedded_tls::TlsConfig<'a>,
    rng: core::cell::RefCell<Rng>,
    read_buf: core::cell::RefCell<Option<&'a mut [u8]>>,
    write_buf: core::cell::RefCell<Option<&'a mut [u8]>>,
    _phantom: core::marker::PhantomData<CipherSuite>,
}

#[cfg(feature = "embassy")]
impl<'a, CipherSuite, Rng> EmbassyTlsConnector<'a, CipherSuite, Rng>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
    Rng: ::rand_core_legacy::CryptoRng + ::rand_core_legacy::RngCore,
{
    /// Creates a new Embassy secure connector with a caller-provided RNG and TLS scratch
    /// buffers. `read_buf`/`write_buf` are consumed by the first `connect()` call — size
    /// them for one TLS session (16KB is a safe default) and construct a separate connector
    /// per concurrent connection.
    pub fn new(
        config: &'a ::embedded_tls::TlsConfig<'a>,
        rng: Rng,
        read_buf: &'a mut [u8],
        write_buf: &'a mut [u8],
    ) -> Self {
        Self {
            config,
            rng: core::cell::RefCell::new(rng),
            read_buf: core::cell::RefCell::new(Some(read_buf)),
            write_buf: core::cell::RefCell::new(Some(write_buf)),
            _phantom: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "embassy")]
impl<'a, RawStream, CipherSuite, Rng> TlsConnector<RawStream>
    for EmbassyTlsConnector<'a, CipherSuite, Rng>
where
    RawStream: AsyncIo + 'static,
    CipherSuite: ::embedded_tls::TlsCipherSuite + 'static,
    Rng: ::rand_core_legacy::CryptoRng + ::rand_core_legacy::RngCore,
{
    type Stream = EmbassyTlsStream<'a, RawStream, CipherSuite>;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawStream,
    ) -> Result<Self::Stream, SocketError> {
        let read_buf = self.read_buf.borrow_mut().take().ok_or(SocketError::Other(
            "EmbassyTlsConnector buffers already consumed by a previous connect() call",
        ))?;
        let write_buf = self
            .write_buf
            .borrow_mut()
            .take()
            .ok_or(SocketError::Other(
                "EmbassyTlsConnector buffers already consumed by a previous connect() call",
            ))?;

        let mut connection = ::embedded_tls::TlsConnection::new(raw_stream, read_buf, write_buf);

        let mut rng = self.rng.borrow_mut();
        let context = ::embedded_tls::TlsContext::new(
            self.config,
            ::embedded_tls::UnsecureProvider::new::<CipherSuite>(&mut *rng),
        );

        connection
            .open(context)
            .await
            .map_err(|_| SocketError::ConnectionAborted)?;

        Ok(EmbassyTlsStream { connection })
    }

    /// `embedded-tls` 0.19 is a TLS 1.3-only client (confirmed against its docs — it
    /// has no TLS 1.2 handshake support and exposes no version-query method, since
    /// there is only ever one possible answer). This is therefore a constant, not a
    /// runtime query: any successful `connect()` on this connector negotiated TLS 1.3.
    /// A model whose `model.quirks().enforce_ftps_tls_1_2()` is true (P2S, X2D) is
    /// consequently incompatible with `EmbassyTlsConnector` — `BambuFtpsClient::connect()`
    /// will correctly reject it with `ProtocolViolation` rather than silently proceeding.
    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        Some(TlsVersion::Tls13)
    }
}

/// Passive/data-channel connection factory for the Embassy network stack.
///
/// Unlike Tokio's `TokioFtpDataStreamFactory` (which dials a fresh `TcpStream` per call),
/// `embassy_net::tcp::TcpSocket` needs pre-allocated rx/tx buffer slices at construction —
/// there's no way to dial a raw connection without them. `FtpDataStreamFactory::create_data_stream`
/// is called repeatedly from `&self` (once for FTPS's control channel, once per data-channel
/// transfer — `list_directory`, `upload_file`, `download_file` each open and close their own),
/// so a single buffer pair handed out once (Phase 2's `EmbassyTlsConnector` pattern) isn't
/// enough here.
///
/// Instead of hand-rolling a buffer pool, this wraps `embassy_net::tcp::client::TcpClient` —
/// embassy-net's own built-in connection pool (`embassy_net::tcp::client` module), which
/// solves exactly this problem: `TcpClientState<N, TX_SZ, RX_SZ>` pre-allocates N buffer
/// pairs, `TcpClient::connect()` checks one out and returns a `TcpConnection` that
/// automatically returns its slot to the pool on `Drop` — no unsafe code needed on our side,
/// and no risk of the panic-based mutual exclusion Phase 2 removed from `EmbassyTlsConnector`
/// (a pool with `N` slots simply fails a `connect()` call with `Error::ConnectionReset` if
/// all `N` are checked out, rather than panicking or aliasing memory).
///
/// **Why `&'static TcpClient`, not an owned one:** `FtpDataStreamFactory<RawIO>`'s `RawIO`
/// is a fixed type for the whole trait impl, not parameterized per call — so the returned
/// `TcpConnection<'x, ...>`'s lifetime `'x` must be a *constant*, chosen once, not tied to
/// however long any individual `create_data_stream` call happens to borrow `&self` for.
/// Storing an *owned* `TcpClient<'d, ...>` field can't satisfy that: borrowing a field out of
/// `&self` can never outlive that particular call's borrow of `self`. Storing a `&'static`
/// *reference* sidesteps the problem entirely — copying a `&'static` reference out from
/// behind an arbitrarily short `&self` borrow yields an independent value that is itself
/// still valid for `'static`, so `TcpConnection<'static, ...>` comes out clean regardless of
/// how briefly any given call borrowed the factory. This pushes the actual `'static` storage
/// question (a `static` item, `static_cell::StaticCell`, or similar) to application setup
/// code, matching Phase 2's "caller supplies the buffer storage" philosophy — see the
/// README's Embassy section for a worked example.
#[cfg(feature = "embassy")]
pub struct EmbassyFtpDataStreamFactory<
    const N: usize,
    const TX_SZ: usize = 2048,
    const RX_SZ: usize = 2048,
> {
    client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>,
}

#[cfg(feature = "embassy")]
impl<const N: usize, const TX_SZ: usize, const RX_SZ: usize>
    EmbassyFtpDataStreamFactory<N, TX_SZ, RX_SZ>
{
    /// `client` must be `'static` (e.g. built from a `static`/`StaticCell`-held
    /// `TcpClientState<N, TX_SZ, RX_SZ>`) — see this type's doc comment for why.
    pub fn new(
        client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>,
    ) -> Self {
        Self { client }
    }
}

#[cfg(feature = "embassy")]
impl<const N: usize, const TX_SZ: usize, const RX_SZ: usize>
    FtpDataStreamFactory<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>>
    for EmbassyFtpDataStreamFactory<N, TX_SZ, RX_SZ>
{
    async fn create_data_stream(
        &self,
        host: &str,
        port: u16,
    ) -> Result<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>, SocketError>
    {
        use ::embedded_nal_async::TcpConnect;

        // IPv4-only is deliberate, not a missing case: `host` here is always
        // `BambuFtpsClient`'s printer IP, which traces back to either a caller-supplied
        // literal IP or SSDP discovery (`discovery/parser.rs::parse_location`), which only
        // ever extracts a dotted-decimal IPv4 address from the LOCATION header — Bambu
        // printers don't advertise IPv6 or hostnames. `embassy-net`'s IPv6 stack
        // (`proto-ipv6`) also isn't enabled in this crate's `Cargo.toml`, so a hostname or
        // IPv6 literal here is a genuine caller error, not an unsupported-but-valid input.
        let ip: core::net::Ipv4Addr = host.parse().map_err(|_| SocketError::InvalidInput)?;
        let addr = core::net::SocketAddr::V4(core::net::SocketAddrV4::new(ip, port));

        self.client
            .connect(addr)
            .await
            .map_err(|_| SocketError::ConnectionRefused)
    }
}
