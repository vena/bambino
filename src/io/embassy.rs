//! # Bare-Metal Embassy Runtime Integration
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for bare-metal targets utilizing the Embassy network
//! stack and `embedded-tls`.

#[cfg(feature = "embassy")]
use crate::io::{AsyncIo, AsyncUdpSocket, SocketError, TimerError, TimerProvider, TlsConnector};

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

/// A wrapper around `UnsafeCell` that implements `Sync` to satisfy raw static storage bounds.
///
/// The blanket `Sync` impl is restricted to the concrete buffer type used below.
/// On single-threaded Embassy executors, concurrent access is structurally prevented
/// by the `TLS_BUFFERS_IN_USE` atomic guard (see `BufferGuard`).
#[cfg(feature = "embassy")]
struct SyncUnsafeCell<T>(core::cell::UnsafeCell<T>);

// SAFETY: Only safe when exclusive access is enforced externally.
// The `BufferGuard` atomic flag guarantees at most one live borrow at a time.
#[cfg(feature = "embassy")]
unsafe impl Sync for SyncUnsafeCell<[u8; 16384]> {}

#[cfg(feature = "embassy")]
static TLS_READ_BUFFER: SyncUnsafeCell<[u8; 16384]> =
    SyncUnsafeCell(core::cell::UnsafeCell::new([0u8; 16384]));
#[cfg(feature = "embassy")]
static TLS_WRITE_BUFFER: SyncUnsafeCell<[u8; 16384]> =
    SyncUnsafeCell(core::cell::UnsafeCell::new([0u8; 16384]));

#[cfg(feature = "embassy")]
static TLS_BUFFERS_IN_USE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// RAII guard ensuring exclusive access to the static TLS buffers.
///
/// Panics on construction if buffers are already held by another `TlsConnection`.
/// Automatically releases the buffers when the owning `GuardedTlsConnection` is dropped.
#[cfg(feature = "embassy")]
struct BufferGuard;

#[cfg(feature = "embassy")]
impl BufferGuard {
    fn acquire() -> Self {
        if TLS_BUFFERS_IN_USE.swap(true, core::sync::atomic::Ordering::SeqCst) {
            panic!("TLS buffers already in use — only one concurrent TLS connection is supported");
        }
        BufferGuard
    }
}

#[cfg(feature = "embassy")]
impl Drop for BufferGuard {
    fn drop(&mut self) {
        TLS_BUFFERS_IN_USE.store(false, core::sync::atomic::Ordering::SeqCst);
    }
}

/// Wrapper coupling a `TlsConnection` with its `BufferGuard` lifetime.
///
/// Dropping this struct releases the static TLS buffers for reuse by a subsequent connection.
#[cfg(feature = "embassy")]
pub struct GuardedTlsConnection<
    'a,
    RawStream: AsyncIo,
    CipherSuite: ::embedded_tls::TlsCipherSuite + 'static,
> {
    connection: ::embedded_tls::TlsConnection<'a, RawStream, CipherSuite>,
    _guard: BufferGuard,
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::ErrorType
    for GuardedTlsConnection<'a, S, C>
{
    type Error = <::embedded_tls::TlsConnection<'a, S, C> as embedded_io_async::ErrorType>::Error;
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::Read
    for GuardedTlsConnection<'a, S, C>
{
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.connection.read(buf).await
    }
}

#[cfg(feature = "embassy")]
impl<'a, S: AsyncIo, C: ::embedded_tls::TlsCipherSuite + 'static> embedded_io_async::Write
    for GuardedTlsConnection<'a, S, C>
{
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.connection.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.connection.flush().await
    }
}

/// TLS Secure connector wrapping the static, stack-friendly `embedded-tls` engine.
///
/// Generic over `Rng`: callers must provide a platform-appropriate RNG implementation
/// (e.g., hardware TRNG peripheral). The RNG must implement the legacy `rand_core` v0.6
/// traits expected by `embedded-tls` v0.19.
#[cfg(feature = "embassy")]
pub struct EmbassyTlsConnector<'a, CipherSuite, Rng>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
    Rng: ::rand_core_legacy::CryptoRng + ::rand_core_legacy::RngCore,
{
    config: &'a ::embedded_tls::TlsConfig<'a>,
    rng: core::cell::RefCell<Rng>,
    _phantom: core::marker::PhantomData<CipherSuite>,
}

#[cfg(feature = "embassy")]
impl<'a, CipherSuite, Rng> EmbassyTlsConnector<'a, CipherSuite, Rng>
where
    CipherSuite: ::embedded_tls::TlsCipherSuite,
    Rng: ::rand_core_legacy::CryptoRng + ::rand_core_legacy::RngCore,
{
    /// Creates a new Embassy secure connector with a caller-provided RNG.
    pub fn new(config: &'a ::embedded_tls::TlsConfig<'a>, rng: Rng) -> Self {
        Self {
            config,
            rng: core::cell::RefCell::new(rng),
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
    type Stream = GuardedTlsConnection<'a, RawStream, CipherSuite>;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawStream,
    ) -> Result<Self::Stream, SocketError> {
        let guard = BufferGuard::acquire();

        // SAFETY: Exclusive access is enforced by BufferGuard — only one borrow
        // can exist at a time, and the guard lives as long as the returned connection.
        let read_buf = unsafe { &mut *TLS_READ_BUFFER.0.get() };
        let write_buf = unsafe { &mut *TLS_WRITE_BUFFER.0.get() };

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

        Ok(GuardedTlsConnection {
            connection,
            _guard: guard,
        })
    }
}
