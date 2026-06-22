#[cfg(feature = "std")]
use thiserror::Error;

#[cfg(feature = "std")]
use std::string::String;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::string::String;

/// Unified error type for the `bambu-lan` crate.
/// 
/// This enum wraps all protocol, serialization, and transport-level failures 
/// with localized error contexts. Under `std` environments, standard formatting 
/// and source error tracing are derived automatically via `thiserror`.
#[cfg_attr(feature = "std", derive(Error))]
#[derive(Debug)]
pub enum BambuError {
    /// Encapsulates direct socket-level failures on TCP, UDP, or TLS streams.
    #[cfg_attr(feature = "std", error("Network transport failure: {0:?}"))]
    NetworkError(crate::io::SocketError),

    /// Emitted when local MQTTS, FTPS, or RTSPS TLS negotiations fail.
    /// This frequently occurs during self-signed certificate verification or SNI mismatches.
    /// Refer to [REF-NET-SECURE] for certificate constraints.
    #[cfg_attr(feature = "std", error("TLS secure channel handshake failed"))]
    TlsHandshakeFailed,

    /// Emitted when a printer violates expected protocol states or emits illegal data lines.
    #[cfg_attr(feature = "std", error("Protocol violation: {0}"))]
    ProtocolViolation(&'static str),

    /// Emitted when a printer violates expected protocol states or emits illegal data lines,
    /// dynamically allocated for customized errors.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[cfg_attr(feature = "std", error("Protocol violation: {0}"))]
    ProtocolViolationDynamic(String),

    /// Serializer and Deserializer mismatches during telemetry JSON parsing.
    /// Refer to [REF-MQTT-ENV] for dynamic string emission anomalies.
    #[cfg_attr(feature = "std", error("JSON payload serialization or deserialization failure"))]
    SerializationError,

    /// Emitted when the provided 8-character LAN access code fails verification checks.
    #[cfg_attr(feature = "std", error("Authentication credentials rejected (access denied)"))]
    AccessDenied,

    /// Handshake, read, or write negotiations exceeded designated timeouts.
    #[cfg_attr(feature = "std", error("Operational transaction timed out"))]
    Timeout,

    /// Physical write or storage block exhaustion faults reported by the printer.
    /// Refer to [REF-FTPS-FLUSH] for MicroSD flush validation bugs.
    #[cfg_attr(feature = "std", error("Physical MicroSD read/write exception detected (System halted)"))]
    DiskWriteFailure,

    /// Emitted when requesting capabilities (e.g. door sensor checking on an open-frame printer) 
    /// not present on the active model target. Refer to [REF-NET-DOOR].
    #[cfg_attr(feature = "std", error("Physical model capability mismatch for the active hardware profile"))]
    ModelMismatch,
}

#[cfg(not(feature = "std"))]
impl core::fmt::Display for BambuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BambuError::NetworkError(e) => write!(f, "Network transport failure: {:?}", e),
            BambuError::TlsHandshakeFailed => write!(f, "TLS secure channel handshake failed"),
            BambuError::ProtocolViolation(s) => write!(f, "Protocol violation: {}", s),
            #[cfg(feature = "alloc")]
            BambuError::ProtocolViolationDynamic(s) => write!(f, "Protocol violation: {}", s),
            BambuError::SerializationError => write!(f, "JSON payload serialization or deserialization failure"),
            BambuError::AccessDenied => write!(f, "Authentication credentials rejected (access denied)"),
            BambuError::Timeout => write!(f, "Operational transaction timed out"),
            BambuError::DiskWriteFailure => write!(f, "Physical MicroSD read/write exception detected (System halted)"),
            BambuError::ModelMismatch => write!(f, "Physical model capability mismatch for the active hardware profile"),
        }
    }
}