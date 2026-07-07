//! # Error Types
//!
//! [`BambuError`] is the single error type returned by all fallible operations in the
//! crate. It covers network failures, TLS handshake issues, protocol violations,
//! authentication rejections, timeouts, and model capability mismatches.
//!
//! Under `std`, variants get `Display`/`Error` impls via `thiserror`. Under `no_std`,
//! a manual `Display` impl is kept in sync (verified by `test_display_consistency`).

#[cfg(feature = "std")]
use thiserror::Error;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::borrow::Cow;

/// Unified error type for the `bambino` crate.
///
/// This enum wraps all protocol, serialization, and transport-level failures
/// with localized error contexts. Under `std` environments, standard formatting
/// and source error tracing are derived automatically via `thiserror`.
#[cfg_attr(feature = "std", derive(Error))]
#[derive(Debug, Clone)]
pub enum BambuError {
    /// Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.
    #[cfg_attr(feature = "std", error("Network transport failure: {0:?}"))]
    NetworkError(crate::io::SocketError),

    /// Encapsulates platform timer/sleep scheduling failures (e.g. ESP-IDF FreeRTOS timer resource exhaustion).
    #[cfg_attr(feature = "std", error("Timer scheduling failure: {0:?}"))]
    TimerFailure(crate::io::TimerError),

    /// Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
    /// This frequently occurs during self-signed certificate verification or SNI mismatches.
    #[cfg_attr(feature = "std", error("TLS secure channel handshake failed"))]
    TlsHandshakeFailed,

    /// Emitted when a printer violates expected protocol states or emits illegal data lines.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[cfg_attr(feature = "std", error("Protocol violation: {0}"))]
    ProtocolViolation(Cow<'static, str>),

    /// Emitted when a printer violates expected protocol states or emits illegal data lines.
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    ProtocolViolation(&'static str),

    /// Serializer and Deserializer mismatches during telemetry JSON parsing.
    #[cfg_attr(
        feature = "std",
        error("JSON payload serialization or deserialization failure")
    )]
    SerializationError,

    /// Emitted when the provided 8-character LAN access code fails verification checks.
    #[cfg_attr(
        feature = "std",
        error("Authentication credentials rejected (access denied)")
    )]
    AccessDenied,

    /// Handshake, read, or write negotiations exceeded designated timeouts.
    #[cfg_attr(feature = "std", error("Operational transaction timed out"))]
    Timeout,

    /// Upload verification failed — printer reported unexpected file size after transfer.
    #[cfg_attr(
        feature = "std",
        error("File upload verification failed (possible SD card write error)")
    )]
    DiskWriteFailure,

    /// Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) not present on the active model target.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[cfg_attr(feature = "std", error("Model capability mismatch: {0}"))]
    ModelMismatch(Cow<'static, str>),

    /// Emitted when requesting capabilities not present on the active model target.
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    ModelMismatch(&'static str),
}

impl From<crate::io::SocketError> for BambuError {
    fn from(e: crate::io::SocketError) -> Self {
        BambuError::NetworkError(e)
    }
}

impl From<crate::io::TimerError> for BambuError {
    fn from(e: crate::io::TimerError) -> Self {
        BambuError::TimerFailure(e)
    }
}

#[cfg(not(feature = "std"))]
impl core::fmt::Display for BambuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BambuError::NetworkError(e) => write!(f, "Network transport failure: {:?}", e),
            BambuError::TimerFailure(e) => write!(f, "Timer scheduling failure: {:?}", e),
            BambuError::TlsHandshakeFailed => write!(f, "TLS secure channel handshake failed"),
            BambuError::ProtocolViolation(s) => write!(f, "Protocol violation: {}", s),
            BambuError::SerializationError => {
                write!(f, "JSON payload serialization or deserialization failure")
            }
            BambuError::AccessDenied => {
                write!(f, "Authentication credentials rejected (access denied)")
            }
            BambuError::Timeout => write!(f, "Operational transaction timed out"),
            BambuError::DiskWriteFailure => write!(
                f,
                "File upload verification failed (possible SD card write error)"
            ),
            BambuError::ModelMismatch(s) => {
                write!(f, "Model capability mismatch: {}", s)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time exhaustiveness guard, never called at runtime.
    /// A `match` with no wildcard arm over every `BambuError` variant: if a future variant is added
    /// without adding an arm here, this fails to *compile* rather than silently passing — a reminder to
    /// also add the new variant to `test_display_consistency`'s `variants` vec below, which only guards
    /// variants it's told about and won't catch a forgotten one on its own.
    #[allow(dead_code)]
    fn assert_all_variants_covered(e: &BambuError) {
        match e {
            BambuError::NetworkError(_) => {}
            BambuError::TimerFailure(_) => {}
            BambuError::TlsHandshakeFailed => {}
            BambuError::ProtocolViolation(_) => {}
            BambuError::SerializationError => {}
            BambuError::AccessDenied => {}
            BambuError::Timeout => {}
            BambuError::DiskWriteFailure => {}
            BambuError::ModelMismatch(_) => {}
        }
    }

    #[test]
    fn test_display_consistency() {
        let variants: Vec<(BambuError, &str)> = vec![
            (
                BambuError::NetworkError(crate::io::SocketError::TimedOut),
                "Network transport failure: TimedOut",
            ),
            (
                BambuError::TimerFailure(crate::io::TimerError::Other("scheduling failed")),
                "Timer scheduling failure: Other(\"scheduling failed\")",
            ),
            (
                BambuError::TlsHandshakeFailed,
                "TLS secure channel handshake failed",
            ),
            (
                BambuError::ProtocolViolation("test message".into()),
                "Protocol violation: test message",
            ),
            (
                BambuError::SerializationError,
                "JSON payload serialization or deserialization failure",
            ),
            (
                BambuError::AccessDenied,
                "Authentication credentials rejected (access denied)",
            ),
            (BambuError::Timeout, "Operational transaction timed out"),
            (
                BambuError::DiskWriteFailure,
                "File upload verification failed (possible SD card write error)",
            ),
            (
                BambuError::ModelMismatch("test capability".into()),
                "Model capability mismatch: test capability",
            ),
        ];

        for (variant, expected) in &variants {
            assert_eq!(
                format!("{}", variant),
                *expected,
                "Display mismatch for {:?}",
                variant
            );
        }
    }

    #[test]
    fn test_from_socket_error() {
        let socket_err = crate::io::SocketError::ConnectionReset;
        let bambu_err: BambuError = socket_err.into();
        assert!(matches!(
            bambu_err,
            BambuError::NetworkError(crate::io::SocketError::ConnectionReset)
        ));
    }

    #[test]
    fn test_from_timer_error() {
        let timer_err = crate::io::TimerError::Other("timer failed");
        let bambu_err: BambuError = timer_err.into();
        assert!(matches!(
            bambu_err,
            BambuError::TimerFailure(crate::io::TimerError::Other("timer failed"))
        ));
    }

    #[test]
    fn test_protocol_violation_from_static_str() {
        let err = BambuError::ProtocolViolation("static message".into());
        assert!(matches!(err, BambuError::ProtocolViolation(_)));
    }

    #[test]
    fn test_protocol_violation_from_dynamic_string() {
        let msg = format!("dynamic error: {}", 42);
        let err = BambuError::ProtocolViolation(msg.into());
        assert_eq!(format!("{}", err), "Protocol violation: dynamic error: 42");
    }

    #[test]
    fn test_bambu_error_is_clone() {
        let err = BambuError::Timeout;
        let cloned = err.clone();
        assert_eq!(format!("{}", err), format!("{}", cloned));
    }
}
