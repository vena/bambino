//! Host-side proof that `EmbassyTlsConnector::negotiated_version` reports the real negotiated
//! TLS version rather than `None` (GitHub issue #289).
//!
//! **Why this can run without an embassy target.** `EmbassyTlsConnector` is gated on the
//! `embassy` *feature*, not on a bare-metal target: it needs `mbedtls-rs` and an `AsyncIo`
//! stream, neither of which requires embassy-net's executor or an ESP32. Building the crate
//! with `--features "embassy,std"` therefore compiles the real connector on the host, where an
//! ordinary `TcpStream` stands in for the embedded stack's socket. That combination is not one
//! of the crate's three shipping targets — it exists so embassy-only code can be exercised in
//! CI. Run it with:
//!
//! ```sh
//! cargo test --no-default-features --features "embassy,std" --test embassy_tls_version_test
//! ```
//!
//! This must be an integration test rather than a `#[cfg(test)]` module: the crate's unit tests
//! unconditionally import `crate::io::tokio`, which does not exist under this feature set.
//!
//! **What this covers and what it does not.** It covers the accessor and both arms of the
//! `mbedtls_rs::TlsVersion` -> `io::TlsVersion` mapping against real handshakes, which is what
//! #289 changed. It does not exercise embassy-net, esp-hal, or any printer, and is not a
//! substitute for first-ever hardware verification of this backend — see `src/io/CLAUDE.md`.
#![cfg(all(feature = "embassy", feature = "std"))]

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use bambino::io::embassy::EmbassyTlsConnector;
use bambino::io::{TlsConnector, TlsVersion};

/// Fixed-sequence RNG standing in for the embedded platform's hardware entropy source.
///
/// `mbedtls_rs::Tls::new` demands `&'static mut (dyn CryptoRng + Send)`, which on a real target
/// is the chip's TRNG. Deterministic output is fine here and nowhere near fine anywhere else:
/// this is a loopback handshake against a server whose key material the test itself generates,
/// so a predictable stream has no secret to leak. Never lift this into library code.
struct TestRng(u64);

impl TestRng {
    /// SplitMix64 — enough structure that MbedTLS doesn't reject a constant stream.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// Only `TryRng` and the `TryCryptoRng` marker are implemented by hand: rand_core 0.10 blanket-
// impls `Rng` and `CryptoRng` for any `TryRng` whose `Error` is `Infallible`, so writing those
// two out as well collides with the blanket impls (E0119) rather than adding anything.
impl rand_core::TryRng for TestRng {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.next() as u32)
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(self.next())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        for chunk in dst.chunks_mut(8) {
            let bytes = self.next().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

impl rand_core::TryCryptoRng for TestRng {}

/// A blocking `TcpStream` presented as an `AsyncIo`.
///
/// The connector only ever awaits reads and writes, so blocking calls inside an `async fn` are
/// enough to drive a handshake to completion. A real embassy target supplies
/// `embassy_net::tcp::TcpSocket` here instead.
struct BlockingStream(TcpStream);

impl embedded_io_async::ErrorType for BlockingStream {
    type Error = embedded_io_async::ErrorKind;
}

/// `embedded-io-async` has no `From<std::io::ErrorKind>`, and the test never branches on which
/// error it got — any I/O failure here fails the test through the `expect` on the handshake.
fn io_err(_: std::io::Error) -> embedded_io_async::ErrorKind {
    embedded_io_async::ErrorKind::Other
}

impl embedded_io_async::Read for BlockingStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf).map_err(io_err)
    }
}

impl embedded_io_async::Write for BlockingStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf).map_err(io_err)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush().map_err(io_err)
    }
}

/// Drives a future to completion on the current thread.
///
/// Every await in this test resolves without ever returning `Pending` (the underlying socket is
/// blocking), so a waker that does nothing is sufficient and no async runtime is needed — which
/// matters because `tokio` is not enabled under this feature set.
fn block_on<F: Future>(fut: F) -> F::Output {
    use core::task::{Context, Poll, Waker};

    let mut fut = core::pin::pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

/// Serves one connection with rustls pinned to exactly `version`, returning the bound port.
///
/// rustls rather than mbedtls-rs for the server half because `ServerSessionConfig` has no
/// `max_version` field — see the dev-dependency comment in `Cargo.toml`. The handshake is
/// therefore cross-stack (MbedTLS client, rustls server), which is a fair proxy for talking to
/// a printer's vsFTPd and not to ourselves.
fn spawn_tls_server(version: &'static rustls::SupportedProtocolVersion) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();

    std::thread::spawn(move || {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("generate self-signed cert");
        let cert_der = cert.cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.signing_key.serialize_der())
            .expect("private key");

        let config = rustls::ServerConfig::builder_with_protocol_versions(&[version])
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("server config");

        let (mut stream, _) = listener.accept().expect("accept");
        let mut conn = rustls::ServerConnection::new(Arc::new(config)).expect("server conn");
        // Completes the handshake, then lets the connection drop; the assertions only need the
        // client side to have finished negotiating.
        let _ = conn.complete_io(&mut stream);
    });

    port
}

/// Connects the real `EmbassyTlsConnector` to a version-pinned server and reports what it says
/// the negotiated version was.
fn negotiated_against(
    tls: mbedtls_rs::TlsReference<'_>,
    version: &'static rustls::SupportedProtocolVersion,
) -> Option<TlsVersion> {
    let port = spawn_tls_server(version);
    let connector = EmbassyTlsConnector::new(tls);

    let raw = BlockingStream(TcpStream::connect(("127.0.0.1", port)).expect("connect"));
    let stream =
        block_on(connector.connect("localhost", raw)).expect("TLS handshake against loopback");

    connector.negotiated_version(&stream)
}

/// Both arms of the mapping, in one `#[test]` because MbedTLS permits exactly one `Tls` instance
/// per process: a second `Tls::new` returns `AlreadyCreated`, and `cargo test` runs every test in
/// this binary in one process. Splitting these in two therefore fails whichever runs second, no
/// matter how the shared instance is handed around. That single-instance rule is the same one
/// `src/io/CLAUDE.md` records as the reason `EmbassyTlsConnector` holds a `TlsReference` rather
/// than a `Tls`, so this is the library's constraint showing through, not a testing artifact.
#[test]
fn negotiated_version_reports_the_version_actually_negotiated() {
    let rng: &'static mut TestRng = Box::leak(Box::new(TestRng(0x0BAD_C0DE_DEAD_BEEF)));
    let tls = mbedtls_rs::Tls::new(rng).expect("client Tls");

    // The regression #289 fixed, in the form that matters: this returned `None` unconditionally,
    // so `require_tls_1_2_if_enforced` could never pass on a P2S or X2D no matter what the
    // printer actually negotiated.
    assert_eq!(
        negotiated_against(tls.reference(), &rustls::version::TLS12),
        Some(TlsVersion::Tls12),
        "connector must report the negotiated TLS 1.2, not None"
    );

    // The other arm. Also pins the distinction the connector depends on: it sets only
    // `min_version`, so against a 1.3-capable peer it reports 1.3 rather than silently capping —
    // which is why `enforces_ftps_tls_1_2` models still fail closed there.
    assert_eq!(
        negotiated_against(tls.reference(), &rustls::version::TLS13),
        Some(TlsVersion::Tls13),
        "connector must map MbedTLS's Tls1_3 onto io::TlsVersion::Tls13"
    );
}
