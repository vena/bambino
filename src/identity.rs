//! # Printer Identity
//!
//! [`PrinterIdentity`] bundles the three pieces of data every "connect to protocol X"
//! entry point in this crate needs to dial and authenticate against a specific
//! printer: its LAN address, serial number, and access code.
//!
//! Bundling these into one struct (instead of three adjacent same-typed `&str`
//! parameters) removes a transposition risk that isn't compiler-catchable
//! otherwise — nothing stops `fn connect(ip: &str, serial: &str, access_code: &str)`
//! from being called with two of those arguments swapped, since all three are the
//! same type.
//!
//! `ip`/`serial`/`access_code` are never `Option` — an omitted field would compile
//! away the caller's obligation to supply it, but a caller could then just as
//! easily supply a fabricated placeholder that type-checks fine and is silently
//! wrong. A missing constructor argument is a compile error; a wrong `Some(value)`
//! is not. Trading the former for the latter is a regression, not a fix.
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
