**bambino > error**

# Module: error

## Contents

**Enums**

- [`BambuError`](#bambuerror) - Unified error type for the `bambino` crate.

---

## bambino::error::BambuError

*Enum*

Unified error type for the `bambino` crate.

This enum wraps all protocol, serialization, and transport-level failures
with localized error contexts. Under `std` environments, standard formatting
and source error tracing are derived automatically via `thiserror`.

**Variants:**
- `NetworkError(crate::io::SocketError)` - Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.
- `TimerFailure(crate::io::TimerError)` - Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS
- `TlsHandshakeFailed` - Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
- `ProtocolViolation(std::borrow::Cow<'static, str>)` - Emitted when a printer violates expected protocol states or emits illegal data lines.
- `SerializationError` - Serializer and Deserializer mismatches during telemetry JSON parsing.
- `AccessDenied` - Emitted when the provided 8-character LAN access code fails verification checks.
- `Timeout` - Handshake, read, or write negotiations exceeded designated timeouts.
- `DiskWriteFailure` - Upload verification failed — printer reported unexpected file size after transfer.
- `ModelMismatch(std::borrow::Cow<'static, str>)` - Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer)

**Traits:** Error

**Trait Implementations:**

- **Display**
  - `fn fmt(self: &Self, __formatter: & mut ::core::fmt::Formatter) -> ::core::fmt::Result`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> BambuError`
- **From**
  - `fn from(e: crate::io::SocketError) -> Self`
- **From**
  - `fn from(e: crate::io::TimerError) -> Self`



