//! # Bare-Metal Embassy Runtime Integration
//!
//! Provides the concrete bindings of the abstract IO, Secure TLS transport,
//! and Timer interfaces for bare-metal targets utilizing the Embassy network
//! stack and `mbedtls-rs`.

#[cfg(feature = "embassy")]
use crate::io::{
    AsyncIo, AsyncUdpSocket, RawStreamFactory, SocketError, TimerError, TimerProvider,
    TlsConnector, TlsVersion, map_mbedtls_verify_flags,
};

#[cfg(all(feature = "embassy", not(feature = "std")))]
use alloc::vec::Vec;

/// Timer implementation designed for the hardware microsecond clock in Embassy.
#[cfg(feature = "embassy")]
pub struct EmbassyTimer;

#[cfg(feature = "embassy")]
impl TimerProvider for EmbassyTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        // Saturate, not truncate — as_micros() is u128 and `as u64` wraps. Unreachable in
        // practice (u64 micros is ~584,942 years), but it is the same defect class fixed in
        // esp_idf.rs's connect_timeout conversion, where it is reachable.
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
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
            // Preserve the specific embassy-net failure mode instead of collapsing
            // every `SendError` variant to `ConnectionReset` — `NoRoute` has no matching
            // `SocketError` variant (kept as `Other`, honest rather than a wrong-shaped
            // guess); `SocketNotBound`/`PacketTooLarge` map to existing variants that fit.
            .map_err(|e| match e {
                ::embassy_net::udp::SendError::NoRoute => {
                    SocketError::Other("embassy UDP send: no route to host".into())
                }
                ::embassy_net::udp::SendError::SocketNotBound => SocketError::NotConnected,
                ::embassy_net::udp::SendError::PacketTooLarge => SocketError::InvalidInput,
            })?;
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
            // `RecvError` currently has one variant (`Truncated` — buffer too small
            // for the received datagram), no matching `SocketError` variant exists, so this
            // stays `Other` rather than the misleading `ConnectionReset` it was before.
            .map_err(|_| {
                SocketError::Other("embassy UDP recv: buffer too small for received datagram".into())
            })?;

        // Under embassy-net 0.9.1, UdpMetadata wraps its IpEndpoint target inside the
        // `endpoint` field. `smoltcp::wire::IpEndpoint` converts directly to
        // `core::net::SocketAddr` — no string round-trip needed.
        Ok((len, from_endpoint.endpoint.into()))
    }
}

/// TLS Secure connector wrapping an `mbedtls-rs` async [`Session`](::mbedtls_rs::Session).
///
/// **One global `Tls` instance.** MbedTLS only permits one active library instance
/// program-wide (enforced by `mbedtls-rs` itself — a second `Tls::new()` call errors while one
/// is already live). The caller constructs that single `::mbedtls_rs::Tls` once at startup
/// (e.g. behind a `static_cell::StaticCell`, mirroring `EmbassyRawStreamFactory`'s `'static`
/// storage convention below — see the README's Embassy setup example) and passes a
/// [`TlsReference`](::mbedtls_rs::TlsReference) — a cheap `Copy` handle, not the `Tls` itself —
/// into each `EmbassyTlsConnector::new()` call. This lets MQTT's connector and FTPS's
/// control/data connectors all share the one instance concurrently.
///
/// **No caller-supplied buffers.** `mbedtls-rs`
/// allocates its own SSL context/config/record buffers per `Session` (via `mbedtls_calloc`,
/// 16 KiB in/out by default — see `Cargo.toml`'s `mbedtls-rs` dependency comment to shrink
/// this via the `ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` features), so `connect()`
/// can be called repeatedly on the same connector — there is no one-shot buffer-consumption
/// constraint to work around.
///
/// **`negotiated_version` always returns `None`, honestly.** `mbedtls-rs` exposes no public
/// API to read back the TLS version actually negotiated (confirmed by reading its source, not
/// assumed). This means `FtpsClient::connect()`'s TLS-1.2 enforcement check still fails
/// closed for P2S/X2D even after this backend swap; use
/// `PrinterClient::with_ftps_allow_unverified_tls_1_2(true)` to opt out of that check when
/// needed (see `src/ftps/CLAUDE.md` and this module's `CLAUDE.md`).
///
/// **No built-in connect timeout**, same as before: `connect()` has no retry/poll loop of its
/// own to bound — the hang risk lives inside `mbedtls-rs`'s handshake await. Callers that need
/// a bounded connect must race `EmbassyTlsConnector::connect` against
/// `embassy_time::with_timeout` themselves.
#[cfg(feature = "embassy")]
pub struct EmbassyTlsConnector<'a> {
    tls: ::mbedtls_rs::TlsReference<'a>,
    ca_chain: Option<::mbedtls_rs::Certificate<'a>>,
    creds: Option<::mbedtls_rs::Credentials<'a>>,
}

#[cfg(feature = "embassy")]
impl<'a> EmbassyTlsConnector<'a> {
    /// Creates a new connector against the single active [`Tls`](::mbedtls_rs::Tls) instance
    /// (via its [`TlsReference`](::mbedtls_rs::TlsReference)), defaulting to no certificate
    /// verification — matching this crate's existing unsafe-by-default convention on other
    /// platforms (`build_unsafe_client_config`), since Bambu printer certs chain to a private
    /// BBL CA that no OS trust store carries.
    pub fn new(tls: ::mbedtls_rs::TlsReference<'a>) -> Self {
        Self {
            tls,
            ca_chain: None,
            creds: None,
        }
    }

    /// Enables server certificate verification against the given CA chain. Without this,
    /// the connector never checks the printer's certificate.
    #[must_use]
    pub fn with_ca_chain(mut self, ca_chain: ::mbedtls_rs::Certificate<'a>) -> Self {
        self.ca_chain = Some(ca_chain);
        self
    }

    /// Supplies client credentials for mutual TLS (mTLS).
    #[must_use]
    pub fn with_client_credentials(mut self, creds: ::mbedtls_rs::Credentials<'a>) -> Self {
        self.creds = Some(creds);
        self
    }
}

#[cfg(feature = "embassy")]
impl<'a, RawStream> TlsConnector<RawStream> for EmbassyTlsConnector<'a>
where
    RawStream: AsyncIo,
{
    // `Session<'a, T>` implements `embedded_io_async::{ErrorType, Read, Write}` directly (see
    // `mbedtls-rs`'s `session/asynch.rs`), so it already satisfies `AsyncIo` via this crate's
    // blanket impl — no wrapper stream type is needed.
    type Stream = ::mbedtls_rs::Session<'a, RawStream>;

    async fn connect(
        &self,
        host: &str,
        raw_stream: RawStream,
    ) -> Result<Self::Stream, SocketError> {
        let mut config = ::mbedtls_rs::ClientSessionConfig::new();
        config.ca_chain = self.ca_chain.clone();
        config.creds = self.creds.clone();
        config.auth_mode = if self.ca_chain.is_some() {
            ::mbedtls_rs::AuthMode::Required
        } else {
            ::mbedtls_rs::AuthMode::None
        };
        config.min_version = ::mbedtls_rs::TlsVersion::Tls1_2;

        let mut session = ::mbedtls_rs::Session::new(
            self.tls,
            raw_stream,
            &::mbedtls_rs::SessionConfig::Client(config),
        )
        .map_err(|e| {
            log::debug!("mbedtls-rs Session::new failed: {e:?}");
            SocketError::ConnectionAborted
        })?;

        // `ClientSessionConfig.server_name` can't hold `host` directly: its lifetime is
        // pinned to this connector's `'a` (the same `'a` as the returned `Self::Stream`), but
        // `host` only lives for this call. `set_server_name` takes an independent, shorter-lived
        // `&CStr` for exactly this reason — MbedTLS copies it internally via
        // `mbedtls_ssl_set_hostname` before `set_server_name` returns, so the `CString` below
        // doesn't need to outlive this function.
        let host_cstring = alloc::ffi::CString::new(host).map_err(|_| SocketError::InvalidInput)?;
        session
            .set_server_name(&host_cstring)
            .map_err(|e| {
                log::debug!("mbedtls-rs set_server_name failed: {e:?}");
                SocketError::ConnectionAborted
            })?;

        // `tls_verification_details()` is the one post-handshake inspector `mbedtls-rs` does
        // expose (unlike a peer-cert accessor or the raw `mbedtls_ssl_context` pointer, whose
        // absence is why `peer_chain_der` returns `None` below) — it wraps
        // `mbedtls_ssl_get_verify_result`, so this backend can name *why* a certificate was
        // rejected even though it cannot hand the certificate itself back (GitHub issue #157).
        // A mask with no verdict keeps the pre-existing `ConnectionAborted`.
        if let Err(e) = session.connect().await {
            log::debug!("mbedtls-rs Session::connect failed: {e:?}");
            return Err(map_mbedtls_verify_flags(session.tls_verification_details())
                .map_or(SocketError::ConnectionAborted, SocketError::CertificateInvalid));
        }

        Ok(session)
    }

    /// `mbedtls-rs` exposes no API to read back the negotiated TLS version — see this
    /// type's doc comment above. Return `None` honestly rather than hard-coding a guess.
    fn negotiated_version(&self, _stream: &Self::Stream) -> Option<TlsVersion> {
        None
    }

    /// `mbedtls-rs` exposes neither a peer-certificate accessor nor the raw
    /// `mbedtls_ssl_context` pointer that would let this crate call
    /// `mbedtls_ssl_get_peer_cert` itself (confirmed by reading its source: the only
    /// post-handshake inspectors on `Session` are `tls_verification_details` and `tls_alpn`).
    /// The ESP-IDF backend can do this only because `esp_tls_get_ssl_context` hands out that
    /// pointer. Return `None` honestly — a consumer pinning certificates cannot do so on this
    /// backend today, and must fail closed rather than be handed a fabricated empty chain.
    fn peer_chain_der(&self, _stream: &Self::Stream) -> Option<Vec<Vec<u8>>> {
        None
    }
}

/// Raw (pre-TLS) connection factory for the Embassy network stack.
///
/// Unlike Tokio's `TokioRawStreamFactory` (which dials a fresh `TcpStream` per call),
/// `embassy_net::tcp::TcpSocket` needs pre-allocated rx/tx buffer slices at construction —
/// there's no way to dial a raw connection without them. `RawStreamFactory::dial` is called
/// repeatedly from `&self` (MQTT's lazy reconnect, and FTPS's control channel once plus one
/// data-channel connect per transfer — `list_directory`, `upload_file`, `download_file` each
/// open and close their own), so a single buffer pair handed out once (Phase 2's
/// `EmbassyTlsConnector` pattern) isn't enough here.
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
/// **Why `&'static TcpClient`, not an owned one:** `RawStreamFactory<RawIO>`'s `RawIO`
/// is a fixed type for the whole trait impl, not parameterized per call — so the returned
/// `TcpConnection<'x, ...>`'s lifetime `'x` must be a *constant*, chosen once, not tied to
/// however long any individual `dial` call happens to borrow `&self` for.
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
pub struct EmbassyRawStreamFactory<
    const N: usize,
    const TX_SZ: usize = 2048,
    const RX_SZ: usize = 2048,
> {
    client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>,
}

#[cfg(feature = "embassy")]
impl<const N: usize, const TX_SZ: usize, const RX_SZ: usize>
    EmbassyRawStreamFactory<N, TX_SZ, RX_SZ>
{
    /// `client` must be `'static` (e.g. built from a `static`/`StaticCell`-held `TcpClientState<N, TX_SZ, RX_SZ>`) — see this type's doc comment for why.
    pub fn new(
        client: &'static ::embassy_net::tcp::client::TcpClient<'static, N, TX_SZ, RX_SZ>,
    ) -> Self {
        Self { client }
    }
}

#[cfg(feature = "embassy")]
impl<const N: usize, const TX_SZ: usize, const RX_SZ: usize>
    RawStreamFactory<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>>
    for EmbassyRawStreamFactory<N, TX_SZ, RX_SZ>
{
    async fn dial(
        &self,
        host: &str,
        port: u16,
    ) -> Result<::embassy_net::tcp::client::TcpConnection<'static, N, TX_SZ, RX_SZ>, SocketError>
    {
        use ::embedded_nal_async::TcpConnect;

        // IPv4-only is deliberate, not a missing case: `host` here is always
        // `FtpsClient`'s printer IP, which traces back to either a caller-supplied
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
            // `embassy_net::tcp::client`'s `TcpConnect::Error` (`tcp::Error`) has a
            // single variant, `ConnectionReset` — used for both a genuine remote RST and pool
            // exhaustion (`TcpConnection::new` on an empty pool). Mapping it to
            // `ConnectionRefused` fabricated a distinction the source type doesn't make, which
            // could misroute `SocketError`-keyed retry/backoff decisions.
            .map_err(|_| SocketError::ConnectionReset)
    }
}
