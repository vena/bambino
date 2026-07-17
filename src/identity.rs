//! # Printer Identity
//!
//! [`PrinterIdentity`] bundles the LAN address, serial number, and access code every
//! "connect to protocol X" entry point in this crate needs to dial and authenticate
//! against a specific printer, instead of passing them as three adjacent same-typed
//! `&str` parameters a caller could transpose without a compile error.
#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Address, serial number, and access code identifying one printer on the LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterIdentity {
    /// LAN IP address or hostname of the printer.
    pub ip: String,
    /// Printer's serial number, used for TLS SNI and MQTT topic scoping.
    pub serial: String,
    /// Printer's local network access code (found in its LAN-only settings screen).
    pub access_code: String,
}
