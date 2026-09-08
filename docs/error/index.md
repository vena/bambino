*[bambino](../index.md) / [error](index.md)*

---

# Module `error`

# Error Types

[`Error`](#error) is the single error type returned by all fallible operations in the
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
    Backpressure,
    InvalidArgument(std::borrow::Cow<'static, str>),
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

  Emitted when the broker refuses the connection with MQTT CONNACK code 4 or 5.
  
  Both codes mean the printer refused the credentials. Usually the 8-character LAN access
  code is wrong or has been rotated (a factory reset regenerates it); on some firmware the
  serial used as the username is the part it rejected, so a caller reporting this should
  name both rather than only the access code. Confirmed against bambuddy, which maps
  CONNACK 4 and 5 to one auth-rejected state on the same reasoning.
  
  A field report (ha-bambulab issue #1863) also attributes code 5 to a printer that is
  powered off or still booting. That is **not** corroborated by either reference client and
  is recorded here as reported, not established — do not present it to a user as a known
  cause without checking it independently.

- **`Timeout`**

  Handshake, read, or write negotiations exceeded designated timeouts.

- **`DiskWriteFailure`**

  Upload verification failed — printer reported unexpected file size after transfer.

- **`ModelMismatch`**

  Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) not present on the active model target.

- **`Backpressure`**

  The unacknowledged QoS 1 command queue is full; the command was not sent.
  
  Distinct from [`Timeout`](https://docs.rs/tokio/latest/tokio/time/timeout/struct.Timeout.html) on purpose: saturation is not a transient stall, so the
  natural retry-on-timeout policy is exactly the wrong response — a caller that keeps
  retrying spins against a queue only inbound PUBACKs (or the in-flight entries aging out)
  can drain.

- **`InvalidArgument`**

  Emitted when a caller-supplied argument fails client-side validation (e.g. an unknown
  axis name) before any command is sent to the printer.

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

