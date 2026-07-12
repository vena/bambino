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
  - [`TokioIoError`](#tokioioerror)
  - [`TokioRawStreamFactory`](#tokiorawstreamfactory)
  - [`TokioTimer`](#tokiotimer)
  - [`TokioTlsConnector`](#tokiotlsconnector)
  - [`TokioUdpSocket`](#tokioudpsocket)
- [Functions](#functions)
  - [`build_unsafe_client_config`](#build-unsafe-client-config)
  - [`build_unsafe_client_config_with_options`](#build-unsafe-client-config-with-options)
  - [`build_verified_client_config`](#build-verified-client-config)
  - [`build_verified_client_config_with_options`](#build-verified-client-config-with-options)
  - [`to_socket_error`](#to-socket-error)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`TokioIo`](#tokioio) | struct | Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds. |
| [`TokioIoError`](#tokioioerror) | struct | Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait. |
| [`TokioRawStreamFactory`](#tokiorawstreamfactory) | struct | Raw (pre-TLS) connection factory for the Tokio runtime. |
| [`TokioTimer`](#tokiotimer) | struct | Timer implementation utilizing Tokio's non-blocking system clock registry. |
| [`TokioTlsConnector`](#tokiotlsconnector) | struct | TLS Secure connector wrapping Tokio-Rustls. |
| [`TokioUdpSocket`](#tokioudpsocket) | struct | UDP socket interface wrapping a native Tokio UdpSocket. |
| [`build_unsafe_client_config`](#build-unsafe-client-config) | fn | Builds an unsafe `ClientConfig` with default TLS version negotiation (TLS 1.2 + 1.3). |
| [`build_unsafe_client_config_with_options`](#build-unsafe-client-config-with-options) | fn | Builds an unsafe `ClientConfig` with configurable TLS version constraints. |
| [`build_verified_client_config`](#build-verified-client-config) | fn | Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs. |
| [`build_verified_client_config_with_options`](#build-verified-client-config-with-options) | fn | Builds a verified `ClientConfig` with configurable TLS version constraints. |
| [`to_socket_error`](#to-socket-error) | fn | Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum. |

## Types

### `TokioIo<T>`

```rust
struct TokioIo<T>(T);
```

Adapter wrapping any Tokio `AsyncRead` and `AsyncWrite` implementation to satisfy `embedded-io-async` bounds.

#### Trait Implementations

##### `impl<T> AsyncIo for TokioIo<T>`

##### `impl<T> ErrorType for TokioIo<T>`

- <span id="tokioio-errortype-type-error"></span>`type Error = TokioIoError`

##### `impl RawStreamFactory<TokioIo<TcpStream>> for TokioRawStreamFactory`

- <span id="tokiorawstreamfactory-rawstreamfactory-dial"></span>`async fn dial(&self, host: &str, port: u16) -> Result<TokioIo<::tokio::net::TcpStream>, SocketError>` — [`TokioIo`](#tokioio), [`SocketError`](../index.md#socketerror)

##### `impl<T: ::tokio::io::AsyncRead + Unpin> Read for TokioIo<T>`

- <span id="tokioio-read"></span>`async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Self as >::Error>`

##### `impl<T> Same for TokioIo<T>`

- <span id="tokioio-same-type-output"></span>`type Output = T`

##### `impl TlsConnector<TokioIo<TcpStream>> for TokioTlsConnector`

- <span id="tokiotlsconnector-tlsconnector-type-stream"></span>`type Stream = TokioIo<TlsStream<TcpStream>>`

- <span id="tokiotlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>` — [`TokioIo`](#tokioio), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="tokiotlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

##### `impl<T: ::tokio::io::AsyncWrite + Unpin> Write for TokioIo<T>`

- <span id="tokioio-write"></span>`async fn write(&mut self, buf: &[u8]) -> Result<usize, <Self as >::Error>`

- <span id="tokioio-write-flush"></span>`async fn flush(&mut self) -> Result<(), <Self as >::Error>`

### `TokioIoError`

```rust
struct TokioIoError(std::io::Error);
```

Wrapper around `std::io::Error` implementing the `embedded-io-async::Error` trait.

#### Trait Implementations

##### `impl Debug for TokioIoError`

- <span id="tokioioerror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for TokioIoError`

- <span id="tokioioerror-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Error for TokioIoError`

- <span id="tokioioerror-error-source"></span>`fn source(&self) -> Option<&dyn std::error::Error>`

##### `impl ToString for TokioIoError`

- <span id="tokioioerror-tostring-to-string"></span>`fn to_string(&self) -> String`

### `TokioRawStreamFactory`

```rust
struct TokioRawStreamFactory;
```

Raw (pre-TLS) connection factory for the Tokio runtime.

Creates raw TCP connections wrapped in [`TokioIo`](#tokioio) — used for MQTT's lazy connect and
FTPS passive-mode data transfers alike (the Tokio counterpart to
[`DummyFactory`](crate::client::dummy::DummyFactory)).

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

### `TokioTlsConnector`

```rust
struct TokioTlsConnector {
    // [REDACTED: Private Fields]
}
```

TLS Secure connector wrapping Tokio-Rustls.

#### Implementations

- <span id="tokiotlsconnector-new"></span>`fn new(connector: tokio_rustls::TlsConnector) -> Self`

  Creates a connector given a pre-configured tokio-rustls connector instance.

#### Trait Implementations

##### `impl TlsConnector<TokioIo<TcpStream>> for TokioTlsConnector`

- <span id="tokiotlsconnector-tlsconnector-type-stream"></span>`type Stream = TokioIo<TlsStream<TcpStream>>`

- <span id="tokiotlsconnector-tlsconnector-connect"></span>`async fn connect(&self, host: &str, raw_stream: TokioIo<::tokio::net::TcpStream>) -> Result<<Self as >::Stream, SocketError>` — [`TokioIo`](#tokioio), [`TlsConnector`](../index.md#tlsconnector), [`SocketError`](../index.md#socketerror)

- <span id="tokiotlsconnector-tlsconnector-negotiated-version"></span>`fn negotiated_version(&self, stream: &<Self as >::Stream) -> Option<TlsVersion>` — [`TlsConnector`](../index.md#tlsconnector), [`TlsVersion`](../index.md#tlsversion)

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

##### `impl BindableUdpSocket for TokioUdpSocket`

- <span id="tokioudpsocket-bindableudpsocket-bind"></span>`async fn bind(addr: SocketAddr) -> Result<Self, SocketError>` — [`SocketError`](../index.md#socketerror)


---

## Functions

### `build_unsafe_client_config`

```rust
fn build_unsafe_client_config() -> std::sync::Arc<rustls::ClientConfig>
```

Builds an unsafe `ClientConfig` with default TLS version negotiation (TLS 1.2 + 1.3).

### `build_unsafe_client_config_with_options`

```rust
fn build_unsafe_client_config_with_options(force_tls_1_2: bool) -> std::sync::Arc<rustls::ClientConfig>
```

Builds an unsafe `ClientConfig` with configurable TLS version constraints.

When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only. This is
required for P2S and X2D models whose embedded vsFTPd servers fail on TLS 1.3
session tickets [REF-FTPS-CONN].

### `build_verified_client_config`

```rust
fn build_verified_client_config(ca_certs: impl IntoIterator<Item = rustls_pki_types::CertificateDer<'static>>, client_auth: Option<(Vec<rustls_pki_types::CertificateDer<'static>>, rustls_pki_types::PrivateKeyDer<'static>)>) -> Result<std::sync::Arc<rustls::ClientConfig>, rustls::Error>
```

Builds a `ClientConfig` that verifies the printer's certificate against provided CA certs.

Use `rustls_pki_types::pem::PemObject` to load PEM files:
```ignore
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
let ca = CertificateDer::from_pem_file("ca.pem")?;
```

`client_auth`: pass `Some((cert_chain, key))` for mutual TLS, `None` for server-only verification.

### `build_verified_client_config_with_options`

```rust
fn build_verified_client_config_with_options(ca_certs: impl IntoIterator<Item = rustls_pki_types::CertificateDer<'static>>, client_auth: Option<(Vec<rustls_pki_types::CertificateDer<'static>>, rustls_pki_types::PrivateKeyDer<'static>)>, force_tls_1_2: bool) -> Result<std::sync::Arc<rustls::ClientConfig>, rustls::Error>
```

Builds a verified `ClientConfig` with configurable TLS version constraints.

When `force_tls_1_2` is true, negotiation is restricted to TLS 1.2 only (required
for FTPS data channels on P2S/X2D models [REF-FTPS-CONN]).

### `to_socket_error`

```rust
fn to_socket_error(err: std::io::Error) -> crate::io::SocketError
```

**Types:** [`SocketError`](../index.md#socketerror)

Helper mapping standard standard Rust IO errors to our runtime-agnostic SocketError enum.

