*[bambino](../../index.md) / [io](../index.md) / [tokio](index.md)*

---

# Module `tokio`

# Tokio Host Runtime Implementation

Provides the concrete bindings of the abstract IO, Secure TLS transport,
and Timer interfaces for standard operating systems using the Tokio runtime
and the Rustls TLS stack.

## Contents

- [Types](#types)
  - [`TokioIo`](#tokioio)
  - [`TokioRawStreamFactory`](#tokiorawstreamfactory)
  - [`TokioTimer`](#tokiotimer)
  - [`TokioTlsConnector`](#tokiotlsconnector)
  - [`TokioUdpSocket`](#tokioudpsocket)
  - [`TokioFtpsClient`](#tokioftpsclient)
- [Functions](#functions)
  - [`build_unsafe_client_config`](#build-unsafe-client-config)
  - [`build_verified_client_config`](#build-verified-client-config)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`TokioIo`](#tokioio) | struct | Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds. |
| [`TokioRawStreamFactory`](#tokiorawstreamfactory) | struct | Raw (pre-TLS) connection factory for the Tokio runtime. |
| [`TokioTimer`](#tokiotimer) | struct | Timer implementation utilizing Tokio's non-blocking system clock registry. |
| [`TokioTlsConnector`](#tokiotlsconnector) | struct | TLS Secure connector wrapping Tokio-Rustls. |
| [`TokioUdpSocket`](#tokioudpsocket) | struct | UDP socket interface wrapping a native Tokio UdpSocket. |
| [`build_unsafe_client_config`](#build-unsafe-client-config) | fn | Builds a `ClientConfig` that accepts any certificate — see [`NoCertificateVerification`](cert_verify/index.md#nocertificateverification). |
| [`build_verified_client_config`](#build-verified-client-config) | fn | Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs. |
| [`TokioFtpsClient`](#tokioftpsclient) | type | An [`FtpsClient`](../../ftps/index.md) on the tokio backend, as `PrinterClient::ftps()` hands it out with `TokioTimer`. |

## Types

### `CnFallbackServerVerifier`

```rust
struct CnFallbackServerVerifier {
    // [REDACTED: Private Fields]
}
```

Certificate verifier for the "verified" (CA-checked) connection path that validates real
chain-of-trust against caller-supplied trusted roots, but — unlike rustls's default
`WebPkiServerVerifier` — works against real Bambu printer certs at all.

**Why this can't use `rustls-webpki`:** real Bambu printer certs are X.509 **v1** (confirmed
against a live P1S — no version tag, implicit v1 encoding per RFC 5280 §4.1.2.1).
`rustls-webpki` (confirmed against the pinned `0.103.13`, `src/cert.rs::version3`) rejects
*any* cert that isn't v3, unconditionally — this is deliberate mozilla::pkix policy
("We allow only v3"), not a bug, and it applies to `EndEntityCert`/`ParsedCertificate`
parsing used by chain validation *and* to the free functions `verify_tls12_signature`/
`verify_tls13_signature` (which independently re-parse the leaf via
`EndEntityCert::try_from` during the handshake's signature check). So neither chain
validation nor the handshake-signature check can be delegated to anything in
`rustls-webpki` for a real Bambu cert — confirmed as a known limitation other real-world
self-signed device certs have hit too: rustls/rustls#1298 (the identical
"UnsupportedCertVersion" error, hit by an unrelated user), rustls/webpki#205 ("Support
self-signed certificate"), and rustls/rustls#772, where the LND project hit the exact same
wall with its own self-signed device cert — their `SingleCertVerifier` pattern (comparing
the peer cert against a pinned expected cert, bypassing webpki's chain logic entirely) is
the community-blessed approach this verifier adapts, using signed-by-root trust instead of
exact-leaf pinning so it survives individual device cert rotation.

This verifier uses `x509-parser` instead (a general ASN.1/X.509 parser, not a
policy-enforcing validator — confirmed via its own test suite that it treats the version
field as optional, defaulting to v1 when absent, exactly per the DER grammar) for all
parsing, and does two independent things no other code in this crate does:
- **Chain-of-trust**: walks from the leaf through the presented intermediates (this
  used to check the leaf directly against the trusted roots only, silently ignoring
  `intermediates` — a legitimate two-level custom CA (offline root + issuing intermediate)
  failed with `UnknownIssuer` even though the chain was valid) until it either lands on a
  caller-supplied trusted root's public key or runs out of intermediates, verifying each
  issuer/subject match and signature link along the way, with an unexpired validity period
  on the leaf. (`verify_server_cert`, via `X509Certificate::verify_signature` — real
  `ring`-backed verification, not hand-rolled crypto.)
- **Handshake-signature check**: does the live TLS handshake signature verify under the
  leaf's own public key? (`verify_tls12_signature`/`verify_tls13_signature`, via
  `rustls_pki_types::SignatureVerificationAlgorithm::verify_signature` directly — this is
  the check that actually proves the peer holds the private key matching the presented
  cert; per the LND issue's own reasoning, this is what prevents MITM here, not the chain
  check alone.)

Identity (SAN-then-CN, mirroring mbedtls's `x509_crt_verify_name` algorithm) is still
checked last, same logic as before — only its data source changed, from a hand-rolled DER
walker to `x509-parser`'s parsed fields.

#### Implementations

- <span id="cnfallbackserververifier-new"></span>`fn new(ca_certs: impl IntoIterator<Item = CertificateDer<'static>>) -> Result<Self, RustlsError>`

  Builds the verifier from a set of trusted root certs. Fails if `ca_certs` is empty or
  any supplied cert fails to parse — there is nothing to validate a chain against
  otherwise, so failing fast at config-build time (rather than silently succeeding and
  only failing later at handshake time) is deliberate.

#### Trait Implementations

##### `impl Debug for CnFallbackServerVerifier`

- <span id="cnfallbackserververifier-debug-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl ServerCertVerifier for CnFallbackServerVerifier`

- <span id="cnfallbackserververifier-servercertverifier-verify-server-cert"></span>`fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], server_name: &ServerName<'_>, _ocsp_response: &[u8], now: UnixTime) -> Result<ServerCertVerified, RustlsError>`

- <span id="cnfallbackserververifier-servercertverifier-verify-tls12-signature"></span>`fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`

- <span id="cnfallbackserververifier-servercertverifier-verify-tls13-signature"></span>`fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`

- <span id="cnfallbackserververifier-servercertverifier-supported-verify-schemes"></span>`fn supported_verify_schemes(&self) -> Vec<SignatureScheme>`

### `NoCertificateVerification`

```rust
struct NoCertificateVerification;
```

Custom certificate verifier that disables **all** peer certificate verification.

This bypasses far more than the CA chain walk: `verify_tls12_signature` and
`verify_tls13_signature` both return `HandshakeSignatureValid::assertion()`
unconditionally, so the peer never proves possession of the private key matching the
certificate it presented. That handshake-signature check — not the chain check alone — is
what actually prevents a MITM (see [`CnFallbackServerVerifier`](#cnfallbackserververifier)'s own doc). Identity is not
checked either: any certificate from any host is accepted for any name.

**Why this is the default:**
Physical Bambu Lab printers host an onboard local MQTTS/FTPS broker whose leaf cert carries
the printer's serial number in the CN field and chains to BBL's own private CA, which is in
no OS certificate store — so standard verifiers reject the connection for lack of a trust
anchor, not because the cert is bad. Callers holding the BBL CA certs can verify properly via
[`build_verified_client_config`](#build-verified-client-config); this type is the fallback for callers who don't.

Earlier revisions of this comment described the leaf as *self-signed*. That is wrong: a live
P1S (firmware 01.10.00.00) completed a full chain-verified handshake against the BBL CA
anchors, so the leaf is CA-issued and a genuine chain of trust is available.

#### Trait Implementations

##### `impl Debug for NoCertificateVerification`

- <span id="nocertificateverification-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl ServerCertVerifier for NoCertificateVerification`

- <span id="nocertificateverification-servercertverifier-verify-server-cert"></span>`fn verify_server_cert(&self, _end_entity: &CertificateDer<'_>, _intermediates: &[CertificateDer<'_>], _server_name: &ServerName<'_>, _ocsp_response: &[u8], _now: UnixTime) -> Result<rustls::client::danger::ServerCertVerified, RustlsError>`

- <span id="nocertificateverification-servercertverifier-verify-tls12-signature"></span>`fn verify_tls12_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`

- <span id="nocertificateverification-servercertverifier-verify-tls13-signature"></span>`fn verify_tls13_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`

- <span id="nocertificateverification-servercertverifier-supported-verify-schemes"></span>`fn supported_verify_schemes(&self) -> Vec<SignatureScheme>`

  The provider's schemes, the same list [`CnFallbackServerVerifier`](#cnfallbackserververifier) offers, so a printer that
  completes a verified handshake also completes an unverified one.

### `TokioIo<T>`

```rust
struct TokioIo<T>();
```

Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.

#### Implementations

- <span id="tokioio-new"></span>`fn new(inner: T) -> Self`

  Wraps a Tokio stream.

- <span id="tokioio-into-inner"></span>`fn into_inner(self) -> T`

  Returns the wrapped stream.

- <span id="tokioio-get-ref"></span>`fn get_ref(&self) -> &T`

  Borrows the wrapped stream.

- <span id="tokioio-get-mut"></span>`fn get_mut(&mut self) -> &mut T`

  Mutably borrows the wrapped stream.

#### Trait Implementations

##### `impl<T> AsyncIo for TokioIo<T>`

##### `impl<T> ErrorType for TokioIo<T>`

- <span id="tokioio-errortype-type-error"></span>`type Error = StdIoError`

##### `impl RawStreamFactory<TokioIo<TcpStream>> for TokioRawStreamFactory`

- <span id="tokiorawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError>` — [`TokioIo`](#tokioio), [`SocketError`](../index.md#socketerror)

##### `impl<T: ::tokio::io::AsyncRead + Unpin> Read for TokioIo<T>`

- <span id="tokioio-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl TlsConnector<TokioIo<TcpStream>> for TokioTlsConnector`

- <span id="tokiotlsconnector-tlsconnector-type-stream"></span>`type Stream = TokioIo<TlsStream<TcpStream>>`

- <span id="tokiotlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>` — [`TokioIo`](#tokioio), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="tokiotlsconnector-tlsconnector-close"></span>`async fn close(&self, stream: &mut <Self as >::Stream) -> Result<(), SocketError>` — [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

  Sends `close_notify` via `AsyncWriteExt::shutdown`, which `tokio-rustls` implements as a
  TLS-level shutdown (queue the alert, flush it) rather than a bare socket close.

  Idempotent: rustls only queues the alert once, so a second call just re-flushes an empty
  buffer.

- <span id="tokiotlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

- <span id="tokiotlsconnector-tlsconnector-peer-chain-der"></span>`fn peer_chain_der(&self, stream: &<Self as >::Stream) -> Option<Vec<Vec<u8>>>` — [`TlsConnector`](../index.md#tlsconnector)

  rustls retains the peer chain on the connection after the handshake, so this is a
  straight copy of what the server sent, in wire order (leaf first).

##### `impl<T: ::tokio::io::AsyncWrite + Unpin> Write for TokioIo<T>`

- <span id="tokioio-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="tokioio-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `TokioRawStreamFactory`

```rust
struct TokioRawStreamFactory;
```

Raw (pre-TLS) connection factory for the Tokio runtime.

Creates raw TCP connections wrapped in [`TokioIo`](#tokioio) — used for MQTT's lazy connect and
FTPS passive-mode data transfers alike (the Tokio counterpart to
[`DummyFactory`](../../client/dummy/index.md)).

#### Trait Implementations

##### `impl RawStreamFactory<TokioIo<TcpStream>> for TokioRawStreamFactory`

- <span id="tokiorawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError>` — [`TokioIo`](#tokioio), [`SocketError`](../index.md#socketerror)

### `TokioTimer`

```rust
struct TokioTimer {
    // [REDACTED: Private Fields]
}
```

Timer implementation utilizing Tokio's non-blocking system clock registry.

#### Implementations

- <span id="tokiotimer-new"></span>`fn new() -> Self`

  Creates a timer, capturing the current instant as its monotonic epoch.

#### Trait Implementations

##### `impl Default for TokioTimer`

- <span id="tokiotimer-default"></span>`fn default() -> Self`

##### `impl TimerProvider for TokioTimer`

- <span id="tokiotimer-timerprovider-sleep"></span>`async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>` — [`TimerError`](../index.md#timererror)

- <span id="tokiotimer-timerprovider-now-millis"></span>`fn now_millis(&self) -> u64`

- <span id="tokiotimer-timerprovider-unix-millis"></span>`fn unix_millis(&self) -> Option<u64>`

### `TokioTlsConnector`

```rust
struct TokioTlsConnector {
    // [REDACTED: Private Fields]
}
```

TLS Secure connector wrapping Tokio-Rustls.

`Clone` shares the underlying `ClientConfig`, so one connector can serve every
`PrinterClient` channel that needs the same settings.

#### Implementations

- <span id="tokiotlsconnector-new"></span>`fn new(connector: tokio_rustls::TlsConnector) -> Self`

  Creates a connector given a pre-configured tokio-rustls connector instance.

- <span id="tokiotlsconnector-unverified"></span>`fn unverified(versions: TlsVersions) -> Self` — [`TlsVersions`](../index.md#tlsversions)

  A connector that accepts any certificate — see [`build_unsafe_client_config`](#build-unsafe-client-config).

- <span id="tokiotlsconnector-verified"></span>`fn verified(ca_certs: impl IntoIterator<Item = CertificateDer<'static>>, client_auth: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>, versions: TlsVersions) -> Result<Self, rustls::Error>` — [`TlsVersions`](../index.md#tlsversions)

  A connector that verifies the printer against `ca_certs` — see [`build_verified_client_config`](#build-verified-client-config).

#### Trait Implementations

##### `impl Clone for TokioTlsConnector`

- <span id="tokiotlsconnector-clone"></span>`fn clone(&self) -> TokioTlsConnector` — [`TokioTlsConnector`](#tokiotlsconnector)

##### `impl TlsConnector<TokioIo<TcpStream>> for TokioTlsConnector`

- <span id="tokiotlsconnector-tlsconnector-type-stream"></span>`type Stream = TokioIo<TlsStream<TcpStream>>`

- <span id="tokiotlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>` — [`TokioIo`](#tokioio), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="tokiotlsconnector-tlsconnector-close"></span>`async fn close(&self, stream: &mut <Self as >::Stream) -> Result<(), SocketError>` — [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

  Sends `close_notify` via `AsyncWriteExt::shutdown`, which `tokio-rustls` implements as a
  TLS-level shutdown (queue the alert, flush it) rather than a bare socket close.

  Idempotent: rustls only queues the alert once, so a second call just re-flushes an empty
  buffer.

- <span id="tokiotlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

- <span id="tokiotlsconnector-tlsconnector-peer-chain-der"></span>`fn peer_chain_der(&self, stream: &<Self as >::Stream) -> Option<Vec<Vec<u8>>>` — [`TlsConnector`](../index.md#tlsconnector)

  rustls retains the peer chain on the connection after the handshake, so this is a
  straight copy of what the server sent, in wire order (leaf first).

### `TokioUdpSocket`

```rust
struct TokioUdpSocket {
    // [REDACTED: Private Fields]
}
```

UDP socket interface wrapping a native Tokio UdpSocket.

#### Trait Implementations

##### `impl AsyncUdpSocket for TokioUdpSocket`

- <span id="tokioudpsocket-asyncudpsocket-send-to"></span>`async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>` — [`SocketError`](../index.md#socketerror)

- <span id="tokioudpsocket-asyncudpsocket-recv-from"></span>`async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), SocketError>` — [`SocketError`](../index.md#socketerror)

  Asynchronously reads an incoming datagram, bounding the wait block with a timeout.

  **Why this is critical:**
  By default, `tokio::net::UdpSocket::recv_from` blocks indefinitely if no packet is available.
  During sweeps where some network environments drop unicast discovery replies, this blocks
  execution threads forever. Wrapping the call in a 100ms timeout enables standard polling
  loops to proceed and exit gracefully.

##### `impl BindableUdpSocket for TokioUdpSocket`

- <span id="tokioudpsocket-bindableudpsocket-bind"></span>`async fn bind(addr: SocketAddr) -> Result<Self, SocketError>` — [`SocketError`](../index.md#socketerror)

### `TokioFtpsClient`

```rust
type TokioFtpsClient = crate::ftps::FtpsClient<TokioIo<::tokio::net::TcpStream>, TokioTlsConnector, TokioRawStreamFactory, TokioTimer>;
```

An [`FtpsClient`](../../ftps/index.md) on the tokio backend, as `PrinterClient::ftps()` hands it out with `TokioTimer`.


---

## Functions

### `build_unsafe_client_config`

```rust
fn build_unsafe_client_config(versions: crate::io::TlsVersions) -> std::sync::Arc<rustls::ClientConfig>
```

**Types:** [`TlsVersions`](../index.md#tlsversions)

Builds a `ClientConfig` that accepts any certificate — see [`NoCertificateVerification`](#nocertificateverification).

[`TlsVersions::Tls12Only`](../index.md#tlsversions) is required for FTPS on P2S and X2D
([`ModelQuirks::requires_ftps_tls_1_2`](../../quirks/index.md#modelquirks))
[REF-FTPS-CONN].

### `build_verified_client_config`

```rust
fn build_verified_client_config(ca_certs: impl IntoIterator<Item = rustls_pki_types::CertificateDer<'static>>, client_auth: Option<(Vec<rustls_pki_types::CertificateDer<'static>>, rustls_pki_types::PrivateKeyDer<'static>)>, versions: crate::io::TlsVersions) -> Result<std::sync::Arc<rustls::ClientConfig>, rustls::Error>
```

**Types:** [`TlsVersions`](../index.md#tlsversions)

Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs.

Use `rustls_pki_types::pem::PemObject` to load PEM files:
```rust,ignore
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
let ca = CertificateDer::from_pem_file("ca.pem")?;
```

`client_auth`: pass `Some((cert_chain, key))` for mutual TLS, `None` for server-only verification.
See [`build_unsafe_client_config`](#build-unsafe-client-config) for when `versions` must be [`TlsVersions::Tls12Only`](../index.md#tlsversions).

