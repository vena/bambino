**bambino > io > tokio**

# Module: io::tokio

## Contents

**Structs**

- [`CnFallbackServerVerifier`](#cnfallbackserververifier) - Certificate verifier for the "verified" (CA-checked) connection path that validates real
- [`NoCertificateVerification`](#nocertificateverification) - Custom certificate verifier that bypasses standard CA chain authority validation.
- [`TokioIo`](#tokioio) - Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.
- [`TokioIoError`](#tokioioerror) - Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.
- [`TokioRawStreamFactory`](#tokiorawstreamfactory) - Raw (pre-TLS) connection factory for the Tokio runtime.
- [`TokioTimer`](#tokiotimer) - Timer implementation utilizing Tokio's non-blocking system clock registry.
- [`TokioTlsConnector`](#tokiotlsconnector) - TLS Secure connector wrapping Tokio-Rustls.
- [`TokioUdpSocket`](#tokioudpsocket) - UDP socket interface wrapping a native Tokio UdpSocket.

**Functions**

- [`build_unsafe_client_config`](#build_unsafe_client_config) - Builds an unsafe `ClientConfig` with default TLS version negotiation (TLS 1.2 + 1.3).
- [`build_unsafe_client_config_with_options`](#build_unsafe_client_config_with_options) - Builds an unsafe `ClientConfig` with configurable TLS version constraints.
- [`build_verified_client_config`](#build_verified_client_config) - Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs.
- [`build_verified_client_config_with_options`](#build_verified_client_config_with_options) - Builds a verified `ClientConfig` with configurable TLS version constraints.
- [`to_socket_error`](#to_socket_error) - Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum.

---

## bambino::io::tokio::CnFallbackServerVerifier

*Struct*

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
self-signed device certs have hit too (see `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md` for the
GitHub issue citations, including the LND project's identical problem).

This verifier uses `x509-parser` instead (a general ASN.1/X.509 parser, not a
policy-enforcing validator — confirmed via its own test suite that it treats the version
field as optional, defaulting to v1 when absent, exactly per the DER grammar) for all
parsing, and does two independent things no other code in this crate does:
- **Chain-of-trust**: is the leaf's signature valid under one of the caller-supplied
  trusted roots' public keys, with a matching issuer/subject and unexpired validity period?
  (`verify_server_cert`, via `X509Certificate::verify_signature` — real `ring`-backed
  verification, not hand-rolled crypto.)
- **Handshake-signature check**: does the live TLS handshake signature verify under the
  leaf's own public key? (`verify_tls12_signature`/`verify_tls13_signature`, via
  `rustls_pki_types::SignatureVerificationAlgorithm::verify_signature` directly — this is
  the check that actually proves the peer holds the private key matching the presented
  cert; per the LND issue's own reasoning, this is what prevents MITM here, not the chain
  check alone.)

Identity (SAN-then-CN, mirroring mbedtls's `x509_crt_verify_name` algorithm) is still
checked last, same logic as before — only its data source changed, from a hand-rolled DER
walker to `x509-parser`'s parsed fields.

**Methods:**

- `fn new<impl IntoIterator<Item = CertificateDer<'static>>>(ca_certs: impl Trait) -> Result<Self, RustlsError>` - Builds the verifier from a set of trusted root certs. Fails if `ca_certs` is empty or

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut core::fmt::Formatter) -> core::fmt::Result`
- **ServerCertVerifier**
  - `fn verify_server_cert(self: &Self, end_entity: &CertificateDer, _intermediates: &[CertificateDer], server_name: &ServerName, _ocsp_response: &[u8], now: UnixTime) -> Result<ServerCertVerified, RustlsError>`
  - `fn verify_tls12_signature(self: &Self, message: &[u8], cert: &CertificateDer, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`
  - `fn verify_tls13_signature(self: &Self, message: &[u8], cert: &CertificateDer, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`
  - `fn supported_verify_schemes(self: &Self) -> Vec<SignatureScheme>`



## bambino::io::tokio::NoCertificateVerification

*Struct*

Custom certificate verifier that bypasses standard CA chain authority validation.

**Why this is required:**
Physical Bambu Lab printers (all models) host an onboard local MQTTS/FTPS broker
utilizing self-signed certificates with the printer's serial number in the CN field.
Because these do not trace back to any root authority in OS certificate stores,
standard verifiers reject the connections immediately.

**Unit Struct**

**Trait Implementations:**

- **ServerCertVerifier**
  - `fn verify_server_cert(self: &Self, _end_entity: &CertificateDer, _intermediates: &[CertificateDer], _server_name: &ServerName, _ocsp_response: &[u8], _now: UnixTime) -> Result<rustls::client::danger::ServerCertVerified, RustlsError>`
  - `fn verify_tls12_signature(self: &Self, _message: &[u8], _cert: &CertificateDer, _dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`
  - `fn verify_tls13_signature(self: &Self, _message: &[u8], _cert: &CertificateDer, _dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, RustlsError>`
  - `fn supported_verify_schemes(self: &Self) -> Vec<SignatureScheme>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::io::tokio::TokioIo

*Struct*

Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.

**Generic Parameters:**
- T

**Tuple Struct**: `(T)`

**Traits:** ErrorType

**Trait Implementations:**

- **Write**
  - `fn write(self: & mut Self, buf: &[u8]) -> Result<usize, <Self as >::Error>`
  - `fn flush(self: & mut Self) -> Result<(), <Self as >::Error>`
- **Read**
  - `fn read(self: & mut Self, buf: & mut [u8]) -> Result<usize, <Self as >::Error>`



## bambino::io::tokio::TokioIoError

*Struct*

Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.

**Tuple Struct**: `(std::io::Error)`

**Trait Implementations:**

- **Error**
  - `fn source(self: &Self) -> Option<&dyn std::error::Error>`
- **Error**
  - `fn kind(self: &Self) -> embedded_io_async::ErrorKind`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Display**
  - `fn fmt(self: &Self, f: & mut core::fmt::Formatter) -> core::fmt::Result`



## bambino::io::tokio::TokioRawStreamFactory

*Struct*

Raw (pre-TLS) connection factory for the Tokio runtime.

Creates raw TCP connections wrapped in [`TokioIo`] — used for MQTT's lazy connect and
FTPS passive-mode data transfers alike (the Tokio counterpart to
[`DummyFactory`](crate::client::dummy::DummyFactory)).

**Unit Struct**

**Trait Implementations:**

- **RawStreamFactory**
  - `fn dial(self: &Self, host: &str, port: u16) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError>`



## bambino::io::tokio::TokioTimer

*Struct*

Timer implementation utilizing Tokio's non-blocking system clock registry.

**Methods:**

- `fn new() -> Self` - Creates a timer, capturing the current instant as its monotonic epoch.

**Trait Implementations:**

- **Default**
  - `fn default() -> Self`
- **TimerProvider**
  - `fn sleep(self: &Self, duration: core::time::Duration) -> Result<(), TimerError>`
  - `fn now_millis(self: &Self) -> u64`



## bambino::io::tokio::TokioTlsConnector

*Struct*

TLS Secure connector wrapping Tokio-Rustls.

**Methods:**

- `fn new(connector: tokio_rustls::TlsConnector) -> Self` - Creates a connector given a pre-configured tokio-rustls connector instance.

**Trait Implementations:**

- **TlsConnector**
  - `fn connect(self: &Self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>`
  - `fn negotiated_version(self: &Self, stream: &<Self as >::Stream) -> Option<TlsVersion>`



## bambino::io::tokio::TokioUdpSocket

*Struct*

UDP socket interface wrapping a native Tokio UdpSocket.

**Trait Implementations:**

- **AsyncUdpSocket**
  - `fn send_to(self: &Self, buf: &[u8], target: SocketAddr) -> Result<usize, SocketError>`
  - `fn recv_from(self: &Self, buf: & mut [u8]) -> Result<(usize, SocketAddr), SocketError>` - Asynchronously reads an incoming datagram, bounding the wait block with a timeout.
- **BindableUdpSocket**
  - `fn bind(addr: SocketAddr) -> Result<Self, SocketError>`



## bambino::io::tokio::build_unsafe_client_config

*Function*

Builds an unsafe `ClientConfig` with default TLS version negotiation (TLS 1.2 + 1.3).

```rust
fn build_unsafe_client_config() -> std::sync::Arc<rustls::ClientConfig>
```



## bambino::io::tokio::build_unsafe_client_config_with_options

*Function*

Builds an unsafe `ClientConfig` with configurable TLS version constraints.

When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only. This is
required for P2S and X2D models whose embedded vsFTPd servers fail on TLS 1.3
session tickets [REF-FTPS-CONN].

```rust
fn build_unsafe_client_config_with_options(force_tls_1_2: bool) -> std::sync::Arc<rustls::ClientConfig>
```



## bambino::io::tokio::build_verified_client_config

*Function*

Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs.

Use `rustls_pki_types::pem::PemObject` to load PEM files:
```ignore
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
let ca = CertificateDer::from_pem_file("ca.pem")?;
```

`client_auth`: pass `Some((cert_chain, key))` for mutual TLS, `None` for server-only verification.

```rust
fn build_verified_client_config<impl IntoIterator<Item = CertificateDer<'static>>>(ca_certs: impl Trait, client_auth: Option<(Vec<rustls_pki_types::CertificateDer<'static>>, rustls_pki_types::PrivateKeyDer<'static>)>) -> Result<std::sync::Arc<rustls::ClientConfig>, rustls::Error>
```



## bambino::io::tokio::build_verified_client_config_with_options

*Function*

Builds a verified `ClientConfig` with configurable TLS version constraints.

When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only (required
for FTPS data channels on P2S/X2D models [REF-FTPS-CONN]).

```rust
fn build_verified_client_config_with_options<impl IntoIterator<Item = CertificateDer<'static>>>(ca_certs: impl Trait, client_auth: Option<(Vec<rustls_pki_types::CertificateDer<'static>>, rustls_pki_types::PrivateKeyDer<'static>)>, force_tls_1_2: bool) -> Result<std::sync::Arc<rustls::ClientConfig>, rustls::Error>
```



## bambino::io::tokio::to_socket_error

*Function*

Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum.

```rust
fn to_socket_error(err: std::io::Error) -> crate::io::SocketError
```



