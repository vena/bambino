*[bambino](../index.md) / [error](index.md)*

---

# Module `error`

# Error Types

[`enum@Error`] is the single error type returned by all fallible operations in the
crate. It covers network failures, TLS handshake issues, protocol violations,
authentication rejections, timeouts, and model capability mismatches.

Under `std`, variants get `Display`/`Error` impls via `thiserror`. Under `no_std`,
a manual `Display` impl delegates to `format_error_no_std`. `test_display_consistency`
(below) runs under the default `std` feature set and verifies the `thiserror`-generated
`std` impl agrees with `format_error_no_std` for every variant — the only piece left
uncovered is the trivial `#[cfg(not(feature = "std"))] impl Display` wiring itself.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`Error`](#error) | enum | Unified error type for the `bambino` crate. |

## Types

### `Error`

```rust
enum Error {
    Network(crate::io::SocketError),
    TimerFailure(crate::io::TimerError),
    TlsHandshakeFailed,
    ProtocolViolation(std::borrow::Cow<'static, str>),
    Serialization,
    AccessDenied,
    Timeout,
    DiskWriteFailure,
    ModelMismatch(std::borrow::Cow<'static, str>),
}
```

Unified error type for the `bambino` crate.

This enum wraps all protocol, serialization, and transport-level failures
with localized error contexts. Under `std` environments, standard formatting
and source error tracing are derived automatically via `thiserror`.

#### Variants

- **`Network`**

  Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.

- **`TimerFailure`**

  Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS timer resource exhaustion).

- **`TlsHandshakeFailed`**

  Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
  This frequently occurs during self-signed certificate verification or SNI mismatches.

- **`ProtocolViolation`**

  Emitted when a printer violates expected protocol states or emits illegal data lines.

- **`Serialization`**

  Serializer and Deserializer mismatches during telemetry JSON parsing.

- **`AccessDenied`**

  Emitted when the provided 8-character LAN access code fails verification checks.

- **`Timeout`**

  Handshake, read, or write negotiations exceeded designated timeouts.

- **`DiskWriteFailure`**

  Upload verification failed — printer reported unexpected file size after transfer.

- **`ModelMismatch`**

  Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) not present on the active model target.

#### Trait Implementations

##### `impl Clone for Error`

- <span id="error-clone"></span>`fn clone(&self) -> Error` — [`Error`](#error)

##### `impl Debug for Error`

- <span id="error-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for Error`

- <span id="error-display-fmt"></span>`fn fmt(&self, __formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result`

##### `impl Error for Error`

##### `impl ToString for Error`

- <span id="error-tostring-to-string"></span>`fn to_string(&self) -> String`

