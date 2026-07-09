*[bambino](../index.md) / [error](index.md)*

---

# Module `error`

# Error Types

[`BambuError`](#bambuerror) is the single error type returned by all fallible operations in the
crate. It covers network failures, TLS handshake issues, protocol violations,
authentication rejections, timeouts, and model capability mismatches.

Under `std`, variants get `Display`/`Error` impls via `thiserror`. Under `no_std`,
a manual `Display` impl is kept in sync (verified by `test_display_consistency`).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`BambuError`](#bambuerror) | enum | Unified error type for the `bambino` crate. |

## Types

### `BambuError`

```rust
enum BambuError {
    NetworkError(crate::io::SocketError),
    TimerFailure(crate::io::TimerError),
    TlsHandshakeFailed,
    ProtocolViolation(std::borrow::Cow<'static, str>),
    SerializationError,
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

- **`NetworkError`**

  Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.

- **`TimerFailure`**

  Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS timer resource exhaustion).

- **`TlsHandshakeFailed`**

  Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
  This frequently occurs during self-signed certificate verification or SNI mismatches.

- **`ProtocolViolation`**

  Emitted when a printer violates expected protocol states or emits illegal data lines.

- **`SerializationError`**

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

##### `impl Clone for BambuError`

- <span id="bambuerror-clone"></span>`fn clone(&self) -> BambuError` — [`BambuError`](#bambuerror)

##### `impl Debug for BambuError`

- <span id="bambuerror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for BambuError`

- <span id="bambuerror-display-fmt"></span>`fn fmt(&self, __formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result`

##### `impl Error for BambuError`

##### `impl ToString for BambuError`

- <span id="bambuerror-tostring-to-string"></span>`fn to_string(&self) -> String`

