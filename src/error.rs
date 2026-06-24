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

    /// Physical write or storage block exhaustion faults reported by the printer.
    #[cfg_attr(
        feature = "std",
        error("Physical MicroSD read/write exception detected (System halted)")
    )]
    DiskWriteFailure,

    /// Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer)
    /// not present on the active model target.
    #[cfg_attr(
        feature = "std",
        error("Physical model capability mismatch for the active hardware profile")
    )]
    ModelMismatch,
}

impl From<crate::io::SocketError> for BambuError {
    fn from(e: crate::io::SocketError) -> Self {
        BambuError::NetworkError(e)
    }
}

#[cfg(not(feature = "std"))]
impl core::fmt::Display for BambuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BambuError::NetworkError(e) => write!(f, "Network transport failure: {:?}", e),
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
                "Physical MicroSD read/write exception detected (System halted)"
            ),
            BambuError::ModelMismatch => write!(
                f,
                "Physical model capability mismatch for the active hardware profile"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_consistency() {
        let variants: Vec<(BambuError, &str)> = vec![
            (
                BambuError::NetworkError(crate::io::SocketError::TimedOut),
                "Network transport failure: TimedOut",
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
                "Physical MicroSD read/write exception detected (System halted)",
            ),
            (
                BambuError::ModelMismatch,
                "Physical model capability mismatch for the active hardware profile",
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
