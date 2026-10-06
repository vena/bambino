//! # Error Types
//!
//! [`enum@Error`] is the single error type returned by all fallible operations in the
//! crate. It covers network failures, TLS handshake issues, protocol violations,
//! authentication rejections, timeouts, and model capability mismatches.
//!
//! One `core::fmt::Display` and one `core::error::Error` impl serve every target: `std`,
//! `alloc`-only and embassy alike.

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::borrow::Cow;

/// Unified error type for the `bambino` crate.
///
/// This enum wraps all protocol, serialization, and transport-level failures. `Network` and
/// `TimerFailure` return the wrapped I/O error from `source()`.
#[derive(Debug, Clone)]
pub enum Error {
    /// Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.
    Network(crate::io::SocketError),

    /// Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS timer resource exhaustion).
    TimerFailure(crate::io::TimerError),

    /// Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
    /// This frequently occurs during self-signed certificate verification or SNI mismatches.
    TlsHandshakeFailed,

    /// Emitted when a printer violates expected protocol states or emits illegal data lines.
    ProtocolViolation(Cow<'static, str>),

    /// Serializer and Deserializer mismatches during telemetry JSON parsing.
    Serialization,

    /// Emitted when the broker refuses the connection with MQTT CONNACK code 4 or 5.
    ///
    /// Both codes mean the printer refused the credentials. Usually the 8-character LAN access
    /// code is wrong or has been rotated (a factory reset regenerates it); on some firmware the
    /// serial used as the username is the part it rejected, so a caller reporting this should
    /// name both rather than only the access code. Confirmed against bambuddy, which maps
    /// CONNACK 4 and 5 to one auth-rejected state on the same reasoning.
    ///
    /// A field report (ha-bambulab issue #1863) also attributes code 5 to a printer that is
    /// powered off or still booting. That is **not** corroborated by either reference client and
    /// is recorded here as reported, not established — do not present it to a user as a known
    /// cause without checking it independently.
    AccessDenied,

    /// Handshake, read, or write negotiations exceeded designated timeouts.
    Timeout,

    /// Upload verification failed — printer reported unexpected file size after transfer.
    DiskWriteFailure,

    /// Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) not present on the active model target.
    ModelMismatch(Cow<'static, str>),

    /// The unacknowledged QoS 1 command queue is full; the command was not sent.
    ///
    /// Distinct from [`Error::Timeout`] on purpose: saturation is not a transient stall, so the
    /// natural retry-on-timeout policy is exactly the wrong response — a caller that keeps
    /// retrying spins against a queue only inbound PUBACKs (or the in-flight entries aging out)
    /// can drain.
    Backpressure,

    /// Emitted when a caller-supplied argument fails client-side validation (e.g. an unknown
    /// axis name) before any command is sent to the printer.
    InvalidArgument(Cow<'static, str>),

    /// Emitted when a command is refused because the printer's observed print state cannot
    /// act on it (e.g. skipping objects with no job loaded).
    ///
    /// Distinct from [`Error::InvalidArgument`]: the caller's arguments are well-formed, the
    /// machine is simply in the wrong state. Distinct from [`Error::ModelMismatch`], which is a
    /// permanent capability gap rather than a transient one — retrying after the printer reaches
    /// the right state is the correct response to this error, and is not for `ModelMismatch`.
    ///
    /// These gates read `PrinterClient`'s *cached* `gcode_state`, so they are only as fresh as
    /// the last `poll_telemetry()`. A caller that has not polled recently can be refused on a
    /// stale reading; poll and retry rather than treating it as final.
    InvalidState(Cow<'static, str>),
}

impl From<crate::io::SocketError> for Error {
    fn from(e: crate::io::SocketError) -> Self {
        Error::Network(e)
    }
}

impl From<crate::io::TimerError> for Error {
    fn from(e: crate::io::TimerError) -> Self {
        Error::TimerFailure(e)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Network(e) => write!(f, "Network transport failure: {e}"),
            Error::TimerFailure(e) => write!(f, "Timer scheduling failure: {e}"),
            Error::TlsHandshakeFailed => f.write_str("TLS secure channel handshake failed"),
            Error::ProtocolViolation(s) => write!(f, "Protocol violation: {s}"),
            Error::Serialization => {
                f.write_str("JSON payload serialization or deserialization failure")
            }
            Error::AccessDenied => {
                f.write_str("Authentication credentials rejected (access denied)")
            }
            Error::Timeout => f.write_str("Operational transaction timed out"),
            Error::DiskWriteFailure => {
                f.write_str("File upload verification failed (possible SD card write error)")
            }
            Error::ModelMismatch(s) => write!(f, "Model capability mismatch: {s}"),
            Error::Backpressure => {
                f.write_str("Command queue saturated with unacknowledged commands")
            }
            Error::InvalidArgument(s) => write!(f, "Invalid argument: {s}"),
            Error::InvalidState(s) => write!(f, "Invalid printer state: {s}"),
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Error::Network(e) => Some(e),
            Error::TimerFailure(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::error::Error as _;

    #[test]
    fn test_io_errors_display_and_chain() {
        let err = Error::Network(crate::io::SocketError::TimedOut);
        assert_eq!(err.to_string(), "Network transport failure: timed out");
        assert!(err.source().is_some());

        let err = Error::TimerFailure(crate::io::TimerError::Other(
            "esp_timer failed: 0x101".into(),
        ));
        assert_eq!(
            err.to_string(),
            "Timer scheduling failure: esp_timer failed: 0x101"
        );
        assert!(Error::Timeout.source().is_none());
    }

    #[test]
    fn test_from_socket_error() {
        let socket_err = crate::io::SocketError::ConnectionReset;
        let bambu_err: Error = socket_err.into();
        assert!(matches!(
            bambu_err,
            Error::Network(crate::io::SocketError::ConnectionReset)
        ));
    }

    #[test]
    fn test_from_timer_error() {
        let bambu_err: Error = crate::io::TimerError::ResourceExhausted.into();
        assert!(matches!(
            bambu_err,
            Error::TimerFailure(crate::io::TimerError::ResourceExhausted)
        ));
    }

    #[test]
    fn test_protocol_violation_from_dynamic_string() {
        let msg = format!("dynamic error: {}", 42);
        let err = Error::ProtocolViolation(msg.into());
        assert_eq!(format!("{}", err), "Protocol violation: dynamic error: 42");
    }
}
